// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { launchCloudJourneyPeer } from "./cloud-journey-peer.mjs";

function idOf(entries) {
  return entries?.[0]?.id ?? entries?.[0] ?? null;
}

function rasterCounts(entries) {
  const id = idOf(entries);
  const match = /planned:(\d+):loaded:(\d+):failed:(\d+)/.exec(id ?? "");
  return match ? { planned: Number(match[1]), loaded: Number(match[2]), failed: Number(match[3]) } : null;
}

export function rasterPlanIsDisplayReady(counts, minimumLoadedRatio = 0.85) {
  if (!counts || counts.planned <= 0) return false;
  return counts.loaded / counts.planned >= minimumLoadedRatio ||
    counts.loaded + counts.failed >= counts.planned;
}

function playbackState(entries) {
  const id = idOf(entries);
  const match = /status:([^:]+):cursor:([0-9.]+):duration:([0-9.]+):rate:([0-9.]+):gaps:(\d+)/.exec(id ?? "");
  return match ? {
    status: match[1],
    cursor: Number(match[2]),
    duration: Number(match[3]),
    rate: Number(match[4]),
    gaps: Number(match[5]),
  } : null;
}

function ownshipState(entries) {
  const id = idOf(entries);
  const match = /mode:([^:]+):draw:(true|false):position:([^:]+):track:([^:]+)/.exec(id ?? "");
  return match ? {
    mode: match[1],
    draw: match[2] === "true",
    position: match[3],
    track: match[4],
  } : null;
}

function liveOverlayState(entries) {
  const id = idOf(entries);
  const match = /metars:(\d+):pireps:(\d+):obstacles:(\d+):tfrs:(\d+)/.exec(id ?? "");
  return match ? {
    metars: Number(match[1]),
    pireps: Number(match[2]),
    obstacles: Number(match[3]),
    tfrs: Number(match[4]),
  } : null;
}

function nexradState(entries) {
  const id = idOf(entries);
  const match = /tiles:(\d+):frame:([^:]+):frames:(\d+)/.exec(id ?? "");
  return match ? {
    tiles: Number(match[1]),
    frame: match[2] === "none" ? null : Number(match[2]),
    frames: Number(match[3]),
  } : null;
}

async function waitForPage(runtime, pageId) {
  return runtime.eventually(
    `${pageId} page`,
    () => runtime.driver.readElement(`page:${pageId}`),
  );
}

async function acceptDisclaimer(runtime, { required = false } = {}) {
  let accepted = false;
  let unobstructedMapSamples = 0;
  await runtime.eventually("disclaimer or unobstructed map", async () => {
    const accept = await runtime.driver.readElement("disclaimer-accept-button");
    if (accept) {
      await runtime.step("disclaimer.accept", () => runtime.driver.performAction("disclaimer-accept-button"));
      accepted = true;
      unobstructedMapSamples = 0;
      return false;
    }
    const appIsUnobstructed = Boolean(await runtime.driver.readElement(
      runtime.platform === "android" ? "primary-navigation" : "page:map",
    ));
    if (!appIsUnobstructed) {
      unobstructedMapSamples = 0;
      return false;
    }
    // Core startup can paint the chart before the persisted disclaimer setting
    // has been read. Require several clean samples so that a late modal cannot
    // race the journey's first action.
    unobstructedMapSamples += 1;
    return unobstructedMapSamples >= 5;
  }, 60_000);
  if (required && !accepted) {
    throw new Error("fresh profile reached the map without presenting the disclaimer");
  }
  return accepted;
}

async function loadedMap(runtime) {
  const raster = await runtime.eventually("loaded raster plan", async () => {
    const counts = rasterCounts(await runtime.driver.readProjection("parity:raster-state:"));
    return rasterPlanIsDisplayReady(counts) ? counts : null;
  }, 60_000);
  const vectors = await runtime.eventually("vector overlay", async () => {
    const id = idOf(await runtime.driver.readProjection("parity:vector-state:"));
    const count = Number(/features:(\d+)/.exec(id ?? "")?.[1] ?? 0);
    return count > 0 ? count : null;
  }, 60_000);
  return { raster, vectors };
}

export async function selectChartSearchSuggestion(runtime, ident) {
  const suggestionProjection = `chart-search-suggestion-${ident}`;
  const selectedProjection = `parity:map-selection-selected:${ident}`;
  return runtime.eventually(`${ident} chart search selection`, async () => {
    const selected = await runtime.driver.readProjection(selectedProjection);
    if (selected.length > 0) return selected[0];
    const suggestions = await runtime.driver.readProjection(suggestionProjection);
    if (suggestions.length === 0) return null;
    await runtime.driver.performAction(suggestionProjection);
    return null;
  }, 45_000);
}

async function enableDeterministicOwnship(runtime) {
  await runtime.driver.openPage("settings");
  const section = await runtime.driver.readElement("settings-section-debug_diagnostics");
  if (section?.expanded !== "true") {
    await runtime.driver.performAction("settings-section-debug_diagnostics");
  }
  await runtime.eventually(
    "Bad Autopilot debug toggle",
    () => runtime.driver.readElement("settings-toggle-debug_bad_autopilot"),
  );
  const toggle = await runtime.driver.readElement("settings-toggle-debug_bad_autopilot");
  if (toggle?.pressed !== "true" && toggle?.selected !== true && toggle?.checked !== true) {
    await runtime.driver.performAction("settings-toggle-debug_bad_autopilot");
  }
  await runtime.driver.openPage("map");
  await runtime.driver.chooseOption("ownship-source-button", "__bad_autopilot__");
  await runtime.eventually("enabled CTR", async () => {
    const value = await runtime.driver.readElement("center-here-button");
    return value?.enabled ? value : null;
  });
}

async function startupNavigation(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  // Android's clean-device package bootstrap must accept the disclaimer before
  // it can install the fixture publication. That bootstrap is release-gated;
  // this journey verifies the resulting persisted state on Android and owns
  // first acceptance itself on web.
  await acceptDisclaimer(runtime, { required: runtime.platform === "web" });
  await runtime.driver.openPage("map");
  const firstMap = await loadedMap(runtime);
  runtime.check("startup.supported-publication", firstMap.raster.failed === 0, JSON.stringify(firstMap));

  await runtime.step("app.reload", () => runtime.driver.reload());
  await runtime.eventually("map after reload", () => runtime.driver.readElement("page:map"), 60_000);
  runtime.check(
    "disclaimer.accept-persist",
    !(await runtime.driver.readElement("disclaimer-accept-button")),
  );

  await runtime.step("navigation.home", () => runtime.driver.openPage("home"));
  runtime.check("navigation.home", Boolean(await waitForPage(runtime, "home")));

  const destinations = [
    ["map", "chart", "navigation.map", "home.chart"],
    ["charts", "plate", "navigation.charts", "home.plate"],
    ["flight_plan", "flight_plan", "navigation.flight-plan", "home.flight-plan"],
    ["altitude_planner", "altitude_planner", "navigation.altitude-planner", "home.altitude-planner"],
    ["data_status", "data_status", "navigation.data-status", "home.data-status"],
    ["settings", "settings", "navigation.settings", "home.settings"],
    ["cloud", "cloud", null, "home.cloud"],
  ];
  for (const [pageId, destinationId, navigationAssertion, homeAssertion] of destinations) {
    await runtime.step(`navigation.open.${pageId}`, () => runtime.driver.openPage(pageId));
    const reached = Boolean(await waitForPage(runtime, pageId === "charts" ? "plate" : pageId));
    if (navigationAssertion) runtime.check(navigationAssertion, reached);
    runtime.check(homeAssertion, reached, destinationId);
    await runtime.driver.openPage("home");
  }

  const aboutId = runtime.platform === "android" ? "home-button:About" : "home-button-about";
  const aboutButton = await runtime.driver.readElement(aboutId);
  runtime.check("home.about", Boolean(aboutButton?.enabled), "About destination is enabled");
}

async function chartBasicUse(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("map");
  await loadedMap(runtime);

  const initialCtr = await runtime.driver.readElement("center-here-button");
  if (initialCtr?.pressed === "true" || initialCtr?.selected === true || initialCtr?.checked === true) {
    await runtime.driver.performAction("center-here-button");
    await runtime.eventually("CTR disabled before free pan", async () => {
      const value = await runtime.driver.readElement("center-here-button");
      return value && value.pressed !== "true" && value.selected !== true && value.checked !== true
        ? value
        : null;
    });
  }

  await runtime.step("chart.search.KSEA", () => runtime.driver.enterText("chart-search-input", "KSEA"));
  const selected = await selectChartSearchSuggestion(runtime, "KSEA");
  runtime.check("chart.search", selected);
  runtime.check("chart.inspect", Boolean(await runtime.driver.readElement("map-selection-tray")));
  await runtime.driver.back();
  const initialViewport = idOf(await runtime.driver.readProjection("parity:viewport:"));
  runtime.result.diagnostics.chart_pan_initial_viewport = initialViewport;

  const dragProbe = await runtime.step(
    "chart.pan",
    () => runtime.driver.drag("map-surface", { x: -360, y: 240 }),
  );
  if (dragProbe) runtime.result.diagnostics.chart_pan_gesture = dragProbe;
  const pannedViewport = await runtime.eventually("panned viewport", async () => {
    const current = idOf(await runtime.driver.readProjection("parity:viewport:"));
    runtime.result.diagnostics.chart_pan_last_viewport = current;
    return current && current !== initialViewport ? current : null;
  });
  runtime.check("chart.pan", pannedViewport !== initialViewport, `${initialViewport} -> ${pannedViewport}`);
  const panned = await loadedMap(runtime);
  runtime.check(
    "chart.raster-repaint",
    panned.raster.loaded > 0 && panned.raster.loaded / panned.raster.planned >= 0.85,
    JSON.stringify(panned.raster),
  );
  runtime.check("chart.vector-repaint", panned.vectors > 0, String(panned.vectors));

  await runtime.step("chart.zoom", () => runtime.driver.zoom("map-surface", -420));
  const zoomedViewport = await runtime.eventually("zoomed viewport", async () => {
    const current = idOf(await runtime.driver.readProjection("parity:viewport:"));
    return current && current !== pannedViewport ? current : null;
  });
  runtime.check("chart.zoom", zoomedViewport !== pannedViewport, `${pannedViewport} -> ${zoomedViewport}`);

  if (runtime.platform === "android") {
    // Android's keyboard zoom shortcut is also delivered to a search field
    // that retained focus after its suggestion was selected.
    await runtime.driver.back();
  }

  await runtime.driver.openPage("flight_plan");
  await appendRoute(runtime, "KSEA KPAE");
  await planAction(runtime, "KPAE", "activate_leg");
  await enableDeterministicOwnship(runtime);
  await runtime.step("chart.ctr.enable", () => runtime.driver.performAction("center-here-button"));
  const following = await runtime.eventually("CTR enabled", async () => {
    const value = await runtime.driver.readElement("center-here-button");
    return value?.pressed === "true" || value?.selected === true || value?.checked === true ? value : null;
  });
  runtime.check("chart.ctr-on", Boolean(following));
  await runtime.driver.performAction("center-here-button");
  const free = await runtime.eventually("CTR disabled", async () => {
    const value = await runtime.driver.readElement("center-here-button");
    return value && value.pressed !== "true" && value.selected !== true && value.checked !== true ? value : null;
  });
  runtime.check("chart.ctr-off", Boolean(free));
}

function projectionId(entry) {
  return entry?.id ?? entry ?? "";
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function setFixtureControl(runtime, update) {
  if (!runtime.fixtureOrigin) throw new Error("journey fixture origin is unavailable");
  return fetchFixtureJson(runtime, "/__control", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(update),
  });
}

async function fixtureHealth(runtime) {
  if (!runtime.fixtureOrigin) throw new Error("journey fixture origin is unavailable");
  return fetchFixtureJson(runtime, "/__health");
}

async function fetchFixtureJson(runtime, path, options = {}) {
  let lastError = null;
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try {
      const response = await fetch(new URL(path, runtime.fixtureOrigin), options);
      if (!response.ok) throw new Error(`${path} failed: HTTP ${response.status}`);
      return await response.json();
    } catch (error) {
      lastError = error;
      if (attempt < 4) await delay(100 * (attempt + 1));
    }
  }
  throw lastError;
}

function planRowUid(entry) {
  return projectionId(entry)
    .replace(/^parity:plan-row:/, "")
    .replace(/^plan-row-/, "");
}

