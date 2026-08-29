// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { writeFileSync } from "node:fs";
import {
  adb, androidImeVisible, androidNodeLabel, androidTag, clickAndroidSemanticNode,
  displayBoundsFromXml, dumpAndroid, findNode, findNodes,
  findVerticalScrollSurface, pressKey, rectOfBounds, screencapPng,
  queryAndroidSemanticNodes, scrollAndroidAndAwait, setAndroidSemanticText,
  queryAndroidExactProjection, queryAndroidStartupProjection,
  scrollUntilTag, scrollUntilTagPrefix, swipe,
  scrollHorizontallyUntilTag, waitFor, waitForAndroidSemanticEvent,
} from "./android-harness.mjs";
import {
  E2E_TIMING, observeUntil, performTransition, TerminalObservationError,
} from "./transition-contract.mjs";

const ANDROID_EXACT_SCALAR_PROJECTIONS = new Map([
  ["parity:live-overlay:", "org.aerobag.app:id/e2e_live_overlay_projection"],
  ["parity:nexrad-state:", "org.aerobag.app:id/e2e_nexrad_state_projection"],
  ["parity:ownship-state:", "org.aerobag.app:id/e2e_ownship_state_projection"],
  ["parity:playback-widget:", "org.aerobag.app:id/e2e_playback_widget_projection"],
  ["parity:viewport:", "org.aerobag.app:id/e2e_viewport_projection"],
  ["parity:map-selection-state:", "org.aerobag.app:id/e2e_map_selection_projection"],
  ["parity:flight-plan-rows:", "org.aerobag.app:id/e2e_flight_plan_rows_projection"],
]);

export const SEMANTIC_DRIVER_OPERATIONS = Object.freeze([
  "reset", "resetApplicationData", "resetApplicationDataExpectingStartupFailure", "openPage", "readCurrentPage", "readPage", "readNavigationAction", "activateNavigation",
  "openChooser", "readOption", "selectOption",
  "inspectMapAt", "activateMapInspection", "performAction",
  "readRepeatedAction", "performRepeatedAction",
  "focusText", "enterText", "submit", "drag", "zoom", "hover", "copyText", "readElement", "readProjection",
  "readAction", "readSessionRevision", "findProjectionMatching", "revealElement", "scanProjection",
  "readCloudActionRevision",
  "revealProjectionMatching", "reload",
  "back", "captureFrame", "injectRasterLoadFault",
]);

export class SemanticJourneyDriver {
  constructor(platform) {
    this.platform = platform;
  }

  async reset() { throw new Error(`${this.platform} driver does not implement reset`); }
  async resetApplicationData() { throw new Error(`${this.platform} driver does not implement resetApplicationData`); }
  async resetApplicationDataExpectingStartupFailure() {
    throw new Error(`${this.platform} driver does not implement resetApplicationDataExpectingStartupFailure`);
  }
  async openPage(pageId) { return navigateSemanticPage(this, pageId); }
  async readCurrentPage() { throw new Error(`${this.platform} driver does not implement readCurrentPage`); }
  async readPage(pageId) {
    const current = await this.readCurrentPage();
    return current?.pageId === pageId ? current : null;
  }
  async readNavigationAction(_pageId) {
    throw new Error(`${this.platform} driver does not implement readNavigationAction`);
  }
  async activateNavigation(_pageId, _readyElement) {
    throw new Error(`${this.platform} driver does not implement activateNavigation`);
  }
  async openChooser(_launcherId, _readyElement) {
    throw new Error(`${this.platform} driver does not implement openChooser`);
  }
  async readOption(_launcherId, _optionId) { throw new Error(`${this.platform} driver does not implement readOption`); }
  async selectOption(_launcherId, _optionId, _readyElement) {
    throw new Error(`${this.platform} driver does not implement selectOption`);
  }
  async inspectMapAt(point) { return inspectSemanticMapAt(this, point); }
  async activateMapInspection(_point, _readyElement) {
    throw new Error(`${this.platform} driver does not implement activateMapInspection`);
  }
  async performAction(_actionId, _readyElement) {
    throw new Error(`${this.platform} driver does not implement performAction`);
  }
  async readRepeatedAction(actionId, _retainedTarget) {
    return this.readAction(actionId);
  }
  async performRepeatedAction(actionId, _retainedTarget, readyElement) {
    return this.performAction(actionId, readyElement);
  }
  async readAction(_actionId) { throw new Error(`${this.platform} driver does not implement readAction`); }
  async readSessionRevision() { throw new Error(`${this.platform} driver does not implement readSessionRevision`); }
  async readCloudActionRevision() {
    throw new Error(`${this.platform} driver does not implement readCloudActionRevision`);
  }
  async focusText(_controlId, _readyElement) {
    throw new Error(`${this.platform} driver does not implement focusText`);
  }
  async enterText(_controlId, _value, _options, _readyElement) {
    throw new Error(`${this.platform} driver does not implement enterText`);
  }
  async submit(_controlId, _readyElement) {
    throw new Error(`${this.platform} driver does not implement submit`);
  }
  async drag(_surfaceId, _delta) { throw new Error(`${this.platform} driver does not implement drag`); }
  async zoom(_surfaceId, _amount) { throw new Error(`${this.platform} driver does not implement zoom`); }
  async hover(_elementId) { throw new Error(`${this.platform} driver does not implement hover`); }
  async copyText(_elementId) { throw new Error(`${this.platform} driver does not implement copyText`); }
  async readElement(_elementId) { throw new Error(`${this.platform} driver does not implement readElement`); }
  async revealElement(_elementId) { throw new Error(`${this.platform} driver does not implement revealElement`); }
  async readProjection(_probe) { throw new Error(`${this.platform} driver does not implement readProjection`); }
  async findProjectionMatching(_probe, _needle) { throw new Error(`${this.platform} driver does not implement findProjectionMatching`); }
  async scanProjection(_probe) { throw new Error(`${this.platform} driver does not implement scanProjection`); }
  async revealProjectionMatching(_probe, _needle) { throw new Error(`${this.platform} driver does not implement revealProjectionMatching`); }
  async reload() { throw new Error(`${this.platform} driver does not implement reload`); }
  async back() { throw new Error(`${this.platform} driver does not implement back`); }
  async captureFrame(_path) { throw new Error(`${this.platform} driver does not implement captureFrame`); }
  async injectRasterLoadFault() { throw new Error(`${this.platform} driver does not implement raster fault injection`); }
}

