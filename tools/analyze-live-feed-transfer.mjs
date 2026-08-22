#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";
import { gzipSync } from "node:zlib";

const MIB = 1024 * 1024;
const NEXRAD_HISTORY_ENTRIES = 6;
const BASELINE_HISTORY_ENTRIES = 12;
const DEFAULT_RESIDENCE_HOURS = Object.freeze({ shown: 4, hidden: 4, asleep: 16 });
const PROJECTED_SEPARATELY = new Set(["tfrs", "winds-aloft"]);

const usage = `usage: analyze-live-feed-transfer.mjs [options]

Accounts for payload and control-plane bytes in a capture produced by
capture-live-feed-transfer.mjs. Initial snapshots establish a warm cache and are
excluded from recurring transfer totals.

Options:
  --capture <dir>              Completed capture directory (required).
  --format <markdown|json>     Output format (default: markdown).
  --residence-hours <s,h,a>    Representative Android shown, hidden, asleep
                               hours; must sum to 24 (default: 4,4,16).
  --help                       Show this help.
`;

export function sseWireBytes(product, payload) {
  return Buffer.byteLength(
    `id: ${product}:${payload.version}\nevent: live-feed-current\ndata: ${JSON.stringify(payload)}\n\n`,
  );
}

function payloadWithoutHistory(payload) {
  const { history: _history, ...withoutHistory } = payload;
  return withoutHistory;
}

function payloadWithHistory(payload, history) {
  const result = payloadWithoutHistory(payload);
  if (history.length > 0) result.history = history;
  return result;
}

function historyDescriptor(payload) {
  return {
    version: payload.version,
    version_manifest_url: payload.version_manifest_url,
    state_url: payload.state_url,
    state_sha256: payload.state_sha256,
  };
}

export function annotateSseAccounting(events) {
  const stateByProduct = new Map();
  for (const event of events) {
    const actualBytes = sseWireBytes(event.product, event.payload);
    if (actualBytes !== event.sse_wire_bytes) {
      throw new Error(
        `captured SSE size mismatch for ${event.product}/${event.version}: `
          + `${event.sse_wire_bytes} recorded, ${actualBytes} encoded`,
      );
    }
    let state = stateByProduct.get(event.product);
    if (!state) {
      state = {
        current: null,
        history: [...(event.payload.history ?? [])],
      };
      stateByProduct.set(event.product, state);
    }
    if (state.current && state.current.version !== event.version) {
      state.history.push(state.current);
      state.history = state.history.slice(-BASELINE_HISTORY_ENTRIES);
    }
    const baselineHistory = state.history.slice(-BASELINE_HISTORY_ENTRIES);
    const currentHistory = event.product === "nexrad"
      ? state.history.slice(-NEXRAD_HISTORY_ENTRIES)
      : [];
    event.accounting = {
      ...event.accounting,
      sseActualBytes: actualBytes,
      sseBaselineBytes: sseWireBytes(
        event.product,
        payloadWithHistory(event.payload, baselineHistory),
      ),
      sseCurrentBytes: sseWireBytes(
        event.product,
        payloadWithHistory(event.payload, currentHistory),
      ),
    };

    state.current = historyDescriptor(event.payload);
  }
  return events;
}

function applicableDelta(manifest, installedVersion) {
  const delta = manifest.delta_from_previous;
  if (delta?.from_version === installedVersion && delta.to_version === manifest.version) {
    return delta;
  }
  return null;
}

function notamDeltaChain(manifest, installedVersion) {
  const deltas = [...(manifest.recent_deltas ?? [])];
  if (manifest.delta_from_previous) deltas.push(manifest.delta_from_previous);
  const byFrom = new Map(deltas.map((delta) => [delta.from_version, delta]));
  const chain = [];
  const seen = new Set();
  let version = installedVersion;
  while (version !== manifest.version) {
    if (seen.has(version)) return null;
    seen.add(version);
    const delta = byFrom.get(version);
    if (!delta) return null;
    chain.push(delta);
    version = delta.to_version;
  }
  return chain;
}

