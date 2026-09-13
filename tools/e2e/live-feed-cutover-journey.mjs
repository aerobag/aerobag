// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { CUTOVER_MARKERS } from "./live-feed-cutover-fixture.mjs";
import { join } from "node:path";
import { E2E_TIMING, observeUntil, ObservationTimeoutError } from "./transition-contract.mjs";

const id = (entry) => typeof entry === "string" ? entry : entry?.id ?? entry?.test_id ?? "";

export async function liveFeedProviderCutover(runtime, helpers, {
  suppressResync = process.env.AEROBAG_E2E_CUTOVER_SUPPRESS_RESYNC === "1",
  observeBeforeDrainMs = Number(process.env.AEROBAG_E2E_CUTOVER_OBSERVE_BEFORE_DRAIN_MS ?? 0),
} = {}) {
  if (!Number.isInteger(observeBeforeDrainMs) || observeBeforeDrainMs < 0 || observeBeforeDrainMs > 15_000) {
    throw new Error("pre-drain observation must be between 0 and 15000 ms");
  }
  const { acceptDisclaimer, appendRoute, setLayerVisible, selectAirportFromMapSearch,
    closeMapDetail, setFixtureControl, fixtureHealth, planRows } = helpers;
  const route = ["KSEA", "KBFI", "KRNT"];
  const planSnapshot = async () => (await planRows(runtime)).map((row) => ({
    id: id(row),
    airports: route.filter((airport) => (row.text ?? id(row)).split(/\s+/).includes(airport)),
  })).filter((row) => row.airports.length);
  const control = async (action, options = {}) => (await setFixtureControl(runtime, {
    live_feed_cutover: { action, ...options },
  })).live_feed_cutover;
  const status = async () => {
    const value = (await fixtureHealth(runtime)).live_feed_cutover;
    if (!value) throw new Error("cutover profile is not running");
    runtime.result.diagnostics.live_feed_cutover = value;
    return value;
  };
  const barrier = (description, predicate) => runtime.eventually(description, async () => {
    const value = await status();
    return predicate(value) ? value : null;
  }, E2E_TIMING.resourceMs);
  const projection = async (prefix) => (await runtime.driver.readProjection(prefix)).map(id).sort();
  const mapState = async () => ({
    viewport: await projection("parity:viewport:"),
    layers: await projection("parity:map-layer:"),
    family: await projection("parity:map-family:"),
  });
  const overlay = async (count) => {
    try {
      return await runtime.eventually(`cutover rendered METAR count ${count}`, async () => {
        const rows = await projection("parity:live-overlay:");
        return rows.some((row) => row.includes(`:metars:${count}:`)) ? rows : null;
      }, E2E_TIMING.resourceMs);
    } catch (error) {
      await status();
      throw error;
    }
  };
  const weather = async (marker, count) => {
    await overlay(count);
    await selectAirportFromMapSearch(runtime, "KSEA");
    await runtime.action(`inspect ${marker} weather`, "wx", {
      complete: () => runtime.driver.readModal("weather-detail-modal"),
    });
    const detail = await runtime.eventually(`rendered ${marker} weather`, async () => {
      const modal = await runtime.driver.readElement("weather-detail-modal");
      return modal?.text?.includes(marker) ? modal : null;
    }, E2E_TIMING.resourceMs);
    await runtime.driver.captureFrame(join(runtime.artifactDir, `cutover-${marker}.png`));
    await closeMapDetail(runtime, "weather-detail-modal");
    return detail.text;
  };

  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("flight_plan");
  await appendRoute(runtime, route.join(" "));
  const planBefore = await planSnapshot();
  if (JSON.stringify(planBefore.flatMap((row) => row.airports)) !== JSON.stringify(route)) {
    throw new Error(`cutover requires the ordered three-leg plan: ${JSON.stringify(planBefore)}`);
  }
  await runtime.openPage("map");
  await setLayerVisible(runtime, "vectors", true);
  await setLayerVisible(runtime, "metars", true);
  await setLayerVisible(runtime, "nexrad", false);
  await selectAirportFromMapSearch(runtime, "KSEA");
  await runtime.driver.back();
  const oldText = await weather(CUTOVER_MARKERS.old, 1);
  runtime.check("cutover.old-weather", oldText.includes(CUTOVER_MARKERS.old), oldText.slice(0, 400));
  const before = await mapState();
  if (!before.viewport.length || !before.layers.length || !before.family.length) {
    throw new Error(`missing preserved-state projections: ${JSON.stringify(before)}`);
  }
  await barrier("old provider SSE connected", (value) => value.streams.old > 0);
  await control("arm", { suppress_resync: suppressResync });
  await barrier("old snapshot response held", (value) => value.pending > 0);
  const switched = await control("switch");
  const switchEvent = switched.trace.find((entry) => entry.kind === "switch");
  runtime.check("cutover.retained-stream", switchEvent?.old_streams > 0 && switchEvent?.pending > 0, JSON.stringify(switched));
  await control("release-old");
  await barrier("old-only reference unavailable on new provider", (value) => value.trace.some((entry) =>
    entry.kind === "resource" && entry.provider === "new" && entry.status === 404 && entry.path.includes("old-missing")));
  if (observeBeforeDrainMs) {
    let refreshed = false;
    try {
      await observeUntil("spontaneous pre-drain catalog refresh", async () => {
        const value = await status();
        return value.trace.some((entry) => entry.kind === "connect" && entry.provider === "new") ? value : null;
      }, { timeoutMs: observeBeforeDrainMs, intervalMs: 100 });
      refreshed = true;
    } catch (error) {
      if (!(error instanceof ObservationTimeoutError)) throw error;
    }
    runtime.result.diagnostics.before_drain = {
      refreshed, observation_ms: observeBeforeDrainMs, fixture: await status(),
      overlay: await projection("parity:live-overlay:"),
    };
  }
  await control("drain");
  await barrier("new provider SSE reconnect", (value) => value.trace.some((entry) => entry.kind === "connect" && entry.provider === "new"));
  await overlay(2);
  const after = await mapState();
  const newText = await weather(CUTOVER_MARKERS.new, 2);
  runtime.check("cutover.new-weather", newText.includes(CUTOVER_MARKERS.new) && !newText.includes(CUTOVER_MARKERS.late), newText.slice(0, 400));
  const recovered = await status();
  runtime.check("cutover.full-resync", recovered.trace.some((entry) =>
    entry.kind === "resource" && entry.provider === "new" && entry.path === "states/metars/new-1.json" && entry.status === 200) &&
    !recovered.trace.some((entry) => entry.kind === "resource" && entry.path.startsWith("deltas/")), JSON.stringify(recovered));
  await control("next");
  const nextText = await weather(CUTOVER_MARKERS.next, 3);
  runtime.check("cutover.next-update", nextText.includes(CUTOVER_MARKERS.next), nextText.slice(0, 400));
  const afterNext = await mapState();
  runtime.check("cutover.map-state-preserved", JSON.stringify(before) === JSON.stringify(after) &&
    JSON.stringify(before) === JSON.stringify(afterNext), JSON.stringify({ before, after, afterNext }));
  await runtime.openPage("flight_plan");
  const planAfter = await planSnapshot();
  runtime.check("cutover.plan-preserved", JSON.stringify(planBefore) === JSON.stringify(planAfter), JSON.stringify({ planBefore, planAfter }));
  await status();
}
