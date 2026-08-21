#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const usage = `usage: capture-live-feed-transfer.mjs [options]

Records producer-side live-feed byte facts without running a web or Android client.
When --archive-artifacts is supplied, immutable artifacts are hard-linked (or copied
across filesystems) before normal live-feed retention removes them. The resulting
live-feeds/v3 tree can be used as an accelerated simulation fixture after capture.

Options:
  --status-url <url>      Status endpoint (default: http://127.0.0.1:18095/live-feeds/status.json)
  --live-root <dir>       Local v3 contract root containing current.json.
  --output <dir>          Capture output directory (required).
  --duration-hours <n>    Capture duration (default: 24).
  --poll-seconds <n>      Poll interval (default: 15).
  --archive-artifacts     Preserve immutable manifests and payloads.
  --help                  Show this help.
`;

const args = parseArgs(process.argv.slice(2));
if (args.help) {
  console.log(usage);
  process.exit(0);
}
if (!args.output) {
  throw new Error("--output is required");
}
if (args.archiveArtifacts && !args.liveRoot) {
  throw new Error("--archive-artifacts requires --live-root");
}

const outputRoot = path.resolve(args.output);
const liveRoot = args.liveRoot ? path.resolve(args.liveRoot) : null;
const archiveRoot = path.join(outputRoot, "live-feeds", "v3");
const samplesPath = path.join(outputRoot, "samples.jsonl");
const eventsPath = path.join(outputRoot, "events.jsonl");
const metadataPath = path.join(outputRoot, "capture.json");
const statusStartPath = path.join(outputRoot, "status-start.json");
const statusLatestPath = path.join(outputRoot, "status-latest.json");
const startedAt = new Date();
const stopAtMs = startedAt.getTime() + args.durationHours * 60 * 60 * 1000;
let stopping = false;
let latestStatusAtUtc = null;

fs.mkdirSync(outputRoot, { recursive: true });
if (args.archiveArtifacts) {
  fs.mkdirSync(archiveRoot, { recursive: true });
}

const seenSamples = loadJsonlKeys(samplesPath, (row) => `${row.product}:${row.version}`);
const lastCurrentEntries = new Map();
let pollCount = 0;
let sampleCount = seenSamples.size;
let eventCount = countJsonlRows(eventsPath);

for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    stopping = true;
  });
}

writeMetadata("running");
while (!stopping && Date.now() < stopAtMs) {
  try {
    await poll();
  } catch (error) {
    console.error(`${new Date().toISOString()} capture poll failed: ${errorMessage(error)}`);
  }
  if (!stopping && Date.now() < stopAtMs) {
    await delay(Math.min(args.pollSeconds * 1000, stopAtMs - Date.now()));
  }
}
writeMetadata(stopping ? "stopped" : "complete");

async function poll() {
  const response = await fetch(args.statusUrl, {
    headers: { Accept: "application/json", "Cache-Control": "no-cache" },
  });
  if (!response.ok) {
    throw new Error(`status HTTP ${response.status}: ${response.statusText}`);
  }
  const status = await response.json();
  latestStatusAtUtc = status.generated_at_utc ?? latestStatusAtUtc;
  pollCount += 1;
  writeJsonAtomic(statusLatestPath, status);
  if (!fs.existsSync(statusStartPath)) {
    writeJsonAtomic(statusStartPath, status);
  }

  const current = readCurrentManifest();
  if (current) {
    captureCurrentChanges(current);
    if (args.archiveArtifacts) {
      copyFileReplacing(path.join(liveRoot, "current.json"), path.join(archiveRoot, "current.json"));
    }
  }

  for (const [product, productStatus] of Object.entries(status.products ?? {})) {
    for (const sample of productStatus.samples ?? []) {
      const key = `${product}:${sample.version}`;
      if (seenSamples.has(key)) continue;
      const versionFacts = captureVersion(product, sample.version);
      appendJsonl(samplesPath, {
        captured_at_utc: new Date().toISOString(),
        product,
        version: sample.version,
        status_sample: sample,
        ...versionFacts,
      });
      seenSamples.add(key);
      sampleCount += 1;
    }
  }
  writeMetadata("running");
  console.log(
    `${new Date().toISOString()} polls=${pollCount} samples=${sampleCount} events=${eventCount}`,
  );
}

function readCurrentManifest() {
  if (!liveRoot) return null;
  const currentPath = path.join(liveRoot, "current.json");
  if (!fs.existsSync(currentPath)) return null;
  return JSON.parse(fs.readFileSync(currentPath, "utf8"));
}

function captureCurrentChanges(current) {
  for (const [product, entry] of Object.entries(current.products ?? {})) {
    const serialized = JSON.stringify(entry);
    if (lastCurrentEntries.get(product) === serialized) continue;
    const kind = lastCurrentEntries.has(product) ? "observed_change" : "initial_snapshot";
    lastCurrentEntries.set(product, serialized);
    const payload = currentEventPayload(product, entry);
    appendJsonl(eventsPath, {
      captured_at_utc: new Date().toISOString(),
      kind,
      product,
      version: entry.current,
      sse_wire_bytes: sseWireBytes(product, payload),
      payload,
    });
    eventCount += 1;
    captureVersion(product, entry.current);
  }
}

function captureVersion(product, version) {
  if (!liveRoot) return { version_manifest_present: false };
  const relative = path.join("versions", product, `${version}.json`);
  const source = resolveUnder(liveRoot, relative);
  if (!fs.existsSync(source)) {
    return { version_manifest_present: false };
  }
  const bytes = fs.readFileSync(source);
  const manifest = JSON.parse(bytes.toString("utf8"));
  if (args.archiveArtifacts) {
    preserveFile(relative);
    const refs = [
      manifest.state,
      manifest.install_state,
      ...Object.values(manifest.install_profiles ?? {}),
      manifest.delta_from_previous,
      ...(manifest.recent_deltas ?? []),
    ].filter(Boolean);
    for (const ref of refs) {
      preservePayload(ref.url, ref === manifest.state);
    }
  }
  return {
    version_manifest_present: true,
    version_manifest_bytes: bytes.length,
    state_ref: manifest.state ?? null,
    install_state_ref: manifest.install_state ?? null,
    delta_from_previous_ref: manifest.delta_from_previous ?? null,
    recent_delta_count: manifest.recent_deltas?.length ?? 0,
  };
}

function preservePayload(relative, includeDirectory) {
  const source = resolveUnder(liveRoot, relative);
  if (!fs.existsSync(source)) return;
  if (includeDirectory && path.basename(source) === "manifest.json") {
    preserveDirectory(path.dirname(source));
  } else if (fs.statSync(source).isFile()) {
    preserveFile(path.relative(liveRoot, source));
  }
}

function preserveDirectory(sourceRoot) {
  for (const entry of fs.readdirSync(sourceRoot, { withFileTypes: true })) {
    const source = path.join(sourceRoot, entry.name);
    if (entry.isDirectory()) {
      preserveDirectory(source);
    } else if (entry.isFile()) {
      preserveFile(path.relative(liveRoot, source));
    }
  }
}

function preserveFile(relative) {
  const source = resolveUnder(liveRoot, relative);
  const target = resolveUnder(archiveRoot, relative);
  if (!fs.existsSync(source) || !fs.statSync(source).isFile()) return;
  fs.mkdirSync(path.dirname(target), { recursive: true });
  if (fs.existsSync(target)) return;
  try {
    fs.linkSync(source, target);
  } catch (error) {
    if (error?.code !== "EXDEV") throw error;
    fs.copyFileSync(source, target, fs.constants.COPYFILE_EXCL);
  }
}

function copyFileReplacing(source, target) {
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.copyFileSync(source, target);
}

function currentEventPayload(product, entry) {
  const payload = {
    schema_version: 3,
    product,
    version: entry.current,
    version_manifest_url: entry.version_manifest_url,
    state_url: entry.state_url,
    state_sha256: entry.state_sha256,
  };
  for (const key of ["published_at_utc", "collected_at_utc"]) {
    if (entry[key] != null) payload[key] = entry[key];
  }
  if (entry.history?.length) payload.history = entry.history;
  return payload;
}

function sseWireBytes(product, payload) {
  return Buffer.byteLength(
    `id: ${product}:${payload.version}\nevent: live-feed-current\ndata: ${JSON.stringify(payload)}\n\n`,
  );
}

function parseArgs(argv) {
  const parsed = {
    statusUrl: "http://127.0.0.1:18095/live-feeds/status.json",
    durationHours: 24,
    pollSeconds: 15,
    archiveArtifacts: false,
    help: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--help" || arg === "-h") parsed.help = true;
    else if (arg === "--status-url") parsed.statusUrl = requiredValue(argv, ++index, arg);
    else if (arg === "--live-root") parsed.liveRoot = requiredValue(argv, ++index, arg);
    else if (arg === "--output") parsed.output = requiredValue(argv, ++index, arg);
    else if (arg === "--duration-hours") {
      parsed.durationHours = positiveNumber(requiredValue(argv, ++index, arg), arg);
    } else if (arg === "--poll-seconds") {
      parsed.pollSeconds = positiveNumber(requiredValue(argv, ++index, arg), arg);
    } else if (arg === "--archive-artifacts") parsed.archiveArtifacts = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  return parsed;
}

function requiredValue(argv, index, flag) {
  const value = argv[index];
  if (!value || value.startsWith("--")) throw new Error(`${flag} requires a value`);
  return value;
}

function positiveNumber(value, flag) {
  const number = Number(value);
  if (!Number.isFinite(number) || number <= 0) throw new Error(`${flag} must be positive`);
  return number;
}

function resolveUnder(root, relative) {
  if (path.isAbsolute(relative)) throw new Error(`absolute live-feed path is forbidden: ${relative}`);
  const resolvedRoot = path.resolve(root);
  const resolved = path.resolve(resolvedRoot, relative);
  if (resolved !== resolvedRoot && !resolved.startsWith(`${resolvedRoot}${path.sep}`)) {
    throw new Error(`live-feed path escapes root: ${relative}`);
  }
  return resolved;
}

function loadJsonlKeys(file, keyForRow) {
  const keys = new Set();
  if (!fs.existsSync(file)) return keys;
  for (const line of fs.readFileSync(file, "utf8").split("\n")) {
    if (!line) continue;
    keys.add(keyForRow(JSON.parse(line)));
  }
  return keys;
}

function countJsonlRows(file) {
  if (!fs.existsSync(file)) return 0;
  return fs.readFileSync(file, "utf8").split("\n").filter(Boolean).length;
}

function appendJsonl(file, value) {
  fs.appendFileSync(file, `${JSON.stringify(value)}\n`);
}

function writeJsonAtomic(file, value) {
  const temporary = `${file}.tmp-${process.pid}`;
  fs.writeFileSync(temporary, `${JSON.stringify(value, null, 2)}\n`);
  fs.renameSync(temporary, file);
}

function writeMetadata(state) {
  writeJsonAtomic(metadataPath, {
    schema_version: 1,
    state,
    started_at_utc: startedAt.toISOString(),
    stop_at_utc: new Date(stopAtMs).toISOString(),
    updated_at_utc: new Date().toISOString(),
    latest_status_at_utc: latestStatusAtUtc,
    status_url: args.statusUrl,
    live_root: liveRoot,
    archive_artifacts: args.archiveArtifacts,
    poll_seconds: args.pollSeconds,
    polls: pollCount,
    samples: sampleCount,
    current_events: eventCount,
  });
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function errorMessage(error) {
  return error instanceof Error ? error.stack ?? error.message : String(error);
}