async function planRows(runtime) {
  return runtime.driver.readProjection("parity:plan-row:");
}

async function findPlanRow(runtime, label) {
  return runtime.eventually(`flight-plan row ${label}`, async () => {
    const entry = await runtime.driver.findProjectionMatching("parity:plan-row:", label);
    return entry?.text?.split(/\s+/).includes(label) ? entry : null;
  }, 45_000);
}

async function appendRoute(runtime, route) {
  await runtime.driver.enterText("plan-append-route-input", route);
  // Route parsing runs through the latency-sensitive core worker. Enter only
  // after its preview has had an opportunity to make the form committable.
  await delay(750);
  await runtime.driver.enterText("plan-append-route-input", route, { submit: true });
  await runtime.driver.performAction("dismiss-plan-row-tray");
  const destination = route.trim().split(/\s+/).at(-1);
  await findPlanRow(runtime, destination);
}

async function openPlanRow(runtime, label) {
  const row = await findPlanRow(runtime, label);
  await runtime.driver.performAction(`plan-row:${planRowUid(row)}`);
  return row;
}

async function findProcedureRow(runtime, procedureId) {
  return runtime.eventually(`flight-plan procedure ${procedureId}`, async () => {
    const entries = await runtime.driver.readProjection(`parity:plan-procedure-row:${procedureId}:uid:`);
    return entries[0] ?? null;
  }, 45_000);
}

function procedureRowUid(entry) {
  const marker = ":uid:";
  const id = projectionId(entry);
  const offset = id.indexOf(marker);
  if (offset < 0) throw new Error(`invalid procedure row probe ${id}`);
  return id.slice(offset + marker.length);
}

async function openProcedureRow(runtime, procedureId) {
  const row = await findProcedureRow(runtime, procedureId);
  await runtime.driver.performAction(`plan-row:${procedureRowUid(row)}`);
  return row;
}

async function planAction(runtime, label, actionId) {
  await openPlanRow(runtime, label);
  await runtime.eventually(`${actionId} action for ${label}`, async () => {
    const id = runtime.platform === "web"
      ? `plan-row-action-${actionId}`
      : `plan-row-action:${actionId}`;
    const action = await runtime.driver.readElement(id);
    return action?.enabled ? action : null;
  });
  await runtime.driver.performAction(actionId);
  if ([
    "activate_leg", "direct_to", "move_up", "move_down", "remove", "remove_all_above",
  ].includes(actionId)) {
    await runtime.driver.performAction("dismiss-plan-row-tray");
  }
}

async function planState(runtime) {
  return projectionId((await runtime.driver.readProjection("parity:plan-state:"))[0]);
}

async function enabledPlanControl(runtime, controlId) {
  const id = runtime.platform === "web" ? `plan-control-${controlId}` : `plan-control:${controlId}`;
  return runtime.eventually(`enabled ${controlId} control`, async () => {
    const control = await runtime.driver.readElement(id);
    return control?.enabled ? control : null;
  });
}

async function planControl(runtime, controlId) {
  return runtime.driver.readElement(
    runtime.platform === "web" ? `plan-control-${controlId}` : `plan-control:${controlId}`,
  );
}

async function flightPlanEditAndNavigate(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("flight_plan");

  await runtime.driver.enterText("plan-append-route-input", "KRNT V2 ZZZZZ");
  const invalidFeedback = await runtime.eventually("invalid route feedback", async () => {
    const feedback = await runtime.driver.readElement("plan-append-route-feedback");
    return feedback?.text && !/^Checking/i.test(feedback.text) ? feedback : null;
  }, 45_000);
  runtime.check("plan.route-invalid", Boolean(invalidFeedback?.text), invalidFeedback?.text);

  await appendRoute(runtime, "KSEA KBFI KRNT KPAE");
  runtime.check("plan.route-valid", (await planRows(runtime)).length >= 4);

  let beforeCount = (await planRows(runtime)).length;
  await planAction(runtime, "KRNT", "insert_before");
  await runtime.eventually("insert-before airport input", () => runtime.driver.readElement("plan-insert-airport-input"));
  await runtime.driver.enterText("plan-insert-airport-input", "KPLU");
  await runtime.eventually("KPLU insert suggestion", () => runtime.driver.readElement(
    runtime.platform === "web" ? "plan-insert-suggestion-KPLU" : "plan-insert-suggestion:KPLU",
  ));
  await runtime.driver.performAction("plan-insert-suggestion:KPLU");
  await findPlanRow(runtime, "KPLU");
  runtime.check("plan.insert-before", (await planRows(runtime)).length > beforeCount);

  beforeCount = (await planRows(runtime)).length;
  await planAction(runtime, "KRNT", "insert_after");
  await runtime.eventually("insert-after airport input", () => runtime.driver.readElement("plan-insert-airport-input"));
  await runtime.driver.enterText("plan-insert-airport-input", "S50");
  await runtime.eventually("S50 insert suggestion", () => runtime.driver.readElement(
    runtime.platform === "web" ? "plan-insert-suggestion-S50" : "plan-insert-suggestion:S50",
  ));
  await runtime.driver.performAction("plan-insert-suggestion:S50");
  await findPlanRow(runtime, "S50");
  runtime.check("plan.insert-after", (await planRows(runtime)).length > beforeCount);

  await planAction(runtime, "S50", "move_up");
  const movedUp = await runtime.eventually("S50 moved above KRNT", async () => {
    const labels = (await planRows(runtime)).map((entry) => entry.text);
    return labels.findIndex((text) => text.includes("S50")) < labels.findIndex((text) => text.includes("KRNT"))
      ? labels
      : null;
  });
  runtime.check("plan.move-up", movedUp.findIndex((text) => text.includes("S50")) < movedUp.findIndex((text) => text.includes("KRNT")));
  await planAction(runtime, "S50", "move_down");
  const movedDown = await runtime.eventually("S50 moved below KRNT", async () => {
    const labels = (await planRows(runtime)).map((entry) => entry.text);
    return labels.findIndex((text) => text.includes("S50")) > labels.findIndex((text) => text.includes("KRNT"))
      ? labels
      : null;
  });
  runtime.check("plan.move-down", movedDown.findIndex((text) => text.includes("S50")) > movedDown.findIndex((text) => text.includes("KRNT")));

  beforeCount = (await planRows(runtime)).length;
  await planAction(runtime, "S50", "remove");
  await runtime.eventually("S50 removed", async () => !(await planRows(runtime)).some((entry) => entry.text.includes("S50")));
  runtime.check("plan.remove", (await planRows(runtime)).length < beforeCount);

  beforeCount = (await planRows(runtime)).length;
  await planAction(runtime, "KPLU", "remove_all_above");
  await runtime.eventually("route trimmed above KPLU", async () => (await planRows(runtime)).length < beforeCount);
  runtime.check("plan.remove-all-above", (await planRows(runtime)).length < beforeCount);

  // Preserve two downstream legs so both manual sequencing controls can be
  // exercised independently after activating the KPAE leg.
  await appendRoute(runtime, "S88 KPLU");

  await runtime.driver.openPage("flight_plan");

  await planAction(runtime, "KPAE", "activate_leg");
  const activated = await enabledPlanControl(runtime, "stop_navigation");
  runtime.check("plan.activate-leg", Boolean(activated), activated?.text);

  // Bad Autopilot intentionally becomes selectable only after core has active
  // leg geometry to fly.
  await enableDeterministicOwnship(runtime);
  await runtime.driver.openPage("flight_plan");

  await planAction(runtime, "KPLU", "direct_to");
  const directState = await enabledPlanControl(runtime, "stop_navigation");
  runtime.check("plan.direct-to", Boolean(directState));
  await runtime.driver.performAction("stop_navigation");
  const stopped = await runtime.eventually("navigation stopped", async () => {
    const control = await planControl(runtime, "stop_navigation");
    return control && !control.enabled ? control : null;
  });
  runtime.check("plan.stop-navigation", Boolean(stopped));

  await planAction(runtime, "KPAE", "activate_leg");

  await enabledPlanControl(runtime, "activate_next_leg");
  await runtime.driver.performAction("activate_next_leg");
  const activeAfterNext = await enabledPlanControl(runtime, "stop_navigation");
  runtime.check("plan.activate-next-leg", Boolean(activeAfterNext));

  await enabledPlanControl(runtime, "suspend_sequencing");
  await runtime.driver.performAction("suspend_sequencing");
  await enabledPlanControl(runtime, "unsuspend_sequencing");
  runtime.check("plan.suspend-sequencing", true);
  await runtime.driver.performAction("unsuspend_sequencing");
  await enabledPlanControl(runtime, "suspend_sequencing");
  runtime.check("plan.unsuspend-sequencing", true);

  await enabledPlanControl(runtime, "sequence_active_leg");
  await runtime.driver.performAction("sequence_active_leg");
  const stateAfterSequence = await enabledPlanControl(runtime, "stop_navigation");
  runtime.check("plan.sequence-active-leg", Boolean(stateAfterSequence));

  await runtime.driver.openPage("map");
  await runtime.driver.enterText("chart-search-input", "S50");
  await selectChartSearchSuggestion(runtime, "S50");
  await runtime.eventually("S50 Direct action", () => runtime.driver.readElement(
    runtime.platform === "web" ? "map-selection-action-direct_to" : "map-selection-action:direct_to",
  ));
  await runtime.driver.performAction("direct_to");
  await runtime.driver.openPage("flight_plan");
  await enabledPlanControl(runtime, "restore_direct_to");
  await runtime.driver.performAction("restore_direct_to");
  const restored = await runtime.eventually("restored underlying plan", async () => {
    const control = await planControl(runtime, "restore_direct_to");
    return control && !control.enabled ? control : null;
  });
  runtime.check("plan.restore-direct-to", Boolean(restored));

  await runtime.driver.openPage("map");
  const routePaint = await runtime.eventually("flight-plan route overlay", async () => {
    const ids = await runtime.driver.readProjection("parity:flight-plan-route-overlay:");
    return ids.length > 0 ? projectionId(ids[0]) : null;
  });
  runtime.check("plan.route-paint", Boolean(routePaint), routePaint);
}

function trayOptionId(entry) {
  return projectionId(entry)
    .replace(/^parity:tray-option:/, "")
    .replace(/^tray-option-/, "");
}

async function selectTrayOptionMatching(runtime, launcherId, needle, { waitForSelection = true } = {}) {
  await runtime.driver.performAction(launcherId);
  const entry = await runtime.eventually(`${launcherId} option matching ${needle}`, async () => {
    return runtime.driver.findProjectionMatching(
      runtime.platform === "web" ? "tray-option-" : "parity:tray-option:",
      needle,
    );
  }, 45_000);
  await runtime.driver.performAction(`tray-option:${trayOptionId(entry)}`);
  if (waitForSelection) {
    await runtime.eventually(`${launcherId} selected ${needle}`, async () => {
      const launcher = await runtime.driver.readElement(launcherId);
      return launcher?.text?.toUpperCase().includes(needle.toUpperCase()) ? launcher : null;
    }, 45_000);
  }
  return entry;
}

function procedureChoiceId(entry) {
  return projectionId(entry)
    .replace(/^parity:plan-procedure:/, "")
    .replace(/^plan-procedure-/, "");
}

function procedureTransitionId(entry) {
  return projectionId(entry)
    .replace(/^parity:plan-procedure-transition:/, "")
    .replace(/^plan-procedure-transition-/, "");
}

async function selectProcedure(runtime, {
  airportId, rowLabel = airportId, actionId, procedureId, transition = null,
}) {
  await planAction(runtime, rowLabel, actionId);
  const procedure = await runtime.eventually(`${procedureId} procedure choice`, async () => {
    const entries = await runtime.driver.readProjection("parity:plan-procedure:");
    return entries.find((entry) => procedureChoiceId(entry) === procedureId) ?? null;
  }, 45_000);
  await runtime.driver.performAction(`plan-procedure:${procedureId}`);
  const choice = await runtime.eventually(`${procedureId} transition choice`, async () => {
    const entries = await runtime.driver.readProjection("parity:plan-procedure-transition:");
    if (transition) {
      return entries.find((entry) =>
        procedureTransitionId(entry) === transition || entry.text?.includes(transition)) ?? null;
    }
    return entries.find((entry) => entry.enabled !== false) ?? null;
  }, 45_000);
  await runtime.driver.performAction(`plan-procedure-transition:${procedureTransitionId(choice)}`);
  return findProcedureRow(runtime, procedureId);
}

