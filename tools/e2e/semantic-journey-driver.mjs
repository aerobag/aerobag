// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { writeFileSync } from "node:fs";
import {
  adb, androidImeVisible, androidNodeLabel, androidTag, displayBoundsFromXml, dumpAndroid, findNode, findNodes,
  findVerticalScrollSurface, pressKey, rectOfBounds, screencapPng,
  scrollAndroidAndAwait, setAndroidSemanticText,
  scrollUntilTag, scrollUntilTagPrefix, swipe,
  scrollHorizontallyUntilTag, tapTag, waitFor,
} from "./android-harness.mjs";
import { E2E_TIMING, observeUntil } from "./transition-contract.mjs";

export const SEMANTIC_DRIVER_OPERATIONS = Object.freeze([
  "reset", "resetApplicationData", "openPage", "chooseOption", "inspectMapAt", "performAction",
  "enterText", "submit", "drag", "zoom", "hover", "copyText", "readElement", "readProjection",
  "findProjectionMatching", "revealElement", "scanProjection", "revealProjectionMatching", "reload",
  "back", "captureFrame",
]);

export class SemanticJourneyDriver {
  constructor(platform) {
    this.platform = platform;
  }

  async reset() { throw new Error(`${this.platform} driver does not implement reset`); }
  async resetApplicationData() { throw new Error(`${this.platform} driver does not implement resetApplicationData`); }
  async openPage(_pageId) { throw new Error(`${this.platform} driver does not implement openPage`); }
  async chooseOption(_launcherId, _optionId) { throw new Error(`${this.platform} driver does not implement chooseOption`); }
  async inspectMapAt(_point) { throw new Error(`${this.platform} driver does not implement inspectMapAt`); }
  async performAction(_actionId) { throw new Error(`${this.platform} driver does not implement performAction`); }
  async enterText(_controlId, _value, _options) { throw new Error(`${this.platform} driver does not implement enterText`); }
  async submit(_controlId) { throw new Error(`${this.platform} driver does not implement submit`); }
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

export class WebSemanticJourneyDriver extends SemanticJourneyDriver {
  constructor(transport) {
    super("web");
    this.transport = transport;
  }

  async reset() {
    await this.transport.reset();
  }

  async resetApplicationData() {
    await this.transport.reset();
  }

  async openPage(pageId) {
    const target = WEB_PAGE_SELECTORS[pageId];
    if (!target) throw new Error(`unsupported web page ${pageId}`);
    if (await this.transport.visible(target)) return;
    if (pageId === "home") {
      await this.transport.click('[data-testid="page-button-home"]');
    } else {
      if (!await this.transport.visible('[data-testid="parity:page:home"]')) {
        await this.transport.click('[data-testid="page-button-home"]');
        await this.transport.waitFor(
          '[data-testid="parity:page:home"]',
          "home page before destination navigation",
        );
      }
      const homeKey = WEB_HOME_KEYS[pageId];
      if (!homeKey) throw new Error(`web page ${pageId} has no Home destination`);
      await this.transport.click(`[data-testid="home-button-${homeKey}"]`);
    }
    await this.transport.waitFor(target, `${pageId} page`);
  }

  async chooseOption(launcherId, optionId) {
    await this.transport.click(`[data-testid="${launcherId}"]`);
    const optionIds = launcherId === "plate-airport-button" && !optionId.includes(":")
      ? [`airport:${optionId}`, optionId]
      : [optionId];
    const option = await this.transport.waitForFirstVisible(
      optionIds.map((id) => `[data-testid="tray-option-${id}"]`),
      `${launcherId} option ${optionId}`,
    );
    const optionState = await this.transport.readElement(option);
    if (!optionState?.enabled) {
      throw new Error(
        `${launcherId} option ${optionId} is disabled${optionState?.disabled_reason ? `: ${optionState.disabled_reason}` : ""}`,
      );
    }
    const previousSelectionState = await this.transport.optionSelectionState(option);
    await this.transport.click(option);
    await this.transport.waitForOptionSelection(
      option,
      previousSelectionState,
      `${launcherId} selection ${optionId}`,
    );
  }

  async inspectMapAt({ x, y }) {
    const probe = await this.transport.pointerClick('[data-testid="map-surface"]', x, y);
    try {
      await this.transport.waitFor('[data-testid="map-selection-tray"]', "map inspector");
    } catch (error) {
      throw new Error(`${error.message}; pointer delivery=${JSON.stringify(probe)}`);
    }
  }

