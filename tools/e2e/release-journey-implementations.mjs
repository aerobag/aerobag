// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { timelineSeekDeltaX } from "./gesture-geometry.mjs";
import {
  E2E_TIMING, TerminalObservationError, TransientObservationError,
} from "./transition-contract.mjs";

function idOf(entries) {
  return entries?.[0]?.id ?? entries?.[0] ?? null;
}

export function viewportGeometryId(entries) {
  return idOf(entries)?.replace(/:up:-?[0-9.]+$/, "") ?? null;
}

export function viewportZoomLevel(entries) {
  const match = /:zoom:(-?[0-9.]+)(?::|$)/.exec(idOf(entries) ?? "");
  return match ? Number(match[1]) : null;
}

export function offlineSyncButtonIsIdle(button) {
  return Boolean(button?.enabled) &&
    !/\b(?:APPLYING|SYNCING|CANCELING)\b/i.test(button.text ?? "");
}

export function rasterStateFromProjection(entries) {
  const id = idOf(entries);
  const match = /plan:([^:]+):maps:([^:]+):planned:(\d+):loaded:(\d+):failed:(\d+)/.exec(id ?? "");
  return match ? {
    planId: match[1],
    mapIds: match[2] === "none" ? [] : match[2].split(",").map(decodeURIComponent),
    planned: Number(match[3]),
    loaded: Number(match[4]),
    failed: Number(match[5]),
  } : null;
}

function rasterRecoveryCount(entries) {
  const match = /count:(\d+)/.exec(idOf(entries) ?? "");
  return match ? Number(match[1]) : 0;
}

export function rasterPlanIsDisplayReady(counts, minimumLoadedRatio = 0.85) {
  if (!counts || counts.planned <= 0) return false;
  return counts.loaded / counts.planned >= minimumLoadedRatio ||
    counts.loaded + counts.failed >= counts.planned;
}