async function assertProcedurePainted(runtime, assertionId) {
  await runtime.driver.openPage("map");
  const projection = await runtime.eventually("procedure route painted", async () => {
    const entries = await runtime.driver.readProjection("parity:flight-plan-route-overlay:");
    return entries.length > 0 ? projectionId(entries[0]) : null;
  }, 45_000);
  runtime.check(assertionId, Boolean(projection), projection);
  await runtime.driver.openPage("flight_plan");
}

async function assertProcedureShowPlate(runtime, procedureId, assertionId, expectedLabel = procedureId) {
  await openProcedureRow(runtime, procedureId);
  await runtime.eventually(`show_plate action for ${procedureId}`, async () => {
    const action = await runtime.driver.readElement(runtime.platform === "web"
      ? "plan-row-action-show_plate"
      : "plan-row-action:show_plate");
    return action?.enabled ? action : null;
  });
  await runtime.driver.performAction("show_plate");
  const plate = await waitForPage(runtime, "plate");
  const chart = await runtime.eventually("selected procedure plate", async () => {
    const selected = await runtime.driver.readElement("plate-chart-button");
    return selected?.text?.toUpperCase().includes(expectedLabel.toUpperCase()) ? selected : null;
  }, 45_000);
  runtime.check(assertionId, Boolean(plate && chart?.text), chart?.text);
}

async function removeProcedure(runtime, procedureId, assertionId) {
  await runtime.driver.openPage("flight_plan");
  await openProcedureRow(runtime, procedureId);
  await runtime.eventually(`remove_procedure action for ${procedureId}`, async () => {
    const action = await runtime.driver.readElement(runtime.platform === "web"
      ? "plan-row-action-remove_procedure"
      : "plan-row-action:remove_procedure");
    return action?.enabled ? action : null;
  });
  await runtime.driver.performAction("remove_procedure");
  const removed = await runtime.eventually(`${procedureId} removed`, async () =>
    (await runtime.driver.readProjection(`parity:plan-procedure-row:${procedureId}:uid:`)).length === 0);
  runtime.check(assertionId, removed);
}

async function procedureDeparture(runtime) {
  const sid = runtime.capability("procedure.sid");
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("flight_plan");
  await appendRoute(runtime, `${sid.airport_id} KPAE`);
  await selectProcedure(runtime, {
    airportId: sid.airport_id,
    actionId: "select_departure",
    procedureId: sid.procedure_id,
  });
  runtime.check("procedure.sid.select", true, `${sid.airport_id} ${sid.procedure_id}`);
  await assertProcedurePainted(runtime, "procedure.sid.render");

  await openPlanRow(runtime, sid.airport_id);
  const moveDown = await runtime.driver.readElement(runtime.platform === "web"
    ? "plan-row-action-move_down"
    : "plan-row-action:move_down");
  runtime.check("procedure.sid.invariant", Boolean(moveDown && !moveDown.enabled), moveDown?.text);
  await runtime.driver.performAction("dismiss-plan-row-tray");

  await assertProcedureShowPlate(runtime, sid.procedure_id, "procedure.sid.show-plate", "BANGR");
  await removeProcedure(runtime, sid.procedure_id, "procedure.sid.remove");
}

async function procedureArrival(runtime) {
  const star = runtime.capability("procedure.star");
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("flight_plan");
  await appendRoute(runtime, `KYKM ${star.airport_id}`);
  await selectProcedure(runtime, {
    airportId: star.airport_id,
    actionId: "select_arrival",
    procedureId: star.procedure_id,
    transition: star.transition,
  });
  runtime.check("procedure.star.select", true, `${star.airport_id} ${star.procedure_id}`);
  await assertProcedurePainted(runtime, "procedure.star.render");

  await openPlanRow(runtime, star.airport_id);
  const moveUp = await runtime.driver.readElement(runtime.platform === "web"
    ? "plan-row-action-move_up"
    : "plan-row-action:move_up");
  runtime.check("procedure.star.invariant", Boolean(moveUp && !moveUp.enabled), moveUp?.text);
  await runtime.driver.performAction("dismiss-plan-row-tray");

  await assertProcedureShowPlate(runtime, star.procedure_id, "procedure.star.show-plate", "CHINS");
  const selected = await runtime.driver.readElement("plate-chart-button");
  runtime.check("plate.multi-page-rotated", selected?.text?.toUpperCase().includes("CHINS"), selected?.text);
  await removeProcedure(runtime, star.procedure_id, "procedure.star.remove");
}

async function procedureApproach(runtime) {
  const approach = runtime.capability("procedure.approach");
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("flight_plan");
  await appendRoute(runtime, `KSEA ${approach.airport_id}`);
  await selectProcedure(runtime, {
    airportId: approach.airport_id,
    actionId: "select_approach",
    procedureId: approach.procedure_id,
    transition: approach.transition,
  });
  runtime.check("procedure.approach.select", true, `${approach.airport_id} ${approach.procedure_id}`);
  await assertProcedurePainted(runtime, "procedure.approach.render");
  await assertProcedureShowPlate(runtime, approach.procedure_id, "procedure.approach.show-plate", "32R");

  await runtime.driver.openPage("flight_plan");
  await selectProcedure(runtime, {
    airportId: approach.airport_id,
    actionId: "select_approach",
    procedureId: approach.procedure_id,
    transition: approach.transition,
  });
  const approachRows = await runtime.driver.readProjection(`parity:plan-procedure-row:${approach.procedure_id}:uid:`);
  runtime.check("procedure.approach.replace", approachRows.length === 1, `${approachRows.length} procedure rows`);
  await removeProcedure(runtime, approach.procedure_id, "procedure.approach.remove");

  await runtime.driver.openPage("charts");
  await selectTrayOptionMatching(runtime, "plate-airport-button", approach.airport_id);
  await selectTrayOptionMatching(runtime, "plate-chart-button", "ILS OR LOC 32R");
  await runtime.eventually("enabled plate load button", async () => {
    const button = await runtime.driver.readElement("plate-load-button");
    return button?.enabled ? button : null;
  }, 45_000);
  await runtime.driver.performAction("plate-load-button");
  const load = await runtime.eventually("plate load option", async () => {
    const entries = await runtime.driver.readProjection(runtime.platform === "web" ? "tray-option-" : "parity:tray-option:");
    return entries.find((entry) => entry.enabled !== false) ?? null;
  });
  await runtime.driver.performAction(`tray-option:${trayOptionId(load)}`);
  await runtime.driver.openPage("flight_plan");
  await findProcedureRow(runtime, approach.procedure_id);
  runtime.check("procedure.approach.load-from-plate", true, load.text);
}

async function plateViewport(runtime) {
  return projectionId((await runtime.driver.readProjection("parity:plate-viewport:"))[0]);
}

async function selectPlateFolderTileMatching(runtime, needle) {
  const entry = await runtime.eventually(`plate folder tile matching ${needle}`, async () => {
    const entries = await runtime.driver.readProjection(runtime.platform === "web"
      ? "plate-folder-tile:"
      : "parity:plate-folder-tile:");
    return entries.find((option) => option.text?.toUpperCase().includes(needle.toUpperCase())) ?? null;
  }, 45_000);
  const chartId = projectionId(entry)
    .replace(/^parity:plate-folder-tile:/, "")
    .replace(/^plate-folder-tile:/, "");
  await runtime.driver.performAction(`plate-folder-tile:${chartId}`);
  await runtime.eventually(`plate selected ${needle}`, async () => {
    const launcher = await runtime.driver.readElement("plate-chart-button");
    return launcher?.text?.toUpperCase().includes(needle.toUpperCase()) ? launcher : null;
  }, 45_000);
  return entry;
}

async function plateOperate(runtime) {
  const georef = runtime.capability("plate.georeferenced");
  const multiPage = runtime.capability("plate.multi_page_rotated");
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("flight_plan");
  // RARYO is the on-plate IAF for this approach, putting deterministic
  // ownship well inside the georeferenced image bounds immediately.
  await appendRoute(runtime, `RARYO ${georef.airport_id}`);
  await planAction(runtime, georef.airport_id, "activate_leg");

  await runtime.driver.openPage("charts");
  const airport = await selectTrayOptionMatching(runtime, "plate-airport-button", georef.airport_id);
  runtime.check("plate.airport-selector", Boolean(airport), airport.text);
  const georefDisplayLabel = georef.label_contains
    .replace(/\s*\(GPS\)\s*/i, " ")
    .replace(/\bRWY\s+/i, "")
    .replace(/\s+/g, " ")
    .trim();
  const chart = await selectTrayOptionMatching(runtime, "plate-chart-button", georefDisplayLabel);
  runtime.check("plate.chart-selector", Boolean(chart), chart.text);
  const chartId = trayOptionId(chart);

  const selected = await runtime.eventually("named georeferenced plate selected", async () => {
    const control = await runtime.driver.readElement("plate-chart-button");
    return control?.text?.toUpperCase().includes(georefDisplayLabel.toUpperCase()) ? control : null;
  });
  runtime.check("plate.named-selection", Boolean(selected), selected.text);

  await runtime.driver.performAction("plate-folder-button");
  const folderTile = await runtime.eventually("selected plate in folder", async () => {
    const entries = await runtime.driver.readProjection(runtime.platform === "web"
      ? "plate-folder-tile:"
      : "parity:plate-folder-tile:");
    return entries.find((entry) => projectionId(entry).endsWith(chartId)) ?? null;
  });
  runtime.check("plate.folder", Boolean(folderTile), projectionId(folderTile));
  await runtime.driver.performAction(`plate-folder-tile:${chartId}`);

  // Bad Autopilot is intentionally accelerated. Start it only after the plate
  // is selected so its first position remains inside the georeferenced image.
  await enableDeterministicOwnship(runtime);
  await runtime.driver.openPage("charts");

  await delay(1000);
  runtime.result.diagnostics.plate_ownship_input = (await runtime.driver.readProjection(
    "parity:plate-ownship-input:",
  )).map(projectionId);
  const ownship = await runtime.eventually("ownship on georeferenced plate", async () => {
    const entries = await runtime.driver.readProjection(runtime.platform === "web"
      ? "plate-ownship-overlay"
      : "parity:plate-ownship-overlay");
    return entries[0] ?? null;
  }, 5_000);
  runtime.check("plate.georeferenced-ownship", Boolean(ownship));

  const initialViewport = await runtime.eventually("initial plate viewport", () => plateViewport(runtime));
  await runtime.driver.drag("plate-surface", { x: -120, y: -100 });
  const pannedViewport = await runtime.eventually("panned plate viewport", async () => {
    const value = await plateViewport(runtime);
    return value && value !== initialViewport ? value : null;
  });
  runtime.check("plate.pan", Boolean(pannedViewport), `${initialViewport} -> ${pannedViewport}`);
  await runtime.driver.zoom("plate-surface", -360);
  const zoomedViewport = await runtime.eventually("zoomed plate viewport", async () => {
    const value = await plateViewport(runtime);
    return value && value !== pannedViewport ? value : null;
  });
  runtime.check("plate.zoom", Boolean(zoomedViewport), `${pannedViewport} -> ${zoomedViewport}`);

  const loadButton = await runtime.eventually("enabled plate procedure load", async () => {
    const button = await runtime.driver.readElement("plate-load-button");
    return button?.enabled ? button : null;
  }, 45_000);
  await runtime.driver.performAction("plate-load-button");
  const loadOption = await runtime.eventually("plate procedure load option", async () => {
    const entries = await runtime.driver.readProjection(runtime.platform === "web" ? "tray-option-" : "parity:tray-option:");
    return entries.find((entry) => entry.enabled !== false) ?? null;
  });
  await runtime.driver.performAction(`tray-option:${trayOptionId(loadOption)}`);
  await runtime.eventually("plate procedure load tray closed", async () => {
    const options = await runtime.driver.readProjection(
      runtime.platform === "web" ? "tray-option-" : "parity:tray-option:",
    );
    return options.length === 0;
  });
  runtime.check("plate.load-procedure", Boolean(loadButton && loadOption), loadOption.text);

  if (multiPage.airport_id !== georef.airport_id) {
    await selectTrayOptionMatching(runtime, "plate-airport-button", multiPage.airport_id);
  }
  const multi = await selectTrayOptionMatching(runtime, "plate-chart-button", multiPage.label_contains);
  const firstPageViewport = await runtime.eventually("multi-page plate initial viewport", () => plateViewport(runtime));
  await runtime.driver.drag("plate-surface", { x: 0, y: -600 });
  const lastPageViewport = await runtime.eventually("multi-page plate last viewport", async () => {
    const value = await plateViewport(runtime);
    return value && value !== firstPageViewport ? value : null;
  });
  runtime.check("plate.first-last-page", Boolean(lastPageViewport), `${firstPageViewport} -> ${lastPageViewport}`);

  await runtime.driver.performAction("plate-folder-button");
  const multiId = trayOptionId(multi);
  await runtime.eventually("multi-page plate folder tile", async () => {
    const entries = await runtime.driver.readProjection(runtime.platform === "web"
      ? "plate-folder-tile:"
      : "parity:plate-folder-tile:");
    return entries.find((entry) => projectionId(entry).endsWith(multiId)) ?? null;
  });
  await runtime.driver.performAction(`plate-folder-tile:${multiId}`);
  const returned = await runtime.eventually("plate returned from folder", async () => {
    const control = await runtime.driver.readElement("plate-chart-button");
    return control?.text?.toUpperCase().includes(multiPage.label_contains.toUpperCase()) ? control : null;
  });
  runtime.check("plate.return-folder", Boolean(returned), returned.text);
}