  async performAction(actionId) {
    if (actionId === "dismiss-plan-row-tray") {
      const scrim = await this.readElement("plan-row-tray-scrim");
      if (scrim) await this.performAction("plan-row-tray-scrim");
      return;
    }
    if (actionId.startsWith("plan-row:")) {
      await this.transport.clickTestId(`plan-row-${actionId.slice("plan-row:".length)}`);
      return;
    }
    if (actionId.startsWith("plan-procedure:")) {
      await this.transport.clickTestId(`plan-procedure-${actionId.slice("plan-procedure:".length)}`);
      return;
    }
    if (actionId.startsWith("plan-procedure-transition:")) {
      await this.transport.clickTestId(`plan-procedure-transition-${actionId.slice("plan-procedure-transition:".length)}`);
      return;
    }
    if (actionId.startsWith("plan-insert-suggestion:")) {
      await this.transport.clickTestId(`plan-insert-suggestion-${actionId.slice("plan-insert-suggestion:".length)}`);
      return;
    }
    if (actionId.startsWith("plate-folder-tile:")) {
      await this.transport.clickTestId(`plate-folder-tile:${actionId.slice("plate-folder-tile:".length)}`);
      return;
    }
    if (actionId.startsWith("tray-option:")) {
      await this.transport.clickTestId(`tray-option-${actionId.slice("tray-option:".length)}`);
      return;
    }
    const selectors = [
      `[data-testid="${actionId}"]`,
      `[data-testid="parity:${actionId}"]`,
      `[data-testid="map-selection-action-${actionId}"]`,
      `[data-testid="plan-row-action-${actionId}"]`,
      `[data-testid="plan-control-${actionId}"]`,
      `[data-testid="cloud-action-${actionId}"]`,
    ];
    const selector = await this.transport.firstExisting(selectors);
    if (!selector) throw new Error(`web action ${actionId} is not visible`);
    await this.transport.click(selector);
  }

  async enterText(controlId, value, options = {}) {
    await this.transport.enterText(`[data-testid="${controlId}"]`, value, options);
  }

  async submit(controlId) {
    await this.transport.submit(`[data-testid="${controlId}"]`);
  }

  async drag(surfaceId, { x, y }) {
    if (surfaceId.startsWith("airport-info-modal:")) {
      await this.transport.wheel(`[data-testid="${surfaceId}"]`, -y);
      return { scroll: -y };
    }
    return this.transport.drag(`[data-testid="${surfaceId}"]`, x, y);
  }

  async zoom(surfaceId, amount) {
    await this.transport.wheel(`[data-testid="${surfaceId}"]`, amount);
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
    return (await observeUntil(
      `${probe} projection matching ${needle}`,
      () => this.findProjectionMatching(probe, needle),
      {
        timeoutMs: E2E_TIMING.localReadyMs,
        intervalMs: E2E_TIMING.pollIntervalMs,
      },
    )).value;
  }

  async readElement(elementId) {
    const exact = await this.transport.readElement(`[data-testid="${elementId}"]`);
    if (exact?.visible) return exact;
    const parity = await this.transport.readElement(`[data-testid="parity:${elementId}"]`);
    return parity?.visible ? parity : null;
  }

  async revealElement(elementId) {
    const selector = await this.transport.firstExisting([
      `[data-testid="${elementId}"]`,
      `[data-testid="parity:${elementId}"]`,
    ]);
    if (!selector) return null;
    await this.transport.revealElement(selector);
    return this.readElement(elementId);
  }

