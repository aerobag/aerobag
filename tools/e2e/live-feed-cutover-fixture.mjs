// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { createHash } from "node:crypto";
import { LIVE_FEED_ROOT, LIVE_FEED_SCHEMA_VERSION } from "./live-feed-contract-paths.mjs";

function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  }
  return value;
}

const bytesFor = (value) => Buffer.from(JSON.stringify(canonical(value)));
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");

export const CUTOVER_MARKERS = Object.freeze({
  old: "CUTOVEROLD", late: "CUTOVEROBSOLETE", missing: "CUTOVERMISSING",
  new: "CUTOVERNEW", next: "CUTOVERNEXT",
});

function seed(version, marker, count, timestamp) {
  const state = {
    schema_version: LIVE_FEED_SCHEMA_VERSION,
    version_label: version,
    generated_at_utc: timestamp,
    observed_at_utc: timestamp,
    metar_count: count,
    metars_by_station: Object.fromEntries(["KSEA", "KBFI", "KRNT", "KPAE"].slice(0, count).map((id, index) => [id, {
      station_id: id,
      raw_text: `METAR ${id} 091200Z AUTO 00000KT 10SM CLR 18/09 A3000 RMK ${marker}`,
      observed_at_utc: timestamp,
      flight_category: "vfr",
      clouds: { symbol: "clr" },
      // Synthetic stations stay inside the inspected viewport on either platform.
      longitude: Number((-122.309306 + index * 0.002).toFixed(6)),
      latitude: Number((47.449889 + index * 0.002).toFixed(6)),
    }])),
    pireps: [],
  };
  const bytes = bytesFor(state);
  const stateRef = {
    kind: "json", url: `states/metars/${version}.json`, bytes: bytes.length,
    blob_sha256: hash(bytes), state_sha256: hash(bytes),
  };
  return {
    version, bytes, state,
    current: {
      current: version, version_manifest_url: `versions/metars/${version}.json`,
      state_url: stateRef.url, state_sha256: stateRef.state_sha256,
      collected_at_utc: timestamp, published_at_utc: timestamp,
    },
    manifest: { schema_version: LIVE_FEED_SCHEMA_VERSION, product: "metars", version, state: stateRef },
  };
}

function send(response, status, body) {
  const bytes = Buffer.isBuffer(body) ? body : bytesFor(body);
  response.writeHead(status, {
    "Content-Type": "application/json", "Content-Length": bytes.length,
    "Cache-Control": "no-store",
  });
  response.end(bytes);
}

function event(response, name, id, payload) {
  response.write(`id: ${id}\nevent: ${name}\ndata: ${JSON.stringify(payload)}\n\n`);
}

// Scripted wire providers, not daemon emulators: each owns an immutable resource
// map. Looking up a missing old version must never synthesize it on the new one.
export class LiveFeedCutoverFixture {
  constructor(baseCatalog) {
    this.baseCatalog = baseCatalog;
    this.streams = new Map();
    this.pending = new Set();
    this.reset();
  }

  reset() {
    this.close();
    const timestamp = this.baseCatalog.generated_at_utc;
    const old = seed("old-1", CUTOVER_MARKERS.old, 1, timestamp);
    const late = seed("old-late", CUTOVER_MARKERS.late, 4, timestamp);
    const missing = seed("old-missing", CUTOVER_MARKERS.missing, 4, timestamp);
    const first = seed("new-1", CUTOVER_MARKERS.new, 2, timestamp);
    const next = seed("new-2", CUTOVER_MARKERS.next, 3, timestamp);
    // This independently seeded provider has a predecessor the client never
    // installed and the provider no longer retains. Only the full state works.
    first.manifest.delta_from_previous = {
      kind: "record_json_delta_xz", from_version: "new-retired", to_version: first.version,
      from_state_sha256: hash(Buffer.from("independent retired state")),
      to_state_sha256: first.current.state_sha256,
      url: "deltas/metars/new-retired__new-1.json.xz", bytes: 1,
      blob_sha256: hash(Buffer.from("retired delta")),
    };
    this.providers = {
      old: { current: old, entries: [old, late, missing] },
      new: { current: first, entries: [first, next] },
    };
    for (const provider of Object.values(this.providers)) {
      provider.resources = new Map(provider.entries.flatMap((entry) => [
        [entry.current.version_manifest_url, bytesFor(entry.manifest)],
        [entry.current.state_url, entry.bytes],
      ]));
    }
    this.active = "old";
    this.phase = "old";
    this.suppressResync = false;
    this.holdLate = false;
    this.trace = [];
    this.sequence = 0;
  }

  record(kind, fields = {}) {
    const previous = this.trace.at(-1);
    if (kind === "resource" && previous?.kind === kind &&
        previous.provider === fields.provider && previous.path === fields.path && previous.status === fields.status) {
      previous.requests = (previous.requests ?? 1) + 1;
      return;
    }
    this.trace.push({ sequence: ++this.sequence, kind, ...fields });
    if (this.trace.length > 200) this.trace.shift();
  }