async function plateAdvisoriesAndReferences(runtime) {
  const notam = runtime.capability("plate.notam");
  const warning = runtime.capability("plate.geometry_warning");
  const legend = runtime.capability("plate.legend");
  const inset = runtime.capability("plate.inset");
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);

  await selectAirportFromMapSearch(runtime, warning.airport_id);
  await runtime.driver.performAction("plates");
  await waitForPage(runtime, "plate");
  await selectPlateFolderTileMatching(runtime, warning.label_contains);
  const warningLauncher = await runtime.eventually("plate procedure geometry warning", () =>
    runtime.driver.readElement("procedure-status-launcher"), 45_000);
  await runtime.driver.performAction("procedure-status-launcher");
  const warningPanel = await runtime.eventually("plate procedure geometry detail", () =>
    runtime.driver.readElement("procedure-status-panel"));
  runtime.check(
    "plate.geometry-warning",
    Boolean(warningLauncher && warningPanel?.text?.includes("This publication")),
    warningPanel?.text,
  );
  await runtime.driver.back();

  if (notam.airport_id !== warning.airport_id || notam.label_contains !== warning.label_contains) {
    await selectAirportFromMapSearch(runtime, notam.airport_id);
    await runtime.driver.performAction("plates");
    await waitForPage(runtime, "plate");
    await selectPlateFolderTileMatching(runtime, notam.label_contains);
  }
  const notamBadge = await runtime.eventually("plate procedure NOTAM badge", async () => {
    const entries = await runtime.driver.readProjection(
      runtime.platform === "web" ? "plate-notam:" : "parity:plate-notam:",
    );
    return entries[0] ?? null;
  }, 60_000);
  await runtime.driver.performAction(projectionId(notamBadge));
  const notamModal = await runtime.eventually("plate NOTAM detail", () =>
    runtime.driver.readElement("procedure-notam-modal"));
  runtime.check(
    "plate.notam",
    Boolean(notamModal?.text && /NOTAM/i.test(notamModal.text)),
    notamModal?.text,
  );
  await runtime.driver.back();

  await runtime.driver.openPage("map");
  await runtime.driver.chooseOption("chart-family-button", legend.family_id);
  await runtime.eventually("TAC family selected for references", async () => {
    const entries = await runtime.driver.readProjection(`parity:map-family:${legend.family_id}:`);
    return entries[0] ?? null;
  }, 45_000);
  await selectAirportFromMapSearch(runtime, inset.map_airport_id ?? "KSEA");
  await runtime.driver.back();
  await runtime.eventually("contextual TAC raster plan", async () => {
    const [entry] = await runtime.driver.readProjection("parity:raster-state:");
    return (rasterCounts([entry])?.planned ?? 0) > 0 ? entry : null;
  }, 45_000);
  runtime.result.diagnostics.chart_reference_raster_state =
    await runtime.driver.readProjection("parity:raster-state:");
  runtime.result.diagnostics.chart_reference_controls =
    await runtime.driver.readProjection("tray-option-accessory-");
  await runtime.driver.performAction("chart-family-button");
  const accessoryAction = runtime.platform === "web"
    ? "tray-option-accessory-tac"
    : "chart-reference-button";
  await runtime.eventually("TAC reference accessory", () => runtime.driver.readElement(accessoryAction));
  await runtime.driver.performAction(accessoryAction);
  await waitForPage(runtime, "plate");

  const legendOption = await selectTrayOptionMatching(runtime, "plate-chart-button", legend.label_contains);
  runtime.check("plate.legend", Boolean(legendOption), legendOption.text);
  const legendViewport = await runtime.eventually("legend viewport", () => plateViewport(runtime));
  await runtime.driver.drag("plate-surface", { x: 0, y: -600 });
  const scrolledLegend = await runtime.eventually("scrolled legend composite", async () => {
    const value = await plateViewport(runtime);
    return value && value !== legendViewport ? value : null;
  });
  runtime.check("plate.composite-scroll", Boolean(scrolledLegend), `${legendViewport} -> ${scrolledLegend}`);

  const insetOption = await selectTrayOptionMatching(runtime, "plate-chart-button", inset.label_contains);
  runtime.check("plate.inset", Boolean(insetOption), insetOption.text);
}

async function otherDocuments(runtime) {
  const csup = runtime.capability("document.csup");
  const other = runtime.capability("document.other");
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);

  await selectAirportFromMapSearch(runtime, csup.airport_id);
  await runtime.driver.performAction("csup");
  await waitForPage(runtime, "plate");
  const csupSelection = await runtime.eventually("airport CSUP selected", async () => {
    const control = await runtime.driver.readElement("plate-chart-button");
    return /CSUP|CHART SUPPLEMENT/i.test(control?.text ?? "") ? control : null;
  }, 45_000);
  const csupViewport = await runtime.eventually("CSUP document painted", () => plateViewport(runtime), 45_000);
  runtime.check("plate.csup", Boolean(csupSelection && csupViewport), csupSelection?.text);

  if (other.airport_id !== csup.airport_id) {
    await selectTrayOptionMatching(runtime, "plate-airport-button", other.airport_id);
  }
  const otherSelection = await selectTrayOptionMatching(
    runtime,
    "plate-chart-button",
    other.label_contains,
  );
  const otherViewport = await runtime.eventually(
    "other airport document painted",
    () => plateViewport(runtime),
    45_000,
  );
  runtime.check(
    "plate.other-document",
    Boolean(otherSelection && otherViewport),
    otherSelection?.text,
  );
}

async function aboutAndSavedState(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("home");

  if (runtime.platform === "web") {
    await runtime.driver.openPage("about");
  } else {
    await runtime.driver.performAction("home-button:About");
  }
  const about = await runtime.eventually("About destination", () =>
    runtime.driver.readElement(runtime.platform === "web" ? "page:about" : "external-page:about"),
  );
  runtime.check("navigation.about", Boolean(about), about?.text);
  if (runtime.platform === "android") {
    await runtime.driver.back();
    await delay(500);
  }

  // Begin the persistence check from a clean operational app. Settings is not
  // the default page, so seeing it after a process/browser reload proves that
  // the user's last page survived rather than merely matching startup policy.
  await runtime.driver.reset();
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("settings");
  await runtime.eventually("Settings selected before restart", () =>
    runtime.driver.readElement("page:settings"));
  await delay(500);
  await runtime.driver.reload();
  const restored = await runtime.eventually("saved Settings page after restart", () =>
    runtime.driver.readElement("page:settings"), 60_000);
  runtime.check("saved-state.restart", Boolean(restored), "Settings page restored after restart");
}

async function pointerDetails(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("map");
  await loadedMap(runtime);
  const target = await runtime.eventually("visible METAR hover target", async () => {
    const entries = await runtime.driver.readProjection("parity:metar-hover-target:");
    return entries[0] ?? null;
  }, 60_000);
  await runtime.driver.hover(projectionId(target));
  const weather = await runtime.eventually("hover weather detail", () =>
    runtime.driver.readElement("weather-detail-modal"), 30_000);
  runtime.check(
    "web.metar-hover",
    Boolean(weather?.text && /METAR/i.test(weather.text)),
    weather?.text,
  );
  const copied = await runtime.driver.copyText("weather-detail-modal");
  runtime.check(
    "web.weather-copy",
    Boolean(copied.clipboard && copied.clipboard === copied.selected && /METAR/i.test(copied.clipboard)),
    copied.clipboard,
  );
}

async function contractFailures(runtime) {
  await setFixtureControl(runtime, { publication: "unsupported", artifact_fault: "none" });
  try {
    await runtime.step("app.reset-unsupported-contract", () =>
      runtime.driver.resetApplicationData());
    const failure = await runtime.eventually("unsupported publication failure", () =>
      runtime.driver.readElement(
        runtime.platform === "web" ? "startup-fatal-error" : "offline-library-panel",
      ), 60_000);
    runtime.check(
      "startup.unsupported-contract",
      Boolean(failure?.text && /unsupported|no manifest supported/i.test(failure.text)),
      failure?.text,
    );
  } finally {
    await setFixtureControl(runtime, { publication: "primary", artifact_fault: "none" });
  }
}

async function waitForOfflineSyncIdle(runtime, description, timeoutMs = 120_000) {
  return runtime.eventually(description, async () => {
    const button = await runtime.driver.readElement("offline-sync-button");
    return button?.enabled && !/SYNCING|CANCELING/i.test(button.text ?? "") ? button : null;
  }, timeoutMs, 500);
}

async function androidPackageMaintenance(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("settings");

  await runtime.eventually("display dim slider", () =>
    runtime.driver.readElement("parity:settings-slider:display_dim_timeout:2m"));
  await runtime.driver.drag("settings-slider:display_dim_timeout:2m", { x: -1_000, y: 0 });
  const dimmed = await runtime.eventually("display dim timeout changed", () =>
    runtime.driver.readElement("parity:settings-slider:display_dim_timeout:10s"));
  runtime.check("settings.display-dim-timeout", Boolean(dimmed), dimmed?.test_id);

  await runtime.eventually("inactivity sleep slider", () =>
    runtime.driver.readElement("parity:settings-slider:inactivity_sleep_timeout:1h"));
  await runtime.driver.drag("settings-slider:inactivity_sleep_timeout:1h", { x: -1_000, y: 0 });
  const sleeps = await runtime.eventually("inactivity sleep timeout changed", () =>
    runtime.driver.readElement("parity:settings-slider:inactivity_sleep_timeout:30m"));
  runtime.check("settings.inactivity-sleep-timeout", Boolean(sleeps), sleeps?.test_id);

  const updated = await setFixtureControl(runtime, {
    publication: "updated",
    artifact_fault: "drop",
  });
  try {
    await runtime.driver.openPage("offline_packages");
    await runtime.driver.performAction("offline-refresh-button");
    await waitForOfflineSyncIdle(runtime, "updated package plan ready");
    await runtime.driver.performAction("offline-sync-button");
    await runtime.eventually("interrupted package request observed", async () => {
      const health = await fixtureHealth(runtime);
      return health.control?.dropped_artifact_requests > 0 ? health : null;
    }, 60_000, 250);
    const recovered = await waitForOfflineSyncIdle(runtime, "failed package sync returned to idle");
    runtime.check(
      "offline.interrupted-sync",
      Boolean(recovered),
      "truncated artifact transfer failed closed and returned the planner to idle",
    );

    await setFixtureControl(runtime, { artifact_fault: "none" });
    await runtime.driver.performAction("offline-sync-button");
    const installed = await runtime.eventually("updated package installed", () =>
      runtime.driver.readElement(`installed-package:${updated.updated_artifact_filename}`),
    120_000, 500);
    await waitForOfflineSyncIdle(runtime, "updated package sync completed");
    const health = await fixtureHealth(runtime);
    runtime.check(
      "offline.update",
      Boolean(installed && health.control?.completed_update_artifact_requests > 0),
      updated.updated_artifact_filename,
    );
  } finally {
    await setFixtureControl(runtime, { publication: "primary", artifact_fault: "none" });
  }
}

const DEBUG_ASSERTIONS = Object.freeze({
  tile_labels: "settings.debug.tile-labels",
  nexrad_tile_labels: "settings.debug.nexrad-tile-labels",
  fast_tiles: "settings.debug.fast-tiles",
  offline_simulated_clock_buttons: "settings.debug.offline-clock",
  sequencing_finish_lines: "settings.debug.sequencing-finish-lines",
  plate_flight_plan: "settings.debug.plate-flight-plan",
  bad_autopilot: "settings.debug.bad-autopilot",
  internet_adsb: "settings.debug.internet-adsb",
  gps_capture: "settings.debug.gps-capture",
  debug_log_to_developer_server: "settings.debug.developer-log",
});