export async function navigateSemanticPage(
  driver,
  pageId,
  {
    observe = async (description, probe) => (await observeUntil(description, probe, {
      waitForNextProbe: driver.waitForObservation?.bind(driver) ?? null,
    })).value,
    transition = async (description, contract) => (await performTransition(description, {
      ...contract,
      waitForObservation: driver.waitForObservation?.bind(driver) ?? null,
    })).value,
  } = {},
) {
  const selectedPage = async (expectedPageId) => {
    const selected = await driver.readCurrentPage();
    return selected?.pageId === expectedPageId ? selected : null;
  };
  const renderedPage = (expectedPageId) => observe(
    `rendered ${expectedPageId} page after navigation`,
    () => driver.readPage(expectedPageId),
  );
  let current = await observe("visible page before navigation", () => driver.readCurrentPage());
  if (current.pageId === pageId) return current;

  if (current.pageId !== "home") {
    current = await transition("navigate to Home", {
      readinessSamples: 1,
      ready: () => driver.readNavigationAction("home"),
      act: (readyElement) => driver.activateNavigation("home", readyElement),
      complete: () => selectedPage("home"),
    });
    current = await renderedPage("home");
  }
  if (pageId === "home") return current;

  await transition(`navigate to ${pageId}`, {
    readinessSamples: 1,
    ready: () => driver.readNavigationAction(pageId),
    act: (readyElement) => driver.activateNavigation(pageId, readyElement),
    complete: () => selectedPage(pageId),
  });
  return renderedPage(pageId);
}

function semanticTextValue(element) {
  return String(element?.value ?? element?.text ?? "");
}

export async function editSemanticText(
  driver,
  description,
  controlId,
  value,
  options = {},
  {
    transition = async (transitionDescription, contract) => (await performTransition(
      transitionDescription,
      {
        ...contract,
        waitForObservation: driver.waitForObservation?.bind(driver) ?? null,
      },
    )).value,
  } = {},
) {
  const expected = String(value);
  let current = await driver.readElement(controlId);
  if (current && semanticTextValue(current) === expected) return current;
  if (!current?.focused) {
    current = await transition(`${description} focus`, {
      readinessSamples: 1,
      ready: async () => {
        const element = await driver.readElement(controlId);
        return element?.enabled && element.actionable !== false ? element : null;
      },
      act: (readyElement) => driver.focusText(controlId, readyElement),
      complete: async () => {
        const element = await driver.readElement(controlId);
        return element?.focused ? element : null;
      },
    });
  }
  return transition(description, {
    readinessSamples: 1,
    ready: async () => {
      const element = await driver.readElement(controlId);
      return element?.focused && element.enabled && element.actionable !== false ? element : null;
    },
    act: (readyElement) => driver.enterText(controlId, expected, options, readyElement),
    complete: async () => {
      const element = await driver.readElement(controlId);
      return element && semanticTextValue(element) === expected ? element : null;
    },
  });
}

export async function inspectSemanticMapAt(
  driver,
  point,
  {
    transition = async (description, contract) => (await performTransition(description, {
      ...contract,
      waitForObservation: driver.waitForObservation?.bind(driver) ?? null,
    })).value,
  } = {},
) {
  return transition("inspect map position", {
    ready: () => driver.readElement("map-surface"),
    act: (readyElement) => driver.activateMapInspection(point, readyElement),
    complete: () => driver.readElement("map-selection-tray"),
  });
}

const WEB_PAGE_SELECTORS = Object.freeze({
  map: '[data-testid="parity:page:map"]',
  charts: '[data-testid="parity:page:plate"]',
  flight_plan: '[data-testid="parity:page:flight_plan"]',
  altitude_planner: '[data-testid="parity:page:altitude_planner"]',
  data_status: '[data-testid="parity:page:data_status"]',
  settings: '[data-testid="parity:page:settings"]',
  home: '[data-testid="parity:page:home"]',
  cloud: '[data-testid="parity:page:cloud"]',
  about: '[data-testid="parity:page:about"]',
});

const WEB_HOME_KEYS = Object.freeze({
  map: "chart",
  charts: "plate",
  flight_plan: "flight_plan",
  altitude_planner: "altitude_planner",
  data_status: "data_status",
  settings: "settings",
  cloud: "cloud",
  about: "about",
});

export function webTestIdSelector(testId) {
  const value = String(testId)
    .replaceAll("\\", "\\\\")
    .replaceAll('"', '\\"')
    .replaceAll("\n", "\\a ")
    .replaceAll("\r", "\\d ")
    .replaceAll("\f", "\\c ");
  return `[data-testid="${value}"]`;
}

function webActionSelectors(actionId) {
  if (actionId.startsWith("plan-row:")) {
    return [webTestIdSelector(`plan-row-${actionId.slice("plan-row:".length)}`)];
  }
  if (actionId.startsWith("plan-procedure:")) {
    return [webTestIdSelector(`plan-procedure-${actionId.slice("plan-procedure:".length)}`)];
  }
  if (actionId.startsWith("plan-procedure-transition:")) {
    return [webTestIdSelector(`plan-procedure-transition-${actionId.slice("plan-procedure-transition:".length)}`)];
  }
  if (actionId.startsWith("plan-insert-suggestion:")) {
    return [webTestIdSelector(`plan-insert-suggestion-${actionId.slice("plan-insert-suggestion:".length)}`)];
  }
  if (actionId.startsWith("plate-folder-tile:")) {
    return [webTestIdSelector(`plate-folder-tile:${actionId.slice("plate-folder-tile:".length)}`)];
  }
  if (actionId.startsWith("tray-option:")) {
    return [webTestIdSelector(`tray-option-${actionId.slice("tray-option:".length)}`)];
  }
  return [
    webTestIdSelector(actionId),
    webTestIdSelector(`parity:${actionId}`),
    webTestIdSelector(`map-selection-action-${actionId}`),
    webTestIdSelector(`plan-row-action-${actionId}`),
    webTestIdSelector(`plan-control-${actionId}`),
    webTestIdSelector(`cloud-action-${actionId}`),
  ];
}

export class WebSemanticJourneyDriver extends SemanticJourneyDriver {
  constructor(transport) {
    super("web");
    this.transport = transport;
  }

  async reset() {
    await this.navigateToOperationalApp(() => this.transport.reset());
  }

  async resetApplicationData() {
    await this.navigateToOperationalApp(() => this.transport.reset());
  }