export function rasterPlanHasVisiblePaint(counts) {
  return Boolean(counts && counts.planned > 0 && counts.loaded > 0);
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
  const match = /mode:([^:]+):source:([^:]+):draw:(true|false):position:([^:]+):track:([^:]+)/.exec(id ?? "");
  return match ? {
    mode: match[1],
    source: match[2],
    draw: match[3] === "true",
    position: match[4],
    track: match[5],
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

function taggedFields(entry, prefix) {
  const id = projectionId(entry);
  if (!id.startsWith(prefix)) return null;
  const fields = {};
  const components = id.slice(prefix.length).split(":");
  for (let index = 0; index + 1 < components.length; index += 2) {
    fields[components[index]] = components[index + 1];
  }
  return fields;
}

async function startupState(runtime, timeoutMs = E2E_TIMING.startupMs) {
  return runtime.eventually("operational startup state", async () => {
    const fatal = await runtime.driver.readElement("startup-fatal-error");
    if (fatal) {
      throw new TerminalObservationError("application startup failed", fatal.text || "unknown failure");
    }
    const entries = await runtime.driver.readProjection("parity:startup-state:");
    const fields = taggedFields(entries[0], "parity:startup-state:");
    return fields?.ready === "true" ? fields : null;
  }, timeoutMs);
}

async function acceptDisclaimer(runtime, { required = false } = {}) {
  const initial = await startupState(runtime);
  if (initial.disclaimer_required !== "true") {
    if (required) {
      throw new Error("fresh profile reached the map without presenting the disclaimer");
    }
    return false;
  }
  await runtime.action("accept mandatory disclaimer", "disclaimer-accept-button", {
    complete: async () => !(await runtime.driver.readElement("disclaimer-accept-button")) || null,
  });
  const completed = await startupState(runtime);
  if (completed.disclaimer_required !== "false") {
    throw new Error("application startup retained mandatory disclaimer after acceptance");
  }
  return true;
}

async function selectedRasterMap(runtime) {
  return runtime.eventually("selected raster map", async () => {
    const id = idOf(await runtime.driver.readProjection("parity:map-family:"));
    const match = /^parity:map-family:([^:]+):map:(.+)$/.exec(id ?? "");
    return match ? { familyId: match[1], mapId: match[2] } : null;
  });
}

async function loadedMap(runtime, { afterPlanId = null } = {}) {
  const selected = await selectedRasterMap(runtime);
  const firstPaint = await runtime.eventually("first visible raster paint", async () => {
    const state = rasterStateFromProjection(
      await runtime.driver.readProjection("parity:raster-state:"),
    );
    return state
      && state.planId !== afterPlanId
      && state.mapIds.includes(selected.mapId)
      && rasterPlanHasVisiblePaint(state)
      ? state
      : null;
  }, E2E_TIMING.localRenderMs);
  const raster = await runtime.eventually("settled raster plan", async () => {
    const state = rasterStateFromProjection(
      await runtime.driver.readProjection("parity:raster-state:"),
    );
    return state
      && state.planId !== afterPlanId
      && state.mapIds.includes(selected.mapId)
      && rasterPlanIsDisplayReady(state)
      ? state
      : null;
  }, E2E_TIMING.localResourceMs, E2E_TIMING.resourcePollIntervalMs);
  const vectors = await runtime.eventually("vector overlay", async () => {
    const id = idOf(await runtime.driver.readProjection("parity:vector-state:"));
    const count = Number(/features:(\d+)/.exec(id ?? "")?.[1] ?? 0);
    return count > 0 ? count : null;
  }, E2E_TIMING.localRenderMs);
  return { firstPaint, raster, vectors };
}

async function rasterLoadRecovery(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("map");
  await loadedMap(runtime);
  const before = rasterRecoveryCount(
    await runtime.driver.readProjection("parity:raster-recovery:"),
  );
  const recovered = await runtime.transition("recover stalled raster image load", {
    ready: async () => {
      const counts = rasterStateFromProjection(await runtime.driver.readProjection("parity:raster-state:"));
      return rasterPlanIsDisplayReady(counts) && counts.failed === 0 ? counts : null;
    },
    act: () => runtime.driver.injectRasterLoadFault(),
    complete: async () => {
      const recoveryCount = rasterRecoveryCount(
        await runtime.driver.readProjection("parity:raster-recovery:"),
      );
      const counts = rasterStateFromProjection(await runtime.driver.readProjection("parity:raster-state:"));
      return recoveryCount > before
        && rasterPlanIsDisplayReady(counts)
        && counts.failed === 0
        ? { recovery_count: recoveryCount, ...counts }
        : null;
    },
  });
  runtime.check("web.raster-load-recovery", Boolean(recovered), JSON.stringify(recovered));
}

async function disableCtrBeforeFreePan(runtime, description) {
  const initial = await runtime.driver.readElement("center-here-button");
  if (initial?.pressed !== "true" && initial?.selected !== true && initial?.checked !== true) {
    return initial;
  }
  return runtime.action(description, "center-here-button", {
    complete: async () => {
      const value = await runtime.driver.readElement("center-here-button");
      return value && value.pressed !== "true" && value.selected !== true && value.checked !== true
        ? value
        : null;
    },
  });
}

export async function selectChartSearchSuggestion(runtime, ident) {
  const suggestionProjection = `chart-search-suggestion-${ident}`;
  const selectedProjection = `parity:map-selection-selected:${ident}`;
  const completedSelection = async () => {
    const selected = (await runtime.driver.readProjection(selectedProjection))[0];
    if (!selected) return null;
    return (await runtime.driver.readElement("map-selection-tray")) ? selected : null;
  };
  const selected = await completedSelection();
  if (selected) return selected;
  return runtime.action(`${ident} chart search selection`, suggestionProjection, {
    complete: completedSelection,
  });
}

async function revealRequiredElement(runtime, elementId, description = elementId) {
  const element = await runtime.revealElement(elementId, description);
  if (!element) throw new Error(`${description} is not present after explicit traversal`);
  return element;
}

async function enableDeterministicOwnship(runtime) {
  await runtime.openPage("settings");
  const section = await revealRequiredElement(
    runtime, "settings-section-debug_diagnostics", "Debug Diagnostics settings section",
  );
  if (section?.expanded !== true) {
    await runtime.action(
      "expand Debug Diagnostics settings",
      "settings-section-debug_diagnostics",
      {
        complete: async () => {
          const value = await runtime.driver.readElement("settings-section-debug_diagnostics");
          return value?.expanded === true ? value : null;
        },
      },
    );
  }
  await revealRequiredElement(
    runtime, "settings-toggle-debug_bad_autopilot", "Bad Autopilot debug toggle",
  );
  await runtime.eventually(
    "Bad Autopilot debug toggle",
    () => runtime.driver.readElement("settings-toggle-debug_bad_autopilot"),
  );
  const toggle = await runtime.driver.readElement("settings-toggle-debug_bad_autopilot");
  if (toggle?.pressed !== "true" && toggle?.selected !== true && toggle?.checked !== true) {
    await runtime.action(
      "enable Bad Autopilot debug setting",
      "settings-toggle-debug_bad_autopilot",
      {
        complete: async () => {
          const value = await runtime.driver.readElement("settings-toggle-debug_bad_autopilot");
          return value?.pressed === "true" || value?.selected === true || value?.checked === true
            ? value
            : null;
        },
      },
    );
  }
  await runtime.eventually("Bad Autopilot debug enabled", async () => {
    const value = await runtime.driver.readElement("settings-toggle-debug_bad_autopilot");
    return value?.pressed === "true" || value?.selected === true || value?.checked === true
      ? value
      : null;
  });
  await runtime.openPage("map");
  await runtime.chooseOption(
    "select Bad Autopilot ownship",
    "ownship-source-button",
    "__bad_autopilot__",
    {
      complete: async () => {
        const state = ownshipState(await runtime.driver.readProjection("parity:ownship-state:"));
        return state?.source === "__bad_autopilot__" && state.position !== "none" ? state : null;
      },
    },
  );
  const initialOwnship = projectionId((await runtime.driver.readProjection(
    "parity:ownship-state:",
  ))[0]);
  if (!initialOwnship || initialOwnship.includes("position:none")) {
    throw new Error(`Bad Autopilot did not publish an initial ownship position: ${initialOwnship}`);
  }
  return initialOwnship;
}

async function selectStationaryPlanPreview(runtime) {
  await runtime.openPage("map");
  await runtime.chooseOption(
    "select stationary Plan Preview ownship",
    "ownship-source-button",
    "__direct_situation__",
    {
    complete: async () => {
      const launcher = await runtime.driver.readElement("ownship-source-button");
      return /Plan Preview/i.test(launcher?.text ?? "") ? launcher : null;
    },
    },
  );
}

async function startupNavigation(runtime) {
  await runtime.reset();
  // Android's clean-device package bootstrap must accept the disclaimer before
  // it can install the fixture publication. That bootstrap is release-gated;
  // this journey verifies the resulting persisted state on Android and owns
  // first acceptance itself on web.
  await acceptDisclaimer(runtime, { required: runtime.platform === "web" });
  await runtime.openPage("map");
  const firstMap = await loadedMap(runtime);
  runtime.check("startup.supported-publication", firstMap.raster.failed === 0, JSON.stringify(firstMap));

  await runtime.reload();
  await runtime.eventually(
    "map after reload",
    () => runtime.driver.readElement("page:map"),
    E2E_TIMING.startupMs,
  );
  runtime.check(
    "disclaimer.accept-persist",
    !(await runtime.driver.readElement("disclaimer-accept-button")),
  );

  await runtime.openPage("home");
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
    await runtime.openPage(pageId);
    const reached = Boolean(await waitForPage(runtime, pageId === "charts" ? "plate" : pageId));
    if (navigationAssertion) runtime.check(navigationAssertion, reached);
    runtime.check(homeAssertion, reached, destinationId);
    await runtime.openPage("home");
  }

  const aboutId = runtime.platform === "android" ? "home-button:About" : "home-button-about";
  const aboutButton = await runtime.driver.readElement(aboutId);
  runtime.check("home.about", Boolean(aboutButton?.enabled), "About destination is enabled");
}

async function chartBasicUse(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("map");
  await loadedMap(runtime);

  await disableCtrBeforeFreePan(runtime, "disable CTR before chart pan");

  await runtime.editText("enter KSEA chart search", "chart-search-input", "KSEA");
  const selected = await selectChartSearchSuggestion(runtime, "KSEA");
  runtime.check("chart.search", selected);
  runtime.check("chart.inspect", Boolean(await runtime.driver.readElement("map-selection-tray")));
  await runtime.transition("dismiss chart search inspector", {
    ready: () => runtime.driver.readElement("map-selection-tray"),
    act: () => runtime.driver.back(),
    complete: async () => (await runtime.driver.readElement("map-selection-tray")) ? null : true,
  });
  const initialViewport = await runtime.stable("settled chart viewport before pan", async () =>
    idOf(await runtime.driver.readProjection("parity:viewport:")));
  const rasterBeforePan = rasterStateFromProjection(
    await runtime.driver.readProjection("parity:raster-state:"),
  );
  runtime.result.diagnostics.chart_pan_initial_viewport = initialViewport;

  let dragProbe = null;
  const pannedViewport = await runtime.transition("chart pan", {
    ready: () => runtime.driver.readElement("map-surface"),
    act: async (readyElement) => {
      dragProbe = await runtime.driver.drag("map-surface", { x: -360, y: 240 }, readyElement);
    },
    complete: async () => {
      const current = idOf(await runtime.driver.readProjection("parity:viewport:"));
      runtime.result.diagnostics.chart_pan_last_viewport = current;
      return current && current !== initialViewport ? current : null;
    },
  });
  if (dragProbe) runtime.result.diagnostics.chart_pan_gesture = dragProbe;
  runtime.check("chart.pan", pannedViewport !== initialViewport, `${initialViewport} -> ${pannedViewport}`);
  const panned = await loadedMap(runtime, { afterPlanId: rasterBeforePan?.planId ?? null });
  runtime.check(
    "chart.raster-repaint",
    panned.raster.loaded > 0 && panned.raster.loaded / panned.raster.planned >= 0.85,
    JSON.stringify(panned.raster),
  );
  runtime.check("chart.vector-repaint", panned.vectors > 0, String(panned.vectors));

  const zoomBefore = viewportZoomLevel(
    await runtime.driver.readProjection("parity:viewport:"),
  );
  if (zoomBefore == null) throw new Error("chart viewport did not report its zoom level");
  const zoomedViewport = await runtime.transition("chart zoom", {
    ready: () => runtime.driver.readElement("map-surface"),
    act: (readyElement) => runtime.driver.zoom("map-surface", -420, readyElement),
    complete: async () => {
      const entries = await runtime.driver.readProjection("parity:viewport:");
      const current = idOf(entries);
      return current && viewportZoomLevel(entries) !== zoomBefore ? current : null;
    },
  });
  const zoomAfter = viewportZoomLevel([zoomedViewport]);
  runtime.check("chart.zoom", zoomAfter !== zoomBefore, `${zoomBefore} -> ${zoomAfter}`);

  await runtime.openPage("flight_plan");
  await appendRoute(runtime, "KSEA KPAE");
  await planAction(runtime, "KPAE", "activate_leg");
  await enableDeterministicOwnship(runtime);
  const following = await runtime.action("enable CTR", "center-here-button", {
    complete: async () => {
      const value = await runtime.driver.readElement("center-here-button");
      return value?.pressed === "true" || value?.selected === true || value?.checked === true
        ? value
        : null;
    },
  });
  runtime.check("chart.ctr-on", Boolean(following));
  const free = await runtime.action("disable CTR", "center-here-button", {
    complete: async () => {
      const value = await runtime.driver.readElement("center-here-button");
      return value && value.pressed !== "true" && value.selected !== true && value.checked !== true
        ? value
        : null;
    },
  });
  runtime.check("chart.ctr-off", Boolean(free));
}

function projectionId(entry) {
  return entry?.id ?? entry ?? "";
}

function projectionState(entry) {
  return entry?.state ?? entry?.text ?? "";
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
  return fetchFixtureJson(runtime, "/__health", {}, { transientNetworkErrors: true });
}

async function fixtureRequests(runtime) {
  if (!runtime.fixtureOrigin) throw new Error("journey fixture origin is unavailable");
  return fetchFixtureJson(runtime, "/__requests", {}, { transientNetworkErrors: true });
}

export function publicationCatalogRequestCount(requests) {
  return requests.filter((request) =>
    request.method === "GET" &&
    ["/packages", "/packages/", "/packages/current_artifacts.json"].includes(
      new URL(request.url, "http://fixture.invalid").pathname,
    )).length;
}

export function publicationArtifactRequestCount(requests, artifactFilename) {
  return requests.filter((request) => {
    const pathname = decodeURIComponent(
      new URL(request.url, "http://fixture.invalid").pathname,
    );
    return request.method === "GET" &&
      pathname.startsWith("/packages/") &&
      pathname.endsWith(`/${artifactFilename}`);
  }).length;
}

async function fetchFixtureJson(
  runtime,
  path,
  options = {},
  { transientNetworkErrors = false } = {},
) {
  let response;
  try {
    const headers = new Headers(options.headers);
    headers.set("connection", "close");
    response = await fetch(new URL(path, runtime.fixtureOrigin), { ...options, headers });
  } catch (error) {
    if (transientNetworkErrors) {
      throw new TransientObservationError(`${path} transport failed`, error);
    }
    throw error;
  }
  if (!response.ok) throw new Error(`${path} failed: HTTP ${response.status}`);
  return response.json();
}

function planRowUid(entry) {
  return projectionId(entry)
    .replace(/^parity:plan-row:/, "")
    .replace(/^plan-row-/, "");
}

async function planRows(runtime) {
  if (runtime.platform === "android") {
    const projection = (await runtime.driver.readProjection("parity:flight-plan-rows:"))[0];
    const labels = projectionId(projection)?.slice("parity:flight-plan-rows:".length);
    if (labels != null) return labels.split("\u001f").filter(Boolean).map((text) => ({ text }));
  }
  return runtime.driver.readProjection("parity:plan-row:");
}

async function findPlanRow(runtime, label, timeoutMs = E2E_TIMING.localReadyMs) {
  const revealed = await runtime.revealProjectionMatching(
    "parity:plan-row:", label, `flight-plan row ${label}`,
  );
  if (!revealed) throw new Error(`flight-plan row ${label} is not reachable`);
  return runtime.eventually(`flight-plan row ${label}`, async () => {
    const entry = await runtime.driver.findProjectionMatching("parity:plan-row:", label);
    return entry?.text?.split(/\s+/).includes(label) ? entry : null;
  }, timeoutMs);
}

function routeEntryState(entries) {
  const id = projectionId(entries[0]);
  const match = /can_commit:(true|false):loading:(true|false)/.exec(id ?? "");
  return match ? {
    canCommit: match[1] === "true",
    loading: match[2] === "true",
  } : null;
}

async function appendRoute(runtime, route) {
  await dismissPlanRowTray(runtime);
  await revealRequiredElement(runtime, "plan-append-route-input", "flight-plan route editor");
  await runtime.editText(`enter route ${route}`, "plan-append-route-input", route);
  const destination = route.trim().split(/\s+/).at(-1);
  await runtime.transition(`append route ${route}`, {
    ready: async () => {
      const state = routeEntryState(await runtime.driver.readProjection(
        "parity:plan-append-route-state:",
      ));
      if (!state?.canCommit || state.loading) return null;
      const input = await runtime.driver.readElement("plan-append-route-input");
      return input?.focused ? input : null;
    },
    act: (readyElement) => runtime.driver.submit("plan-append-route-input", readyElement),
    complete: async () => {
      const entry = await runtime.driver.findProjectionMatching("parity:plan-row:", destination);
      return entry?.text?.split(/\s+/).includes(destination) ? entry : null;
    },
  });
}

async function openPlanRow(runtime, label) {
  const row = await findPlanRow(runtime, label);
  await runtime.action(`open flight-plan row ${label}`, `plan-row:${planRowUid(row)}`, {
    complete: () => runtime.driver.readElement("plan-row-tray-scrim"),
  });
  return row;
}

async function findProcedureRow(runtime, procedureId) {
  return runtime.eventually(`flight-plan procedure ${procedureId}`, async () => {
    const entries = await runtime.driver.readProjection(`parity:plan-procedure-row:${procedureId}:uid:`);
    return entries[0] ?? null;
  }, E2E_TIMING.localReadyMs);
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
  await runtime.action(
    `open flight-plan procedure ${procedureId}`,
    `plan-row:${procedureRowUid(row)}`,
    {
      complete: () => runtime.driver.readElement("plan-row-tray-scrim"),
    },
  );
  return row;
}

export async function dismissPlanRowTray(runtime) {
  if (!(await runtime.driver.readElement("plan-row-tray-scrim"))) return;
  await runtime.transition("dismiss flight-plan row tray", {
    ready: () => runtime.driver.readElement("plan-row-tray-scrim"),
    act: () => runtime.driver.back(),
    complete: async () =>
      (await runtime.driver.readElement("plan-row-tray-scrim")) === null,
  });
}

async function planAction(runtime, label, actionId, { observeResult = null } = {}) {
  await openPlanRow(runtime, label);
  let completion = null;
  const trayStaysOpen = ["move_up", "move_down"].includes(actionId);
  const trayCloses = [
    "activate_leg", "direct_to", "remove", "remove_all_above",
  ].includes(actionId);
  if (trayStaysOpen || trayCloses) {
    if (trayStaysOpen && !observeResult) {
      throw new Error(`${actionId} ${label} must declare its visible row result`);
    }
    completion = await runtime.action(`${actionId} ${label}`, actionId, {
      complete: async () => {
        const open = (await runtime.driver.readProjection("parity:plan-row-action:")).length > 0;
        if (open !== trayStaysOpen) return null;
        if (!trayStaysOpen) return { open };
        return observeResult();
      },
    });
  } else {
    const completionByAction = {
      insert_before: () => runtime.driver.readElement("plan-insert-airport-input"),
      insert_after: () => runtime.driver.readElement("plan-insert-airport-input"),
      add_airway: async () =>
        (await runtime.driver.readProjection("parity:plan-airway-suggestion:"))[0] ?? null,
      select_departure: () => runtime.driver.readElement("plan-procedure-picker"),
      select_arrival: () => runtime.driver.readElement("plan-procedure-picker"),
      select_approach: () => runtime.driver.readElement("plan-procedure-picker"),
    };
    const complete = completionByAction[actionId];
    if (!complete) throw new Error(`plan action ${actionId} has no semantic completion contract`);
    await runtime.action(`${actionId} ${label}`, actionId, {
      complete: complete,
    });
  }
  if (trayStaysOpen) {
    await dismissPlanRowTray(runtime);
  }
  return completion;
}

async function planState(runtime) {
  return projectionId((await runtime.driver.readProjection("parity:plan-state:"))[0]);
}

function planStateRowCount(state) {
  const match = /:rows:(\d+):/.exec(state ?? "");
  return match ? Number(match[1]) : null;
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
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("flight_plan");

  await runtime.editText(
    "enter invalid route KRNT V2 ZZZZZ",
    "plan-append-route-input",
    "KRNT V2 ZZZZZ",
  );
  const invalidFeedback = await runtime.eventually("invalid route feedback", async () => {
    const feedback = await runtime.driver.readElement("plan-append-route-feedback");
    return feedback?.text && !/^Checking/i.test(feedback.text) ? feedback : null;
  }, E2E_TIMING.localReadyMs);
  runtime.check("plan.route-invalid", Boolean(invalidFeedback?.text), invalidFeedback?.text);

  await appendRoute(runtime, "KSEA KBFI KRNT KPAE");
  runtime.check("plan.route-valid", (await planRows(runtime)).length >= 4);

  let beforeCount = (await planRows(runtime)).length;
  await planAction(runtime, "KRNT", "insert_before");
  await runtime.eventually("insert-before airport input", () => runtime.driver.readElement("plan-insert-airport-input"));
  await runtime.editText("enter KPLU insert", "plan-insert-airport-input", "KPLU");
  await runtime.eventually("KPLU insert suggestion", () => runtime.driver.readElement(
    runtime.platform === "web" ? "plan-insert-suggestion-KPLU" : "plan-insert-suggestion:KPLU",
  ));
  await runtime.action("insert KPLU", "plan-insert-suggestion:KPLU", {
    complete: () => runtime.driver.findProjectionMatching("parity:plan-row:", "KPLU"),
  });
  runtime.check("plan.insert-before", (await planRows(runtime)).length > beforeCount);

  beforeCount = (await planRows(runtime)).length;
  await planAction(runtime, "KRNT", "insert_after");
  await runtime.eventually("insert-after airport input", () => runtime.driver.readElement("plan-insert-airport-input"));
  await runtime.editText("enter S50 insert", "plan-insert-airport-input", "S50");
  await runtime.eventually("S50 insert suggestion", () => runtime.driver.readElement(
    runtime.platform === "web" ? "plan-insert-suggestion-S50" : "plan-insert-suggestion:S50",
  ));
  await runtime.action("insert S50", "plan-insert-suggestion:S50", {
    complete: () => runtime.driver.findProjectionMatching("parity:plan-row:", "S50"),
  });
  runtime.check("plan.insert-after", (await planRows(runtime)).length > beforeCount);

  const movedUp = await planAction(runtime, "S50", "move_up", {
    observeResult: async () => {
      const labels = (await planRows(runtime)).map((entry) => entry.text);
      return labels.findIndex((text) => text.includes("S50")) < labels.findIndex((text) => text.includes("KRNT"))
        ? labels
        : null;
    },
  });
  runtime.check("plan.move-up", movedUp.findIndex((text) => text.includes("S50")) < movedUp.findIndex((text) => text.includes("KRNT")));
  const movedDown = await planAction(runtime, "S50", "move_down", {
    observeResult: async () => {
      const labels = (await planRows(runtime)).map((entry) => entry.text);
      return labels.findIndex((text) => text.includes("S50")) > labels.findIndex((text) => text.includes("KRNT"))
        ? labels
        : null;
    },
  });
  runtime.check("plan.move-down", movedDown.findIndex((text) => text.includes("S50")) > movedDown.findIndex((text) => text.includes("KRNT")));

  beforeCount = (await planRows(runtime)).length;
  await planAction(runtime, "S50", "remove");
  await runtime.eventually("S50 removed", async () => !(await planRows(runtime)).some((entry) => entry.text.includes("S50")));
  runtime.check("plan.remove", (await planRows(runtime)).length < beforeCount);

  const beforeTrimRows = await planRows(runtime);
  const beforeTrimLabels = beforeTrimRows.map((entry) => entry.text);
  const trimIndex = beforeTrimLabels.findIndex((text) => text.includes("KPLU"));
  if (trimIndex < 0) throw new Error(`KPLU is absent before remove-all-above: ${beforeTrimLabels}`);
  const expectedTrimmedLabels = beforeTrimLabels.slice(trimIndex + 1);
  await planAction(runtime, "KPLU", "remove_all_above");
  const trimmedLabels = await runtime.eventually("route trimmed above KPLU", async () => {
    const labels = (await planRows(runtime)).map((entry) => entry.text);
    return labels.length === expectedTrimmedLabels.length &&
      expectedTrimmedLabels.every((label, index) => labels[index]?.includes(label))
      ? labels
      : null;
  }, E2E_TIMING.userResponseMs);
  runtime.check(
    "plan.remove-all-above",
    !trimmedLabels.some((label) => label.includes("KPLU")),
    `${beforeTrimLabels.join(" ")} -> ${trimmedLabels.join(" ")}`,
  );

  // Preserve two downstream legs so both manual sequencing controls can be
  // exercised independently after activating the KPAE leg.
  await appendRoute(runtime, "S88 KPLU");

  await runtime.openPage("flight_plan");

  await planAction(runtime, "KPAE", "activate_leg");
  const activated = await enabledPlanControl(runtime, "stop_navigation");
  runtime.check("plan.activate-leg", Boolean(activated), activated?.text);

  // Bad Autopilot intentionally becomes selectable only after core has active
  // leg geometry to fly.
  await enableDeterministicOwnship(runtime);
  await runtime.openPage("flight_plan");

  const beforeDirectTo = await planState(runtime);
  await planAction(runtime, "KPLU", "direct_to");
  await runtime.eventually("direct-to KPLU applied", async () => {
    const state = await planState(runtime);
    return state && state !== beforeDirectTo ? state : null;
  });
  // Bad Autopilot supplied the position needed to construct Direct-To, but it
  // must not keep auto-sequencing while this journey exercises manual controls.
  await selectStationaryPlanPreview(runtime);
  await runtime.openPage("flight_plan");
  const directState = await enabledPlanControl(runtime, "stop_navigation");
  runtime.check("plan.direct-to", Boolean(directState));
  const stopped = await runtime.action("stop navigation", "stop_navigation", {
    complete: async () => {
      const control = await planControl(runtime, "stop_navigation");
      return control && !control.enabled ? control : null;
    },
  });
  runtime.check("plan.stop-navigation", Boolean(stopped));

  await planAction(runtime, "KPAE", "activate_leg");

  const beforeNext = await planState(runtime);
  await runtime.action("activate next leg", "activate_next_leg", {
    complete: async () => {
      const state = await planState(runtime);
      return state && state !== beforeNext ? state : null;
    },
  });
  const activeAfterNext = await enabledPlanControl(runtime, "stop_navigation");
  runtime.check("plan.activate-next-leg", Boolean(activeAfterNext));

  const suspensionControl = "toggle_sequencing_suspension";
  const suspended = await runtime.action("suspend sequencing", suspensionControl, {
    complete: async () => {
      const control = await planControl(runtime, suspensionControl);
      return control?.pressed === "true" || control?.selected === true ? control : null;
    },
  });
  runtime.check("plan.suspend-sequencing", Boolean(suspended), suspended?.text);
  const resumed = await runtime.action("resume sequencing", suspensionControl, {
    complete: async () => {
      const control = await planControl(runtime, suspensionControl);
      return control && control.pressed !== "true" && control.selected !== true ? control : null;
    },
  });
  runtime.check("plan.unsuspend-sequencing", Boolean(resumed), resumed?.text);

  await runtime.openPage("map");
  await runtime.editText("enter S50 chart search", "chart-search-input", "S50");
  await selectChartSearchSuggestion(runtime, "S50");
  await runtime.action("direct to selected S50", "direct_to", {
    complete: async () => (await runtime.driver.readElement("map-selection-tray")) === null,
  });
  await runtime.openPage("flight_plan");
  const restored = await runtime.action("restore underlying plan", "restore_direct_to", {
    complete: async () => {
      const control = await planControl(runtime, "restore_direct_to");
      return control && !control.enabled ? control : null;
    },
  });
  runtime.check("plan.restore-direct-to", Boolean(restored));

  await runtime.openPage("map");
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

function plateChartId(entry) {
  return projectionId(entry)
    .replace(/^parity:tray-option:/, "")
    .replace(/^tray-option-/, "")
    .replace(/^parity:plate-folder-tile:/, "")
    .replace(/^plate-folder-tile:/, "");
}

async function visibleTrayOptions(runtime) {
  return runtime.driver.readProjection(runtime.platform === "web" ? "tray-option-" : "parity:tray-option:");
}

async function dismissTrayOptions(runtime, description) {
  if ((await visibleTrayOptions(runtime)).length === 0) return;
  await runtime.transition(description, {
    ready: async () => (await visibleTrayOptions(runtime))[0] ?? null,
    act: () => runtime.driver.back(),
    complete: async () => (await visibleTrayOptions(runtime)).length === 0,
  });
}

export async function selectTrayOptionMatching(runtime, launcherId, needle) {
  const selected = await runtime.driver.readElement(launcherId);
  if (selected?.text?.toUpperCase().includes(needle.toUpperCase())) {
    if (launcherId === "plate-chart-button") {
      const folderTiles = await runtime.driver.readProjection(runtime.platform === "web"
        ? "plate-folder-tile:"
        : "parity:plate-folder-tile:");
      if (folderTiles.some((entry) => entry.text?.toUpperCase().includes(needle.toUpperCase()))) {
        return selectPlateFolderTileMatching(runtime, needle);
      }
    }
    await dismissTrayOptions(runtime, `dismiss already-selected ${launcherId} options`);
    return selected;
  }

  const projection = runtime.platform === "web" ? "tray-option-" : "parity:tray-option:";
  await runtime.action(`open ${launcherId} options`, launcherId, {
    complete: async () => (await runtime.driver.readProjection(projection))[0] ?? null,
  });
  const entry = await runtime.revealProjectionMatching(
    projection, needle, `${launcherId} option ${needle}`,
  );
  if (!entry) throw new Error(`${launcherId} option matching ${needle} is not reachable`);
  const refreshed = await runtime.driver.readElement(launcherId);
  if (refreshed?.text?.toUpperCase().includes(needle.toUpperCase())) {
    await dismissTrayOptions(runtime, `dismiss already-selected ${launcherId} options`);
    return refreshed;
  }
  await runtime.action(`select ${needle} from ${launcherId}`, `tray-option:${trayOptionId(entry)}`, {
    complete: async () => {
      const launcher = await runtime.driver.readElement(launcherId);
      return launcher?.text?.toUpperCase().includes(needle.toUpperCase()) ? launcher : null;
    },
  });
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

export async function selectProcedure(runtime, {
  airportId, rowLabel = airportId, actionId, procedureId, transition = null,
}) {
  await planAction(runtime, rowLabel, actionId);
  const procedure = await runtime.eventually(`${procedureId} procedure choice`, async () => {
    const entries = await runtime.driver.readProjection("parity:plan-procedure:");
    return entries.find((entry) => procedureChoiceId(entry) === procedureId) ?? null;
  }, E2E_TIMING.localReadyMs);
  const choice = await runtime.action(
    `select ${procedureId} procedure`,
    `plan-procedure:${procedureId}`,
    {
      complete: async () => {
        const entries = await runtime.driver.readProjection("parity:plan-procedure-transition:");
        if (transition) {
          return entries.find((entry) =>
            procedureTransitionId(entry) === transition || entry.text?.includes(transition)) ?? null;
        }
        return entries.find((entry) => entry.enabled !== false) ?? null;
      },
    },
  );
  const selectedTransitionId = procedureTransitionId(choice);
  await runtime.action(
    `select ${procedureId} transition ${selectedTransitionId}`,
    `plan-procedure-transition:${selectedTransitionId}`,
    {
      complete: async () =>
        (await runtime.driver.readElement("plan-procedure-picker")) === null,
    },
  );
  return findProcedureRow(runtime, procedureId);
}

async function assertProcedurePainted(runtime, assertionId) {
  await runtime.openPage("map");
  const projection = await runtime.eventually("procedure route painted", async () => {
    const entries = await runtime.driver.readProjection("parity:flight-plan-route-overlay:");
    return entries.length > 0 ? projectionId(entries[0]) : null;
  }, E2E_TIMING.resourceMs);
  runtime.check(assertionId, Boolean(projection), projection);
  await runtime.openPage("flight_plan");
}

async function assertProcedureShowPlate(runtime, procedureId, assertionId, expectedLabel = procedureId) {
  await openProcedureRow(runtime, procedureId);
  const plate = await runtime.action(`show plate for ${procedureId}`, "show_plate", {
    complete: () => runtime.driver.readElement("page:plate"),
  });
  const chart = await runtime.eventually("selected procedure plate", async () => {
    const selected = await runtime.driver.readElement("plate-chart-button");
    return selected?.text?.toUpperCase().includes(expectedLabel.toUpperCase()) ? selected : null;
  }, E2E_TIMING.resourceMs);
  runtime.check(assertionId, Boolean(plate && chart?.text), chart?.text);
}

async function removeProcedure(runtime, procedureId, assertionId) {
  await runtime.openPage("flight_plan");
  await openProcedureRow(runtime, procedureId);
  const beforeRevision = await runtime.driver.readSessionRevision();
  await runtime.action(`remove procedure ${procedureId}`, "remove_procedure", {
    complete: async () => {
      const tray = await runtime.driver.readElement("plan-row-tray-scrim");
      const revision = await runtime.driver.readSessionRevision();
      return !tray && revision > beforeRevision ? { revision } : null;
    },
  });
  const removed = await runtime.eventually(`procedure ${procedureId} removed`, async () =>
    (await runtime.driver.readProjection(`parity:plan-procedure-row:${procedureId}:uid:`)).length === 0);
  runtime.check(assertionId, removed);
}

async function procedureDeparture(runtime) {
  const sid = runtime.capability("procedure.sid");
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("flight_plan");
  await appendRoute(runtime, `${sid.airport_id} KPAE`);
  await selectProcedure(runtime, {
    airportId: sid.airport_id,
    actionId: "select_departure",
    procedureId: sid.procedure_id,
  });
  runtime.check("procedure.sid.select", true, `${sid.airport_id} ${sid.procedure_id}`);
  await assertProcedurePainted(runtime, "procedure.sid.render");

  await openPlanRow(runtime, sid.airport_id);
  const moveDown = await runtime.eventually("departure move-down action", () =>
    runtime.driver.readElement(runtime.platform === "web"
      ? "plan-row-action-move_down"
      : "plan-row-action:move_down"));
  runtime.check("procedure.sid.invariant", Boolean(moveDown && !moveDown.enabled), moveDown?.text);
  await dismissPlanRowTray(runtime);

  await assertProcedureShowPlate(runtime, sid.procedure_id, "procedure.sid.show-plate", "BANGR");
  await removeProcedure(runtime, sid.procedure_id, "procedure.sid.remove");
}

async function procedureArrival(runtime) {
  const star = runtime.capability("procedure.star");
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("flight_plan");
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
  const moveUp = await runtime.eventually("arrival move-up action", () =>
    runtime.driver.readElement(runtime.platform === "web"
      ? "plan-row-action-move_up"
      : "plan-row-action:move_up"));
  runtime.check("procedure.star.invariant", Boolean(moveUp && !moveUp.enabled), moveUp?.text);
  await dismissPlanRowTray(runtime);

  await assertProcedureShowPlate(runtime, star.procedure_id, "procedure.star.show-plate", "CHINS");
  const selected = await runtime.driver.readElement("plate-chart-button");
  runtime.check("plate.multi-page-rotated", selected?.text?.toUpperCase().includes("CHINS"), selected?.text);
  await removeProcedure(runtime, star.procedure_id, "procedure.star.remove");
}

async function procedureApproach(runtime) {
  const approach = runtime.capability("procedure.approach");
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("flight_plan");
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

  await runtime.openPage("flight_plan");
  await selectProcedure(runtime, {
    airportId: approach.airport_id,
    actionId: "select_approach",
    procedureId: approach.procedure_id,
    transition: approach.transition,
  });
  const approachRows = await runtime.driver.readProjection(`parity:plan-procedure-row:${approach.procedure_id}:uid:`);
  runtime.check("procedure.approach.replace", approachRows.length === 1, `${approachRows.length} procedure rows`);
  await removeProcedure(runtime, approach.procedure_id, "procedure.approach.remove");

  await runtime.openPage("charts");
  await selectTrayOptionMatching(runtime, "plate-airport-button", approach.airport_id);
  await selectTrayOptionMatching(runtime, "plate-chart-button", "ILS OR LOC 32R");
  const load = await runtime.action("open plate load choices", "plate-load-button", {
    complete: async () => {
      const entries = await runtime.driver.readProjection(runtime.platform === "web" ? "tray-option-" : "parity:tray-option:");
      return entries.find((entry) => entry.enabled !== false) ?? null;
    },
  });
  await runtime.action("load procedure from plate", `tray-option:${trayOptionId(load)}`, {
    complete: async () => {
      const entries = await runtime.driver.readProjection(runtime.platform === "web" ? "tray-option-" : "parity:tray-option:");
      return entries.length === 0;
    },
  });
  await runtime.openPage("flight_plan");
  await findProcedureRow(runtime, approach.procedure_id);
  runtime.check("procedure.approach.load-from-plate", true, load.text);
}

async function plateViewport(runtime, expectedChartId = null) {
  const entries = await runtime.driver.readProjection("parity:plate-viewport:");
  const expected = expectedChartId == null ? null : `:chart:${expectedChartId}:zoom:`;
  const entry = expected == null
    ? entries[0]
    : entries.find((candidate) => projectionId(candidate).includes(expected));
  return projectionId(entry);
}

async function initializedPlateViewport(runtime, chartId, description) {
  return runtime.eventually(
    description,
    () => plateViewport(runtime, chartId),
    E2E_TIMING.localRenderMs,
  );
}

async function selectPlateFolderTileMatching(runtime, needle) {
  const entry = await runtime.eventually(`plate folder tile matching ${needle}`, async () => {
    const entries = await runtime.driver.readProjection(runtime.platform === "web"
      ? "plate-folder-tile:"
      : "parity:plate-folder-tile:");
    return entries.find((option) => option.text?.toUpperCase().includes(needle.toUpperCase())) ?? null;
  }, E2E_TIMING.localReadyMs);
  const chartId = projectionId(entry)
    .replace(/^parity:plate-folder-tile:/, "")
    .replace(/^plate-folder-tile:/, "");
  await runtime.action(`select plate ${needle}`, `plate-folder-tile:${chartId}`, {
    complete: async () => {
      const launcher = await runtime.driver.readElement("plate-chart-button");
      const folderTiles = await runtime.driver.readProjection(runtime.platform === "web"
        ? "plate-folder-tile:"
        : "parity:plate-folder-tile:");
      return launcher?.text?.toUpperCase().includes(needle.toUpperCase()) && folderTiles.length === 0
        ? launcher
        : null;
    },
  });
  return entry;
}

async function plateOperate(runtime) {
  const georef = runtime.capability("plate.georeferenced");
  const multiPage = runtime.capability("plate.multi_page_rotated");
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("flight_plan");
  // RARYO is the on-plate IAF for this approach, putting deterministic
  // ownship well inside the georeferenced image bounds immediately.
  await appendRoute(runtime, `RARYO ${georef.airport_id}`);
  await planAction(runtime, georef.airport_id, "activate_leg");

  await runtime.openPage("charts");
  const airport = await selectTrayOptionMatching(runtime, "plate-airport-button", georef.airport_id);
  runtime.check("plate.airport-selector", Boolean(airport), airport.text);
  const georefDisplayLabel = georef.label_contains
    .replace(/\s*\(GPS\)\s*/i, " ")
    .replace(/\bRWY\s+/i, "")
    .replace(/\s+/g, " ")
    .trim();
  const chart = await selectTrayOptionMatching(runtime, "plate-chart-button", georefDisplayLabel);
  runtime.check("plate.chart-selector", Boolean(chart), chart.text);
  const chartId = plateChartId(chart);

  const selected = await runtime.eventually("named georeferenced plate selected", async () => {
    const control = await runtime.driver.readElement("plate-chart-button");
    return control?.text?.toUpperCase().includes(georefDisplayLabel.toUpperCase()) ? control : null;
  });
  runtime.check("plate.named-selection", Boolean(selected), selected.text);

  const folderTile = await runtime.action("open plate folder", "plate-folder-button", {
    complete: async () => {
      const entries = await runtime.driver.readProjection(runtime.platform === "web"
        ? "plate-folder-tile:"
        : "parity:plate-folder-tile:");
      return entries.find((entry) => projectionId(entry).endsWith(chartId)) ?? null;
    },
  });
  runtime.check("plate.folder", Boolean(folderTile), projectionId(folderTile));
  await runtime.action("return to selected plate", `plate-folder-tile:${chartId}`, {
    complete: async () => {
      const entries = await runtime.driver.readProjection(runtime.platform === "web"
        ? "plate-folder-tile:"
        : "parity:plate-folder-tile:");
      return entries.length === 0;
    },
  });

  // Bad Autopilot is intentionally accelerated. Start it only after the plate
  // is selected so its first position remains inside the georeferenced image.
  await enableDeterministicOwnship(runtime);
  await runtime.openPage("charts");

  runtime.result.diagnostics.plate_ownship_input = (await runtime.driver.readProjection(
    "parity:plate-ownship-input:",
  )).map(projectionId);
  const ownship = await runtime.eventually("ownship on georeferenced plate", async () => {
    const entries = await runtime.driver.readProjection(runtime.platform === "web"
      ? "plate-ownship-overlay"
      : "parity:plate-ownship-overlay");
    return entries[0] ?? null;
  }, E2E_TIMING.userResponseMs);
  runtime.check("plate.georeferenced-ownship", Boolean(ownship));

  const initialViewport = await initializedPlateViewport(
    runtime,
    chartId,
    "initialized selected plate viewport",
  );
  const pannedViewport = await runtime.transition("pan georeferenced plate", {
    ready: () => runtime.driver.readElement("plate-surface"),
    act: (readyElement) => runtime.driver.drag("plate-surface", { x: -120, y: -100 }, readyElement),
    complete: async () => {
      const value = await plateViewport(runtime);
      return value && value !== initialViewport ? value : null;
    },
  });
  runtime.check("plate.pan", Boolean(pannedViewport), `${initialViewport} -> ${pannedViewport}`);
  const zoomedViewport = await runtime.transition("zoom georeferenced plate", {
    ready: () => runtime.driver.readElement("plate-surface"),
    act: (readyElement) => runtime.driver.zoom("plate-surface", -360, readyElement),
    complete: async () => {
      const value = await plateViewport(runtime);
      return value && value !== pannedViewport ? value : null;
    },
  });
  runtime.check("plate.zoom", Boolean(zoomedViewport), `${pannedViewport} -> ${zoomedViewport}`);

  const loadOption = await runtime.action("open plate procedure load choices", "plate-load-button", {
    complete: async () => {
      const entries = await runtime.driver.readProjection(runtime.platform === "web" ? "tray-option-" : "parity:tray-option:");
      return entries.find((entry) => entry.enabled !== false) ?? null;
    },
  });
  await runtime.action("load selected plate procedure", `tray-option:${trayOptionId(loadOption)}`, {
    complete: async () => {
      const options = await runtime.driver.readProjection(
        runtime.platform === "web" ? "tray-option-" : "parity:tray-option:",
      );
      return options.length === 0;
    },
  });
  runtime.check("plate.load-procedure", Boolean(loadOption), loadOption.text);

  if (multiPage.airport_id !== georef.airport_id) {
    await selectTrayOptionMatching(runtime, "plate-airport-button", multiPage.airport_id);
  }
  const multi = await selectTrayOptionMatching(runtime, "plate-chart-button", multiPage.label_contains);
  const multiId = plateChartId(multi);
  const firstPageViewport = await initializedPlateViewport(
    runtime,
    multiId,
    "initialized selected multi-page plate viewport",
  );
  const lastPageViewport = await runtime.transition("scroll multi-page plate", {
    ready: () => runtime.driver.readElement("plate-surface"),
    act: (readyElement) => runtime.driver.drag("plate-surface", { x: 0, y: -600 }, readyElement),
    complete: async () => {
      const value = await plateViewport(runtime);
      return value && value !== firstPageViewport ? value : null;
    },
  });
  runtime.check("plate.first-last-page", Boolean(lastPageViewport), `${firstPageViewport} -> ${lastPageViewport}`);

  await runtime.action("open multi-page plate folder", "plate-folder-button", {
    complete: async () => {
      const entries = await runtime.driver.readProjection(runtime.platform === "web"
        ? "plate-folder-tile:"
        : "parity:plate-folder-tile:");
      return entries.find((entry) => projectionId(entry).endsWith(multiId)) ?? null;
    },
  });
  const returned = await runtime.action("return to multi-page plate", `plate-folder-tile:${multiId}`, {
    complete: async () => {
      const control = await runtime.driver.readElement("plate-chart-button");
      const folderTiles = await runtime.driver.readProjection(runtime.platform === "web"
        ? "plate-folder-tile:"
        : "parity:plate-folder-tile:");
      return control?.text?.toUpperCase().includes(multiPage.label_contains.toUpperCase()) && folderTiles.length === 0
        ? control
        : null;
    },
  });
  runtime.check("plate.return-folder", Boolean(returned), returned.text);
}

async function plateAdvisoriesAndReferences(runtime) {
  const notam = runtime.capability("plate.notam");
  const warning = runtime.capability("plate.geometry_warning");
  const legend = runtime.capability("plate.legend");
  const inset = runtime.capability("plate.inset");
  await runtime.reset();
  await acceptDisclaimer(runtime);

  await selectAirportFromMapSearch(runtime, warning.airport_id);
  await runtime.action("open selected airport plates", "plates", {
    complete: () => runtime.driver.readElement("page:plate"),
  });
  await selectPlateFolderTileMatching(runtime, warning.label_contains);
  const warningLauncher = await runtime.eventually("plate procedure geometry warning", () =>
    runtime.driver.readElement("procedure-status-launcher"), E2E_TIMING.resourceMs);
  const warningPanel = await runtime.action(
    "open plate procedure geometry detail",
    "procedure-status-launcher",
    { complete: () => runtime.driver.readElement("procedure-status-panel") },
  );
  runtime.check(
    "plate.geometry-warning",
    Boolean(warningLauncher && warningPanel?.text?.includes("This publication")),
    warningPanel?.text,
  );
  await runtime.transition("dismiss plate geometry detail", {
    ready: () => runtime.driver.readElement("procedure-status-panel"),
    act: () => runtime.driver.back(),
    complete: async () => (await runtime.driver.readElement("procedure-status-panel")) ? null : true,
  });

  if (notam.airport_id !== warning.airport_id || notam.label_contains !== warning.label_contains) {
    await selectAirportFromMapSearch(runtime, notam.airport_id);
    await runtime.action("open NOTAM airport plates", "plates", {
      complete: () => runtime.driver.readElement("page:plate"),
    });
    await selectPlateFolderTileMatching(runtime, notam.label_contains);
  }
  const notamBadge = await runtime.eventually("plate procedure NOTAM badge", async () => {
    const entries = await runtime.driver.readProjection(
      runtime.platform === "web" ? "plate-notam:" : "parity:plate-notam:",
    );
    return entries[0] ?? null;
  }, E2E_TIMING.resourceMs);
  const notamModal = await runtime.action("open plate NOTAM detail", projectionId(notamBadge), {
    complete: () => runtime.driver.readElement("procedure-notam-modal"),
  });
  runtime.check(
    "plate.notam",
    Boolean(notamModal?.text && /NOTAM/i.test(notamModal.text)),
    notamModal?.text,
  );
  await runtime.transition("dismiss plate NOTAM detail", {
    ready: () => runtime.driver.readElement("procedure-notam-modal"),
    act: () => runtime.driver.back(),
    complete: async () => (await runtime.driver.readElement("procedure-notam-modal")) ? null : true,
  });

  await runtime.openPage("map");
  await ensureMapFamily(
    runtime,
    legend.family_id,
    "select TAC family for references",
  );
  await selectAirportFromMapSearch(runtime, inset.map_airport_id ?? "KSEA");
  await runtime.transition("dismiss contextual TAC inspector", {
    ready: () => runtime.driver.readElement("map-selection-tray"),
    act: () => runtime.driver.back(),
    complete: async () => (await runtime.driver.readElement("map-selection-tray")) ? null : true,
  });
  await runtime.eventually("contextual TAC raster plan", async () => {
    const [entry] = await runtime.driver.readProjection("parity:raster-state:");
    return (rasterStateFromProjection([entry])?.planned ?? 0) > 0 ? entry : null;
  }, E2E_TIMING.resourceMs);
  runtime.result.diagnostics.chart_reference_raster_state =
    await runtime.driver.readProjection("parity:raster-state:");
  runtime.result.diagnostics.chart_reference_controls =
    await runtime.driver.readProjection("tray-option-accessory-");
  const accessoryAction = runtime.platform === "web"
    ? "tray-option-accessory-tac"
    : "chart-reference-button";
  await runtime.action("open TAC family choices", "chart-family-button", {
    complete: () => runtime.driver.readElement(accessoryAction),
  });
  await runtime.action("open TAC reference document", accessoryAction, {
    complete: () => runtime.driver.readElement("page:plate"),
  });

  const legendOption = await selectTrayOptionMatching(runtime, "plate-chart-button", legend.label_contains);
  runtime.check("plate.legend", Boolean(legendOption), legendOption.text);
  const legendChartId = plateChartId(legendOption);
  const legendViewport = await initializedPlateViewport(
    runtime,
    legendChartId,
    "initialized selected legend viewport",
  );
  const zoomedLegendViewport = await runtime.transition("zoom legend for composite scroll", {
    ready: () => runtime.driver.readElement("plate-surface"),
    act: (readyElement) => runtime.driver.zoom("plate-surface", -360, readyElement),
    complete: async () => {
      const value = await plateViewport(runtime);
      return value && value !== legendViewport ? value : null;
    },
  });
  const scrolledLegend = await runtime.transition("scroll legend composite", {
    ready: () => runtime.driver.readElement("plate-surface"),
    act: (readyElement) => runtime.driver.drag("plate-surface", { x: 0, y: -600 }, readyElement),
    complete: async () => {
      const value = await plateViewport(runtime);
      return value && value !== zoomedLegendViewport ? value : null;
    },
  });
  runtime.check("plate.composite-scroll", Boolean(scrolledLegend), `${zoomedLegendViewport} -> ${scrolledLegend}`);

  const insetOption = await selectTrayOptionMatching(runtime, "plate-chart-button", inset.label_contains);
  runtime.check("plate.inset", Boolean(insetOption), insetOption.text);
}

async function otherDocuments(runtime) {
  const csup = runtime.capability("document.csup");
  const other = runtime.capability("document.other");
  await runtime.reset();
  await acceptDisclaimer(runtime);

  await selectAirportFromMapSearch(runtime, csup.airport_id);
  await runtime.action("open airport chart supplement", "csup", {
    complete: () => runtime.driver.readElement("page:plate"),
  });
  const csupSelection = await runtime.eventually("airport CSUP selected", async () => {
    const control = await runtime.driver.readElement("plate-chart-button");
    return /CSUP|CHART SUPPLEMENT/i.test(control?.text ?? "") ? control : null;
  }, E2E_TIMING.localReadyMs);
  const csupViewport = await runtime.eventually(
    "CSUP document painted",
    () => plateViewport(runtime),
    E2E_TIMING.resourceMs,
  );
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
    E2E_TIMING.resourceMs,
  );
  runtime.check(
    "plate.other-document",
    Boolean(otherSelection && otherViewport),
    otherSelection?.text,
  );
}

async function aboutAndSavedState(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("home");

  if (runtime.platform === "web") {
    await runtime.openPage("about");
  } else {
    await runtime.action("open About destination", "home-button:About", {
      complete: () => runtime.driver.readElement("external-page:about"),
    });
  }
  const about = await runtime.eventually("About destination", () =>
    runtime.driver.readElement(runtime.platform === "web" ? "page:about" : "external-page:about"),
  );
  runtime.check("navigation.about", Boolean(about), about?.text);
  if (runtime.platform === "android") {
    await runtime.transition("return from external About page", {
      ready: () => runtime.driver.readElement("external-page:about"),
      act: () => runtime.driver.back(),
      complete: async () => {
        const entries = await runtime.driver.readProjection("parity:startup-state:");
        return taggedFields(entries[0], "parity:startup-state:");
      },
    });
  }

  // Begin the persistence check from a clean operational app. Settings is not
  // the default page, so seeing it after a process/browser reload proves that
  // the user's last page survived rather than merely matching startup policy.
  await runtime.reset("app.reset.saved-state");
  await acceptDisclaimer(runtime);
  await runtime.openPage("settings");
  await runtime.eventually("Settings selected before restart", () =>
    runtime.driver.readElement("page:settings"));
  await runtime.eventually("Settings page persisted before restart", async () => {
    const entries = await runtime.driver.readProjection("parity:startup-state:");
    const fields = taggedFields(entries[0], "parity:startup-state:");
    return fields?.persisted_page === (runtime.platform === "web" ? "settings" : "Settings")
      ? fields
      : null;
  }, E2E_TIMING.localReadyMs);
  await runtime.reload("app.reload.saved-state");
  const restored = await runtime.eventually("saved Settings page after restart", () =>
    runtime.driver.readElement("page:settings"), E2E_TIMING.startupMs);
  runtime.check("saved-state.restart", Boolean(restored), "Settings page restored after restart");
}

async function pointerDetails(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("map");
  await loadedMap(runtime);
  await setLayerVisible(runtime, "metars", true);
  const stationId = runtime.fixture?.capabilities?.live_feeds?.pirep_target_airport ?? "KSEA";
  await selectAirportFromMapSearch(runtime, stationId);
  await runtime.transition("dismiss hover-target airport inspector", {
    ready: () => runtime.driver.readElement("map-selection-tray"),
    act: () => runtime.driver.back(),
    complete: async () => (await runtime.driver.readElement("map-selection-tray")) ? null : true,
  });
  const target = await runtime.eventually("visible METAR hover target", async () => {
    const id = `parity:metar-hover-target:${stationId}`;
    return (await runtime.driver.readElement(id)) ? { id } : null;
  }, E2E_TIMING.resourceMs);
  const weather = await runtime.transition("hover METAR target", {
    ready: () => runtime.driver.readElement(projectionId(target)),
    act: () => runtime.driver.hover(projectionId(target)),
    complete: () => runtime.driver.readElement("weather-detail-modal"),
  });
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
    await runtime.resetApplicationDataExpectingStartupFailure("app.reset-unsupported-contract");
    const failure = await runtime.eventually("unsupported publication failure", async () => {
      const panel = await runtime.driver.readElement(
        runtime.platform === "web" ? "startup-fatal-error" : "offline-library-panel",
      );
      return panel?.text && /unsupported|no manifest supported/i.test(panel.text) ? panel : null;
    }, E2E_TIMING.startupMs);
    runtime.check(
      "startup.unsupported-contract",
      Boolean(failure?.text && /unsupported|no manifest supported/i.test(failure.text)),
      failure?.text,
    );
  } finally {
    await setFixtureControl(runtime, { publication: "primary", artifact_fault: "none" });
  }
}

async function waitForOfflineSyncIdle(
  runtime,
  description,
  timeoutMs = E2E_TIMING.bulkOperationMs,
) {
  return runtime.eventually(description, async () => {
    const button = await runtime.driver.readElement("offline-sync-button");
    return offlineSyncButtonIsIdle(button) ? button : null;
  }, timeoutMs, 500);
}

async function androidPackageMaintenance(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("settings");

  await revealRequiredElement(
    runtime, "parity:settings-slider:display_dim_timeout:2m", "display dim slider",
  );
  const dimmed = await runtime.transition("change display dim timeout", {
    ready: () => runtime.driver.readElement("parity:settings-slider:display_dim_timeout:2m"),
    act: (readyElement) => runtime.driver.drag(
      "settings-slider:display_dim_timeout:2m", { x: -1_000, y: 0 }, readyElement,
    ),
    complete: () => runtime.driver.readElement("parity:settings-slider:display_dim_timeout:10s"),
  });
  runtime.check("settings.display-dim-timeout", Boolean(dimmed), dimmed?.test_id);

  await revealRequiredElement(
    runtime, "parity:settings-slider:inactivity_sleep_timeout:1h", "inactivity sleep slider",
  );
  const sleeps = await runtime.transition("change inactivity sleep timeout", {
    ready: () => runtime.driver.readElement("parity:settings-slider:inactivity_sleep_timeout:1h"),
    act: (readyElement) => runtime.driver.drag(
      "settings-slider:inactivity_sleep_timeout:1h", { x: -1_000, y: 0 }, readyElement,
    ),
    complete: () => runtime.driver.readElement("parity:settings-slider:inactivity_sleep_timeout:30m"),
  });
  runtime.check("settings.inactivity-sleep-timeout", Boolean(sleeps), sleeps?.test_id);

  await runtime.openPage("offline_packages");
  await waitForOfflineSyncIdle(runtime, "initial package plan ready");
  const updated = await setFixtureControl(runtime, {
    publication: "updated",
    artifact_fault: "drop",
  });
  try {
    const catalogRequestsBeforeRefresh = publicationCatalogRequestCount(
      await fixtureRequests(runtime),
    );
    await runtime.action("refresh offline package catalog", "offline-refresh-button", {
      complete: async () => {
        const requests = await fixtureRequests(runtime);
        return publicationCatalogRequestCount(requests) > catalogRequestsBeforeRefresh
          ? requests
          : null;
      },
    });
    await waitForOfflineSyncIdle(runtime, "updated package plan ready");
    const interruptedRequestsBefore = publicationArtifactRequestCount(
      await fixtureRequests(runtime),
      updated.updated_artifact_filename,
    );
    await runtime.action("start interrupted offline package sync", "offline-sync-button", {
      complete: async () => {
        const requests = await fixtureRequests(runtime);
        return publicationArtifactRequestCount(requests, updated.updated_artifact_filename) >
            interruptedRequestsBefore
          ? requests
          : null;
      },
    });
    await runtime.eventually("interrupted package request observed", async () => {
      const health = await fixtureHealth(runtime);
      return health.control?.dropped_artifact_requests > 0 ? health : null;
    }, E2E_TIMING.resourceMs, 250);
    const recovered = await waitForOfflineSyncIdle(runtime, "failed package sync returned to idle");
    runtime.check(
      "offline.interrupted-sync",
      Boolean(recovered),
      "truncated artifact transfer failed closed and returned the planner to idle",
    );

    await setFixtureControl(runtime, { artifact_fault: "none" });
    const successfulRequestsBefore = publicationArtifactRequestCount(
      await fixtureRequests(runtime),
      updated.updated_artifact_filename,
    );
    await runtime.action("start successful offline package sync", "offline-sync-button", {
      complete: async () => {
        const requests = await fixtureRequests(runtime);
        return publicationArtifactRequestCount(requests, updated.updated_artifact_filename) >
            successfulRequestsBefore
          ? requests
          : null;
      },
    });
    const installed = await runtime.eventually("updated package installed", () =>
      runtime.driver.readElement(`installed-package:${updated.updated_artifact_filename}`),
    E2E_TIMING.bulkOperationMs, 500);
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

function mixedStatusSeverity(platform, rowId) {
  // Android deliberately keeps the full winds package on demand, while web's
  // JIT NavKv path loads the current forecast directly. Both statuses are
  // core-owned consequences of those configured acquisition policies.
  if (platform === "android" && rowId === "live_feed:winds-aloft") return "info";
  return MIXED_STATUS_SEVERITIES[rowId];
}

async function statusAndSettings(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("settings");

  const flightDataChoices = await runtime.driver.readProjection("settings-choice-flight_data_visibility");
  const firstFlightDataChoice = flightDataChoices[0] ?? null;
  runtime.check("settings.flight-data-visibility", Boolean(firstFlightDataChoice));
  if (firstFlightDataChoice) {
    const actionId = projectionId(firstFlightDataChoice).replace(/^parity:/, "");
    await runtime.action("change flight data visibility", actionId, {
      complete: async () => {
        const choices = await runtime.driver.readProjection("settings-choice-flight_data_visibility");
        const changed = choices.find((choice) =>
          projectionId(choice) === projectionId(firstFlightDataChoice)
          && (choice.checked !== firstFlightDataChoice.checked
            || choice.selected !== firstFlightDataChoice.selected
            || choice.pressed !== firstFlightDataChoice.pressed));
        return changed ?? null;
      },
    });
  }

  const debugSection = await revealRequiredElement(
    runtime, "settings-section-debug_diagnostics", "Debug Diagnostics settings section",
  );
  runtime.check("settings.debug-folded", debugSection?.expanded === false, debugSection?.test_id);
  await runtime.action("expand Debug Diagnostics", "settings-section-debug_diagnostics", {
    complete: async () => {
      const section = await runtime.driver.readElement("settings-section-debug_diagnostics");
      return section?.expanded === true ? section : null;
    },
  });
  let firstDebugToggle = true;
  for (const [flagId, assertionId] of Object.entries(DEBUG_ASSERTIONS)) {
    const actionId = `settings-toggle-debug_${flagId}`;
    const before = await revealRequiredElement(runtime, actionId, `${flagId} debug setting`);
    const after = await runtime.action(`${flagId} debug setting changed`, actionId, {
      complete: async () => {
        const value = await runtime.driver.readElement(actionId);
        return value && value.checked !== before.checked ? value : null;
      },
    });
    runtime.check(assertionId, Boolean(after));
    if (firstDebugToggle) {
      runtime.check("settings.debug-toggle", Boolean(after));
      firstDebugToggle = false;
    }
  }

  await runtime.openPage("data_status");
  const statusRows = await runtime.eventually("expected data status projection", async () => {
    const rows = await runtime.driver.readProjection("parity:data-status-row:");
    runtime.result.diagnostics.data_status_wait_rows = rows.map(projectionId);
    const complete = Object.keys(STATUS_ASSERTIONS).every((rowId) => {
      const severity = mixedStatusSeverity(runtime.platform, rowId);
      return rows.some((row) => {
        const id = projectionId(row);
        return id.startsWith(`parity:data-status-row:${rowId}:`) &&
          (!severity || id.endsWith(`:severity:${severity}`));
      });
    });
    return complete ? rows : null;
  }, E2E_TIMING.resourceMs);
  runtime.check("status.all-rows", statusRows.length >= Object.keys(STATUS_ASSERTIONS).length, `${statusRows.length} rows`);
  const statusIds = statusRows.map(projectionId);
  for (const [rowId, assertionId] of Object.entries(STATUS_ASSERTIONS)) {
    const row = statusRows.find((entry) => projectionId(entry).startsWith(`parity:data-status-row:${rowId}:`));
    const expectedSeverity = mixedStatusSeverity(runtime.platform, rowId);
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
    Object.keys(MIXED_STATUS_SEVERITIES).every((rowId) => {
      const severity = mixedStatusSeverity(runtime.platform, rowId);
      return statusIds.some((id) =>
        id.startsWith(`parity:data-status-row:${rowId}:`) && id.endsWith(`:severity:${severity}`));
    }),
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

async function selectedMapFamily(runtime, familyId) {
  const selected = (await runtime.driver.readProjection("parity:map-family:"))[0] ?? null;
  return projectionId(selected)?.startsWith(`parity:map-family:${familyId}:`) ? selected : null;
}

async function ensureMapFamily(runtime, familyId, description) {
  const current = await selectedMapFamily(runtime, familyId);
  if (current) return current;
  return runtime.chooseOption(description, "chart-family-button", familyId, {
    complete: () => selectedMapFamily(runtime, familyId),
  });
}

async function setLayerVisible(runtime, layerId, visible) {
  const probeName = layerProbeId(runtime, layerId);
  const current = await runtime.eventually(`${layerId} layer state`, async () => {
    const id = projectionId((await runtime.driver.readProjection(`parity:map-layer:${probeName}:`))[0]);
    return id ? { id, visible: /:visible:true:/.test(id), enabled: /:enabled:true$/.test(id) } : null;
  });
  if (current.visible === visible) return current.id;
  if (!current.enabled) throw new Error(`${layerId} layer is disabled: ${current.id}`);
  await runtime.toggleOption(
    `${layerId} layer ${visible ? "visible" : "hidden"}`,
    "layers-button",
    layerId,
    visible,
  );
  await dismissTrayOptions(runtime, `dismiss ${layerId} layer choices`);
  return runtime.eventually(`${layerId} map projection`, async () => {
    const id = projectionId((await runtime.driver.readProjection(`parity:map-layer:${probeName}:`))[0]);
    return id && /:visible:true:/.test(id) === visible ? id : null;
  });
}

async function mapModesAndOverlays(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("settings");
  const debugSection = await revealRequiredElement(
    runtime, "settings-section-debug_diagnostics", "Debug Diagnostics settings section",
  );
  if (debugSection.expanded !== true) {
    await runtime.action("expand Debug Diagnostics for map modes", "settings-section-debug_diagnostics", {
      complete: async () => {
        const section = await runtime.driver.readElement("settings-section-debug_diagnostics");
        return section?.expanded === true ? section : null;
      },
    });
  }
  const adsb = await revealRequiredElement(
    runtime, "settings-toggle-debug_internet_adsb", "internet ADS-B setting",
  );
  if (!adsb.checked) {
    await runtime.action("enable internet ADS-B", "settings-toggle-debug_internet_adsb", {
      complete: async () => {
        const value = await runtime.driver.readElement("settings-toggle-debug_internet_adsb");
        return value?.checked ? value : null;
      },
    });
  }
  await runtime.openPage("map");

  const rasterFamilies = runtime.capability("raster_families");
  let initiallySelectedFamily = null;
  for (const familyId of rasterFamilies) {
    if (await selectedMapFamily(runtime, familyId)) {
      initiallySelectedFamily = familyId;
      break;
    }
  }
  const orderedFamilies = initiallySelectedFamily
    ? [...rasterFamilies.filter((familyId) => familyId !== initiallySelectedFamily), initiallySelectedFamily]
    : rasterFamilies;
  for (const familyId of orderedFamilies) {
    const previousRaster = rasterStateFromProjection(
      await runtime.driver.readProjection("parity:raster-state:"),
    );
    const selected = await runtime.chooseOption(
      `${familyId} raster family selected`,
      "chart-family-button",
      familyId,
      {
      complete: async () => {
        return selectedMapFamily(runtime, familyId);
      },
      },
    );
    await dismissTrayOptions(runtime, `dismiss ${familyId} raster family choices`);
    if (familyId === "none") {
      const empty = await runtime.eventually("empty raster plan", async () => {
        const counts = rasterStateFromProjection(
          await runtime.driver.readProjection("parity:raster-state:"),
        );
        return counts?.planId !== previousRaster?.planId && counts?.planned === 0 ? counts : null;
      }, E2E_TIMING.localRenderMs);
      runtime.check(RASTER_ASSERTIONS[familyId], Boolean(selected && empty), JSON.stringify(empty));
    } else {
      const paint = await loadedMap(runtime, { afterPlanId: previousRaster?.planId ?? null });
      runtime.check(RASTER_ASSERTIONS[familyId], Boolean(selected && paint.raster.failed === 0), JSON.stringify(paint.raster));
    }
  }

  const changedLayers = [];
  for (const [layerId, assertionId] of Object.entries(LAYER_ASSERTIONS)) {
    const probeName = layerProbeId(runtime, layerId);
    const before = projectionId((await runtime.driver.readProjection(`parity:map-layer:${probeName}:`))[0]);
    const option = await runtime.openOption(
      `${layerId} layer option`,
      "layers-button",
      layerId,
    );
    // The rendered toggle is the authoritative current state. Some layers do
    // not emit a map projection while hidden, so absence is not equivalent to
    // an unchecked option.
    const beforeVisible = selectedSemantic(option);
    runtime.result.diagnostics[`layer_${layerId}`] = { before, option };
    if (option.enabled) {
      const control = await runtime.toggleOption(
        `toggle ${layerId} layer`,
        "layers-button",
        layerId,
        !beforeVisible,
      );
      changedLayers.push({ layerId, assertionId, probeName, beforeVisible, control });
    } else {
      runtime.check(assertionId, Boolean(option.text), `${option.text} (disabled with reason)`);
    }
  }
  await dismissTrayOptions(runtime, "dismiss layer choices");
  for (const { layerId, assertionId, probeName, beforeVisible, control } of changedLayers) {
    const changed = await runtime.eventually(`${layerId} map projection changed`, async () => {
      const id = projectionId((await runtime.driver.readProjection(
        `parity:map-layer:${probeName}:`,
      ))[0]);
      return id && /:visible:true:/.test(id) !== beforeVisible ? id : null;
    });
    runtime.check(
      assertionId,
      Boolean(control && changed),
      `${projectionId(control)} -> ${projectionId(changed)}`,
    );
  }

  const north = await runtime.driver.readElement("map-orientation-button");
  runtime.check("map.n-up", north?.pressed !== "true", north?.text);
  await loadReplayFixture(runtime);
  await setReplayRate(runtime, 0.25);
  const track = await runtime.action("select track-up orientation", "map-orientation-button", {
    complete: async () => {
      const button = await runtime.driver.readElement("map-orientation-button");
      return button?.pressed === "true" || button?.selected === true ? button : null;
    },
  });
  await runtime.action("start map-mode replay", "playback-play-toggle", {
    complete: async () => {
      const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
      return state?.status === "playing" ? state : null;
    },
  });
  const trackViewport = await runtime.eventually("rotated track-up viewport", async () => {
    const id = projectionId((await runtime.driver.readProjection("parity:viewport:"))[0]);
    const up = Number(/:up:(-?[0-9.]+)/.exec(id)?.[1] ?? 0);
    return Math.abs(up) > 1 ? id : null;
  });
  runtime.check("map.trk-up", Boolean(track && trackViewport), trackViewport);
  const gapViewport = await runtime.eventually("map missing-track sample", async () => {
    const playback = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    if (playback?.status !== "playing") return null;
    const state = ownshipState(await runtime.driver.readProjection("parity:ownship-state:"));
    if (state?.track !== "none") return null;
    const viewport = idOf(await runtime.driver.readProjection("parity:viewport:"));
    return viewport ? { state, viewport } : null;
  }, E2E_TIMING.replayProgressMs, 40);
  await runtime.action("pause map-mode replay", "playback-play-toggle", {
    complete: async () => {
      const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
      return state?.status === "paused" ? state : null;
    },
  });
  const pausedViewport = projectionId((await runtime.driver.readProjection("parity:viewport:"))[0]);
  await runtime.openPage("flight_plan");
  await runtime.openPage("map");
  const heldViewport = projectionId((await runtime.driver.readProjection("parity:viewport:"))[0]);
  const gapUp = /:up:(-?[0-9.]+)/.exec(gapViewport.viewport)?.[1];
  const pausedUp = /:up:(-?[0-9.]+)/.exec(pausedViewport)?.[1];
  const heldUp = /:up:(-?[0-9.]+)/.exec(heldViewport)?.[1];
  runtime.check(
    "map.track-gap",
    gapUp !== undefined && Number(gapUp) !== 0 && pausedUp !== undefined && pausedUp === heldUp,
    `gap ${gapViewport.viewport}; paused ${pausedViewport}; returned ${heldViewport}`,
  );

  const warning = await openAndDismissDataStatus(runtime);
  runtime.check("map.warning", Boolean(warning), warning?.text);

  await runtime.openPage("map");
  await ensureMapFamily(runtime, "tac", "select TAC raster family");
  await dismissTrayOptions(runtime, "dismiss TAC raster family choices");
  const referenceAction = runtime.platform === "web"
    ? "tray-option-accessory-tac"
    : "chart-reference-button";
  const reference = await runtime.action("open TAC chart reference choices", "chart-family-button", {
    complete: () => runtime.driver.readElement(referenceAction),
  });
  const plate = reference ? await runtime.action("open TAC chart reference", referenceAction, {
    complete: () => runtime.driver.readElement("page:plate"),
  }) : null;
  runtime.check("map.chart-reference", Boolean(reference && plate));
}

export async function openAndDismissDataStatus(runtime) {
  const warning = await runtime.driver.readElement("data-status-launcher");
  if (!warning) return null;

  await runtime.action("open data status popup", "data-status-launcher", {
    complete: () => runtime.driver.readElement("data-status-panel"),
  });
  await runtime.transition("dismiss data status popup", {
    ready: () => runtime.driver.readElement("data-status-panel"),
    act: () => runtime.driver.back(),
    complete: async () => (await runtime.driver.readElement("data-status-panel")) ? null : true,
  });
  return warning;
}

async function selectAirportFromMapSearch(runtime, airportId) {
  const suggestionId = runtime.platform === "android"
    ? `chart-search-suggestion:${airportId}`
    : `chart-search-suggestion-${airportId}`;
  await runtime.openPage("map");
  await runtime.editText(
    `enter ${airportId} chart search`,
    "chart-search-input",
    airportId,
  );
  return runtime.action(`select ${airportId} chart search result`, suggestionId, {
    complete: async () => {
      const entries = await runtime.driver.readProjection(`parity:map-selection-selected:${airportId}`);
      return entries[0] ?? null;
    },
  });
}

export async function waitForMapSelectionAction(runtime, actionId, description) {
  const testId = runtime.platform === "web"
    ? `map-selection-action-${actionId}`
    : `map-selection-action:${actionId}`;
  return runtime.eventually(
    description ?? `map selection ${actionId} action`,
    () => runtime.driver.readElement(testId),
  );
}

export async function selectTfrFromPreparedMap(runtime, airportId) {
  // Searching establishes the target viewport. Do not ask the resulting
  // selection snapshot about TFRs until the asynchronous overlay for that
  // viewport is present; snapshots intentionally do not mutate underneath an
  // open inspector.
  await selectAirportFromMapSearch(runtime, airportId);
  await runtime.transition("dismiss TFR target inspector", {
    ready: () => runtime.driver.readElement("map-selection-tray"),
    act: () => runtime.driver.back(),
    complete: async () => (await runtime.driver.readElement("map-selection-tray")) ? null : true,
  });
  const overlay = await runtime.eventually("TFR overlay in target viewport", async () => {
    const state = liveOverlayState(await runtime.driver.readProjection("parity:live-overlay:"));
    return state?.tfrs > 0 ? state : null;
  }, E2E_TIMING.externalConsistencyMs);
  const selection = await selectAirportFromMapSearch(runtime, airportId);
  return { overlay, selection };
}

async function openAirportInfo(runtime, airportId) {
  await selectAirportFromMapSearch(runtime, airportId);
  return runtime.action(`${airportId} airport info`, "airport_info", {
    complete: () => runtime.driver.readModal(`airport-info-modal:${airportId}`),
  });
}

async function closeMapDetail(runtime, modalId) {
  await runtime.transition(`dismiss ${modalId}`, {
    ready: () => runtime.driver.readModal(modalId),
    act: () => runtime.driver.back(),
    complete: async () => (await runtime.driver.readModal(modalId)) ? null : true,
  });
  if (await runtime.driver.readElement("map-selection-tray")) {
    await runtime.transition("dismiss map detail inspector", {
      ready: () => runtime.driver.readElement("map-selection-tray"),
      act: () => runtime.driver.back(),
      complete: async () => (await runtime.driver.readElement("map-selection-tray")) ? null : true,
    });
  }
}

async function airportInfo(runtime) {
  const complexAirport = runtime.capability("airport.runway_complex");
  const fallbackAirport = runtime.capability("airport.runway_fallback");
  const publishedTpaAirport = runtime.capability("airport.published_tpa");
  const derivedTpaAirport = runtime.capability("airport.derived_tpa");
  await runtime.reset();
  await acceptDisclaimer(runtime);

  await openAirportInfo(runtime, complexAirport);
  const beforeTime = (await runtime.driver.readElement("airport-info-time-toggle"))?.text;
  const afterTime = await runtime.action("change airport time mode", "airport-info-time-toggle", {
    complete: async () => {
      const text = (await runtime.driver.readElement("airport-info-time-toggle"))?.text;
      return text && text !== beforeTime ? text : null;
    },
  });
  runtime.check("airport-info.time-toggle", Boolean(afterTime), `${beforeTime} -> ${afterTime}`);
  const initialScroll = await runtime.stable("settled airport-info scroll position", async () =>
    projectionId((await runtime.driver.readProjection("parity:airport-info-scroll:"))[0]));
  const scrolled = await runtime.transition("scroll airport info", {
    ready: () => runtime.driver.readElement(`airport-info-modal:${complexAirport}`),
    act: (readyElement) => runtime.driver.drag(
      `airport-info-modal:${complexAirport}`, { x: 0, y: -500 }, readyElement,
    ),
    complete: async () => {
      const id = projectionId((await runtime.driver.readProjection("parity:airport-info-scroll:"))[0]);
      return id && id !== initialScroll ? id : null;
    },
  });
  runtime.check("airport-info.scroll", Boolean(scrolled), `${initialScroll} -> ${scrolled}`);
  const complex = (await runtime.driver.readProjection("airport-info-runways:complex:true:"))[0];
  runtime.check("airport-info.runway-complex", Boolean(complex), projectionId(complex));
  await closeMapDetail(runtime, `airport-info-modal:${complexAirport}`);

  await openAirportInfo(runtime, publishedTpaAirport);
  const publishedFacts = await runtime.driver.readProjection("airport-info-fact:");
  const published = publishedFacts.find((entry) => /TRAFFIC PATTERN ALTITUDE/i.test(entry.text) && /PUBLISHED/i.test(entry.text));
  runtime.check("airport-info.tpa-published", Boolean(published), published?.text);
  await closeMapDetail(runtime, `airport-info-modal:${publishedTpaAirport}`);

  await openAirportInfo(runtime, derivedTpaAirport);
  const derivedFacts = await runtime.driver.readProjection("airport-info-fact:");
  const derived = derivedFacts.find((entry) => /TRAFFIC PATTERN ALTITUDE/i.test(entry.text) && /DERIVED/i.test(entry.text));
  runtime.check("airport-info.tpa-derived", Boolean(derived), derived?.text);
  await closeMapDetail(runtime, `airport-info-modal:${derivedTpaAirport}`);

  await openAirportInfo(runtime, fallbackAirport);
  const fallback = (await runtime.driver.readProjection("airport-info-runways:complex:false:"))[0];
  runtime.check("airport-info.runway-fallback", Boolean(fallback), projectionId(fallback));
}

async function inspectorDetails(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("flight_plan");
  await appendRoute(runtime, "KSEA KPAE");
  await planAction(runtime, "KPAE", "activate_leg");
  await enableDeterministicOwnship(runtime);

  const airport = await selectAirportFromMapSearch(runtime, "KSEA");
  runtime.check("inspector.airport-priority", Boolean(airport), airport.text);
  const initialDistance = airport.text;
  const distanceSamples = [initialDistance];
  const changedDistance = await runtime.eventually("live inspector distance", async () => {
    const entry = (await runtime.driver.readProjection("parity:map-selection-selected:KSEA"))[0];
    if (entry?.text && distanceSamples.at(-1) !== entry.text) distanceSamples.push(entry.text);
    runtime.result.diagnostics.inspector_distance_samples = distanceSamples;
    return entry?.text && entry.text !== initialDistance ? entry.text : null;
  }, E2E_TIMING.syntheticOwnshipProgressMs, 250);
  runtime.check("inspector.distance-live", Boolean(changedDistance), `${initialDistance} -> ${changedDistance}`);

  const info = await runtime.action("open inspector airport info", "airport_info", {
    complete: () => runtime.driver.readModal("airport-info-modal:KSEA"),
  });
  runtime.check("inspector.info", Boolean(info));
  await closeMapDetail(runtime, "airport-info-modal:KSEA");

  await selectAirportFromMapSearch(runtime, "KSEA");
  const weatherAction = await waitForMapSelectionAction(
    runtime,
    "wx",
    "inspector weather action",
  );
  const weather = weatherAction?.enabled
    ? await runtime.action("open inspector weather", "wx", {
      complete: () => runtime.driver.readModal("weather-detail-modal"),
    })
    : null;
  runtime.check("inspector.weather", Boolean(weatherAction && (weather || weatherAction.disabled_reason)), weatherAction?.text);
  if (weather) {
    await closeMapDetail(runtime, "weather-detail-modal");
  } else if (await runtime.driver.readElement("map-selection-tray")) {
    await runtime.transition("dismiss weather inspector", {
      ready: () => runtime.driver.readElement("map-selection-tray"),
      act: () => runtime.driver.back(),
      complete: async () => (await runtime.driver.readElement("map-selection-tray")) ? null : true,
    });
  }

  await selectAirportFromMapSearch(runtime, "KSEA");
  const platesAction = await waitForMapSelectionAction(
    runtime,
    "plates",
    "inspector plates action",
  );
  const plate = platesAction?.enabled ? await runtime.action("open inspector plates", "plates", {
    complete: () => runtime.driver.readElement("page:plate"),
  }) : null;
  runtime.check("inspector.plates", Boolean(platesAction && plate));

  await runtime.openPage("map");
  if (await runtime.driver.readElement("map-selection-tray")) {
    await runtime.transition("dismiss retained inspector", {
      ready: () => runtime.driver.readElement("map-selection-tray"),
      act: () => runtime.driver.back(),
      complete: async () => (await runtime.driver.readElement("map-selection-tray")) ? null : true,
    });
  }
  await disableCtrBeforeFreePan(runtime, "disable CTR before SPOT pan");
  const viewportBeforeSpotPan = await runtime.stable("settled viewport before SPOT pan", async () =>
    viewportGeometryId(await runtime.driver.readProjection("parity:viewport:")));
  await runtime.transition("pan before SPOT inspection", {
    ready: () => runtime.driver.readElement("map-surface"),
    act: (readyElement) => runtime.driver.drag(
      "map-surface", { x: 360, y: 260 }, readyElement,
    ),
    complete: async () => {
      const viewport = viewportGeometryId(await runtime.driver.readProjection("parity:viewport:"));
      return viewport && viewport !== viewportBeforeSpotPan ? viewport : null;
    },
  });
  await runtime.inspectMapAt({ x: 0.30, y: 0.45 });
  const spot = await runtime.eventually("raw SPOT selection", async () => {
    const entries = await runtime.driver.readProjection("parity:map-selection-selected:");
    return entries.find((entry) => /SPOT/i.test(entry.text)) ?? null;
  }, E2E_TIMING.userResponseMs);
  runtime.check("inspector.spot-fallback", Boolean(spot), spot?.text);
  const terrain = await runtime.eventually("SPOT terrain result", async () => {
    const entry = (await runtime.driver.readProjection("parity:map-selection-selected:"))
      .find((candidate) => /SPOT/i.test(candidate.text));
    return entry && /(MSL|ELEV|FT)/i.test(entry.text) ? entry : null;
  }, E2E_TIMING.localResourceMs);
  runtime.check("inspector.terrain-async", Boolean(terrain), terrain?.text);
  if (await runtime.driver.readElement("map-selection-tray")) {
    await runtime.transition("dismiss SPOT inspector", {
      ready: () => runtime.driver.readElement("map-selection-tray"),
      act: () => runtime.driver.back(),
      complete: async () => (await runtime.driver.readElement("map-selection-tray")) ? null : true,
    });
  }
  await runtime.openPage("flight_plan");
  await openPlanRow(runtime, "KSEA");
  const unavailableArrival = await runtime.driver.readElement(runtime.platform === "web"
    ? "plan-row-action-select_arrival"
    : "plan-row-action:select_arrival");
  const disabledReason = unavailableArrival?.disabled_reason;
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
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("flight_plan");
  await appendRoute(runtime, `KSEA ${airway.entry}`);
  await planAction(runtime, airway.entry, "add_airway");
  await runtime.eventually("airway picker", async () => {
    const options = await runtime.driver.readProjection("parity:plan-airway-suggestion:");
    return options.length > 0 ? options : null;
  });
  await revealRequiredElement(
    runtime, `plan-airway-suggestion:${airway.airway}`, `${airway.airway} airway suggestion`,
  );
  await runtime.action(`select airway ${airway.airway}`, `plan-airway-suggestion:${airway.airway}`, {
    complete: async () => {
      const entries = await runtime.driver.readProjection("parity:plan-airway-entry:");
      return entries.length > 0 ? entries : null;
    },
  });
  await revealRequiredElement(
    runtime, `plan-airway-entry:${airway.entry}`, `${airway.entry} airway entry`,
  );
  await runtime.action(`select airway entry ${airway.entry}`, `plan-airway-entry:${airway.entry}`, {
    complete: async () => {
      const entries = await runtime.driver.readProjection("parity:plan-airway-exit:");
      return entries.length > 0 ? entries : null;
    },
  });
  runtime.result.diagnostics.airway_exits = {
    selected: airway.exit,
  };
  await revealRequiredElement(
    runtime, `plan-airway-exit:${airway.exit}`, `${airway.exit} airway exit`,
  );
  const beforeAirwayRevision = await runtime.driver.readSessionRevision();
  const airwayExit = await runtime.action(`select airway exit ${airway.exit}`, `plan-airway-exit:${airway.exit}`, {
    complete: async () => {
      const pickerOpen = (await runtime.driver.readProjection("parity:plan-airway-exit:")).length > 0;
      const revision = await runtime.driver.readSessionRevision();
      return !pickerOpen && revision > beforeAirwayRevision ? { revision } : null;
    },
  });
  const airwayDestination = await findPlanRow(runtime, airway.exit, E2E_TIMING.localReadyMs);
  runtime.check("plan.airway-scroll", Boolean(airwayExit && airwayDestination), airwayDestination?.text);
  runtime.check("plan.add-airway", Boolean(airwayExit && airwayDestination), airwayDestination?.text);

  await findPlanRow(runtime, "KSEA", E2E_TIMING.localReadyMs);
  const weather = await runtime.eventually("KSEA flight-plan weather badge", async () =>
    (await runtime.driver.readProjection("parity:plan-weather-badge:"))[0] ?? null);
  runtime.check("plan.weather-badge", Boolean(weather), projectionId(weather));

  const eteColumn = (await runtime.driver.readProjection("parity:plan-column:"))
    .find((entry) => /\bETE\b/i.test(projectionState(entry)));
  const eteBefore = projectionState(eteColumn);
  const eteAfter = eteColumn?.enabled ? await runtime.action("change ETE scope", projectionId(eteColumn), {
    complete: async () => {
      const column = (await runtime.driver.readProjection("parity:plan-column:"))
        .find((entry) => /\bETE\b/i.test(projectionState(entry)));
      return projectionState(column) && projectionState(column) !== eteBefore ? column : null;
    },
  }) : null;
  runtime.check("plan.ete-scope", Boolean(eteAfter), `${eteBefore} -> ${projectionState(eteAfter)}`);

  const etaColumn = (await runtime.driver.readProjection("parity:plan-column:"))
    .find((entry) => /\bETA\b/i.test(projectionState(entry)));
  const etaBefore = projectionState(etaColumn);
  const etaAfter = etaColumn?.enabled ? await runtime.action("change ETA time basis", projectionId(etaColumn), {
    complete: async () => {
      const column = (await runtime.driver.readProjection("parity:plan-column:"))
        .find((entry) => /\bETA\b/i.test(projectionState(entry)));
      return projectionState(column) && projectionState(column) !== etaBefore ? column : null;
    },
  }) : null;
  runtime.check("plan.time-mode", Boolean(etaAfter), `${etaBefore} -> ${projectionState(etaAfter)}`);

  await appendRoute(runtime, "S88");
  const undone = await runtime.action("undo route append", "undo", {
    complete: async () => !(await planRows(runtime)).some((entry) => entry.text.includes("S88")),
  });
  runtime.check("plan.undo", undone);
  const redone = await runtime.action("redo route append", "redo", {
    complete: async () => (await planRows(runtime)).some((entry) => entry.text.includes("S88")),
  });
  runtime.check("plan.redo", redone);

  await runtime.reset("app.reset.vectors");
  await acceptDisclaimer(runtime);
  await runtime.openPage("flight_plan");
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
  }, E2E_TIMING.resourceMs);
  runtime.check("plan.estimates-vectors", Boolean(estimates), `${estimates?.length ?? 0} populated cells`);
}

function altitudeControlId(runtime, controlId) {
  return runtime.platform === "web"
    ? `altitude-planner-control-${controlId}`
    : `altitude-planner-control:${controlId}`;
}

function semanticTextSignature(value) {
  return String(value ?? "").replace(/\s+/g, " ").trim();
}

async function chooseDifferentAltitudeOption(runtime, controlId) {
  const launcherId = altitudeControlId(runtime, controlId);
  const before = await revealRequiredElement(runtime, launcherId, `${controlId} altitude control`);
  const opened = await runtime.action(`open ${controlId} choices`, launcherId, {
    complete: async () => {
      const directAfter = await runtime.driver.readElement(launcherId);
      if (directAfter?.text &&
          semanticTextSignature(directAfter.text) !== semanticTextSignature(before?.text)) {
        return { directAfter, option: null };
      }
      const options = await runtime.driver.readProjection(runtime.platform === "web"
        ? "tray-option-"
        : `parity:altitude-planner-option:${controlId}:`);
      const option = options.find((entry) => entry.enabled !== false && entry.pressed !== "true")
        ?? options.find((entry) => entry.enabled !== false)
        ?? null;
      return option ? { directAfter: null, option } : null;
    },
  });
  if (opened.directAfter) {
    return { before, option: null, after: opened.directAfter };
  }
  const option = opened.option;
  const after = await runtime.action(`${controlId} selection changed`, runtime.platform === "web"
    ? `tray-option:${trayOptionId(option)}`
    : projectionId(option), {
    complete: async () => {
      const value = await runtime.driver.readElement(launcherId);
      return value?.text &&
          semanticTextSignature(value.text) !== semanticTextSignature(before?.text)
        ? value
        : null;
    },
  });
  return { before, option, after };
}

function altitudeWindActionId(runtime, rowId) {
  return runtime.platform === "web"
    ? `altitude-planner-wind-action-${rowId}`
    : `altitude-planner-wind-action:${rowId}`;
}

function selectedSemantic(element) {
  return element?.pressed === "true" || element?.selected === true || element?.checked === true;
}

export async function chooseForecastWindModel(runtime) {
  await runtime.eventually("altitude calculation completed", async () =>
    (await runtime.driver.readElement("altitude-comparison-loading")) ? null : true,
  E2E_TIMING.resourceMs);
  const noWindId = altitudeWindActionId(runtime, "no_wind");
  const readyId = altitudeWindActionId(runtime, "ready_forecast");
  const latestId = altitudeWindActionId(runtime, "latest_forecast");
  const noWind = await runtime.eventually("no-wind model row", () => runtime.driver.readElement(noWindId));
  const available = await runtime.eventually("forecast wind choice ready", async () => {
    const ready = await runtime.driver.readElement(readyId);
    if (ready && ready.enabled !== false) return { ready, latest: null };
    const latest = await runtime.driver.readElement(latestId);
    return latest && latest.enabled !== false ? { ready: null, latest } : null;
  });
  let ready = available.ready;
  let downloaded = false;

  if (available.latest) {
    const latestBefore = available.latest.text;
    await runtime.action("request latest wind forecast", latestId, {
      complete: async () => {
        const readyForecast = await runtime.driver.readElement(readyId);
        if (readyForecast && readyForecast.enabled !== false) return readyForecast;
        const value = await runtime.driver.readElement(latestId);
        return value && (value.text !== latestBefore || value.enabled === false) ? value : null;
      },
    });
    downloaded = true;
    ready = await runtime.eventually("downloaded forecast is ready", async () => {
      const value = await runtime.driver.readElement(readyId);
      return value && value.enabled !== false ? value : null;
    }, E2E_TIMING.externalConsistencyMs);
  }

  const response = await runtime.action("select ready wind forecast", readyId, {
    complete: async () => {
      const value = await runtime.driver.readElement(readyId);
      if (selectedSemantic(value)) return { selected: value, loading: null };
      const loading = await runtime.driver.readElement("altitude-comparison-loading");
      return loading ? { selected: null, loading } : null;
    },
  });
  const selected = response.selected ?? await runtime.eventually(
    "ready wind forecast selected",
    async () => {
      const value = await runtime.driver.readElement(readyId);
      return selectedSemantic(value) ? value : null;
    },
    E2E_TIMING.resourceMs,
  );
  return { noWind, ready, selected, downloaded };
}

async function altitudePlanner(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("altitude_planner");
  const unavailable = await runtime.eventually("altitude planner unavailable reason", () =>
    runtime.driver.readElement("altitude-planner-status"));
  runtime.check("altitude.unavailable-reason", Boolean(unavailable?.text), unavailable?.text);

  await runtime.openPage("flight_plan");
  await appendRoute(runtime, "KSEA KPAE");
  await runtime.openPage("altitude_planner");
  const initialPanel = await runtime.eventually("altitude comparison panel", () =>
    runtime.driver.readElement("altitude-comparison-panel"), E2E_TIMING.resourceMs);
  const initialText = initialPanel.text;

  const aircraft = await chooseDifferentAltitudeOption(runtime, "aircraft");
  runtime.check("altitude.aircraft", Boolean(aircraft.after), `${aircraft.before?.text} -> ${aircraft.after?.text}`);
  const afterAircraft = await runtime.eventually("aircraft comparison changed", async () => {
    const panel = await runtime.driver.readElement("altitude-comparison-panel");
    return panel?.text && panel.text !== initialText ? panel : null;
  }, E2E_TIMING.resourceMs);
  runtime.check("altitude.changed-estimate", Boolean(afterAircraft), afterAircraft?.text.slice(0, 240));

  const profile = await chooseDifferentAltitudeOption(runtime, "aircraft_profile");
  runtime.check("altitude.aircraft-profile", Boolean(profile.after), `${profile.before?.text} -> ${profile.after?.text}`);

  const wind = await chooseForecastWindModel(runtime);
  runtime.check(
    "altitude.wind-model",
    selectedSemantic(wind.noWind) && Boolean(wind.selected),
    `downloaded=${wind.downloaded}; ${wind.noWind?.text} -> ${wind.selected?.text}`,
  );
  const forecast = await runtime.driver.readElement("altitude-planner-forecast");
  runtime.check(
    "altitude.forecast-fallback",
    Boolean(forecast?.text || wind.selected?.text),
    forecast?.text ?? wind.selected?.text,
  );

  const basisBefore = await revealRequiredElement(
    runtime, "altitude-planner-departure-basis", "altitude departure-time basis",
  );
  const basisBeforeSignature = `${basisBefore?.test_id ?? ""}|${basisBefore?.text ?? ""}`;
  const basisAfter = await runtime.action("change departure time basis", "altitude-planner-departure-basis", {
    complete: async () => {
      const value = await runtime.driver.readElement("altitude-planner-departure-basis");
      const signature = `${value?.test_id ?? ""}|${value?.text ?? ""}`;
      return value && signature !== basisBeforeSignature ? value : null;
    },
  });
  runtime.check(
    "altitude.time",
    Boolean(basisAfter),
    `${basisBeforeSignature} -> ${basisAfter?.test_id ?? ""}|${basisAfter?.text ?? ""}`,
  );

  const rows = await runtime.driver.readProjection(runtime.platform === "web"
    ? "altitude-comparison-row-"
    : "parity:altitude-comparison-row:");
  const alternateAltitude = rows.find((row) => row.enabled !== false && row.selected !== "true" && row.pressed !== "true")
    ?? rows.find((row) => row.enabled !== false);
  const alternateAltitudeId = alternateAltitude ? projectionId(alternateAltitude) : null;
  const selectedAltitude = alternateAltitude ? await runtime.action(
    "select altitude row",
    alternateAltitudeId,
    {
      complete: async () => {
        const nextRows = await runtime.driver.readProjection(runtime.platform === "web"
          ? "altitude-comparison-row-"
          : "parity:altitude-comparison-row:");
        return nextRows.find((row) =>
          projectionId(row) === alternateAltitudeId &&
          (row.selected === "true" || row.pressed === "true")) ?? null;
      },
    },
  ) : null;
  runtime.check("altitude.altitude", Boolean(selectedAltitude), selectedAltitude?.text);
}

async function selectReplaySource(runtime) {
  const option = await runtime.action("open ownship source choices", "ownship-source-button", {
    complete: async () => {
      const entries = await runtime.driver.readProjection(runtime.platform === "web"
        ? "tray-option-"
        : "parity:ownship-source:");
      return entries.find((entry) => /REPLAY/i.test(entry.text ?? "")) ?? null;
    },
  });
  const actionId = runtime.platform === "web"
    ? `tray-option:${trayOptionId(option)}`
    : projectionId(option);
  return runtime.action("select Replay ownship source", actionId, {
    complete: () => runtime.driver.readElement("playback-source-input"),
  });
}

async function loadReplayFixture(runtime) {
  const tracePath = runtime.fixtureUrl(runtime.capability("replay_trace"));
  const traceUrl = runtime.platform === "web"
    ? (() => {
        const url = new URL(tracePath);
        url.searchParams.set("aerobag_e2e_abort_once", runtime.artifactDir);
        return url.href;
      })()
    : tracePath;
  await selectReplaySource(runtime);
  await runtime.editText(
    "enter replay trace location",
    "playback-source-input",
    traceUrl,
  );
  await runtime.eventually("replay trace ready to load", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state?.status === "empty" ? state : null;
  });
  return runtime.action("load replay trace", "playback-load-button", {
    complete: async () => {
      const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
      return state?.status === "paused" && state.duration > 0 ? state : null;
    },
  });
}

async function setReplayRate(runtime, rate) {
  if (runtime.platform === "web") {
    await runtime.editText("enter replay rate", "playback-rate-input", String(rate));
  } else {
    return runtime.transition("set Android replay rate", {
      ready: () => runtime.driver.readElement("playback-rate-input"),
      act: (readyElement) => runtime.driver.setProgress(
        "playback-rate-input",
        rate,
        readyElement,
      ),
      complete: async () => {
        const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
        return state && Math.abs(state.rate - rate) < 0.01 ? state : null;
      },
    });
  }
  return runtime.eventually("replay rate set", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state && Math.abs(state.rate - rate) < 0.01 ? state : null;
  });
}

async function replayTrackUp(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  const loaded = await loadReplayFixture(runtime);
  runtime.check("replay.load", Boolean(loaded), JSON.stringify(loaded));
  await setReplayRate(runtime, 0.25);

  await runtime.action("select TRK-up for replay", "map-orientation-button", {
    complete: async () => {
      const button = await runtime.driver.readElement("map-orientation-button");
      return button?.pressed === "true" || button?.selected === true ? button : null;
    },
  });
  const playbackControl = await runtime.repeatableAction(
    "start replay playback",
    "playback-play-toggle",
    {
      complete: async () => {
        const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
        return state?.status === "playing" ? state : null;
      },
    },
  );
  const initialOwnship = await runtime.eventually("initial replay ownship", async () => {
    const state = ownshipState(await runtime.driver.readProjection("parity:ownship-state:"));
    return state?.mode === "replay" && state.draw && state.position !== "none" ? state : null;
  });
  runtime.result.diagnostics.replay_initial_ownship = initialOwnship;

  const playing = await runtime.eventually("replay playing", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state?.status === "playing" && state.cursor > 0.2 ? state : null;
  });
  const rotated = await runtime.eventually("replay TRK-up rotation", async () => {
    const id = idOf(await runtime.driver.readProjection("parity:viewport:"));
    const up = Number(/:up:(-?[0-9.]+)/.exec(id ?? "")?.[1] ?? 0);
    return Math.abs(up) > 1 ? { id, up } : null;
  });
  const gap = await runtime.eventually("replay ownship entered track gap", async () => {
    const playback = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    if (playback?.status !== "playing") return null;
    const state = ownshipState(await runtime.driver.readProjection("parity:ownship-state:"));
    if (state?.mode !== "replay" || !state.draw || state.track !== "none") return null;
    const viewport = idOf(await runtime.driver.readProjection("parity:viewport:"));
    const up = Number(/:up:(-?[0-9.]+)/.exec(viewport ?? "")?.[1] ?? 0);
    return { state, viewport, up };
  }, E2E_TIMING.replayProgressMs, 40);
  const paused = await runtime.repeatAction(
    "pause replay after track gap observation",
    playbackControl.handle,
    {
      complete: async () => {
        const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
        return state?.status === "paused" ? state : null;
      },
    },
  );
  runtime.check("replay.rotation", Boolean(rotated), rotated?.id);
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
  const nextRate = priorRate === 2 ? 3 : 2;
  if (runtime.platform === "web") {
    await runtime.editText(
      "change replay rate",
      "playback-rate-input",
      String(nextRate),
    );
  } else {
    await runtime.transition("change Android replay rate", {
      ready: () => runtime.driver.readElement("playback-rate-input"),
      act: (readyElement) => runtime.driver.setProgress(
        "playback-rate-input",
        nextRate,
        readyElement,
      ),
      complete: async () => {
        const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
        return state && Math.abs(state.rate - priorRate) > 0.01 ? state : null;
      },
    });
  }
  const changedRate = await runtime.eventually("replay rate changed", async () => {
    const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
    return state && Math.abs(state.rate - priorRate) > 0.01 ? state : null;
  });
  runtime.check("replay.rate", Boolean(changedRate), `${priorRate} -> ${changedRate.rate}`);

  const priorCursor = changedRate.cursor;
  const targetCursor = priorCursor < changedRate.duration * 0.5
    ? changedRate.duration * 0.8
    : changedRate.duration * 0.2;
  const sought = await runtime.transition("seek replay timeline", {
    ready: () => runtime.driver.readElement("playback-overview"),
    act: (readyElement) => runtime.platform === "android"
      ? runtime.driver.setProgress("playback-overview", targetCursor, readyElement)
      : runtime.driver.drag("playback-overview", {
        x: timelineSeekDeltaX(priorCursor, changedRate.duration),
        y: 0,
      }),
    complete: async () => {
      const state = playbackState(await runtime.driver.readProjection("parity:playback-widget:"));
      return state && Math.abs(state.cursor - priorCursor) > 0.1 ? state : null;
    },
  });
  runtime.check("replay.seek", Boolean(sought), `${priorCursor} -> ${sought.cursor}`);
}