const STATUS_ASSERTIONS = Object.freeze({
  client: "status.client",
  "publication:current_artifacts": "status.publication",
  "contracts:expected": "status.contracts",
  nav_db: "status.nav-db",
  "cycle:charts": "status.cycle-charts",
  "cycle:airport_docs": "status.cycle-airport-docs",
  "static:base_data": "status.static-base-data",
  "live_feed:connection": "status.live-feed-connection",
  "live_feed:tfrs": "status.tfrs",
  "live_feed:notams": "status.notams",
  "live_feed:metars": "status.metars",
  "live_feed:pireps": "status.pireps",
  "live_feed:tafs": "status.tafs",
  "live_feed:nexrad": "status.nexrad",
  "live_feed:obstacles": "status.obstacles",
  "live_feed:winds-aloft": "status.winds-aloft",
});

const MIXED_STATUS_SEVERITIES = Object.freeze({
  "live_feed:connection": "ok",
  "live_feed:tfrs": "warning",
  "live_feed:notams": "ok",
  "live_feed:metars": "ok",
  "live_feed:pireps": "unavailable",
  "live_feed:tafs": "ok",
  "live_feed:nexrad": "ok",
  "live_feed:obstacles": "ok",
  "live_feed:winds-aloft": "ok",
});

async function statusAndSettings(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("settings");

  const flightDataChoices = await runtime.driver.readProjection("settings-choice-flight_data_visibility");
  const firstFlightDataChoice = flightDataChoices[0] ?? null;
  runtime.check("settings.flight-data-visibility", Boolean(firstFlightDataChoice));
  if (firstFlightDataChoice) {
    await runtime.driver.performAction(projectionId(firstFlightDataChoice).replace(/^parity:/, ""));
  }

  const debugSection = await runtime.driver.readElement("settings-section-debug_diagnostics");
  runtime.check("settings.debug-folded", debugSection?.expanded === "false", debugSection?.test_id);
  await runtime.driver.performAction("settings-section-debug_diagnostics");
  let firstDebugToggle = true;
  for (const [flagId, assertionId] of Object.entries(DEBUG_ASSERTIONS)) {
    const actionId = `settings-toggle-debug_${flagId}`;
    const before = await runtime.eventually(`${flagId} debug setting`, () => runtime.driver.readElement(actionId));
    await runtime.driver.performAction(actionId);
    const after = await runtime.eventually(`${flagId} debug setting changed`, async () => {
      const value = await runtime.driver.readElement(actionId);
      return value && value.checked !== before.checked ? value : null;
    });
    runtime.check(assertionId, Boolean(after));
    if (firstDebugToggle) {
      runtime.check("settings.debug-toggle", Boolean(after));
      firstDebugToggle = false;
    }
  }

  await runtime.driver.openPage("data_status");
  const statusRows = await runtime.eventually("data status rows", async () => {
    const entries = await runtime.driver.readProjection("parity:data-status-row:");
    return entries.length >= Object.keys(STATUS_ASSERTIONS).length ? entries : null;
  }, 45_000);
  runtime.check("status.all-rows", statusRows.length >= Object.keys(STATUS_ASSERTIONS).length, `${statusRows.length} rows`);
  const statusIds = statusRows.map(projectionId);
  for (const [rowId, assertionId] of Object.entries(STATUS_ASSERTIONS)) {
    const row = statusRows.find((entry) => projectionId(entry).startsWith(`parity:data-status-row:${rowId}:`));
    const expectedSeverity = MIXED_STATUS_SEVERITIES[rowId];
    const matchesSeverity = !expectedSeverity || projectionId(row).endsWith(`:severity:${expectedSeverity}`);
    runtime.check(assertionId, Boolean(row) && matchesSeverity, row?.text ?? rowId);
  }
  runtime.result.diagnostics.data_status_rows = statusRows.map((row) => ({ id: projectionId(row), text: row.text }));
  const severities = new Set(statusIds.flatMap((id) => {
    const severity = id.match(/:severity:([^:]+)$/)?.[1];
    return severity ? [severity] : [];
  }));
  runtime.check(
    "status.fresh-stale-missing",
    Object.entries(MIXED_STATUS_SEVERITIES).every(([rowId, severity]) =>
      statusIds.some((id) =>
        id.startsWith(`parity:data-status-row:${rowId}:`) && id.endsWith(`:severity:${severity}`))),
    [...severities].join(", "),
  );
}

const RASTER_ASSERTIONS = Object.freeze({
  none: "raster.none",
  sec: "raster.sec",
  tac: "raster.tac",
  flyway: "raster.flyway",
  "enr-l": "raster.enr-l",
  "enr-h": "raster.enr-h",
  "shaded-relief": "raster.shaded-relief",
});

const LAYER_ASSERTIONS = Object.freeze({
  world_basemap: "layer.world-basemap",
  vectors: "layer.vectors",
  metars: "layer.metars",
  nexrad: "layer.nexrad",
  traffic: "layer.traffic",
  terrain_warning: "layer.terrain-warning",
  offline_regions: "layer.offline-regions",
});

const ANDROID_LAYER_NAMES = Object.freeze({
  world_basemap: "WorldBasemap",
  vectors: "Vectors",
  metars: "Metars",
  nexrad: "Nexrad",
  traffic: "Traffic",
  terrain_warning: "TerrainWarning",
  offline_regions: "OfflineRegions",
});

function layerProbeId(runtime, layerId) {
  return runtime.platform === "android" ? ANDROID_LAYER_NAMES[layerId] : layerId;
}

async function setLayerVisible(runtime, layerId, visible) {
  const probeName = layerProbeId(runtime, layerId);
  const current = await runtime.eventually(`${layerId} layer state`, async () => {
    const id = projectionId((await runtime.driver.readProjection(`parity:map-layer:${probeName}:`))[0]);
    return id ? { id, visible: /:visible:true:/.test(id), enabled: /:enabled:true$/.test(id) } : null;
  });
  if (current.visible === visible) return current.id;
  if (!current.enabled) throw new Error(`${layerId} layer is disabled: ${current.id}`);
  await runtime.driver.chooseOption("layers-button", layerId);
  if (runtime.platform === "android") await runtime.driver.back();
  return runtime.eventually(`${layerId} layer ${visible ? "visible" : "hidden"}`, async () => {
    const id = projectionId((await runtime.driver.readProjection(`parity:map-layer:${probeName}:`))[0]);
    return id && /:visible:true:/.test(id) === visible ? id : null;
  }, 45_000);
}

async function mapModesAndOverlays(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("settings");
  await runtime.driver.performAction("settings-section-debug_diagnostics");
  const adsb = await runtime.eventually(
    "internet ADS-B setting",
    () => runtime.driver.readElement("settings-toggle-debug_internet_adsb"),
  );
  if (!adsb.checked) {
    await runtime.driver.performAction("settings-toggle-debug_internet_adsb");
    await runtime.eventually("internet ADS-B enabled", async () => {
      const value = await runtime.driver.readElement("settings-toggle-debug_internet_adsb");
      return value?.checked ? value : null;
    });
  }
  await runtime.driver.openPage("map");

  for (const familyId of runtime.capability("raster_families")) {
    await runtime.driver.chooseOption("chart-family-button", familyId);
    const selected = await runtime.eventually(`${familyId} raster family selected`, async () => {
      const entries = await runtime.driver.readProjection(`parity:map-family:${familyId}:`);
      return entries[0] ?? null;
    }, 45_000);
    if (familyId === "none") {
      const empty = await runtime.eventually("empty raster plan", async () => {
        const counts = rasterCounts(await runtime.driver.readProjection("parity:raster-state:"));
        return counts?.planned === 0 ? counts : null;
      });
      runtime.check(RASTER_ASSERTIONS[familyId], Boolean(selected && empty), JSON.stringify(empty));
    } else {
      const paint = await loadedMap(runtime);
      runtime.check(RASTER_ASSERTIONS[familyId], Boolean(selected && paint.raster.failed === 0), JSON.stringify(paint.raster));
    }
  }

  for (const [layerId, assertionId] of Object.entries(LAYER_ASSERTIONS)) {
    const probeName = layerProbeId(runtime, layerId);
    const before = projectionId((await runtime.driver.readProjection(`parity:map-layer:${probeName}:`))[0]);
    const beforeVisible = /:visible:true:/.test(before);
    await runtime.driver.performAction("layers-button");
    const option = await runtime.eventually(`${layerId} layer option`, async () => {
      const entries = await runtime.driver.readProjection(
        runtime.platform === "web" ? "tray-option-" : "parity:tray-option:",
      );
      const suffix = runtime.platform === "web" ? layerId : probeName;
      return entries.find((entry) => projectionId(entry).endsWith(suffix)) ?? null;
    });
    runtime.result.diagnostics[`layer_${layerId}`] = { before, option };
    if (option.enabled) {
      await runtime.driver.performAction(`tray-option:${trayOptionId(option)}`);
      if (runtime.platform === "android") await runtime.driver.back();
      const changed = await runtime.eventually(`${layerId} layer changed`, async () => {
        const entry = (await runtime.driver.readProjection(`parity:map-layer:${probeName}:`))[0];
        const id = projectionId(entry);
        return id && /:visible:true:/.test(id) !== beforeVisible ? id : null;
      });
      runtime.check(assertionId, Boolean(changed), changed);
      if (runtime.platform !== "android") await runtime.driver.back();
      await delay(350);
    } else {
      runtime.check(assertionId, Boolean(option.text), `${option.text} (disabled with reason)`);
      await runtime.driver.back();
    }
  }

  const north = await runtime.driver.readElement("map-orientation-button");
  runtime.check("map.n-up", north?.pressed !== "true", north?.text);
  await loadReplayFixture(runtime);
  await setReplayRate(runtime, 0.25);
  await runtime.driver.performAction("map-orientation-button");
  const track = await runtime.eventually("track-up orientation", async () => {
    const button = await runtime.driver.readElement("map-orientation-button");
    return button?.pressed === "true" || button?.selected === true ? button : null;
  });
  await runtime.driver.performAction("playback-play-toggle");
  const trackViewport = await runtime.eventually("rotated track-up viewport", async () => {
    const id = projectionId((await runtime.driver.readProjection("parity:viewport:"))[0]);
    const up = Number(/:up:(-?[0-9.]+)/.exec(id)?.[1] ?? 0);
    return Math.abs(up) > 1 ? id : null;
  });
  runtime.check("map.trk-up", Boolean(track && trackViewport), trackViewport);
  const gapViewport = await runtime.eventually("map missing-track sample", async () => {
    const state = ownshipState(await runtime.driver.readProjection("parity:ownship-state:"));
    if (state?.track !== "none") return null;
    const viewport = idOf(await runtime.driver.readProjection("parity:viewport:"));
    return viewport ? { state, viewport } : null;
  }, 15_000, 40);
  await runtime.driver.performAction("playback-play-toggle");
  await runtime.eventually("replay paused in track gap", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state?.status === "paused" ? state : null;
  });
  const pausedViewport = projectionId((await runtime.driver.readProjection("parity:viewport:"))[0]);
  await runtime.driver.openPage("flight_plan");
  await runtime.driver.openPage("map");
  const heldViewport = projectionId((await runtime.driver.readProjection("parity:viewport:"))[0]);
  const gapUp = /:up:(-?[0-9.]+)/.exec(gapViewport.viewport)?.[1];
  const pausedUp = /:up:(-?[0-9.]+)/.exec(pausedViewport)?.[1];
  const heldUp = /:up:(-?[0-9.]+)/.exec(heldViewport)?.[1];
  runtime.check(
    "map.track-gap",
    gapUp !== undefined && Number(gapUp) !== 0 && pausedUp !== undefined && pausedUp === heldUp,
    `gap ${gapViewport.viewport}; paused ${pausedViewport}; returned ${heldViewport}`,
  );

  const warning = await runtime.driver.readElement("data-status-launcher");
  runtime.check("map.warning", Boolean(warning), warning?.text);
  if (warning) {
    await runtime.driver.performAction("data-status-launcher");
    await runtime.driver.back();
  }

  await runtime.driver.openPage("map");
  await runtime.driver.chooseOption("chart-family-button", "tac");
  await runtime.driver.performAction("chart-family-button");
  const reference = await runtime.driver.readElement(
    runtime.platform === "web" ? "tray-option-accessory-tac" : "chart-reference-button",
  );
  if (reference) await runtime.driver.performAction(
    runtime.platform === "web" ? "tray-option-accessory-tac" : "chart-reference-button",
  );
  const plate = reference ? await waitForPage(runtime, "plate") : null;
  runtime.check("map.chart-reference", Boolean(reference && plate));
}