  async resetApplicationDataExpectingStartupFailure() {
    await this.transport.reset();
  }

  async navigateToOperationalApp(navigate) {
    for (let attempt = 0; attempt < 2; attempt += 1) {
      await navigate();
      const outcome = (await observeUntil(
        "web application startup after navigation",
        async () => {
          const fatal = await this.readElement("startup-fatal-error");
          if (fatal) return { kind: "fatal", detail: fatal.text || "unknown failure" };
          const startup = await this.readProjection("parity:startup-state:");
          return startup.some((entry) => entry.id.startsWith("parity:startup-state:ready:true"))
            ? { kind: "ready" }
            : null;
        },
        {
          timeoutMs: E2E_TIMING.startupMs,
          intervalMs: E2E_TIMING.pollIntervalMs,
        },
      )).value;
      if (outcome.kind === "ready") return;
      if (attempt === 0 && this.transport.hasCanceledStartupModuleRequest()) continue;
      throw new TerminalObservationError("application startup failed", outcome.detail);
    }
  }

  async readCurrentPage() {
    for (const [pageId, selector] of Object.entries(WEB_PAGE_SELECTORS)) {
      if (await this.transport.visible(selector)) return { pageId, test_id: selector };
    }
    return null;
  }

  async readNavigationAction(pageId) {
    const selector = pageId === "home"
      ? '[data-testid="page-button-home"]'
      : webTestIdSelector(`home-button-${WEB_HOME_KEYS[pageId]}`);
    if (pageId !== "home" && !WEB_HOME_KEYS[pageId]) {
      throw new Error(`web page ${pageId} has no Home destination`);
    }
    if (!await this.transport.visible(selector)) return null;
    const value = await this.transport.readElement(selector);
    return value?.enabled && value.actionable ? value : null;
  }

  async activateNavigation(pageId, readyElement) {
    const selector = pageId === "home"
      ? '[data-testid="page-button-home"]'
      : webTestIdSelector(`home-button-${WEB_HOME_KEYS[pageId]}`);
    const expectedTestId = pageId === "home" ? "page-button-home" : `home-button-${WEB_HOME_KEYS[pageId]}`;
    if (readyElement?.test_id !== expectedTestId) {
      throw new Error(`web navigation to ${pageId} has no matching readiness evidence`);
    }
    await this.transport.click(selector, readyElement);
  }

  async openChooser(launcherId, readyElement) {
    if (readyElement?.test_id !== launcherId) {
      throw new Error(`${launcherId} has no matching readiness evidence`);
    }
    await this.transport.click(webTestIdSelector(launcherId), readyElement);
  }

  async readOption(launcherId, optionId) {
    const optionIds = launcherId === "plate-airport-button" && !optionId.includes(":")
      ? [`airport:${optionId}`, optionId]
      : [optionId];
    const option = await this.transport.firstExisting(
      optionIds.map((id) => webTestIdSelector(`tray-option-${id}`)),
    );
    if (!option) return null;
    const optionState = await this.transport.readElement(option);
    return optionState?.enabled && optionState.actionable ? optionState : null;
  }

  async selectOption(launcherId, optionId, readyElement) {
    const optionIds = launcherId === "plate-airport-button" && !optionId.includes(":")
      ? [`airport:${optionId}`, optionId]
      : [optionId];
    const expectedTestIds = optionIds.map((id) => `tray-option-${id}`);
    if (!expectedTestIds.includes(readyElement?.test_id)) {
      throw new Error(`${launcherId} option ${optionId} has no matching readiness evidence`);
    }
    await this.transport.click(webTestIdSelector(readyElement.test_id), readyElement);
  }

  async activateMapInspection({ x, y }) {
    return this.transport.pointerClick('[data-testid="map-surface"]', x, y);
  }

  async performAction(actionId, readyElement) {
    if (!readyElement?.test_id) {
      throw new Error(`web action ${actionId} has no readiness evidence`);
    }
    await this.transport.click(webTestIdSelector(readyElement.test_id), readyElement);
  }

  async readAction(actionId) {
    const selector = await this.transport.firstExisting(webActionSelectors(actionId));
    if (!selector) return null;
    const element = await this.transport.readElement(selector);
    return element?.enabled && element.actionable ? element : null;
  }

  async readSessionRevision() {
    return this.transport.page.evaluate("window.__aerobagE2e.render().session_revision");
  }

  async readCloudActionRevision() {
    return this.transport.page.evaluate(`Number(
      document.querySelector('[data-testid="cloud-overall-status"]')
        ?.getAttribute('data-e2e-action-revision') ?? -1
    )`);
  }

  async injectRasterLoadFault() {
    return this.transport.page.evaluate(`(() => {
      const inject = window.__aerobagE2e?.rasterFaultOnce;
      if (typeof inject !== "function") throw new Error("raster fault injector is unavailable");
      return inject();
    })()`);
  }

  async enterText(controlId, value, { submit = false, dismissKeyboard = false } = {}, readyElement = null) {
    if (submit || dismissKeyboard) {
      throw new Error("text editing cannot bundle submit or keyboard actions");
    }
    if (readyElement?.test_id !== controlId) {
      throw new Error(`web text control ${controlId} has no matching readiness evidence`);
    }
    await this.transport.enterText(webTestIdSelector(controlId), value, readyElement);
  }

  async focusText(controlId, readyElement) {
    if (readyElement?.test_id !== controlId) {
      throw new Error(`web text control ${controlId} has no matching focus evidence`);
    }
    await this.transport.focusText(webTestIdSelector(controlId), readyElement);
  }

  async submit(controlId, readyElement) {
    if (readyElement?.test_id !== controlId || !readyElement.focused) {
      throw new Error(`web text control ${controlId} has no focused readiness evidence`);
    }
    await this.transport.submit(webTestIdSelector(controlId), readyElement);
  }

  async drag(surfaceId, { x, y }) {
    if (surfaceId.startsWith("airport-info-modal:")) {
      await this.transport.wheel(webTestIdSelector(surfaceId), -y);
      return { scroll: -y };
    }
    return this.transport.drag(webTestIdSelector(surfaceId), x, y);
  }

  async zoom(surfaceId, amount) {
    await this.transport.wheel(webTestIdSelector(surfaceId), amount);
  }

  async hover(elementId) {
    await this.transport.hoverTestId(elementId);
  }

  async copyText(elementId) {
    return this.transport.copyTextTestId(elementId);
  }