  status() {
    return {
      active: this.active, phase: this.phase, suppress_resync: this.suppressResync,
      streams: Object.fromEntries(["old", "new"].map((name) => [
        name, [...this.streams.values()].filter((provider) => provider === name).length,
      ])),
      pending: this.pending.size, trace: this.trace.slice(),
    };
  }

  catalog(providerName) {
    const current = this.providers[providerName].current;
    return {
      ...this.baseCatalog,
      products: { ...this.baseCatalog.products, metars: current.current },
    };
  }

  publish(providerName, version) {
    const provider = this.providers[providerName];
    const entry = provider.entries.find((entry) => entry.version === version);
    if (!entry) throw new Error(`unknown ${providerName} version ${version}`);
    provider.current = entry;
    for (const [stream, owner] of this.streams) {
      if (owner !== providerName || (owner === "new" && this.suppressResync)) continue;
      const { current: _, ...current } = entry.current;
      event(stream, "live-feed-current", `metars:${version}`, {
        schema_version: LIVE_FEED_SCHEMA_VERSION, product: "metars", version, ...current,
      });
    }
    this.record("publish", { provider: providerName, version });
  }

  control(update) {
    const action = update?.action;
    if (action === "arm") {
      if (this.phase !== "old") throw new Error("arm requires old phase");
      if (typeof update.suppress_resync !== "boolean") throw new Error("arm requires boolean suppress_resync");
      if (!this.status().streams.old) throw new Error("arm requires a connected old client");
      this.suppressResync = update.suppress_resync;
      this.phase = "armed";
      this.holdLate = true;
      this.publish("old", "old-late");
    } else if (action === "switch") {
      if (this.phase !== "armed" || !this.pending.size) throw new Error("switch requires the old response barrier");
      this.active = "new";
      this.phase = "switched";
      this.record("switch", { old_streams: this.status().streams.old, pending: this.pending.size });
      this.publish("old", "old-missing");
    } else if (action === "release-old") {
      if (this.phase !== "switched") throw new Error("release-old requires switched phase");
      this.holdLate = false;
      for (const pending of [...this.pending]) {
        this.pending.delete(pending);
        this.record("release-old", { canceled: pending.response.destroyed });
        if (!pending.response.destroyed) send(pending.response, 200, pending.bytes);
      }
    } else if (action === "drain") {
      if (this.phase !== "switched") throw new Error("drain requires switched phase");
      this.phase = "drained";
      for (const [stream, owner] of this.streams) {
        if (owner === "old") stream.end();
      }
      this.record("drain");
    } else if (action === "next") {
      if (this.phase !== "drained") throw new Error("next requires drained phase");
      this.phase = "next";
      this.publish("new", "new-2");
    } else {
      throw new Error(`unknown live-feed cutover action ${action}`);
    }
    return this.status();
  }

  handle(request, response, pathname) {
    if (!pathname.startsWith(`${LIVE_FEED_ROOT}/`)) return false;
    const relative = pathname.slice(LIVE_FEED_ROOT.length + 1);
    if (!["events", "current.json"].includes(relative) &&
        !/^(versions|states|deltas)\/metars\//.test(relative)) return false;
    const provider = this.active;
    if (relative === "events") {
      response.writeHead(200, { "Content-Type": "text/event-stream", "Cache-Control": "no-store", Connection: "keep-alive" });
      response.write(": cutover provider connected\n\n");
      this.streams.set(response, provider);
      this.record("connect", { provider, suppressed: provider === "new" && this.suppressResync });
      if (!(provider === "new" && this.suppressResync)) {
        event(response, "live-feed-catalog", `catalog:${provider}:${this.providers[provider].current.version}`, this.catalog(provider));
      }
      response.once("close", () => {
        this.streams.delete(response);
        this.record("disconnect", { provider });
      });
    } else if (relative === "current.json") {
      this.record("catalog", { provider });
      send(response, provider === "new" && this.suppressResync ? 503 : 200, this.catalog(provider));
    } else {
      const bytes = this.providers[provider].resources.get(relative);
      this.record("resource", { provider, path: relative, status: bytes ? 200 : 404 });
      if (provider === "old" && relative === "states/metars/old-late.json" && this.holdLate) {
        const pending = { response, bytes };
        this.pending.add(pending);
        this.record("held-old");
        response.once("close", () => {
          // Keep canceled completions in the barrier until explicitly released.
          this.record("old-response-closed", { finished: response.writableFinished });
        });
      } else {
        send(response, bytes ? 200 : 404, bytes ?? { error: "resource not retained by this provider" });
      }
    }
    return true;
  }

  close() {
    for (const stream of this.streams.keys()) stream.destroy();
    this.streams.clear();
    for (const pending of this.pending) pending.response.destroy();
    this.pending.clear();
  }
}