async function selectAirportFromMapSearch(runtime, airportId) {
  const suggestionId = runtime.platform === "android"
    ? `chart-search-suggestion:${airportId}`
    : `chart-search-suggestion-${airportId}`;
  await runtime.driver.openPage("map");
  await runtime.driver.enterText("chart-search-input", airportId);
  await runtime.eventually(`${airportId} chart search suggestion`, async () =>
    Boolean(await runtime.driver.readElement(suggestionId)));
  await runtime.driver.performAction(suggestionId);
  return runtime.eventually(`${airportId} map selection`, async () => {
    const entries = await runtime.driver.readProjection(`parity:map-selection-selected:${airportId}`);
    return entries[0] ?? null;
  });
}

async function openAirportInfo(runtime, airportId) {
  await selectAirportFromMapSearch(runtime, airportId);
  await runtime.driver.performAction("airport_info");
  return runtime.eventually(`${airportId} airport info`, () =>
    runtime.driver.readElement(`airport-info-modal:${airportId}`), 45_000);
}

async function closeMapDetail(runtime) {
  await runtime.driver.back();
  await runtime.driver.back();
}

async function airportInfo(runtime) {
  const complexAirport = runtime.capability("airport.runway_complex");
  const fallbackAirport = runtime.capability("airport.runway_fallback");
  const publishedTpaAirport = runtime.capability("airport.published_tpa");
  const derivedTpaAirport = runtime.capability("airport.derived_tpa");
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);

  await openAirportInfo(runtime, complexAirport);
  const beforeTime = (await runtime.driver.readElement("airport-info-time-toggle"))?.text;
  await runtime.driver.performAction("airport-info-time-toggle");
  const afterTime = await runtime.eventually("airport time mode changed", async () => {
    const text = (await runtime.driver.readElement("airport-info-time-toggle"))?.text;
    return text && text !== beforeTime ? text : null;
  });
  runtime.check("airport-info.time-toggle", Boolean(afterTime), `${beforeTime} -> ${afterTime}`);
  const initialScroll = projectionId((await runtime.driver.readProjection("parity:airport-info-scroll:"))[0]);
  await runtime.driver.drag(`airport-info-modal:${complexAirport}`, { x: 0, y: -500 });
  const scrolled = await runtime.eventually("airport info scrolled", async () => {
    const id = projectionId((await runtime.driver.readProjection("parity:airport-info-scroll:"))[0]);
    return id && id !== initialScroll ? id : null;
  });
  runtime.check("airport-info.scroll", Boolean(scrolled), `${initialScroll} -> ${scrolled}`);
  const complex = (await runtime.driver.readProjection("airport-info-runways:complex:true:"))[0];
  runtime.check("airport-info.runway-complex", Boolean(complex), projectionId(complex));
  await closeMapDetail(runtime);

  await openAirportInfo(runtime, publishedTpaAirport);
  const publishedFacts = await runtime.driver.readProjection("airport-info-fact:");
  const published = publishedFacts.find((entry) => /TRAFFIC PATTERN ALTITUDE/i.test(entry.text) && /PUBLISHED/i.test(entry.text));
  runtime.check("airport-info.tpa-published", Boolean(published), published?.text);
  await closeMapDetail(runtime);

  await openAirportInfo(runtime, derivedTpaAirport);
  const derivedFacts = await runtime.driver.readProjection("airport-info-fact:");
  const derived = derivedFacts.find((entry) => /TRAFFIC PATTERN ALTITUDE/i.test(entry.text) && /DERIVED/i.test(entry.text));
  runtime.check("airport-info.tpa-derived", Boolean(derived), derived?.text);
  await closeMapDetail(runtime);

  await openAirportInfo(runtime, fallbackAirport);
  const fallback = (await runtime.driver.readProjection("airport-info-runways:complex:false:"))[0];
  runtime.check("airport-info.runway-fallback", Boolean(fallback), projectionId(fallback));
}

async function inspectorDetails(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("flight_plan");
  await appendRoute(runtime, "KSEA KPAE");
  await planAction(runtime, "KPAE", "activate_leg");
  await enableDeterministicOwnship(runtime);

  const airport = await selectAirportFromMapSearch(runtime, "KSEA");
  runtime.check("inspector.airport-priority", Boolean(airport), airport.text);
  const initialDistance = airport.text;
  const changedDistance = await runtime.eventually("live inspector distance", async () => {
    const entry = (await runtime.driver.readProjection("parity:map-selection-selected:KSEA"))[0];
    return entry?.text && entry.text !== initialDistance ? entry.text : null;
  }, 5_000, 250);
  runtime.check("inspector.distance-live", Boolean(changedDistance), `${initialDistance} -> ${changedDistance}`);

  await runtime.driver.performAction("airport_info");
  const info = await runtime.eventually("airport info modal", () => runtime.driver.readElement("airport-info-modal:KSEA"));
  runtime.check("inspector.info", Boolean(info));
  await closeMapDetail(runtime);

  await selectAirportFromMapSearch(runtime, "KSEA");
  const weatherAction = await runtime.driver.readElement(
    runtime.platform === "web" ? "map-selection-action-wx" : "map-selection-action:wx",
  );
  if (weatherAction?.enabled) await runtime.driver.performAction("wx");
  const weather = weatherAction?.enabled
    ? await runtime.eventually("weather modal", () => runtime.driver.readElement("weather-detail-modal"))
    : null;
  runtime.check("inspector.weather", Boolean(weatherAction && (weather || weatherAction.disabled_reason)), weatherAction?.text);
  if (weather) await closeMapDetail(runtime);
  else await runtime.driver.back();

  await selectAirportFromMapSearch(runtime, "KSEA");
  const platesAction = await runtime.driver.readElement(
    runtime.platform === "web" ? "map-selection-action-plates" : "map-selection-action:plates",
  );
  if (platesAction?.enabled) await runtime.driver.performAction("plates");
  const plate = platesAction?.enabled ? await waitForPage(runtime, "plate") : null;
  runtime.check("inspector.plates", Boolean(platesAction && plate));

  await runtime.driver.openPage("map");
  if (await runtime.driver.readElement("map-selection-tray")) {
    await runtime.driver.back();
  }
  await runtime.driver.drag("map-surface", { x: 360, y: 260 });
  await runtime.driver.inspectMapAt({ x: 0.30, y: 0.45 });
  const spot = await runtime.eventually("raw SPOT selection", async () => {
    const entries = await runtime.driver.readProjection("parity:map-selection-selected:");
    return entries.find((entry) => /SPOT/i.test(entry.text)) ?? null;
  }, 15_000);
  runtime.check("inspector.spot-fallback", Boolean(spot), spot?.text);
  const terrain = await runtime.eventually("SPOT terrain result", async () => {
    const entry = (await runtime.driver.readProjection("parity:map-selection-selected:"))
      .find((candidate) => /SPOT/i.test(candidate.text));
    return entry && /(MSL|ELEV|FT)/i.test(entry.text) ? entry : null;
  }, 15_000);
  runtime.check("inspector.terrain-async", Boolean(terrain), terrain?.text);
  if (await runtime.driver.readElement("map-selection-tray")) {
    await runtime.driver.back();
  }
  await runtime.driver.openPage("flight_plan");
  await openPlanRow(runtime, "KSEA");
  const unavailableArrival = await runtime.driver.readElement(runtime.platform === "web"
    ? "plan-row-action-select_arrival"
    : "plan-row-action:select_arrival");
  const disabledReason = unavailableArrival?.disabled_reason ?? unavailableArrival?.text;
  runtime.check(
    "inspector.disabled-reason",
    Boolean(unavailableArrival && !unavailableArrival.enabled && disabledReason),
    disabledReason,
  );
  for (const [actionId, assertionId] of [
    ["waypoint_info", "plan.row-waypoint-info"],
    ["weather", "plan.row-weather"],
    ["plates", "plan.row-plates"],
  ]) {
    const action = await runtime.driver.readElement(runtime.platform === "web"
      ? `plan-row-action-${actionId}`
      : `plan-row-action:${actionId}`);
    runtime.check(assertionId, Boolean(action), action?.text);
  }
}