  async readProjection(probe) {
    const translated = probe === "parity:plan-row:"
      ? "plan-row-"
      : probe === "parity:plan-row-action:"
        ? "plan-row-action-"
      : probe === "parity:plan-procedure-transition:"
        ? "plan-procedure-transition-"
        : probe === "parity:plan-procedure:"
          ? "plan-procedure-"
          : probe;
    return this.transport.collectTestIds(translated);
  }

  async findProjectionMatching(probe, needle) {
    const entries = await this.readProjection(probe);
    return entries.find((entry) => entry.text?.toUpperCase().includes(needle.toUpperCase())) ?? null;
  }

  async scanProjection(probe) {
    return this.readProjection(probe);
  }

  async revealProjectionMatching(probe, needle) {
    const entry = (await observeUntil(
      `${probe} projection matching ${needle}`,
      () => this.findProjectionMatching(probe, needle),
      {
        timeoutMs: E2E_TIMING.localReadyMs,
        intervalMs: E2E_TIMING.pollIntervalMs,
      },
    )).value;
    const elementId = entry?.id ?? entry?.test_id;
    if (!elementId) {
      throw new Error(`${probe} projection matching ${needle} has no semantic identity`);
    }

    // Discovery includes rendered offscreen controls so a long tray can be
    // searched. Reveal exactly that control once, then observe its hit-testable
    // readiness without rediscovering or repeatedly mutating the page.
    const revealed = await this.revealElement(elementId);
    if (!revealed?.actionable) {
      await observeUntil(
        `${elementId} actionable after reveal`,
        async () => {
          const current = await this.readElement(elementId);
          return current?.actionable ? current : null;
        },
        {
          timeoutMs: E2E_TIMING.localReadyMs,
          intervalMs: E2E_TIMING.pollIntervalMs,
        },
      );
    }
    return entry;
  }

  async readElement(elementId) {
    const exact = await this.transport.readElement(webTestIdSelector(elementId));
    if (exact?.visible) return exact;
    const parity = await this.transport.readElement(webTestIdSelector(`parity:${elementId}`));
    return parity?.visible ? parity : null;
  }

  async revealElement(elementId) {
    const selector = await this.transport.firstExisting([
      webTestIdSelector(elementId),
      webTestIdSelector(`parity:${elementId}`),
    ]);
    if (!selector) return null;
    await this.transport.revealElement(selector);
    return this.readElement(elementId);
  }

  async reload() {
    await this.navigateToOperationalApp(() => this.transport.reload());
  }

  async back() {
    const scrim = await this.transport.firstExisting([
      '.trayScrim',
      '[aria-label^="Close "]',
    ]);
    if (scrim) {
      if (await this.transport.clickIfVisible(scrim)) return;
    }
    await this.transport.page.send("Input.dispatchKeyEvent", {
      type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27,
    });
    await this.transport.page.send("Input.dispatchKeyEvent", {
      type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27,
    });
  }

  async captureFrame(path) {
    await this.transport.captureScreenshot(path);
    return path;
  }
}

const ANDROID_PAGE_TAGS = Object.freeze({
  map: "parity:map-surface",
  charts: "parity:page:plate",
  // Compose merges the page container into the structured plan-state node.
  flight_plan: "parity:plan-state:",
  altitude_planner: "parity:page:altitude_planner",
  data_status: "parity:page:data_status",
  settings: "parity:page:settings",
  home: "parity:page:home",
  cloud: "parity:page:cloud",
  offline_packages: "parity:page:offline_packages",
});

export function androidPageTag(pageId) {
  return ANDROID_PAGE_TAGS[pageId] ?? null;
}

const ANDROID_PAGE_ELEMENT_TAGS = Object.freeze({
  "ownship-source-button": "parity:ownship-launcher",
  "page:map": ANDROID_PAGE_TAGS.map,
  "page:plate": ANDROID_PAGE_TAGS.charts,
  "page:flight_plan": ANDROID_PAGE_TAGS.flight_plan,
  "page:altitude_planner": ANDROID_PAGE_TAGS.altitude_planner,
  "page:data_status": ANDROID_PAGE_TAGS.data_status,
  "page:settings": ANDROID_PAGE_TAGS.settings,
  "page:home": ANDROID_PAGE_TAGS.home,
  "page:cloud": ANDROID_PAGE_TAGS.cloud,
  "page:offline_packages": ANDROID_PAGE_TAGS.offline_packages,
});

const ANDROID_HOME_KEYS = Object.freeze({
  map: "Chart",
  charts: "Plate",
  flight_plan: "FlightPlan",
  altitude_planner: "AltitudePlanner",
  data_status: "DataStatus",
  settings: "Settings",
  cloud: "Cloud",
  offline_packages: "OfflinePackages",
});

const ANDROID_PERSISTED_PAGE_IDS = Object.freeze({
  Map: "map",
  Plan: "flight_plan",
  AltitudePlanner: "altitude_planner",
  Charts: "charts",
  Home: "home",
  DataStatus: "data_status",
  Settings: "settings",
  Cloud: "cloud",
  OfflinePackages: "offline_packages",
});

const ANDROID_LAYER_OPTION_IDS = Object.freeze({
  world_basemap: "WorldBasemap",
  vectors: "Vectors",
  metars: "Metars",
  nexrad: "Nexrad",
  traffic: "Traffic",
  terrain_warning: "TerrainWarning",
  offline_regions: "OfflineRegions",
});

function androidOptionTag(launcherId, optionId) {
  if (launcherId === "ownship-source-button") return `parity:ownship-source:${optionId}`;
  const androidOptionId = launcherId === "plate-airport-button"
    ? optionId.replace(/^airport:/, "")
    : launcherId === "layers-button"
      ? ANDROID_LAYER_OPTION_IDS[optionId] ?? optionId
      : optionId;
  return `parity:tray-option:${androidOptionId}`;
}

const ANDROID_PLAN_CONTROL_IDS = Object.freeze({
  undo: "Undo",
  redo: "Redo",
  activate_next_leg: "ActivateNextLeg",
  stop_navigation: "StopNavigation",
  toggle_sequencing_suspension: "ToggleSequencingSuspension",
  restore_direct_to: "RestoreDirectTo",
});

const ANDROID_CLOUD_ACTION_IDS = new Set([
  "begin_setup", "begin_create", "back_setup", "scan_setup_code", "accept_setup_code",
  "create_account", "backup_setup_code", "add_device", "close_linked_detail",
  "begin_unlink", "confirm_unlink", "sync_now", "copy_setup_code",
]);
const ANDROID_MAX_VIRTUALIZED_REVEAL_STEPS = 64;