async function preparedLiveFeeds(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("map");
  await setLayerVisible(runtime, "vectors", true);
  await setLayerVisible(runtime, "metars", true);
  const overlay = await runtime.eventually("prepared weather overlays", async () => {
    const state = liveOverlayState(await runtime.driver.readProjection("parity:live-overlay:"));
    return state && state.metars > 0 && state.pireps > 0 ? state : null;
  }, E2E_TIMING.resourceMs);

  await selectAirportFromMapSearch(runtime, "KSEA");
  const weatherAction = await waitForMapSelectionAction(runtime, "wx", "KSEA weather action");
  if (!weatherAction.enabled) throw new Error(`KSEA weather is unavailable: ${weatherAction.text}`);
  await runtime.action("open prepared weather modal", "wx", {
    complete: () => runtime.driver.readModal("weather-detail-modal"),
  });
  const detail = await runtime.eventually("prepared weather detail", async () => {
      const modal = await runtime.driver.readElement("weather-detail-modal");
      return modal?.text && /METAR/i.test(modal.text) && /TAF/i.test(modal.text) && /NOTAM/i.test(modal.text)
        ? modal
        : null;
  }, E2E_TIMING.resourceMs);
  runtime.check(
    "livefeed.metar-taf-pirep-notam",
    Boolean(detail && overlay.metars > 0 && overlay.pireps > 0),
    `${JSON.stringify(overlay)} ${detail.text.slice(0, 240)}`,
  );
}