async function flightPlanAirwayEstimates(runtime) {
  const airway = runtime.capability("airway");
  const star = runtime.capability("procedure.star");
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("flight_plan");
  await appendRoute(runtime, `KSEA ${airway.entry}`);
  await planAction(runtime, airway.entry, "add_airway");
  await runtime.eventually("airway picker", async () => {
    const options = await runtime.driver.readProjection("parity:plan-airway-suggestion:");
    return options.length > 0 ? options : null;
  });
  await runtime.eventually(`${airway.airway} airway suggestion`, () =>
    runtime.driver.readElement(`plan-airway-suggestion:${airway.airway}`));
  await runtime.driver.performAction(`plan-airway-suggestion:${airway.airway}`);
  await runtime.driver.performAction(`plan-airway-entry:${airway.entry}`);
  await runtime.eventually("airway exits", async () => {
    const entries = await runtime.driver.readProjection("parity:plan-airway-exit:");
    return entries.length > 0 ? entries : null;
  });
  runtime.result.diagnostics.airway_exits = {
    selected: airway.exit,
  };
  await runtime.driver.performAction(`plan-airway-exit:${airway.exit}`);
  const airwayExit = await findPlanRow(runtime, airway.exit);
  runtime.check("plan.airway-scroll", Boolean(airwayExit), airwayExit?.text);
  runtime.check("plan.add-airway", Boolean(airwayExit), airwayExit?.text);

  const weather = await runtime.eventually("flight-plan weather badge", async () => {
    return runtime.driver.findProjectionMatching("parity:plan-weather-badge:", "");
  }, 45_000);
  runtime.check("plan.weather-badge", Boolean(weather), projectionId(weather));

  const eteColumn = (await runtime.driver.readProjection("parity:plan-column:"))
    .find((entry) => /\bETE\b/i.test(entry.text));
  const eteBefore = eteColumn?.text;
  if (eteColumn?.enabled) await runtime.driver.performAction(projectionId(eteColumn));
  const eteAfter = eteColumn ? await runtime.eventually("ETE scope changed", async () => {
    const column = (await runtime.driver.readProjection("parity:plan-column:"))
      .find((entry) => /\bETE\b/i.test(entry.text));
    return column?.text && column.text !== eteBefore ? column : null;
  }) : null;
  runtime.check("plan.ete-scope", Boolean(eteAfter), `${eteBefore} -> ${eteAfter?.text}`);

  const etaColumn = (await runtime.driver.readProjection("parity:plan-column:"))
    .find((entry) => /\bETA\b/i.test(entry.text));
  const etaBefore = etaColumn?.text;
  if (etaColumn?.enabled) await runtime.driver.performAction(projectionId(etaColumn));
  const etaAfter = etaColumn ? await runtime.eventually("ETA time basis changed", async () => {
    const column = (await runtime.driver.readProjection("parity:plan-column:"))
      .find((entry) => /\bETA\b/i.test(entry.text));
    return column?.text && column.text !== etaBefore ? column : null;
  }) : null;
  runtime.check("plan.time-mode", Boolean(etaAfter), `${etaBefore} -> ${etaAfter?.text}`);

  await appendRoute(runtime, "S88");
  await enabledPlanControl(runtime, "undo");
  await runtime.driver.performAction("undo");
  const undone = await runtime.eventually("undo route append", async () =>
    !(await planRows(runtime)).some((entry) => entry.text.includes("S88")));
  runtime.check("plan.undo", undone);
  await enabledPlanControl(runtime, "redo");
  await runtime.driver.performAction("redo");
  const redone = await runtime.eventually("redo route append", async () =>
    (await planRows(runtime)).some((entry) => entry.text.includes("S88")));
  runtime.check("plan.redo", redone);

  await runtime.step("app.reset.vectors", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("flight_plan");
  await appendRoute(runtime, `KYKM ${star.airport_id}`);
  await selectProcedure(runtime, {
    airportId: star.airport_id,
    actionId: "select_arrival",
    procedureId: star.procedure_id,
    transition: star.transition,
  });
  const estimates = await runtime.eventually("estimates across vector discontinuity", async () => {
    const entries = await runtime.driver.readProjection("parity:plan-data:");
    const populated = entries.filter((entry) => !/:none$/.test(projectionId(entry)));
    return populated.length >= 3 ? populated : null;
  }, 45_000);
  runtime.check("plan.estimates-vectors", Boolean(estimates), `${estimates?.length ?? 0} populated cells`);
}

function altitudeControlId(runtime, controlId) {
  return runtime.platform === "web"
    ? `altitude-planner-control-${controlId}`
    : `altitude-planner-control:${controlId}`;
}

async function chooseDifferentAltitudeOption(runtime, controlId) {
  const launcherId = altitudeControlId(runtime, controlId);
  const before = await runtime.driver.readElement(launcherId);
  await runtime.driver.performAction(launcherId);
  await delay(150);
  const directAfter = await runtime.driver.readElement(launcherId);
  if (directAfter?.text && directAfter.text !== before?.text) {
    return { before, option: null, after: directAfter };
  }
  const option = await runtime.eventually(`${controlId} alternate option`, async () => {
    const options = await runtime.driver.readProjection(runtime.platform === "web"
      ? "tray-option-"
      : `parity:altitude-planner-option:${controlId}:`);
    return options.find((entry) => entry.enabled !== false && entry.pressed !== "true")
      ?? options.find((entry) => entry.enabled !== false)
      ?? null;
  });
  await runtime.driver.performAction(runtime.platform === "web"
    ? `tray-option:${trayOptionId(option)}`
    : projectionId(option));
  const after = await runtime.eventually(`${controlId} selection changed`, async () => {
    const value = await runtime.driver.readElement(launcherId);
    return value?.text && value.text !== before?.text ? value : null;
  }, 45_000);
  return { before, option, after };
}

async function altitudePlanner(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("altitude_planner");
  const unavailable = await runtime.eventually("altitude planner unavailable reason", () =>
    runtime.driver.readElement("altitude-planner-status"));
  runtime.check("altitude.unavailable-reason", Boolean(unavailable?.text), unavailable?.text);

  await runtime.driver.openPage("flight_plan");
  await appendRoute(runtime, "KSEA KPAE");
  await runtime.driver.openPage("altitude_planner");
  const initialPanel = await runtime.eventually("altitude comparison panel", () =>
    runtime.driver.readElement("altitude-comparison-panel"), 60_000);
  const initialText = initialPanel.text;

  const aircraft = await chooseDifferentAltitudeOption(runtime, "aircraft");
  runtime.check("altitude.aircraft", Boolean(aircraft.after), `${aircraft.before?.text} -> ${aircraft.after?.text}`);
  const afterAircraft = await runtime.eventually("aircraft comparison changed", async () => {
    const panel = await runtime.driver.readElement("altitude-comparison-panel");
    return panel?.text && panel.text !== initialText ? panel : null;
  }, 60_000);
  runtime.check("altitude.changed-estimate", Boolean(afterAircraft), afterAircraft?.text.slice(0, 240));

  const profile = await chooseDifferentAltitudeOption(runtime, "aircraft_profile");
  runtime.check("altitude.aircraft-profile", Boolean(profile.after), `${profile.before?.text} -> ${profile.after?.text}`);

  const wind = await chooseDifferentAltitudeOption(runtime, "wind_model");
  runtime.check("altitude.wind-model", Boolean(wind.after), `${wind.before?.text} -> ${wind.after?.text}`);
  const forecast = await runtime.driver.readElement("altitude-planner-forecast");
  runtime.check(
    "altitude.forecast-fallback",
    Boolean(forecast?.text || wind.after?.text),
    forecast?.text ?? wind.after?.text,
  );

  const basisBefore = await runtime.driver.readElement("altitude-planner-departure-basis");
  await runtime.driver.performAction("altitude-planner-departure-basis");
  const basisAfter = await runtime.eventually("departure time basis changed", async () => {
    const value = await runtime.driver.readElement("altitude-planner-departure-basis");
    return value?.text && value.text !== basisBefore?.text ? value : null;
  });
  runtime.check("altitude.time", Boolean(basisAfter), `${basisBefore?.text} -> ${basisAfter?.text}`);

  const rows = await runtime.driver.readProjection(runtime.platform === "web"
    ? "altitude-comparison-row-"
    : "parity:altitude-comparison-row:");
  const alternateAltitude = rows.find((row) => row.enabled !== false && row.selected !== "true" && row.pressed !== "true")
    ?? rows.find((row) => row.enabled !== false);
  if (alternateAltitude) await runtime.driver.performAction(projectionId(alternateAltitude));
  const selectedAltitude = alternateAltitude ? await runtime.eventually("selected altitude row", async () => {
    const nextRows = await runtime.driver.readProjection(runtime.platform === "web"
      ? "altitude-comparison-row-"
      : "parity:altitude-comparison-row:");
    return nextRows.find((row) => row.selected === "true" || row.pressed === "true") ?? null;
  }) : null;
  runtime.check("altitude.altitude", Boolean(selectedAltitude), selectedAltitude?.text);
}

async function selectReplaySource(runtime) {
  await runtime.driver.performAction("ownship-source-button");
  const option = await runtime.eventually("Replay ownship source", async () => {
    const entries = await runtime.driver.readProjection(runtime.platform === "web"
      ? "tray-option-"
      : "parity:ownship-source:");
    return entries.find((entry) => /REPLAY/i.test(entry.text ?? "")) ?? null;
  });
  if (runtime.platform === "web") {
    await runtime.driver.performAction(`tray-option:${trayOptionId(option)}`);
  } else {
    await runtime.driver.performAction(projectionId(option));
  }
  return runtime.eventually("playback controls", () => runtime.driver.readElement("playback-source-input"));
}

async function loadReplayFixture(runtime) {
  const tracePath = runtime.fixtureUrl(runtime.capability("replay_trace"));
  await selectReplaySource(runtime);
  await runtime.driver.enterText(
    "playback-source-input",
    tracePath,
    { dismissKeyboard: runtime.platform === "android" },
  );
  return runtime.eventually("loaded replay trace", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    if (state?.status === "paused" && state.duration > 0) return state;
    if (state?.status === "empty") await runtime.driver.performAction("playback-load-button");
    return null;
  }, 45_000);
}

async function setReplayRate(runtime, rate) {
  if (runtime.platform === "web") {
    await runtime.driver.enterText("playback-rate-input", String(rate));
  } else {
    await runtime.driver.drag("playback-rate-input", { x: -1_000, y: 0 });
  }
  return runtime.eventually("replay rate set", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state && Math.abs(state.rate - rate) < 0.01 ? state : null;
  });
}

async function replayTrackUp(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  const loaded = await loadReplayFixture(runtime);
  runtime.check("replay.load", Boolean(loaded), JSON.stringify(loaded));
  await setReplayRate(runtime, 0.25);

  await runtime.driver.performAction("map-orientation-button");
  await runtime.eventually("TRK-up selected for replay", async () => {
    const button = await runtime.driver.readElement("map-orientation-button");
    return button?.pressed === "true" || button?.selected === true ? button : null;
  });
  await runtime.driver.performAction("playback-play-toggle");
  runtime.result.diagnostics.replay_initial_ownship =
    await runtime.driver.readProjection("parity:ownship-state:");
  const initialOwnship = await runtime.eventually("initial replay ownship", async () => {
    const state = ownshipState(await runtime.driver.readProjection("parity:ownship-state:"));
    return state?.mode === "replay" && state.draw && state.position !== "none" ? state : null;
  });

  const playing = await runtime.eventually("replay playing", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state?.status === "playing" && state.cursor > 0.2 ? state : null;
  });
  const rotated = await runtime.eventually("replay TRK-up rotation", async () => {
    const id = idOf(await runtime.driver.readProjection("parity:viewport:"));
    const up = Number(/:up:(-?[0-9.]+)/.exec(id ?? "")?.[1] ?? 0);
    return Math.abs(up) > 1 ? { id, up } : null;
  });
  runtime.check("replay.rotation", Boolean(rotated), rotated?.id);

  await runtime.eventually("replay cursor entered track gap", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state?.status === "playing" && state.cursor >= 2.1 && state.cursor < 3.2 ? state : null;
  }, 12_000, 40);
  await runtime.driver.performAction("playback-play-toggle");
  const paused = await runtime.eventually("replay paused in track gap", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state?.status === "paused" ? state : null;
  });
  const gap = await runtime.eventually("paused missing replay track sample", async () => {
    const state = ownshipState(await runtime.driver.readProjection("parity:ownship-state:"));
    if (state?.track !== "none") return null;
    const viewport = idOf(await runtime.driver.readProjection("parity:viewport:"));
    const up = Number(/:up:(-?[0-9.]+)/.exec(viewport ?? "")?.[1] ?? 0);
    return { state, viewport, up };
  });
  runtime.check(
    "replay.track-gap",
    Math.abs(gap.up) > 1 && Math.abs(gap.up - rotated.up) < 0.2,
    `${rotated.id} -> ${gap.viewport}`,
  );
  runtime.check(
    "replay.ownship",
    gap.state.position !== initialOwnship.position,
    `${initialOwnship.position} -> ${gap.state.position}`,
  );

  runtime.check("replay.play-pause", Boolean(playing && paused), `${playing.cursor} -> ${paused.cursor}`);

  const priorRate = paused.rate;
  if (runtime.platform === "web") {
    await runtime.driver.enterText("playback-rate-input", priorRate === 2 ? "3" : "2");
  } else {
    await runtime.driver.drag("playback-rate-input", { x: 80, y: 0 });
  }
  const changedRate = await runtime.eventually("replay rate changed", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state && Math.abs(state.rate - priorRate) > 0.01 ? state : null;
  });
  runtime.check("replay.rate", Boolean(changedRate), `${priorRate} -> ${changedRate.rate}`);

  const priorCursor = changedRate.cursor;
  await runtime.driver.drag("playback-overview", { x: 120, y: 0 });
  const sought = await runtime.eventually("replay seek committed", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state && Math.abs(state.cursor - priorCursor) > 0.1 ? state : null;
  });
  runtime.check("replay.seek", Boolean(sought), `${priorCursor} -> ${sought.cursor}`);
}

async function preparedLiveFeeds(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("map");
  await setLayerVisible(runtime, "vectors", true);
  await setLayerVisible(runtime, "metars", true);
  const overlay = await runtime.eventually("prepared weather overlays", async () => {
    const state = liveOverlayState(await runtime.driver.readProjection("parity:live-overlay:"));
    return state && state.metars > 0 && state.pireps > 0 ? state : null;
  }, 60_000);

  await selectAirportFromMapSearch(runtime, "KSEA");
  const weatherAction = await runtime.eventually("KSEA weather action", () => runtime.driver.readElement(
    runtime.platform === "web" ? "map-selection-action-wx" : "map-selection-action:wx",
  ));
  if (!weatherAction.enabled) throw new Error(`KSEA weather is unavailable: ${weatherAction.text}`);
  await runtime.driver.performAction("wx");
  const detail = await runtime.eventually("prepared weather detail", async () => {
    const modal = await runtime.driver.readElement("weather-detail-modal");
    return modal?.text && /METAR/i.test(modal.text) && /TAF/i.test(modal.text) && /NOTAM/i.test(modal.text)
      ? modal
      : null;
  }, 60_000);
  runtime.check(
    "livefeed.metar-taf-pirep-notam",
    Boolean(detail && overlay.metars > 0 && overlay.pireps > 0),
    `${JSON.stringify(overlay)} ${detail.text.slice(0, 240)}`,
  );
}

async function nexradFrames(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("map");
  await setLayerVisible(runtime, "nexrad", true);
  const first = await runtime.eventually("painted NEXRAD history frame", async () => {
    const state = nexradState(await runtime.driver.readProjection("parity:nexrad-state:"));
    return state && state.tiles > 0 && state.frames >= 2 && state.frame !== null ? state : null;
  }, 90_000);
  const next = await runtime.eventually("advanced NEXRAD history frame", async () => {
    const state = nexradState(await runtime.driver.readProjection("parity:nexrad-state:"));
    return state && state.tiles > 0 && state.frames === first.frames && state.frame !== first.frame ? state : null;
  }, 12_000, 100);
  runtime.check("livefeed.nexrad-frames", Boolean(next), `${JSON.stringify(first)} -> ${JSON.stringify(next)}`);
}