export function androidActionCandidates(actionId) {
  if (actionId === "ownship-source-button") return ["parity:ownship-launcher"];
  if (actionId === "playback-play-toggle") return ["parity:playback-play-toggle"];
  if (actionId.startsWith("plan-row:")) {
    return [`parity:plan-row:${actionId.slice("plan-row:".length)}`];
  }
  if (actionId.startsWith("plan-procedure:")) {
    return [`parity:plan-procedure:${actionId.slice("plan-procedure:".length)}`];
  }
  if (actionId.startsWith("plan-procedure-transition:")) {
    return [`parity:plan-procedure-transition:${actionId.slice("plan-procedure-transition:".length)}`];
  }
  if (actionId.startsWith("plan-insert-suggestion:")) {
    return [`parity:plan-insert-suggestion:${actionId.slice("plan-insert-suggestion:".length)}`];
  }
  if (actionId.startsWith("plate-folder-tile:")) {
    return [`parity:plate-folder-tile:${actionId.slice("plate-folder-tile:".length)}`];
  }
  if (actionId.startsWith("tray-option:")) {
    return [`parity:tray-option:${actionId.slice("tray-option:".length)}`];
  }
  if (ANDROID_CLOUD_ACTION_IDS.has(actionId)) {
    return [`parity:cloud-action:${actionId}`];
  }
  return [
    androidSemanticTag(actionId),
    `parity:${actionId}`,
    `parity:map-selection-action:${actionId}`,
    `parity:plan-row-action:${actionId}`,
    `parity:plan-control:${ANDROID_PLAN_CONTROL_IDS[actionId] ?? actionId}`,
  ];
}

export function androidActionUsesSubmit(actionId) {
  return androidSemanticTag(actionId).startsWith("parity:chart-search-suggestion:");
}

export function androidElementMayRequireVerticalScroll(elementId) {
  const tag = androidSemanticTag(elementId);
  return ANDROID_CLOUD_ACTION_IDS.has(elementId) ||
    tag.startsWith("parity:cloud-action:") ||
    tag.startsWith("parity:settings-") ||
    tag.startsWith("parity:offline-product:") ||
    tag.startsWith("parity:offline-region:") ||
    tag.startsWith("parity:offline-zoom-level") ||
    tag === "parity:cloud-setup-code-output" ||
    tag.startsWith("parity:plan-airway-entry:") ||
    tag.startsWith("parity:plan-airway-exit:");
}

export function androidElementMayRequireHorizontalScroll(elementId) {
  return elementId.startsWith("plan-control:") ||
    elementId.startsWith("altitude-planner-control:") ||
    elementId === "altitude-planner-departure-basis";
}

export function androidProjectionMayRequireVerticalScan(probe) {
  const tag = androidSemanticTag(probe);
  return tag.startsWith("parity:data-status-row:") ||
    tag.startsWith("parity:offline-");
}

export function androidDataStatusRowsFromStateTag(tag) {
  const prefix = "parity:data-status-state:";
  if (!tag.startsWith(prefix)) return [];
  return tag.slice(prefix.length).split("|").flatMap((entry) => {
    const separator = entry.lastIndexOf("=");
    if (separator <= 0 || separator === entry.length - 1) return [];
    const rowId = entry.slice(0, separator);
    const severity = entry.slice(separator + 1);
    return [{
      id: `parity:data-status-row:${rowId}:severity:${severity}`,
      text: "",
      enabled: true,
      pressed: "false",
    }];
  });
}

export function androidSessionRevisionFromStateTag(tag) {
  const revision = tag.match(/:session_revision:(\d+)(?::|$)/)?.[1];
  return revision == null ? null : Number(revision);
}

export function findTagOrPrefix(xml, tag) {
  return findNode(xml, (node) => androidTag(node) === tag) ??
    findNode(xml, (node) => androidTag(node).startsWith(tag));
}

export function androidElementEnabled(node) {
  const projected = androidTag(node).match(/:enabled:(true|false)(?::|$)/)?.[1];
  return projected == null ? node.enabled === "true" : projected === "true";
}

function androidProjectedElement(node, elementId = androidTag(node)) {
  if (!node) return null;
  return {
    test_id: androidTag(node),
    text: node.text || "",
    enabled: androidElementEnabled(node),
    selected: node.selected === "true",
    checked: node.checked === "true",
    pressed: node.selected === "true" || node.checked === "true" ? "true" : "false",
    disabled_reason: node["state-description"] || null,
    expanded: elementId.startsWith("settings-section-")
      ? node.checked === "true" || node.selected === "true"
      : null,
    state: androidTag(node).match(/:state:([^:]+)(?::|$)/)?.[1] ?? null,
    bounds: node.bounds,
    semantic_path: node["semantic-path"],
    focused: node.focused === "true",
  };
}

function queryFirstAndroidSemanticNode(
  serial,
  tag,
  { allowPrefix = true, requireVisible = false, requireReachable = false } = {},
) {
  const choose = (nodes) => nodes?.find((node) =>
    (!requireVisible || node.visible === "true") &&
    (!requireReachable || node["center-reachable"] === "true")) ?? null;
  const exact = choose(queryAndroidSemanticNodes(serial, tag));
  if (exact || !allowPrefix) return exact;
  return choose(queryAndroidSemanticNodes(serial, tag, { prefix: true }));
}

function readinessEvidenceMatchesTag(expectedTag, readyElement) {
  const observedTag = readyElement?.test_id ?? readyElement?.["resource-id"];
  return observedTag === expectedTag &&
    Boolean(readyElement?.bounds) &&
    Boolean(readyElement?.semantic_path);
}

function activateAndroidSemanticTag(serial, tag, readyElement = null) {
  if (!readyElement) {
    throw new Error(`Android semantic action ${tag} has no readiness evidence`);
  }
  if (!readinessEvidenceMatchesTag(tag, readyElement)) {
    throw new Error(`Android semantic action ${tag} readiness evidence does not match`);
  }
  if (!clickAndroidSemanticNode(
    serial,
    tag,
    readyElement.bounds,
    readyElement.semantic_path,
    {
      selected: readyElement.selected,
      checked: readyElement.checked,
      stateDescription: readyElement.disabled_reason,
    },
  )) {
    throw new Error(`Android semantic action ${tag} was rejected`);
  }
}