export function selectWarmPayload(manifest, installedVersion, options = {}) {
  const { historicalTfr = false, windsOnDemand = false } = options;
  if (!manifest || manifest.version === installedVersion) return null;
  if (manifest.product === "nexrad") {
    throw new Error("NEXRAD payload selection requires an acquisition policy");
  }
  if (manifest.product === "winds-aloft" && windsOnDemand) return null;
  const full = manifest.install_state ?? manifest.state ?? null;
  if (manifest.product === "notams") {
    const chain = notamDeltaChain(manifest, installedVersion);
    if (chain) {
      return {
        kind: "delta_chain",
        bytes: chain.reduce((sum, delta) => sum + delta.bytes, 0),
        refs: chain,
      };
    }
    return full && { kind: "full", bytes: full.bytes, refs: [full] };
  }
  if (manifest.product === "tfrs" && historicalTfr) {
    return full && { kind: "full", bytes: full.bytes, refs: [full] };
  }
  const delta = applicableDelta(manifest, installedVersion);
  if (delta && (!full || delta.bytes <= full.bytes)) {
    return { kind: "delta", bytes: delta.bytes, refs: [delta] };
  }
  return full && { kind: "full", bytes: full.bytes, refs: [full] };
}

function cadenceIntervalMs(cadence) {
  if (cadence === "every") return 0;
  if (cadence === "10m") return 10 * 60 * 1000;
  if (cadence === "30m") return 30 * 60 * 1000;
  if (cadence === "never") return null;
  throw new Error(`unsupported NEXRAD cadence: ${cadence}`);
}

function profileRef(manifest, profile) {
  return manifest?.install_profiles?.[profile] ?? null;
}