async function nexradFrames(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("map");
  await setLayerVisible(runtime, "nexrad", true);
  const first = await runtime.eventually("painted NEXRAD history frame", async () => {
    const state = nexradState(await runtime.driver.readProjection("parity:nexrad-state:"));
    return state && state.tiles > 0 && state.frames >= 2 && state.frame !== null ? state : null;
  }, E2E_TIMING.externalConsistencyMs);
  const next = await runtime.eventually("advanced NEXRAD history frame", async () => {
    const state = nexradState(await runtime.driver.readProjection("parity:nexrad-state:"));
    return state && state.tiles > 0 && state.frames === first.frames && state.frame !== first.frame ? state : null;
  }, E2E_TIMING.animationCycleMs, 100);
  runtime.check("livefeed.nexrad-frames", Boolean(next), `${JSON.stringify(first)} -> ${JSON.stringify(next)}`);
}

async function obstaclesNavKv(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("map");
  await setLayerVisible(runtime, "vectors", true);
  const overlay = await runtime.eventually("faulted obstacle NavKv tiles", async () => {
    const state = liveOverlayState(await runtime.driver.readProjection("parity:live-overlay:"));
    return state && state.obstacles > 0 ? state : null;
  }, E2E_TIMING.externalConsistencyMs);
  runtime.check("livefeed.obstacles-navkv", overlay.obstacles > 0, JSON.stringify(overlay));
}