export function androidPageIdFromStartupStateTag(tag) {
  const currentPage = tag.match(/:page:([^:]+)(?::|$)/)?.[1];
  const persistedPage = tag.match(/:persisted_page:([^:]+)(?::|$)/)?.[1];
  const page = currentPage ?? persistedPage;
  return page ? ANDROID_PERSISTED_PAGE_IDS[page] ?? null : null;
}

function visibleAndroidPage(serial) {
  const state = queryAndroidStartupProjection(serial);
  const page = state?.page ?? state?.persisted_page;
  const pageId = page ? ANDROID_PERSISTED_PAGE_IDS[page] ?? null : null;
  return pageId ? { pageId, state } : null;
}

export function androidZoomKeyCode(amount) {
  return amount < 0 ? "KEYCODE_PLUS" : "KEYCODE_MINUS";
}

export function androidSemanticTag(value) {
  if (value.startsWith("chart-search-suggestion-")) {
    return `parity:chart-search-suggestion:${value.slice("chart-search-suggestion-".length)}`;
  }
  for (const prefix of ["settings-toggle", "settings-section", "settings-choice", "settings-slider"]) {
    if (value.startsWith(`${prefix}-`)) return `parity:${prefix}:${value.slice(prefix.length + 1)}`;
  }
  if (value.startsWith("plan-control:")) {
    const controlId = value.slice("plan-control:".length);
    return `parity:plan-control:${ANDROID_PLAN_CONTROL_IDS[controlId] ?? controlId}`;
  }
  return value.startsWith("parity:") ? value : `parity:${value}`;
}

export function androidElementSemanticTag(elementId) {
  return ANDROID_PAGE_ELEMENT_TAGS[elementId] ?? androidSemanticTag(elementId);
}

export class AndroidSemanticJourneyDriver extends SemanticJourneyDriver {
  constructor(serial, {
    resetApp,
    resetApplicationData = null,
    reloadApp = null,
    pressBack = null,
  }) {
    super("android");
    this.serial = serial;
    this.resetApp = resetApp;
    this.resetApplicationDataCallback = resetApplicationData;
    this.reloadAppCallback = reloadApp;
    this.pressBackCallback = pressBack ?? (() => pressKey(this.serial, "KEYCODE_BACK"));
  }

  async waitForObservation(intervalMs) {
    // A full Compose accessibility-tree read is expensive. UI events wake
    // immediately; this backstop handles coalesced or missed notifications.
    waitForAndroidSemanticEvent(this.serial, Math.max(200, intervalMs));
  }

  async readCloudActionRevision() {
    const node = queryFirstAndroidSemanticNode(
      this.serial,
      "parity:cloud-action-revision:",
      { allowPrefix: true },
    );
    const revision = androidTag(node).match(/:cloud-action-revision:(\d+)(?::|$)/)?.[1];
    return revision == null ? -1 : Number(revision);
  }

  async reset() {
    await this.resetApp();
  }

  async resetApplicationData() {
    if (this.resetApplicationDataCallback) {
      await this.resetApplicationDataCallback();
    } else {
      await this.reset();
    }
  }

  async resetApplicationDataExpectingStartupFailure() {
    await this.resetApplicationData();
  }

  async readCurrentPage() {
    return visibleAndroidPage(this.serial);
  }

  async readPage(pageId) {
    const tag = androidPageTag(pageId);
    if (!tag) throw new Error(`Android page ${pageId} has no semantic page tag`);
    const node = queryFirstAndroidSemanticNode(
      this.serial,
      tag,
      {
        allowPrefix: tag.endsWith(":"),
        requireVisible: true,
      },
    );
    if (!node) return null;
    const current = visibleAndroidPage(this.serial);
    return current?.pageId === pageId ? { pageId, node } : null;
  }

  async readNavigationAction(pageId) {
    const tag = pageId === "home"
      ? "parity:button:HOME"
      : `parity:home-button:${ANDROID_HOME_KEYS[pageId]}`;
    if (pageId !== "home" && !ANDROID_HOME_KEYS[pageId]) {
      throw new Error(`Android page ${pageId} has no Home destination`);
    }
    const node = queryFirstAndroidSemanticNode(
      this.serial,
      tag,
      { allowPrefix: false, requireVisible: true, requireReachable: true },
    );
    return node && androidElementEnabled(node) ? androidProjectedElement(node) : null;
  }

  async activateNavigation(pageId, readyElement) {
    const tag = pageId === "home"
      ? "parity:button:HOME"
      : `parity:home-button:${ANDROID_HOME_KEYS[pageId]}`;
    activateAndroidSemanticTag(this.serial, tag, readyElement);
  }

  async openChooser(launcherId, readyElement) {
    await this.performAction(launcherId, readyElement);
  }

  async readOption(launcherId, optionId) {
    const node = queryFirstAndroidSemanticNode(
      this.serial,
      androidOptionTag(launcherId, optionId),
      { requireVisible: true, requireReachable: true },
    );
    if (!node || !androidElementEnabled(node)) return null;
    return androidProjectedElement(node);
  }

  async selectOption(launcherId, optionId, readyElement) {
    activateAndroidSemanticTag(
      this.serial,
      androidOptionTag(launcherId, optionId),
      readyElement,
    );
  }

  async activateMapInspection({ x, y }, readyElement) {
    if (!readyElement?.bounds) throw new Error("Android map surface has no semantic bounds");
    const bounds = rectOfBounds(readyElement.bounds);
    const px = bounds.left + Math.round(bounds.width * x);
    const py = bounds.top + Math.round(bounds.height * y);
    adb(this.serial, ["shell", "input", "tap", String(px), String(py)]);
  }

  async performAction(actionId, readyElement = null) {
    if (androidActionUsesSubmit(actionId)) {
      if (!readyElement) throw new Error(`Android submit action ${actionId} has no readiness evidence`);
      pressKey(this.serial, "KEYCODE_ENTER");
      return;
    }
    const readyTag = readyElement?.test_id ?? readyElement?.["resource-id"];
    if (!readyTag) {
      throw new Error(`Android action ${actionId} has no readiness evidence`);
    }
    activateAndroidSemanticTag(this.serial, readyTag, readyElement);
  }

  async readRepeatedAction(actionId, retainedTarget) {
    const retainedTag = retainedTarget?.test_id ?? retainedTarget?.["resource-id"];
    if (
      !retainedTag ||
      !androidActionCandidates(actionId).includes(retainedTag) ||
      !readinessEvidenceMatchesTag(retainedTag, retainedTarget) ||
      !retainedTarget.enabled
    ) {
      throw new Error(`Android repeated action ${actionId} has no retained exact target`);
    }
    return retainedTarget;
  }