export function simulateNexradPolicy(events, options) {
  const {
    profile,
    cadenceForEpochMs,
  } = options;
  let lastInstallEpochMs = null;
  let bytes = 0;
  let frames = 0;
  const versions = [];
  const missingVersions = [];
  for (const event of events) {
    const cadence = cadenceForEpochMs(event.epochMs);
    const intervalMs = cadenceIntervalMs(cadence);
    if (intervalMs === null) continue;
    const due = lastInstallEpochMs === null
      || intervalMs === 0
      || event.epochMs - lastInstallEpochMs >= intervalMs;
    if (!due) continue;
    const ref = profileRef(event.manifest, profile);
    if (!ref) {
      missingVersions.push(event.version);
      continue;
    }
    bytes += ref.bytes;
    frames += 1;
    versions.push(event.version);
    lastInstallEpochMs = event.epochMs;
  }
  return { bytes, frames, versions, missingVersions };
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function readJsonLines(file) {
  return fs.readFileSync(file, "utf8")
    .split("\n")
    .filter(Boolean)
    .map((line, index) => {
      try {
        return JSON.parse(line);
      } catch (error) {
        throw new Error(`${file}:${index + 1}: ${error.message}`);
      }
    });
}

function resolveUnder(root, relative) {
  if (path.isAbsolute(relative)) throw new Error(`absolute capture path is forbidden: ${relative}`);
  const resolvedRoot = path.resolve(root);
  const resolved = path.resolve(resolvedRoot, relative);
  if (resolved !== resolvedRoot && !resolved.startsWith(`${resolvedRoot}${path.sep}`)) {
    throw new Error(`capture path escapes root: ${relative}`);
  }
  return resolved;
}

function alternateNotamManifestPath(relative) {
  if (!relative.startsWith("versions/notams/")) return null;
  if (relative.endsWith(".checkpoint.json")) {
    return relative.slice(0, -".checkpoint.json".length) + ".json";
  }
  if (relative.endsWith(".json")) {
    return relative.slice(0, -".json".length) + ".checkpoint.json";
  }
  return null;
}

function attachManifest(event, liveRoot, manifestCache) {
  const relative = event.payload.version_manifest_url;
  if (!relative) return;
  let cached = manifestCache.get(relative);
  if (!cached) {
    let file = resolveUnder(liveRoot, relative);
    let archivedAs = relative;
    const alternate = alternateNotamManifestPath(relative);
    if (!fs.existsSync(file) && alternate) {
      const alternateFile = resolveUnder(liveRoot, alternate);
      if (fs.existsSync(alternateFile)) {
        file = alternateFile;
        archivedAs = alternate;
      }
    }
    if (!fs.existsSync(file)) {
      cached = { missing: true, file };
    } else {
      const raw = fs.readFileSync(file);
      const manifest = JSON.parse(raw.toString("utf8"));
      if (manifest.product !== event.product || manifest.version !== event.version) {
        throw new Error(
          `archived manifest ${archivedAs} does not match ${event.product}/${event.version}`,
        );
      }
      cached = {
        missing: false,
        file,
        archivedAs,
        aliased: archivedAs !== relative,
        rawBytes: raw.length,
        gzipBytes: gzipSync(raw, { level: 6 }).length,
        manifest,
      };
    }
    manifestCache.set(relative, cached);
  }
  event.manifest = cached.manifest ?? null;
  event.manifestRawBytes = cached.rawBytes ?? null;
  event.manifestGzipBytes = cached.gzipBytes ?? null;
  event.manifestMissing = cached.missing;
  event.manifestAliased = cached.aliased ?? false;
}

export function loadCapture(captureRoot) {
  const root = path.resolve(captureRoot);
  const metadata = readJson(path.join(root, "capture.json"));
  const liveRoot = path.join(root, "live-feeds", "v3");
  const events = readJsonLines(path.join(root, "events.jsonl"))
    .map((event, index) => ({
      ...event,
      captureIndex: index,
      epochMs: Date.parse(event.captured_at_utc),
    }))
    .sort((left, right) => left.epochMs - right.epochMs || left.captureIndex - right.captureIndex);
  for (const event of events) {
    if (!Number.isFinite(event.epochMs)) {
      throw new Error(`invalid event timestamp: ${event.captured_at_utc}`);
    }
  }
  const manifestCache = new Map();
  for (const event of events) attachManifest(event, liveRoot, manifestCache);
  annotateSseAccounting(events);
  return { root, liveRoot, metadata, events, manifestCache };
}

function installedVersionsBefore(events, startEpochMs) {
  const versions = new Map();
  for (const event of events) {
    if (event.epochMs >= startEpochMs) break;
    versions.set(event.product, event.version);
  }
  return versions;
}

function initialInstalledVersions(events) {
  return new Map(
    events
      .filter((event) => event.kind === "initial_snapshot")
      .map((event) => [event.product, event.version]),
  );
}

function initialManifestUrls(events) {
  return new Set(
    events
      .filter((event) => event.kind === "initial_snapshot" && !event.manifestMissing)
      .map((event) => event.payload.version_manifest_url),
  );
}

function seenManifestUrlsBefore(events, startEpochMs) {
  return new Set(
    events
      .filter((event) => event.epochMs < startEpochMs && !event.manifestMissing)
      .map((event) => event.payload.version_manifest_url),
  );
}

function accountControlPlane(events, seenManifestUrls = new Set()) {
  const seen = new Set(seenManifestUrls);
  const result = {
    sseBaselineBytes: 0,
    sseCurrentBytes: 0,
    manifestRawBytes: 0,
    manifestGzipBytes: 0,
    manifestFetches: 0,
    missingManifestEvents: 0,
  };
  for (const event of events) {
    result.sseBaselineBytes += event.accounting.sseBaselineBytes;
    result.sseCurrentBytes += event.accounting.sseCurrentBytes;
    const url = event.payload.version_manifest_url;
    if (seen.has(url)) continue;
    seen.add(url);
    if (event.manifestMissing) {
      result.missingManifestEvents += 1;
      continue;
    }
    result.manifestRawBytes += event.manifestRawBytes;
    result.manifestGzipBytes += event.manifestGzipBytes;
    result.manifestFetches += 1;
  }
  result.baselineBytes = result.sseBaselineBytes + result.manifestRawBytes;
  result.currentBytes = result.sseCurrentBytes + result.manifestGzipBytes;
  return result;
}

function accountNonNexradPayloads(events, installedAtStart, options = {}) {
  const installed = new Map(installedAtStart);
  const byProduct = {};
  const selections = [];
  let bytes = 0;
  for (const event of events) {
    if (event.product === "nexrad" || !event.manifest) continue;
    const installedVersion = installed.get(event.product) ?? null;
    const selected = selectWarmPayload(event.manifest, installedVersion, options);
    if (!selected) continue;
    bytes += selected.bytes;
    byProduct[event.product] = (byProduct[event.product] ?? 0) + selected.bytes;
    selections.push({
      product: event.product,
      version: event.version,
      kind: selected.kind,
      bytes: selected.bytes,
    });
    installed.set(event.product, event.version);
  }
  return { bytes, byProduct, selections };
}

function firstProfileCompleteNexradEvent(events) {
  return events.find((event) => event.product === "nexrad"
    && event.kind === "observed_change"
    && profileRef(event.manifest, "offline_0")
    && profileRef(event.manifest, "offline_low1"));
}

function residenceCadence(startEpochMs, residenceHours) {
  const shownEnd = startEpochMs + residenceHours.shown * 60 * 60 * 1000;
  const hiddenEnd = shownEnd + residenceHours.hidden * 60 * 60 * 1000;
  return (epochMs) => {
    if (epochMs < shownEnd) return "every";
    if (epochMs < hiddenEnd) return "30m";
    return "never";
  };
}

function nexradPolicies(events, startEpochMs, residenceHours) {
  const policies = {};
  for (const profile of ["offline_0", "offline_low1"]) {
    for (const cadence of ["every", "10m", "30m", "never"]) {
      policies[`${profile}:${cadence}`] = simulateNexradPolicy(events, {
        profile,
        cadenceForEpochMs: () => cadence,
      });
    }
    policies[`${profile}:representative`] = simulateNexradPolicy(events, {
      profile,
      cadenceForEpochMs: residenceCadence(startEpochMs, residenceHours),
    });
  }
  return policies;
}

function byProductRows(before, after) {
  const products = new Set([...Object.keys(before), ...Object.keys(after)]);
  return [...products]
    .sort()
    .map((product) => ({
      product,
      beforeBytes: before[product] ?? 0,
      afterBytes: after[product] ?? 0,
    }));
}

export function analyzeCapture(captureRoot, options = {}) {
  const residenceHours = options.residenceHours ?? DEFAULT_RESIDENCE_HOURS;
  const capture = loadCapture(captureRoot);
  if (capture.metadata.state !== "complete") {
    throw new Error(`capture state is ${capture.metadata.state}, expected complete`);
  }
  const startEpochMs = Date.parse(capture.metadata.started_at_utc);
  const stopEpochMs = Date.parse(capture.metadata.stop_at_utc);
  const changes = capture.events.filter((event) => event.kind === "observed_change");
  const fullControl = accountControlPlane(
    changes,
    initialManifestUrls(capture.events),
  );
  const initialInstalled = initialInstalledVersions(capture.events);
  const fullHistoricalPayload = accountNonNexradPayloads(changes, initialInstalled, {
    historicalTfr: true,
  });
  const fullCurrentPayload = accountNonNexradPayloads(changes, initialInstalled);
  const fullOnDemandWindsPayload = accountNonNexradPayloads(changes, initialInstalled, {
    windsOnDemand: true,
  });

  const profileStartEvent = firstProfileCompleteNexradEvent(capture.events);
  if (!profileStartEvent) throw new Error("capture has no complete NEXRAD profile window");
  const profileStartEpochMs = profileStartEvent.epochMs;
  const windowChanges = changes.filter(
    (event) => event.epochMs >= profileStartEpochMs && event.epochMs < stopEpochMs,
  );
  const windowNexrad = windowChanges.filter((event) => event.product === "nexrad");
  const windowInstalled = installedVersionsBefore(capture.events, profileStartEpochMs);
  const windowControl = accountControlPlane(
    windowChanges,
    seenManifestUrlsBefore(capture.events, profileStartEpochMs),
  );
  const windowHistoricalPayload = accountNonNexradPayloads(windowChanges, windowInstalled, {
    historicalTfr: true,
  });
  const windowCurrentPayload = accountNonNexradPayloads(windowChanges, windowInstalled);
  const windowOnDemandWindsPayload = accountNonNexradPayloads(windowChanges, windowInstalled, {
    windsOnDemand: true,
  });
  const policies = nexradPolicies(windowNexrad, profileStartEpochMs, residenceHours);
  const referenceNexrad = policies["offline_0:every"].bytes;
  const referenceTotal = windowControl.baselineBytes
    + windowHistoricalPayload.bytes
    + referenceNexrad;
  const scenario = (profile, windsOnDemand = false) => {
    const payload = windsOnDemand ? windowOnDemandWindsPayload : windowCurrentPayload;
    const nexrad = policies[`${profile}:representative`];
    return {
      bytes: windowControl.currentBytes + payload.bytes + nexrad.bytes,
      controlBytes: windowControl.currentBytes,
      payloadBytes: payload.bytes,
      nexradBytes: nexrad.bytes,
      nexradFrames: nexrad.frames,
    };
  };
  const capturedCurrentByProduct = fullCurrentPayload.byProduct;
  const capturedHistoricalByProduct = fullHistoricalPayload.byProduct;
  const projectedTfrBytes = (windowCurrentPayload.byProduct.tfrs ?? 0)
    * 24 / ((stopEpochMs - profileStartEpochMs) / (60 * 60 * 1000));
  const unchangedCurrentPayloadBytes = Object.entries(capturedCurrentByProduct)
    .filter(([product]) => !PROJECTED_SEPARATELY.has(product))
    .reduce((sum, [, productBytes]) => sum + productBytes, 0);
  const automaticWindsBytes = capturedCurrentByProduct["winds-aloft"] ?? 0;
  const nexradDailyBytes = (profile) => {
    const everyRate = policies[`${profile}:every`].bytes
      / ((stopEpochMs - profileStartEpochMs) / (60 * 60 * 1000));
    const hiddenRate = policies[`${profile}:30m`].bytes
      / ((stopEpochMs - profileStartEpochMs) / (60 * 60 * 1000));
    return everyRate * residenceHours.shown + hiddenRate * residenceHours.hidden;
  };
  const dailyScenario = (profile, windsOnDemand = false) => {
    const nexradBytes = nexradDailyBytes(profile);
    const windsBytes = windsOnDemand ? 0 : automaticWindsBytes;
    const payloadBytes = unchangedCurrentPayloadBytes + projectedTfrBytes + windsBytes;
    return {
      bytes: fullControl.currentBytes + payloadBytes + nexradBytes,
      controlBytes: fullControl.currentBytes,
      payloadBytes,
      nexradBytes,
      windsBytes,
      projectedTfrBytes,
    };
  };
  const referenceNexradDailyBytes = policies["offline_0:every"].bytes
    * 24 / ((stopEpochMs - profileStartEpochMs) / (60 * 60 * 1000));
  const dailyReferenceBytes = fullControl.baselineBytes
    + Object.values(capturedHistoricalByProduct).reduce((sum, value) => sum + value, 0)
    + referenceNexradDailyBytes;

  const missingManifestEvents = changes.filter((event) => event.manifestMissing).length;
  const aliasedManifestEvents = changes.filter((event) => event.manifestAliased).length;
  return {
    schemaVersion: 1,
    capture: {
      root: capture.root,
      startedAtUtc: capture.metadata.started_at_utc,
      stoppedAtUtc: capture.metadata.stop_at_utc,
      durationHours: (stopEpochMs - startEpochMs) / (60 * 60 * 1000),
      polls: capture.metadata.polls,
      eventCount: capture.events.length,
      changeCount: changes.length,
      missingManifestEvents,
      aliasedManifestEvents,
    },
    assumptions: {
      warmCacheAtStart: true,
      initialSnapshotsExcluded: true,
      transportHeadersExcluded: true,
      baselineClientHistoryEntries: BASELINE_HISTORY_ENTRIES,
      currentNexradHistoryEntries: NEXRAD_HISTORY_ENTRIES,
      currentOtherHistoryEntries: 0,
      gzipLevel: 6,
      representativeResidenceHours: residenceHours,
    },
    fullCapture: {
      control: fullControl,
      historicalNonNexradPayload: fullHistoricalPayload,
      currentNonNexradPayload: fullCurrentPayload,
      onDemandWindsNonNexradPayload: fullOnDemandWindsPayload,
      payloadRows: byProductRows(
        fullHistoricalPayload.byProduct,
        fullCurrentPayload.byProduct,
      ),
    },
    profileWindow: {
      startedAtUtc: profileStartEvent.captured_at_utc,
      stoppedAtUtc: capture.metadata.stop_at_utc,
      durationHours: (stopEpochMs - profileStartEpochMs) / (60 * 60 * 1000),
      eventCount: windowChanges.length,
      nexradEventCount: windowNexrad.length,
      control: windowControl,
      historicalNonNexradPayload: windowHistoricalPayload,
      currentNonNexradPayload: windowCurrentPayload,
      onDemandWindsNonNexradPayload: windowOnDemandWindsPayload,
      nexradPolicies: policies,
      reference: {
        description: "raw control, TFR full states, automatic winds, full-profile NEXRAD every update",
        bytes: referenceTotal,
      },
      scenarios: {
        full: scenario("offline_0"),
        reduced: scenario("offline_low1"),
        fullWindsOnDemand: scenario("offline_0", true),
        reducedWindsOnDemand: scenario("offline_low1", true),
      },
    },
    dailyProjection: {
      description: "current-format corpus rates projected across explicit residence hours",
      reference: {
        description: "raw control, TFR full states, automatic winds, full-profile NEXRAD every update",
        bytes: dailyReferenceBytes,
      },
      scenarios: {
        full: dailyScenario("offline_0"),
        reduced: dailyScenario("offline_low1"),
        fullWindsOnDemand: dailyScenario("offline_0", true),
        reducedWindsOnDemand: dailyScenario("offline_low1", true),
      },
    },
    limitations: [
      "The capture crossed implementation changes, so full-day current-policy totals are not available for every policy.",
      "The profile-compatible window is exact for advertised bytes but is shorter than 24 hours.",
      "Suppressed TFR semantic no-ops do not appear as events; TFR savings here measure delta selection only.",
      "Viewport-only NEXRAD, web winds, and web obstacles require viewport/route request traces and are not totaled.",
      "HTTP request/response headers, TCP, TLS, retries, and carrier accounting are excluded.",
    ],
  };
}

function formatBytes(bytes) {
  if (bytes >= MIB) return `${(bytes / MIB).toFixed(2)} MiB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${bytes} B`;
}

function saving(before, after) {
  if (before === 0) return "n/a";
  return `${((before - after) * 100 / before).toFixed(1)}%`;
}

function markdownTable(headers, rows, rightAligned = new Set()) {
  const widths = headers.map((header, index) => Math.max(
    header.length,
    ...rows.map((row) => String(row[index]).length),
  ));
  const formatRow = (row) => `| ${row.map((value, index) => {
    const text = String(value);
    return rightAligned.has(index) ? text.padStart(widths[index]) : text.padEnd(widths[index]);
  }).join(" | ")} |`;
  const separators = widths.map((width, index) => {
    if (rightAligned.has(index)) return `${"-".repeat(Math.max(2, width - 1))}:`;
    return "-".repeat(Math.max(3, width));
  });
  return [formatRow(headers), formatRow(separators), ...rows.map(formatRow)].join("\n");
}

export function formatMarkdown(report) {
  const full = report.fullCapture;
  const window = report.profileWindow;
  const residence = report.assumptions.representativeResidenceHours;
  const daily = report.dailyProjection;
  const controlRows = [
    ["SSE history", formatBytes(full.control.sseBaselineBytes), formatBytes(full.control.sseCurrentBytes), saving(full.control.sseBaselineBytes, full.control.sseCurrentBytes)],
    ["Version manifests", formatBytes(full.control.manifestRawBytes), formatBytes(full.control.manifestGzipBytes), saving(full.control.manifestRawBytes, full.control.manifestGzipBytes)],
    ["Combined control", formatBytes(full.control.baselineBytes), formatBytes(full.control.currentBytes), saving(full.control.baselineBytes, full.control.currentBytes)],
  ];
  const payloadRows = full.payloadRows.map((row) => [
    row.product,
    formatBytes(row.beforeBytes),
    formatBytes(row.afterBytes),
    saving(row.beforeBytes, row.afterBytes),
  ]);
  const policyRows = [
    ["Full", "Every update", window.nexradPolicies["offline_0:every"]],
    ["Full", "10 minutes", window.nexradPolicies["offline_0:10m"]],
    ["Full", "30 minutes", window.nexradPolicies["offline_0:30m"]],
    ["Reduced", "Every update", window.nexradPolicies["offline_low1:every"]],
    ["Reduced", "10 minutes", window.nexradPolicies["offline_low1:10m"]],
    ["Reduced", "30 minutes", window.nexradPolicies["offline_low1:30m"]],
    ["Either", "Never", window.nexradPolicies["offline_0:never"]],
  ].map(([profile, cadence, result]) => [
    profile,
    cadence,
    result.frames,
    formatBytes(result.bytes),
    `${(result.bytes / MIB / window.durationHours).toFixed(2)} MiB/h`,
  ]);
  const scenarioRows = [
    ["Reference", window.reference.bytes],
    ["Current, full", window.scenarios.full.bytes],
    ["Current, reduced", window.scenarios.reduced.bytes],
    ["Current, full + winds on demand", window.scenarios.fullWindsOnDemand.bytes],
    ["Current, reduced + winds on demand", window.scenarios.reducedWindsOnDemand.bytes],
  ].map(([name, bytes]) => [name, formatBytes(bytes), saving(window.reference.bytes, bytes)]);
  const dailyRows = [
    ["Current-format reference", daily.reference.bytes],
    ["Current, full", daily.scenarios.full.bytes],
    ["Current, reduced", daily.scenarios.reduced.bytes],
    ["Current, full + winds on demand", daily.scenarios.fullWindsOnDemand.bytes],
    ["Current, reduced + winds on demand", daily.scenarios.reducedWindsOnDemand.bytes],
  ].map(([name, bytes]) => [name, formatBytes(bytes), saving(daily.reference.bytes, bytes)]);

  return `# Live-feed transfer corpus accounting

Capture: \`${report.capture.root}\`

Interval: ${report.capture.startedAtUtc} through ${report.capture.stoppedAtUtc}

Observed changes: ${report.capture.changeCount}; missing version manifests: ${report.capture.missingManifestEvents}; recovered NOTAM aliases: ${report.capture.aliasedManifestEvents}

Initial snapshots establish a warm cache and are excluded. Byte totals exclude
HTTP headers, TCP/TLS overhead, retries, and carrier accounting.

## Full captured day: exact control-plane model

${markdownTable(["Component", "Historical", "Current", "Saving"], controlRows, new Set([1, 2, 3]))}

"Historical" reconstructs twelve history entries per product and raw version
manifests. "Current" keeps six history entries only for NEXRAD and applies gzip
level 6 to version manifests.

## Full captured day: non-NEXRAD Android payloads

${markdownTable(["Product", "Historical", "Captured current", "Saving"], payloadRows, new Set([1, 2, 3]))}

The TFR current column is transition-aware and therefore conservative: the first
part of the day predates the new delta. Suppressed semantic no-ops are absent from
the event log and are not counted as additional savings. Winds remains automatic
in both columns.

## Profile-compatible NEXRAD window

Exact advertised profile accounting is available from ${window.startedAtUtc}
through ${window.stoppedAtUtc} (${window.durationHours.toFixed(2)} hours,
${window.nexradEventCount} NEXRAD publications).

${markdownTable(["Profile", "Cadence", "Frames", "Bytes", "Observed rate"], policyRows, new Set([2, 3, 4]))}

## Combined profile-window scenarios

The representative current scenario is explicitly ${residence.shown} hours shown,
${residence.hidden} hours hidden, and ${residence.asleep} hours asleep, starting at
the profile-window boundary. Current default cadence is every update while shown,
every 30 minutes while hidden, and never while asleep. Only the first
${window.durationHours.toFixed(2)} hours of that scenario occur in this exact window.

${markdownTable(["Scenario", "Bytes in window", "Saving vs reference"], scenarioRows, new Set([1, 2]))}

Reference: ${window.reference.description}.

## Modeled 24-hour comparison

This projection combines the exact full-day control and non-NEXRAD observations,
the fully implemented TFR rate from the profile-compatible window, and the exact
observed NEXRAD profile/cadence rates. Residence is ${residence.shown}/${residence.hidden}/${residence.asleep}
hours shown/hidden/asleep.

${markdownTable(["Scenario", "Modeled 24-hour bytes", "Saving vs reference"], dailyRows, new Set([1, 2]))}

The current-format reference is not the original 365.1 MiB/day baseline: it
already benefits from publishing a single base resolution in the full NEXRAD
profile. It is the apples-to-apples reference supported by this corpus.

## Limits

${report.limitations.map((limit) => `- ${limit}`).join("\n")}
`;
}

function parseResidenceHours(value) {
  const parts = value.split(",").map(Number);
  if (parts.length !== 3 || parts.some((part) => !Number.isFinite(part) || part < 0)) {
    throw new Error("--residence-hours must be three non-negative numbers");
  }
  const [shown, hidden, asleep] = parts;
  if (Math.abs(shown + hidden + asleep - 24) > 1e-9) {
    throw new Error("--residence-hours must sum to 24");
  }
  return { shown, hidden, asleep };
}

function parseArgs(argv) {
  const result = {
    format: "markdown",
    residenceHours: DEFAULT_RESIDENCE_HOURS,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--help" || arg === "-h") result.help = true;
    else if (arg === "--capture") result.capture = argv[++index];
    else if (arg === "--format") result.format = argv[++index];
    else if (arg === "--residence-hours") result.residenceHours = parseResidenceHours(argv[++index]);
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (!result.help && !result.capture) throw new Error("--capture is required");
  if (!new Set(["markdown", "json"]).has(result.format)) {
    throw new Error("--format must be markdown or json");
  }
  return result;
}

function isMain() {
  return process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url;
}

if (isMain()) {
  try {
    const args = parseArgs(process.argv.slice(2));
    if (args.help) {
      process.stdout.write(usage);
    } else {
      const report = analyzeCapture(args.capture, { residenceHours: args.residenceHours });
      process.stdout.write(args.format === "json"
        ? `${JSON.stringify(report, null, 2)}\n`
        : formatMarkdown(report));
    }
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.stack : String(error)}\n`);
    process.exitCode = 1;
  }
}