  async reload() {
    await this.transport.reload();
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

const ANDROID_LAYER_OPTION_IDS = Object.freeze({
  world_basemap: "WorldBasemap",
  vectors: "Vectors",
  metars: "Metars",
  nexrad: "Nexrad",
  traffic: "Traffic",
  terrain_warning: "TerrainWarning",
  offline_regions: "OfflineRegions",
});

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

export function androidActionCandidates(actionId) {
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
  return actionId.startsWith("chart-search-suggestion:");
}

export function androidTextControlNeedsTap(node) {
  return node?.focused !== "true";
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

export function findTagOrPrefix(xml, tag) {
  return findNode(xml, (node) => androidTag(node) === tag) ??
    findNode(xml, (node) => androidTag(node).startsWith(tag));
}

export function androidElementFallback(xml, elementId) {
  if (elementId !== "plan-insert-airport-input") return null;
  if (!findTagOrPrefix(xml, "parity:button:Enter")) return null;
  const fields = findNodes(xml, (node) => node.class === "android.widget.EditText");
  return fields.find((node) => node.focused === "true") ?? (fields.length === 1 ? fields[0] : null);
}

export function androidElementEnabled(node) {
  const projected = androidTag(node).match(/:enabled:(true|false)(?::|$)/)?.[1];
  return projected == null ? node.enabled === "true" : projected === "true";
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

  async openPage(pageId) {
    const target = androidPageTag(pageId);
    if (!target) throw new Error(`unsupported Android page ${pageId}`);
    if (findTagOrPrefix(dumpAndroid(this.serial), target)) return;
    if (pageId === "home") {
      await tapTag(this.serial, "parity:button:HOME", E2E_TIMING.localReadyMs);
    } else {
      if (!findTagOrPrefix(dumpAndroid(this.serial), ANDROID_PAGE_TAGS.home)) {
        await tapTag(this.serial, "parity:button:HOME", E2E_TIMING.localReadyMs);
        await waitFor(
          () => findTagOrPrefix(dumpAndroid(this.serial), ANDROID_PAGE_TAGS.home) !== null,
          E2E_TIMING.localReadyMs,
          "Android Home navigation did not land after one tap",
          E2E_TIMING.pollIntervalMs,
        );
      }
      await tapTag(this.serial, `parity:home-button:${ANDROID_HOME_KEYS[pageId]}`, E2E_TIMING.localReadyMs);
    }
    await waitFor(
      () => findTagOrPrefix(dumpAndroid(this.serial), target) !== null,
      E2E_TIMING.localReadyMs,
      `${pageId} page`,
      E2E_TIMING.pollIntervalMs,
    );
  }

  async chooseOption(launcherId, optionId) {
    if (launcherId === "ownship-source-button") {
      await tapTag(this.serial, "parity:ownship-launcher", E2E_TIMING.localReadyMs);
      await tapTag(this.serial, `parity:ownship-source:${optionId}`, E2E_TIMING.localReadyMs);
      return;
    }
    await tapTag(this.serial, `parity:${launcherId}`, E2E_TIMING.localReadyMs);
    const androidOptionId = launcherId === "plate-airport-button"
      ? optionId.replace(/^airport:/, "")
      : launcherId === "layers-button"
        ? ANDROID_LAYER_OPTION_IDS[optionId] ?? optionId
      : optionId;
    await tapTag(this.serial, `parity:tray-option:${androidOptionId}`, E2E_TIMING.localReadyMs);
  }

  async inspectMapAt({ x, y }) {
    const surface = findTagOrPrefix(dumpAndroid(this.serial), "parity:map-surface");
    if (!surface) throw new Error("Android map surface is not visible");
    const bounds = rectOfBounds(surface.bounds);
    const px = bounds.left + Math.round(bounds.width * x);
    const py = bounds.top + Math.round(bounds.height * y);
    adb(this.serial, ["shell", "input", "tap", String(px), String(py)]);
    await waitFor(
      () => findTagOrPrefix(dumpAndroid(this.serial), "parity:map-selection-tray") !== null,
      E2E_TIMING.localReadyMs,
      "map inspector",
    );
  }

  async performAction(actionId) {
    if (actionId === "dismiss-plan-row-tray") {
      const xml = dumpAndroid(this.serial);
      if (!findTagOrPrefix(xml, "parity:plan-row-action:")) return;
      await tapTag(this.serial, "parity:plan-row-tray-scrim", E2E_TIMING.localReadyMs);
      await waitFor(
        () => !findTagOrPrefix(dumpAndroid(this.serial), "parity:plan-row-action:"),
        E2E_TIMING.localReadyMs,
        "dismissed flight-plan row tray",
      );
      return;
    }
    if (actionId === "ownship-source-button") {
      await tapTag(this.serial, "parity:ownship-launcher", E2E_TIMING.localReadyMs);
      return;
    }
    if (actionId === "playback-play-toggle") {
      const node = findTagOrPrefix(dumpAndroid(this.serial), "parity:playback-play-toggle");
      if (!node) throw new Error("Android playback toggle is not visible");
      const bounds = rectOfBounds(node.bounds);
      adb(this.serial, [
        "shell", "input", "tap",
        String(Math.round(bounds.left + bounds.width / 2)),
        String(Math.round(bounds.top + bounds.height / 2)),
      ]);
      return;
    }
    if (actionId.startsWith("plan-row:")) {
      await tapTag(this.serial, `parity:plan-row:${actionId.slice("plan-row:".length)}`, E2E_TIMING.localReadyMs);
      return;
    }
    if (actionId.startsWith("plan-procedure:")) {
      await tapTag(this.serial, `parity:plan-procedure:${actionId.slice("plan-procedure:".length)}`, E2E_TIMING.localReadyMs);
      return;
    }
    if (actionId.startsWith("plan-procedure-transition:")) {
      await tapTag(this.serial, `parity:plan-procedure-transition:${actionId.slice("plan-procedure-transition:".length)}`, E2E_TIMING.localReadyMs);
      return;
    }
    if (actionId.startsWith("plan-insert-suggestion:")) {
      await tapTag(this.serial, `parity:plan-insert-suggestion:${actionId.slice("plan-insert-suggestion:".length)}`, E2E_TIMING.localReadyMs);
      return;
    }
    if (actionId.startsWith("plate-folder-tile:")) {
      await tapTag(this.serial, `parity:plate-folder-tile:${actionId.slice("plate-folder-tile:".length)}`, E2E_TIMING.localReadyMs);
      return;
    }
    if (androidActionUsesSubmit(actionId)) {
      const tag = `parity:${actionId}`;
      if (!findTagOrPrefix(dumpAndroid(this.serial), tag)) {
        throw new Error(`Android action ${actionId} is not visible`);
      }
      pressKey(this.serial, "KEYCODE_ENTER");
      return;
    }
    if (actionId.startsWith("tray-option:")) {
      await tapTag(this.serial, `parity:tray-option:${actionId.slice("tray-option:".length)}`, E2E_TIMING.localReadyMs);
      return;
    }
    const candidates = androidActionCandidates(actionId);
    let xml = dumpAndroid(this.serial);
    const initialCandidate = candidates.find((candidate) => findTagOrPrefix(xml, candidate));
    let tag = initialCandidate ? androidTag(findTagOrPrefix(xml, initialCandidate)) : null;
    if (!tag && androidElementMayRequireHorizontalScroll(actionId)) {
      for (const candidate of candidates) {
        if (!await scrollHorizontallyUntilTag(this.serial, candidate, 8)) continue;
        xml = dumpAndroid(this.serial);
        tag = androidTag(findTagOrPrefix(xml, candidate));
        break;
      }
    }
    if (!tag || androidElementMayRequireVerticalScroll(actionId)) {
      if (androidElementMayRequireVerticalScroll(actionId)) tag = null;
      for (const candidate of candidates) {
        if (!await scrollUntilTagPrefix(
          this.serial, candidate, 8, androidElementMayRequireVerticalScroll(actionId),
        )) continue;
        tag = candidate;
        xml = dumpAndroid(this.serial);
        tag = androidTag(findTagOrPrefix(xml, candidate));
        break;
      }
    }
    if (!tag) throw new Error(`Android action ${actionId} is not visible`);
    if (!await scrollUntilTag(this.serial, tag, 8)) throw new Error(`Android action ${actionId} cannot be reached`);
    await tapTag(this.serial, tag, E2E_TIMING.localReadyMs);
  }

  async enterText(controlId, value, { submit = false, dismissKeyboard = false } = {}) {
    const semanticTag = `parity:${controlId}`;
    const focusControl = async (forceTap) => {
      let xml = dumpAndroid(this.serial);
      let tagged = findTagOrPrefix(xml, semanticTag);
      if (!tagged && await scrollUntilTagPrefix(this.serial, semanticTag, 20)) {
        xml = dumpAndroid(this.serial);
        tagged = findTagOrPrefix(xml, semanticTag);
      }
      const fallback = tagged ? null : androidElementFallback(xml, controlId);
      if (tagged) {
        if (forceTap || androidTextControlNeedsTap(tagged)) {
          await tapTag(this.serial, semanticTag, E2E_TIMING.localReadyMs);
        }
        return;
      }
      if (fallback) {
        if (forceTap || androidTextControlNeedsTap(fallback)) {
          const bounds = rectOfBounds(fallback.bounds);
          adb(this.serial, [
            "shell", "input", "tap",
            String(Math.round(bounds.left + bounds.width / 2)),
            String(Math.round(bounds.top + bounds.height / 2)),
          ]);
        }
        return;
      }
      throw new Error(`Android text control ${controlId} is not visible`);
    };

    await focusControl(false);
    if (controlId === "plan-append-route-input") {
      await waitFor(
        () => {
          const state = findTagOrPrefix(
            dumpAndroid(this.serial),
            "parity:plan-append-route-state:can_commit:",
          );
          return state ? androidTag(state).endsWith(":ready_for_input:true") : false;
        },
        E2E_TIMING.localReadyMs,
        "Android flight-plan editor did not finish dismissing its overlay",
        E2E_TIMING.pollIntervalMs,
      );
    }
    if (!setAndroidSemanticText(this.serial, semanticTag, value)) {
      throw new Error(
        `Android semantic text action is unavailable for ${controlId}; refusing to retry a synthetic user edit`,
      );
    }
    await waitFor(
      () => findTagOrPrefix(dumpAndroid(this.serial), semanticTag)?.text === value,
      E2E_TIMING.localReadyMs,
      `Android semantic text action did not commit ${controlId}`,
      E2E_TIMING.pollIntervalMs,
    );
    if (submit) pressKey(this.serial, "KEYCODE_ENTER");
    if (dismissKeyboard && androidImeVisible(dumpAndroid(this.serial))) {
      pressKey(this.serial, "KEYCODE_BACK");
    }
  }

  async submit(controlId) {
    const semanticTag = `parity:${controlId}`;
    const node = findTagOrPrefix(dumpAndroid(this.serial), semanticTag);
    if (!node) throw new Error(`Android text control ${controlId} is not rendered for submit`);
    if (node.focused !== "true") {
      throw new Error(`Android text control ${controlId} lost focus before submit`);
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
    const xml = dumpAndroid(this.serial);
    if (prefix === "parity:data-status-row:") {
      const stateNode = findNode(xml, (node) =>
        androidTag(node).startsWith("parity:data-status-state:"));
      if (stateNode) return androidDataStatusRowsFromStateTag(androidTag(stateNode));
    }
    const collect = (xml) => findNodes(xml, (node) => androidTag(node).startsWith(prefix))
      .map((node) => ({
        id: androidTag(node),
        text: androidNodeLabel(xml, node) || node.text || "",
        enabled: androidElementEnabled(node),
        pressed: node.selected === "true" || node.checked === "true" ? "true" : "false",
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
    let visibleMatch = null;
    try {
      await waitFor(() => {
        visibleMatch = findMatch(dumpAndroid(this.serial));
        return visibleMatch !== null;
      }, E2E_TIMING.localReadyMs, `${probe} projection matching ${needle}`, E2E_TIMING.pollIntervalMs);
      return visibleMatch;
    } catch (_error) {
      // The collection is ready but the item may be outside the rendered lazy-list viewport.
    }
    // A picker can take one Compose frame to appear after its launcher accepts
    // the click. Do not mistake the underlying page's scroll surface for the
    // requested lazy collection during that frame.
    const initialXml = dumpAndroid(this.serial);
    if (projectionEntries(initialXml).length === 0) return null;
    for (const direction of ["down", "up"]) {
      let previousSignature = null;
      let unchangedFrames = 0;
      for (let attempt = 0; attempt < 10; attempt += 1) {
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
    const semanticTag = ANDROID_PAGE_ELEMENT_TAGS[elementId] ?? androidSemanticTag(elementId);
    const xml = dumpAndroid(this.serial);
    let node = findTagOrPrefix(xml, semanticTag);
    if (!node) node = androidElementFallback(xml, elementId);
    return node ? {
      test_id: androidTag(node),
      text: androidNodeLabel(xml, node) || node.text || "",
      enabled: androidElementEnabled(node),
      selected: node.selected === "true",
      checked: node.checked === "true",
      pressed: node.selected === "true" || node.checked === "true" ? "true" : "false",
      expanded: elementId.startsWith("settings-section-")
        ? node.checked === "true" || node.selected === "true"
        : null,
      state: androidTag(node).match(/:state:([^:]+)(?::|$)/)?.[1] ?? null,
      bounds: node.bounds,
    } : null;
  }

  async revealElement(elementId) {
    const existing = await this.readElement(elementId);
    if (existing) return existing;
    const semanticTag = ANDROID_PAGE_ELEMENT_TAGS[elementId] ?? androidSemanticTag(elementId);
    if (androidElementMayRequireHorizontalScroll(elementId)) {
      await scrollHorizontallyUntilTag(this.serial, semanticTag, 8);
    }
    if (!await this.readElement(elementId) && androidElementMayRequireVerticalScroll(elementId)) {
      await scrollUntilTag(this.serial, semanticTag, 8, true);
    }
    return this.readElement(elementId);
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