  async performRepeatedAction(actionId, retainedTarget, readyElement) {
    if (readyElement !== retainedTarget) {
      throw new Error(`Android repeated action ${actionId} did not retain its readiness target`);
    }
    const retainedTag = retainedTarget?.test_id ?? retainedTarget?.["resource-id"];
    if (
      !retainedTag ||
      !androidActionCandidates(actionId).includes(retainedTag) ||
      !readinessEvidenceMatchesTag(retainedTag, retainedTarget)
    ) {
      throw new Error(`Android repeated action ${actionId} target no longer matches its identity`);
    }
    const bounds = rectOfBounds(retainedTarget.bounds);
    adb(this.serial, [
      "shell",
      "input",
      "tap",
      String(Math.round(bounds.left + bounds.width / 2)),
      String(Math.round(bounds.top + bounds.height / 2)),
    ]);
  }

  async readAction(actionId) {
    for (const candidate of androidActionCandidates(actionId)) {
      const node = queryFirstAndroidSemanticNode(
        this.serial,
        candidate,
        { requireVisible: true, requireReachable: true },
      );
      if (!node || !androidElementEnabled(node)) continue;
      return androidProjectedElement(node);
    }
    return null;
  }

  async readSessionRevision() {
    const revision = queryAndroidStartupProjection(this.serial)?.session_revision;
    return revision == null ? null : Number(revision);
  }

  async enterText(
    controlId,
    value,
    { submit = false, dismissKeyboard = false } = {},
    readyElement = null,
  ) {
    const semanticTag = `parity:${controlId}`;
    if (submit || dismissKeyboard) {
      throw new Error("text editing cannot bundle submit or keyboard actions");
    }
    if (
      readyElement?.test_id !== semanticTag ||
      !readyElement.bounds ||
      !readyElement.semantic_path
    ) {
      throw new Error(`Android text control ${controlId} has no matching readiness evidence`);
    }
    if (!setAndroidSemanticText(
      this.serial,
      semanticTag,
      value,
      readyElement.bounds,
      readyElement.semantic_path,
    )) {
      throw new Error(
        `Android semantic text action is unavailable for ${controlId}; refusing to retry a synthetic user edit`,
      );
    }
  }

  async focusText(controlId, readyElement) {
    const semanticTag = `parity:${controlId}`;
    if (!readinessEvidenceMatchesTag(semanticTag, readyElement)) {
      throw new Error(`Android text control ${controlId} has no matching focus evidence`);
    }
    activateAndroidSemanticTag(this.serial, semanticTag, readyElement);
  }

  async submit(controlId, readyElement) {
    const semanticTag = `parity:${controlId}`;
    if (
      readyElement?.test_id !== semanticTag ||
      !readyElement.bounds ||
      !readyElement.focused
    ) {
      throw new Error(`Android text control ${controlId} has no focused readiness evidence`);
    }
    pressKey(this.serial, "KEYCODE_ENTER");
  }

  async drag(surfaceId, { x, y }) {
    const node = findTagOrPrefix(dumpAndroid(this.serial), `parity:${surfaceId}`);
    if (!node) throw new Error(`Android surface ${surfaceId} is not visible`);
    const bounds = rectOfBounds(node.bounds);
    // The chart's instrument grid occupies its center. Start shared map drags
    // on exposed chart pixels so the gesture reaches the map input layer.
    const startX = bounds.left + bounds.width * 0.75;
    const startY = bounds.top + bounds.height * 0.72;
    const endX = Math.max(bounds.left + 8, Math.min(bounds.right - 8, startX + x));
    const endY = Math.max(bounds.top + 8, Math.min(bounds.bottom - 8, startY + y));
    swipe(this.serial, startX, startY, endX, endY, 650);
    return { startX, startY, endX, endY };
  }

  async zoom(surfaceId, amount) {
    const node = findTagOrPrefix(dumpAndroid(this.serial), `parity:${surfaceId}`);
    if (!node) throw new Error(`Android surface ${surfaceId} is not visible`);
    adb(this.serial, [
      "shell", "input", "keyevent", androidZoomKeyCode(amount),
    ]);
  }

  async readProjection(probe) {
    const prefix = androidSemanticTag(probe);
    if (prefix.startsWith("parity:map-selection-selected:")) {
      const expected = prefix.slice("parity:map-selection-selected:".length);
      const queried = queryAndroidExactProjection(
        this.serial,
        ANDROID_EXACT_SCALAR_PROJECTIONS.get("parity:map-selection-state:"),
      );
      const state = queried[0]?.["state-description"] ?? "";
      const selected = /^selected:([^:]+):/.exec(state)?.[1] ?? "none";
      return selected === expected ? [{
        id: prefix,
        text: "",
        enabled: true,
        pressed: null,
        state,
      }] : [];
    }
    if (ANDROID_EXACT_SCALAR_PROJECTIONS.has(prefix)) {
      const queried = queryAndroidExactProjection(
        this.serial,
        ANDROID_EXACT_SCALAR_PROJECTIONS.get(prefix),
      );
      return queried.map((node) => ({
        id: `${prefix}${node["state-description"] ?? ""}`,
        text: node.text || "",
        enabled: androidElementEnabled(node),
        pressed: null,
        state: node["state-description"] || null,
      }));
    }
    if (prefix === "parity:data-status-row:") {
      const stateNode = queryFirstAndroidSemanticNode(
        this.serial,
        "parity:data-status-state:",
      );
      if (stateNode) return androidDataStatusRowsFromStateTag(androidTag(stateNode));
    }
    const queried = queryAndroidSemanticNodes(this.serial, prefix, { prefix: true });
    if (queried) {
      return queried.map((node) => ({
        id: androidTag(node),
        text: node.text || "",
        enabled: androidElementEnabled(node),
        pressed: node.selected === "true" || node.checked === "true" ? "true" : "false",
        state: node["state-description"] || null,
      }));
    }
    const xml = dumpAndroid(this.serial);
    const collect = (xml) => findNodes(xml, (node) => androidTag(node).startsWith(prefix))
      .map((node) => ({
        id: androidTag(node),
        text: androidNodeLabel(xml, node) || node.text || "",
        enabled: androidElementEnabled(node),
        pressed: node.selected === "true" || node.checked === "true" ? "true" : "false",
        state: node["state-description"] || null,
      }));
    return collect(xml);
  }