async function windsAloftNavKv(runtime) {
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("flight_plan");
  await appendRoute(runtime, "KSEA KPAE");
  await runtime.openPage("altitude_planner");
  const wind = await chooseForecastWindModel(runtime);
  const forecast = await runtime.eventually("forecast-backed altitude comparison", async () => {
    const value = await runtime.driver.readElement("altitude-planner-forecast");
    return value?.text && /(forecast from|extends|valid through)/i.test(value.text) ? value : null;
  }, E2E_TIMING.externalConsistencyMs);
  const comparison = await runtime.eventually("wind-backed altitude rows", () =>
    runtime.driver.readElement("altitude-comparison-panel"), E2E_TIMING.resourceMs);
  runtime.check(
    "livefeed.winds-aloft-navkv",
    Boolean(forecast && comparison?.text),
    `${wind.noWind?.text} -> ${wind.selected?.text}; ${forecast.text} ${comparison.text.slice(0, 200)}`,
  );
}

async function tfrMapDetail(runtime) {
  const airportId = runtime.fixture?.capabilities?.live_feeds?.tfr_target_airport ?? "27W";
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("map");
  await setLayerVisible(runtime, "vectors", true);
  await ensureMapFamily(runtime, "none", "select no raster family for TFR");
  await dismissTrayOptions(runtime, "dismiss no-raster family choices");
  await selectTfrFromPreparedMap(runtime, airportId);
  const tfrItemId = runtime.platform === "web"
    ? "map-selection-item-airspace-TFR"
    : "map-selection-item:airspace-TFR";
  const tfrItem = await runtime.eventually(
    "TFR map selection item",
    () => runtime.driver.readElement(tfrItemId),
    E2E_TIMING.localReadyMs,
  );
  await runtime.action("open TFR item actions", tfrItemId, {
    complete: () => runtime.driver.readAction("tfr_text"),
  });
  await runtime.action("open TFR text detail", "tfr_text", {
    complete: () => runtime.driver.readModal("map-selection-detail-modal:TFR"),
  });
  const detail = await runtime.eventually(
    "rendered TFR text detail",
    () => runtime.driver.readElement("map-selection-detail-modal:TFR"),
    E2E_TIMING.resourceMs,
  );
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

function cloudPanelElementId(runtime, panelId) {
  return runtime.platform === "web" ? `cloud-panel-${panelId}` : `cloud-panel:${panelId}`;
}

async function waitForCloudPanel(runtime, panelId, state) {
  return runtime.eventually(`${panelId} ${state} cloud panel`, async () => {
    const panel = await runtime.driver.readElement(cloudPanelElementId(runtime, panelId));
    return panel?.state === state ? panel : null;
  });
}

async function waitForCloudActive(runtime) {
  return runtime.eventually("active Sync Account", async () => {
    const status = await runtime.driver.readElement(cloudStatusElementId(runtime));
    return status?.text && /Cloud active/i.test(status.text) ? status : null;
  }, E2E_TIMING.cloudConsistencyMs);
}

async function waitForPlanIdents(runtime, expected) {
  return runtime.eventually(`flight plan ${expected.join(" ")}`, async () => {
    const rows = await runtime.driver.readProjection("parity:plan-row:");
    const text = rows.map((row) => row.text).join(" ");
    return expected.every((ident) => text.includes(ident)) ? rows : null;
  }, E2E_TIMING.cloudConsistencyMs);
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
    }, E2E_TIMING.cloudConsistencyMs);
  }

  await runtime.openPage("offline_packages");
  const expectedStateFragments = [
    ...Object.entries(preferences.regions).map(([id, selection]) =>
      `:region:${id}:${selection}`),
    ...Object.entries(preferences.products).map(([id, selection]) =>
      `:product:${id}:${selection}`),
  ];
  const state = await runtime.eventually("Android cloud package preferences", async () => {
    const [entry] = await runtime.driver.readProjection("parity:offline-preferences:");
    const id = projectionId(entry);
    return id && expectedStateFragments.every((fragment) => id.includes(fragment)) ? entry : null;
  }, E2E_TIMING.cloudConsistencyMs);
  const expectedRows = [
    ...Object.entries(preferences.regions).map(([id, selection]) =>
      `parity:offline-region:${id}:selection:${selection}`),
    ...Object.entries(preferences.products).map(([id, selection]) =>
      `parity:offline-product:${id}:selection:${selection}`),
  ];
  for (const expected of expectedRows) {
    await revealRequiredElement(
      runtime,
      expected.replace(/^parity:/, ""),
      `synchronized package preference ${expected}`,
    );
  }
  return state;
}