async function obstaclesNavKv(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("map");
  await setLayerVisible(runtime, "vectors", true);
  const overlay = await runtime.eventually("faulted obstacle NavKv tiles", async () => {
    const state = liveOverlayState(await runtime.driver.readProjection("parity:live-overlay:"));
    return state && state.obstacles > 0 ? state : null;
  }, 90_000);
  runtime.check("livefeed.obstacles-navkv", overlay.obstacles > 0, JSON.stringify(overlay));
}

async function windsAloftNavKv(runtime) {
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("flight_plan");
  await appendRoute(runtime, "KSEA KPAE");
  await runtime.driver.openPage("altitude_planner");
  const wind = await chooseDifferentAltitudeOption(runtime, "wind_model");
  const forecast = await runtime.eventually("forecast-backed altitude comparison", async () => {
    const value = await runtime.driver.readElement("altitude-planner-forecast");
    return value?.text && !/(unavailable|no.wind|ISA)/i.test(value.text) ? value : null;
  }, 90_000);
  const comparison = await runtime.eventually("wind-backed altitude rows", () =>
    runtime.driver.readElement("altitude-comparison-panel"), 60_000);
  runtime.check(
    "livefeed.winds-aloft-navkv",
    Boolean(forecast && comparison?.text),
    `${wind.before?.text} -> ${wind.after?.text}; ${forecast.text} ${comparison.text.slice(0, 200)}`,
  );
}

async function tfrMapDetail(runtime) {
  const airportId = runtime.fixture?.capabilities?.live_feeds?.tfr_target_airport ?? "27W";
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("map");
  await setLayerVisible(runtime, "vectors", true);
  await runtime.driver.chooseOption("chart-family-button", "none");
  await selectAirportFromMapSearch(runtime, airportId);
  const tfrItemId = runtime.platform === "web"
    ? "map-selection-item-airspace-TFR"
    : "map-selection-item:airspace-TFR";
  const tfrItem = await runtime.eventually(
    "TFR map selection item",
    () => runtime.driver.readElement(tfrItemId),
    90_000,
  );
  await runtime.driver.performAction(tfrItemId);
  await runtime.driver.performAction("tfr_text");
  const detail = await runtime.eventually("TFR text detail", () =>
    runtime.driver.readElement("map-selection-detail-modal:TFR"));
  runtime.check(
    "livefeed.tfr-map-detail",
    Boolean(tfrItem && detail?.text && /TEMPORARY FLIGHT RESTRICTIONS/i.test(detail.text)),
    detail?.text?.slice(0, 300),
  );
}

function cloudStatusElementId(runtime) {
  return runtime.platform === "web" ? "cloud-overall-status" : "cloud-panel:overall_status";
}

function cloudActionElementId(runtime, actionId) {
  return runtime.platform === "web" ? `cloud-action-${actionId}` : `cloud-action:${actionId}`;
}

async function waitForCloudActive(runtime) {
  return runtime.eventually("active Sync Account", async () => {
    const status = await runtime.driver.readElement(cloudStatusElementId(runtime));
    return status?.text && /Cloud active/i.test(status.text) ? status : null;
  }, 30_000);
}

async function waitForPlanIdents(runtime, expected) {
  return runtime.eventually(`flight plan ${expected.join(" ")}`, async () => {
    const rows = await runtime.driver.readProjection("parity:plan-row:");
    const text = rows.map((row) => row.text).join(" ");
    return expected.every((ident) => text.includes(ident)) ? rows : null;
  }, 30_000);
}

async function waitForTargetPackagePreferences(runtime, preferences) {
  if (runtime.platform === "web") {
    return runtime.eventually("web cloud package preferences", async () => {
      const state = await runtime.driver.transport.page.evaluate(
        "window.__aerobagE2e?.cloud?.state() ?? null",
      );
      return state
        && JSON.stringify(state.offline_package_preferences) === JSON.stringify(preferences)
        ? state
        : null;
    }, 30_000);
  }

  await runtime.driver.openPage("offline_packages");
  const expected = [
    ...Object.entries(preferences.regions).map(([id, selection]) =>
      `parity:offline-region:${id}:selection:${selection}`),
    ...Object.entries(preferences.products).map(([id, selection]) =>
      `parity:offline-product:${id}:selection:${selection}`),
  ];
  return runtime.eventually("Android cloud package preferences", async () => {
    for (const probe of expected) {
      if (!await runtime.driver.readElement(probe)) return null;
    }
    return expected;
  }, 30_000);
}

async function cloudCrossfill(runtime) {
  const peerUrl = process.env.AEROBAG_E2E_PEER_URL ?? "http://127.0.0.1:8085/";
  let peer = null;
  await runtime.step("app.reset", () => runtime.driver.reset());
  await acceptDisclaimer(runtime);
  await runtime.driver.openPage("cloud");

  const beginSetup = await runtime.eventually(
    "begin cloud setup action",
    () => runtime.driver.readElement(cloudActionElementId(runtime, "begin_setup")),
  );
  runtime.check("cloud.begin-setup", Boolean(beginSetup?.enabled));
  await runtime.driver.performAction("begin_setup");

  const scanCode = await runtime.eventually(
    "scan setup code action",
    () => runtime.driver.readElement(cloudActionElementId(runtime, "scan_setup_code")),
  );
  runtime.check(
    "cloud.scan-code",
    Boolean(scanCode),
    scanCode?.enabled ? "scanner action enabled" : "scanner action explains platform unavailability",
  );
  const setupInput = await runtime.driver.readElement("cloud-setup-code-input");
  if (!setupInput) throw new Error("cloud setup code input is unavailable");

  const backSetup = await runtime.driver.readElement(cloudActionElementId(runtime, "back_setup"));
  await runtime.driver.performAction("back_setup");
  runtime.check("cloud.back-setup", Boolean(backSetup?.enabled));

  const beginCreate = await runtime.eventually(
    "begin account creation action",
    () => runtime.driver.readElement(cloudActionElementId(runtime, "begin_create")),
  );
  runtime.check("cloud.begin-create", Boolean(beginCreate?.enabled));
  await runtime.driver.performAction("begin_create");
  const createAccount = await runtime.eventually(
    "create account action",
    () => runtime.driver.readElement(cloudActionElementId(runtime, "create_account")),
  );
  await runtime.driver.performAction("create_account");
  const active = await waitForCloudActive(runtime);
  runtime.check("cloud.create-account", Boolean(createAccount?.enabled && active));

  const syncNow = await runtime.eventually(
    "sync now action",
    () => runtime.driver.readElement(cloudActionElementId(runtime, "sync_now")),
  );
  await runtime.driver.performAction("sync_now");
  await waitForCloudActive(runtime);
  runtime.check("cloud.sync-now", Boolean(syncNow?.enabled));

  const backup = await runtime.driver.readElement(
    cloudActionElementId(runtime, "backup_setup_code"),
  );
  await runtime.driver.performAction("backup_setup_code");
  const setupCodeElement = await runtime.eventually("Device Setup Code", async () => {
    const element = await runtime.driver.readElement("cloud-setup-code-output");
    const value = element?.value || element?.text;
    return value?.startsWith("AB3.") ? { ...element, value } : null;
  });
  runtime.check("cloud.backup-code", Boolean(backup?.enabled && setupCodeElement));

  await runtime.driver.performAction("copy_setup_code");
  const copy = await runtime.eventually(
    "copy setup code action",
    () => runtime.driver.readElement(cloudActionElementId(runtime, "copy_setup_code")),
  );
  runtime.check("cloud.copy-code", Boolean(copy?.enabled));
  const closeBackup = await runtime.driver.readElement(
    cloudActionElementId(runtime, "close_linked_detail"),
  );
  await runtime.driver.performAction("close_linked_detail");

  const addDevice = await runtime.eventually(
    "add device action",
    () => runtime.driver.readElement(cloudActionElementId(runtime, "add_device")),
  );
  await runtime.driver.performAction("add_device");
  const addDeviceCode = await runtime.eventually(
    "add-device setup code",
    () => runtime.driver.readElement("cloud-setup-code-output"),
  );
  runtime.check("cloud.add-device", Boolean(addDevice?.enabled && addDeviceCode));
  const closeAddDevice = await runtime.driver.readElement(
    cloudActionElementId(runtime, "close_linked_detail"),
  );
  await runtime.driver.performAction("close_linked_detail");
  runtime.check("cloud.close-detail", Boolean(closeBackup?.enabled && closeAddDevice?.enabled));

  try {
    peer = await launchCloudJourneyPeer({
      url: peerUrl,
      referenceEpochMs: null,
    });
    await peer.acceptSetupCode(setupCodeElement.value);
    runtime.check("cloud.accept-code", true, "second client linked with the pasted setup code");

    await peer.appendRoute("KSEA KPAE");
    await runtime.driver.openPage("flight_plan");
    const adoptedPlan = await waitForPlanIdents(runtime, ["KSEA", "KPAE"]);
    runtime.check("cloud.crossfill-plan", Boolean(adoptedPlan));

    const preferences = {
      regions: { nw: "pause" },
      products: { terrain: "pause" },
    };
    await peer.setOfflinePackagePreferences(preferences);
    const adoptedPreferences = await waitForTargetPackagePreferences(runtime, preferences);
    runtime.check("cloud.crossfill-packages", Boolean(adoptedPreferences));

    if (runtime.platform === "web") {
      await runtime.driver.transport.page.evaluate(
        "window.__aerobagE2e.cloud.dropEventStream()",
      );
    } else {
      await runtime.driver.reload();
      await runtime.eventually("Android app after cloud reconnect restart", () =>
        runtime.driver.readElement("primary-navigation"), 60_000);
    }
    await peer.appendRoute("KPLU");
    await runtime.driver.openPage("flight_plan");
    const postReconnectPlan = await waitForPlanIdents(runtime, ["KPLU"]);
    runtime.check("cloud.reconnect", Boolean(postReconnectPlan));
  } finally {
    await peer?.close();
  }

  await runtime.driver.openPage("cloud");
  const beginUnlink = await runtime.eventually(
    "begin unlink action",
    () => runtime.driver.readElement(cloudActionElementId(runtime, "begin_unlink")),
  );
  await runtime.driver.performAction("begin_unlink");
  runtime.check("cloud.begin-unlink", Boolean(beginUnlink?.enabled));
  const confirmUnlink = await runtime.eventually(
    "confirm unlink action",
    () => runtime.driver.readElement(cloudActionElementId(runtime, "confirm_unlink")),
  );
  await runtime.driver.performAction("confirm_unlink");
  const inactive = await runtime.eventually("unlinked Sync Account", async () => {
    const status = await runtime.driver.readElement(cloudStatusElementId(runtime));
    return status?.text && /Cloud not active/i.test(status.text) ? status : null;
  });
  runtime.check("cloud.confirm-unlink", Boolean(confirmUnlink?.enabled && inactive));
}

export const RELEASE_JOURNEY_IMPLEMENTATIONS = Object.freeze({
  "shared.startup-navigation": startupNavigation,
  "shared.chart-basic-use": chartBasicUse,
  "shared.flight-plan-edit-and-navigate": flightPlanEditAndNavigate,
  "shared.procedure-departure": procedureDeparture,
  "shared.procedure-arrival": procedureArrival,
  "shared.procedure-approach": procedureApproach,
  "shared.plate-operate": plateOperate,
  "shared.plate-advisories-and-references": plateAdvisoriesAndReferences,
  "shared.status-and-settings": statusAndSettings,
  "shared.map-modes-and-overlays": mapModesAndOverlays,
  "shared.airport-info": airportInfo,
  "shared.inspector-details": inspectorDetails,
  "shared.flight-plan-airway-estimates": flightPlanAirwayEstimates,
  "shared.altitude-planner": altitudePlanner,
  "shared.replay-track-up": replayTrackUp,
  "shared.prepared-live-feeds": preparedLiveFeeds,
  "shared.nexrad-frames": nexradFrames,
  "shared.obstacles-navkv": obstaclesNavKv,
  "shared.winds-aloft-navkv": windsAloftNavKv,
  "shared.tfr-map-detail": tfrMapDetail,
  "shared.cloud-crossfill": cloudCrossfill,
  "shared.other-documents": otherDocuments,
  "shared.about-and-saved-state": aboutAndSavedState,
  "web.pointer-details": pointerDetails,
  "shared.contract-failures": contractFailures,
  "android.package-maintenance": androidPackageMaintenance,
});

export function releaseJourneyImplementation(id) {
  return RELEASE_JOURNEY_IMPLEMENTATIONS[id] ?? null;
}