  async scanProjection(probe) {
    const prefix = androidSemanticTag(probe);
    const collect = (xml) => findNodes(xml, (node) => androidTag(node).startsWith(prefix))
      .map((node) => ({
        id: androidTag(node),
        text: androidNodeLabel(xml, node) || node.text || "",
        enabled: androidElementEnabled(node),
        pressed: node.selected === "true" || node.checked === "true" ? "true" : "false",
      }));
    if (!androidProjectionMayRequireVerticalScan(probe)) return collect(dumpAndroid(this.serial));

    const accumulated = new Map();
    for (const direction of ["down", "up"]) {
      let previousSignature = null;
      let unchangedFrames = 0;
      for (let attempt = 0; attempt < 24; attempt += 1) {
        const xml = dumpAndroid(this.serial);
        const visible = collect(xml);
        for (const entry of visible) accumulated.set(entry.id, entry);
        const signature = visible.map((entry) => entry.id).join("\n");
        unchangedFrames = signature === previousSignature ? unchangedFrames + 1 : 0;
        previousSignature = signature;
        if (unchangedFrames >= 2) break;
        const scrollSurface = findVerticalScrollSurface(xml);
        if (!scrollSurface) break;
        if (!await scrollAndroidAndAwait(this.serial, scrollSurface.bounds, direction)) break;
      }
    }
    return [...accumulated.values()];
  }

  async findProjectionMatching(probe, needle) {
    const normalizedNeedle = needle.toUpperCase();
    return (await this.readProjection(probe))
      .find((entry) => entry.text.toUpperCase().includes(normalizedNeedle)) ?? null;
  }

  async revealProjectionMatching(probe, needle) {
    const prefix = androidSemanticTag(probe);
    const normalizedNeedle = needle.toUpperCase();
    const projectionEntries = (xml) => findNodes(
      xml,
      (node) => androidTag(node).startsWith(prefix),
    ).map((node) => ({
        id: androidTag(node),
        text: androidNodeLabel(xml, node) || node.text || "",
        enabled: androidElementEnabled(node),
        pressed: node.selected === "true" || node.checked === "true" ? "true" : "false",
      }));
    const findMatch = (xml) => projectionEntries(xml)
      .find((entry) => entry.text.toUpperCase().includes(normalizedNeedle)) ?? null;
    let initialXml;
    try {
      initialXml = (await observeUntil(
        `${probe} rendered collection`,
        () => {
          const xml = dumpAndroid(this.serial);
          return projectionEntries(xml).length > 0 ? xml : null;
        },
        {
          timeoutMs: E2E_TIMING.localReadyMs,
          intervalMs: E2E_TIMING.pollIntervalMs,
        },
      )).value;
    } catch (_error) {
      return null;
    }
    const initialMatch = findMatch(initialXml);
    if (initialMatch) return initialMatch;
    for (const direction of ["down", "up"]) {
      let previousSignature = null;
      let unchangedFrames = 0;
      for (let attempt = 0; attempt < ANDROID_MAX_VIRTUALIZED_REVEAL_STEPS; attempt += 1) {
        const xml = dumpAndroid(this.serial);
        const match = findMatch(xml);
        if (match) return match;
        const signature = projectionEntries(xml).map((entry) => entry.id).join("\n");
        unchangedFrames = signature === previousSignature ? unchangedFrames + 1 : 0;
        previousSignature = signature;
        if (unchangedFrames >= 2) break;
        const scrollSurface = findVerticalScrollSurface(xml);
        if (!scrollSurface) break;
        if (!await scrollAndroidAndAwait(this.serial, scrollSurface.bounds, direction)) break;
      }
    }
    return findMatch(dumpAndroid(this.serial));
  }

  async readElement(elementId) {
    if (elementId === "software-keyboard") {
      return androidImeVisible(dumpAndroid(this.serial)) ? {
        test_id: elementId,
        text: "Android software keyboard",
        enabled: true,
      } : null;
    }
    if (elementId.startsWith("installed-package:")) {
      const filename = elementId.slice("installed-package:".length);
      const listing = adb(this.serial, [
        "shell", "run-as", "org.aerobag.app", "ls", "files/packages",
      ]);
      return listing.split(/\r?\n/).includes(filename) ? {
        test_id: elementId,
        text: filename,
        enabled: true,
      } : null;
    }
    if (elementId === "external-page:about") {
      const activities = adb(this.serial, ["shell", "dumpsys", "activity", "activities"]);
      return activities.includes("https://aerobag.org/about") ? {
        test_id: elementId,
        text: "https://aerobag.org/about",
        enabled: true,
      } : null;
    }
    const semanticTag = androidElementSemanticTag(elementId);
    const queried = queryFirstAndroidSemanticNode(
      this.serial,
      semanticTag,
      { requireVisible: true },
    );
    if (!queried) return null;
    return androidProjectedElement(queried, elementId);
  }

  async revealElement(elementId) {
    const semanticTag = androidElementSemanticTag(elementId);
    const reachable = () => {
      const node = queryFirstAndroidSemanticNode(
        this.serial,
        semanticTag,
        { requireVisible: true, requireReachable: true },
      );
      return androidProjectedElement(node, elementId);
    };
    const existing = reachable();
    if (existing) return existing;
    if (androidElementMayRequireHorizontalScroll(elementId)) {
      await scrollHorizontallyUntilTag(this.serial, semanticTag, 8);
      const horizontal = reachable();
      if (horizontal) return horizontal;
    }
    if (!await scrollUntilTagPrefix(this.serial, semanticTag, 20, true)) return null;
    return reachable();
  }

  async reload() {
    if (this.reloadAppCallback) {
      await this.reloadAppCallback();
      return;
    }
    adb(this.serial, ["shell", "am", "force-stop", "org.aerobag.app"]);
    adb(this.serial, ["shell", "am", "start", "-n", "org.aerobag.app/.MainActivity"]);
  }

  async back() {
    this.pressBackCallback();
  }

  async captureFrame(path) {
    writeFileSync(path, screencapPng(this.serial));
    return path;
  }
}

export function validateSemanticDriver(driver) {
  for (const operation of SEMANTIC_DRIVER_OPERATIONS) {
    if (typeof driver?.[operation] !== "function") {
      throw new Error(`semantic driver is missing ${operation}`);
    }
  }
  return driver;
}