async function revealCloudAction(runtime, actionId, description) {
  return revealRequiredElement(runtime, cloudActionElementId(runtime, actionId), description);
}

async function cloudCrossfill(runtime) {
  const peerUrl = process.env.AEROBAG_E2E_PEER_URL ?? "http://127.0.0.1:8085/";
  let peer = null;
  await runtime.reset();
  await acceptDisclaimer(runtime);
  await runtime.openPage("cloud");

  const beginSetup = await revealCloudAction(runtime, "begin_setup", "begin cloud setup action");
  runtime.check("cloud.begin-setup", Boolean(beginSetup?.enabled));
  await runtime.action("begin cloud setup", "begin_setup", {
    complete: async () => {
      const panel = await runtime.driver.readElement(cloudPanelElementId(runtime, "receive_setup"));
      return panel?.state === "active" ? panel : null;
    },
  });

  const scanCode = await revealCloudAction(runtime, "scan_setup_code", "scan setup code action");
  runtime.check(
    "cloud.scan-code",
    Boolean(scanCode),
    scanCode?.enabled ? "scanner action enabled" : "scanner action explains platform unavailability",
  );
  const setupInput = await revealRequiredElement(
    runtime, "cloud-setup-code-input", "cloud setup code input",
  );
  if (!setupInput) throw new Error("cloud setup code input is unavailable");

  const backSetup = await revealCloudAction(runtime, "back_setup", "back from cloud setup");
  await runtime.action("back out of cloud setup", "back_setup", {
    complete: async () => {
      const panel = await runtime.driver.readElement(cloudPanelElementId(runtime, "get_started"));
      return panel?.state === "active" ? panel : null;
    },
  });
  runtime.check("cloud.back-setup", Boolean(backSetup?.enabled));

  const beginCreate = await revealCloudAction(
    runtime, "begin_create", "begin account creation action",
  );
  runtime.check("cloud.begin-create", Boolean(beginCreate?.enabled));
  await runtime.action("begin Sync Account creation", "begin_create", {
    complete: async () => {
      const panel = await runtime.driver.readElement(cloudPanelElementId(runtime, "create_account"));
      return panel?.state === "active" ? panel : null;
    },
  });
  const createAccount = await revealCloudAction(runtime, "create_account", "create account action");
  await runtime.action("start Sync Account creation", "create_account", {
    complete: async () => {
      const working = await runtime.driver.readElement(
        cloudPanelElementId(runtime, "create_account"),
      );
      if (working?.state === "working") return working;
      const linked = await runtime.driver.readElement(cloudPanelElementId(runtime, "linked"));
      return linked?.state === "active" ? linked : null;
    },
  });
  const active = await waitForCloudActive(runtime);
  runtime.check("cloud.create-account", Boolean(createAccount?.enabled && active));

  const backup = await revealCloudAction(runtime, "backup_setup_code", "backup setup code action");
  await runtime.action("show backup setup code", "backup_setup_code", {
    complete: async () => {
      const panel = await runtime.driver.readElement(cloudPanelElementId(runtime, "backup_code"));
      return panel?.state === "active" ? panel : null;
    },
  });
  await revealRequiredElement(runtime, "cloud-setup-code-output", "Device Setup Code output");
  const setupCodeElement = await runtime.eventually("Device Setup Code", async () => {
    const element = await runtime.driver.readElement("cloud-setup-code-output");
    const value = element?.value || element?.text;
    return value?.startsWith("AB3.") ? { ...element, value } : null;
  });
  runtime.check("cloud.backup-code", Boolean(backup?.enabled && setupCodeElement));

  const copy = await revealCloudAction(runtime, "copy_setup_code", "copy setup code action");
  await runtime.action("copy Device Setup Code", "copy_setup_code", {
    complete: () => runtime.driver.readElement("cloud-copy-status"),
  });
  runtime.check("cloud.copy-code", Boolean(copy?.enabled));
  const closeBackup = await revealCloudAction(
    runtime, "close_linked_detail", "close backup detail action",
  );
  await runtime.action("close backup setup code", "close_linked_detail", {
    complete: async () => {
      const panel = await runtime.driver.readElement(cloudPanelElementId(runtime, "linked"));
      return panel?.state === "active" ? panel : null;
    },
  });

  const addDevice = await revealCloudAction(runtime, "add_device", "add device action");
  await runtime.action("show add-device setup code", "add_device", {
    complete: async () => {
      const panel = await runtime.driver.readElement(cloudPanelElementId(runtime, "add_device"));
      return panel?.state === "active" ? panel : null;
    },
  });
  await revealRequiredElement(runtime, "cloud-setup-code-output", "add-device setup code output");
  const addDeviceCode = await runtime.eventually(
    "add-device setup code",
    () => runtime.driver.readElement("cloud-setup-code-output"),
  );
  runtime.check("cloud.add-device", Boolean(addDevice?.enabled && addDeviceCode));
  const closeAddDevice = await revealCloudAction(
    runtime, "close_linked_detail", "close add-device detail action",
  );
  await runtime.action("close add-device setup code", "close_linked_detail", {
    complete: async () => {
      const panel = await runtime.driver.readElement(cloudPanelElementId(runtime, "linked"));
      return panel?.state === "active" ? panel : null;
    },
  });
  runtime.check("cloud.close-detail", Boolean(closeBackup?.enabled && closeAddDevice?.enabled));

  // Revealing a control may overlap automatic root-publication and event-stream
  // work. Finish all UI preparation before establishing the quiescent revision
  // that the manual synchronization action must advance.
  const syncNow = await revealCloudAction(runtime, "sync_now", "sync now action");
  const actionRevisionBeforeSync = await runtime.stable(
    "settled cloud state before manual synchronization",
    async () => {
      const status = await runtime.driver.readElement(cloudStatusElementId(runtime));
      if (!status?.text || !/Cloud active/i.test(status.text)) return null;
      return runtime.driver.readCloudActionRevision();
    },
  );
  await runtime.action("synchronize cloud state now", "sync_now", {
    complete: async () => {
      const revision = await runtime.driver.readCloudActionRevision();
      return revision > actionRevisionBeforeSync ? revision : null;
    },
  });
  await waitForCloudActive(runtime);
  runtime.check("cloud.sync-now", Boolean(syncNow?.enabled));

  try {
    const { launchCloudJourneyPeer } = await import("./cloud-journey-peer.mjs");
    peer = await launchCloudJourneyPeer({
      url: peerUrl,
      referenceEpochMs: null,
      requestOriginRoutes: runtime.platform === "android" ? [{
        sourceOrigin: `http://127.0.0.1:${process.env.AEROBAG_ANDROID_CLOUD_DEVICE_PORT ?? "18094"}`,
        targetOrigin: `http://127.0.0.1:${process.env.AEROBAG_E2E_CLOUD_PORT ?? "18094"}`,
      }] : [],
    });
    await peer.acceptSetupCode(setupCodeElement.value);
    runtime.check("cloud.accept-code", true, "second client linked with the pasted setup code");

    await peer.appendRoute("KSEA KPAE");
    await runtime.openPage("flight_plan");
    let adoptedPlan;
    try {
      adoptedPlan = await waitForPlanIdents(runtime, ["KSEA", "KPAE"]);
    } catch (error) {
      throw new Error(`${error.message}; browser peer cloud state: ${JSON.stringify(await peer.state())}`);
    }
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
      await runtime.reload("app.reload.cloud-reconnect");
      await runtime.eventually("Android app after cloud reconnect restart", () =>
        runtime.driver.readElement("primary-navigation"), E2E_TIMING.startupMs);
    }
    await peer.appendRoute("KPLU");
    await runtime.openPage("flight_plan");
    let postReconnectPlan;
    try {
      postReconnectPlan = await waitForPlanIdents(runtime, ["KPLU"]);
    } catch (error) {
      let targetCloudState = null;
      try {
        await runtime.openPage("cloud");
        targetCloudState = {
          overall: await runtime.driver.readElement(cloudStatusElementId(runtime)),
          linked: await runtime.driver.readElement(cloudPanelElementId(runtime, "linked")),
          provider: await runtime.driver.readElement(cloudPanelElementId(runtime, "provider")),
          plan: await runtime.driver.readProjection("parity:plan-row:"),
        };
      } catch (diagnosticError) {
        targetCloudState = { diagnostic_error: diagnosticError.message };
      }
      throw new Error(
        `${error.message}; target cloud state: ${JSON.stringify(targetCloudState ?? null)}; ` +
        `browser peer cloud state: ${JSON.stringify(await peer.state())}`,
      );
    }
    runtime.check("cloud.reconnect", Boolean(postReconnectPlan));
  } finally {
    await peer?.close();
  }

  await runtime.openPage("cloud");
  const beginUnlink = await revealCloudAction(runtime, "begin_unlink", "begin unlink action");
  await runtime.action("begin unlinking this device", "begin_unlink", {
    complete: async () => {
      const panel = await runtime.driver.readElement(cloudPanelElementId(runtime, "confirm_unlink"));
      return panel?.state === "caution" ? panel : null;
    },
  });
  runtime.check("cloud.begin-unlink", Boolean(beginUnlink?.enabled));
  const confirmUnlink = await revealCloudAction(runtime, "confirm_unlink", "confirm unlink action");
  const inactive = await runtime.action("confirm unlinking this device", "confirm_unlink", {
    complete: async () => {
      const status = await runtime.driver.readElement(cloudStatusElementId(runtime));
      return status?.text && /Cloud not active/i.test(status.text) ? status : null;
    },
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
  "web.raster-load-recovery": rasterLoadRecovery,
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
