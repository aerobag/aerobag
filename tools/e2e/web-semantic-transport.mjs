// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { writeFile } from "node:fs/promises";
import { waitFor } from "../../ui/web-app/scripts/chrome-cdp.mjs";
import { clampDragEndpoint } from "./gesture-geometry.mjs";
import { E2E_TIMING } from "./transition-contract.mjs";

function expressionArgument(value) {
  return JSON.stringify(value);
}

const RENDERED_ELEMENT_PREDICATE = `((element) => {
  if (!(element instanceof Element)) return false;
  const rect = element.getBoundingClientRect();
  if (rect.width <= 0 || rect.height <= 0 || element.getClientRects().length === 0) return false;
  for (let current = element; current; current = current.parentElement) {
    const style = getComputedStyle(current);
    if (style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse" || Number(style.opacity) === 0) {
      return false;
    }
  }
  return true;
})`;

export class WebSemanticTransport {
  constructor(page, { url, origin = new URL(url).origin } = {}) {
    this.page = page;
    this.url = url;
    this.origin = origin;
  }

  async reset() {
    // Stop the application before clearing its origin. Otherwise a live effect
    // can persist stale state between clearDataForOrigin and navigation.
    await this.page.navigate("about:blank");
    await this.page.waitForLoad();
    await this.page.send("Storage.clearDataForOrigin", {
      origin: this.origin,
      storageTypes: "all",
    });
    await this.page.navigate(this.url);
    await this.page.waitForLoad();
  }

  async reload() {
    await this.page.navigate(this.url);
    await this.page.waitForLoad();
  }

  async exists(selector) {
    return this.page.evaluate(`Boolean(document.querySelector(${expressionArgument(selector)}))`);
  }

  async visible(selector) {
    return this.page.evaluate(`(() => {
      return [...document.querySelectorAll(${expressionArgument(selector)})]
        .some((element) => ${RENDERED_ELEMENT_PREDICATE}(element));
    })()`);
  }

  async waitFor(selector, description, timeoutMs = E2E_TIMING.localReadyMs) {
    return waitFor(
      () => this.visible(selector),
      timeoutMs,
      `timed out waiting for ${description}`,
      E2E_TIMING.pollIntervalMs,
    );
  }

  async click(selector) {
    await this.page.evaluate(`(() => {
      const element = [...document.querySelectorAll(${expressionArgument(selector)})]
        .find((candidate) => ${RENDERED_ELEMENT_PREDICATE}(candidate));
      element?.scrollIntoView({ block: "center", inline: "center", behavior: "instant" });
    })()`);
    if (!await this.clickIfVisible(selector)) {
      throw new Error(`web control is missing: ${selector}`);
    }
  }

  async clickIfVisible(selector) {
    let lastProbe = null;
    for (let attempt = 0; attempt < 8; attempt += 1) {
      const result = await this.page.evaluate(`(() => {
        const element = [...document.querySelectorAll(${expressionArgument(selector)})]
          .find((candidate) => ${RENDERED_ELEMENT_PREDICATE}(candidate));
        if (!element) return { status: "missing" };
        const rect = element.getBoundingClientRect();
        const fractions = [0.5, 0.1, 0.9, 0.25, 0.75];
        let lastHit = null;
        let unobstructed = false;
        for (const yFraction of fractions) {
          for (const xFraction of fractions) {
            const hit = document.elementFromPoint(
              rect.left + rect.width * xFraction,
              rect.top + rect.height * yFraction,
            );
            lastHit = hit;
            if (hit === element || element.contains(hit)) {
              unobstructed = true;
              break;
            }
          }
          if (unobstructed) break;
        }
        if (!unobstructed) {
          return {
            status: "obstructed",
            hit: lastHit ? {
              tag: lastHit.tagName,
              test_id: lastHit.getAttribute("data-testid"),
              aria_label: lastHit.getAttribute("aria-label"),
              class_name: typeof lastHit.className === "string" ? lastHit.className : lastHit.className?.baseVal ?? null,
            } : null,
          };
        }
        if (typeof element.click !== "function") {
          return { status: "unsupported", tag: element.tagName };
        }

        const probe = { click: 0, matched: 0, actionable_clicks: 0 };
        const listener = (event) => {
          probe.click += 1;
          if (event.target instanceof Element && event.target.closest(${expressionArgument(selector)})) {
            probe.matched += 1;
          }
          if (event.target instanceof Element && event.target.closest("button,input,a,[role=button]")) {
            probe.actionable_clicks += 1;
          }
        };
        document.addEventListener("click", listener, true);
        try {
          // Visibility, hit testing, and activation happen in one browser task so
          // a render cannot move the control between observation and delivery.
          element.click();
        } finally {
          document.removeEventListener("click", listener, true);
        }
        return { status: "activated", probe };
      })()`);
      if (result.status === "missing") return false;
      if (result.status === "obstructed") {
        lastProbe = { obstructed_by: result.hit };
        await new Promise((resolve) => setTimeout(resolve, E2E_TIMING.pollIntervalMs));
        continue;
      }
      if (result.status !== "activated") {
        throw new Error(`web control cannot be activated: ${selector}; probe=${JSON.stringify(result)}`);
      }
      if (result.probe?.click === 1 && result.probe?.matched === 1 && result.probe?.actionable_clicks === 1) {
        return true;
      }
      throw new Error(
        `web semantic action did not complete on its target: ${selector}; ` +
          `probe=${JSON.stringify(result.probe)}`,
      );
    }
    throw new Error(
      `web control did not become unobstructed: ${selector}; probe=${JSON.stringify(lastProbe)}`,
    );
  }

  async clickTestId(testId) {
    await this.click(`[data-testid=${expressionArgument(testId)}]`);
  }

  async firstExisting(selectors) {
    for (const selector of selectors) {
      if (!await this.exists(selector)) continue;
      if (await this.visible(selector)) return selector;
    }
    return null;
  }

  async waitForFirstVisible(selectors, description, timeoutMs = E2E_TIMING.localReadyMs) {
    return waitFor(
      () => this.firstExisting(selectors),
      timeoutMs,
      `timed out waiting for ${description}`,
      E2E_TIMING.pollIntervalMs,
    );
  }

  async optionSelectionState(selector) {
    return this.page.evaluate(`(() => {
      const element = document.querySelector(${expressionArgument(selector)});
      if (!element || !${RENDERED_ELEMENT_PREDICATE}(element)) return null;
      return JSON.stringify({
        pressed: element.getAttribute("aria-pressed"),
        selected: element.getAttribute("aria-selected"),
        checked: element.getAttribute("aria-checked"),
        on: element.classList.contains("isOn"),
        off: element.classList.contains("isOff"),
      });
    })()`);
  }

  async waitForOptionSelection(selector, previousState, description, timeoutMs = E2E_TIMING.localReadyMs) {
    return waitFor(
      async () => {
        const currentState = await this.optionSelectionState(selector);
        return currentState === null || currentState !== previousState ? currentState ?? "dismissed" : null;
      },
      timeoutMs,
      `timed out waiting for ${description}`,
      E2E_TIMING.pollIntervalMs,
    );
  }

  async enterText(selector, value, { submit = false } = {}) {
    const changed = await this.page.evaluate(`(() => {
      const input = [...document.querySelectorAll(${expressionArgument(selector)})]
        .find((candidate) => ${RENDERED_ELEMENT_PREDICATE}(candidate));
      if (!(input instanceof HTMLInputElement || input instanceof HTMLTextAreaElement)) return false;
      const prototype = input instanceof HTMLTextAreaElement
        ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype;
      Object.getOwnPropertyDescriptor(prototype, "value").set.call(input, ${expressionArgument(value)});
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.dispatchEvent(new Event("change", { bubbles: true }));
      input.focus();
      return true;
    })()`);
    if (!changed) throw new Error(`web text control is missing: ${selector}`);
    if (submit) {
      await this.page.send("Input.dispatchKeyEvent", {
        type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13,
      });
      await this.page.send("Input.dispatchKeyEvent", {
        type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13,
      });
    }
  }

  async submit(selector) {
    const focused = await this.page.evaluate(`(() => {
      const input = [...document.querySelectorAll(${expressionArgument(selector)})]
        .find((candidate) => ${RENDERED_ELEMENT_PREDICATE}(candidate));
      if (!(input instanceof HTMLInputElement || input instanceof HTMLTextAreaElement)) return false;
      input.focus();
      return document.activeElement === input;
    })()`);
    if (!focused) throw new Error(`web text control is missing for submit: ${selector}`);
    await this.page.send("Input.dispatchKeyEvent", {
      type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13,
    });
    await this.page.send("Input.dispatchKeyEvent", {
      type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13,
    });
  }

  async pointerClick(selector, xFraction = 0.5, yFraction = 0.5) {
    const point = await this.elementPoint(selector, xFraction, yFraction);
    await this.page.evaluate(`(() => {
      const surface = [...document.querySelectorAll(${expressionArgument(selector)})]
        .find((candidate) => ${RENDERED_ELEMENT_PREDICATE}(candidate));
      const probe = { pointerdown: 0, pointerup: 0, targets: [] };
      const listeners = {};
      for (const type of ["pointerdown", "pointerup"]) {
        listeners[type] = (event) => {
          probe[type] += 1;
          probe.targets.push({
            type,
            tag: event.target?.tagName ?? null,
            class_name: event.target?.className?.baseVal ?? event.target?.className ?? null,
          });
        };
        surface?.addEventListener(type, listeners[type], true);
      }
      window.__aerobagReleaseClickProbe = { probe, surface, listeners };
    })()`);
    await this.page.send("Input.dispatchMouseEvent", {
      type: "mouseMoved", x: point.x, y: point.y, button: "none", pointerType: "mouse",
    });
    await this.page.send("Input.dispatchMouseEvent", {
      type: "mousePressed", x: point.x, y: point.y, button: "left", buttons: 1,
      clickCount: 1, pointerType: "mouse", pointerId: 1,
    });
    await this.page.send("Input.dispatchMouseEvent", {
      type: "mouseReleased", x: point.x, y: point.y, button: "left", buttons: 0,
      clickCount: 1, pointerType: "mouse", pointerId: 1,
    });
    return this.page.evaluate(`(() => {
      const state = window.__aerobagReleaseClickProbe;
      if (!state) return null;
      for (const [type, listener] of Object.entries(state.listeners)) {
        state.surface?.removeEventListener(type, listener, true);
      }
      delete window.__aerobagReleaseClickProbe;
      return state.probe;
    })()`);
  }

  async hoverTestId(testId) {
    const point = await this.elementPoint(`[data-testid="${testId}"]`, 0.5, 0.5);
    await this.page.send("Input.dispatchMouseEvent", {
      type: "mouseMoved", x: point.x, y: point.y, button: "none", pointerType: "mouse",
    });
  }

  async copyTextTestId(testId) {
    await this.page.send("Browser.grantPermissions", {
      origin: this.origin,
      permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
    });
    const selected = await this.page.evaluate(`(() => {
      const element = [...document.querySelectorAll("[data-testid]")].find((candidate) =>
        candidate.dataset.testid === ${expressionArgument(testId)} &&
        ${RENDERED_ELEMENT_PREDICATE}(candidate));
      if (!element) return null;
      const range = document.createRange();
      range.selectNodeContents(element);
      const selection = window.getSelection();
      selection.removeAllRanges();
      selection.addRange(range);
      return selection.toString();
    })()`);
    if (!selected) throw new Error(`web copy source is missing or empty: data-testid=${testId}`);
    await this.page.send("Input.dispatchKeyEvent", {
      type: "keyDown", key: "Control", code: "ControlLeft", windowsVirtualKeyCode: 17,
    });
    await this.page.send("Input.dispatchKeyEvent", {
      type: "keyDown", key: "c", code: "KeyC", windowsVirtualKeyCode: 67, modifiers: 2,
    });
    await this.page.send("Input.dispatchKeyEvent", {
      type: "keyUp", key: "c", code: "KeyC", windowsVirtualKeyCode: 67, modifiers: 2,
    });
    await this.page.send("Input.dispatchKeyEvent", {
      type: "keyUp", key: "Control", code: "ControlLeft", windowsVirtualKeyCode: 17,
    });
    const clipboard = await this.page.evaluate("navigator.clipboard.readText()");
    return { selected, clipboard };
  }

  async drag(selector, deltaX, deltaY) {
    const start = await this.elementPoint(selector, 0.72, 0.72);
    const minimum = await this.elementPoint(selector, 0.02, 0.02);
    const maximum = await this.elementPoint(selector, 0.98, 0.98);
    const end = clampDragEndpoint(start, { x: deltaX, y: deltaY }, minimum, maximum);
    await this.page.evaluate(`(() => {
      const surface = [...document.querySelectorAll(${expressionArgument(selector)})]
        .find((candidate) => ${RENDERED_ELEMENT_PREDICATE}(candidate));
      const probe = { pointerdown: 0, pointermove: 0, pointerup: 0, targets: [], blocked_by: null };
      const listeners = {};
      for (const type of ["pointerdown", "pointermove", "pointerup"]) {
        listeners[type] = (event) => {
          probe[type] += 1;
          if (probe.targets.length < 6) {
            probe.targets.push({ type, tag: event.target?.tagName ?? null, class_name: event.target?.className?.baseVal ?? event.target?.className ?? null });
          }
        };
        surface?.addEventListener(type, listeners[type], true);
      }
      if (document.querySelector('[data-testid="map-selection-tray"]')) probe.blocked_by = "map-selection";
      else if (document.querySelector('.trayScrim')) probe.blocked_by = "tray-scrim";
      window.__aerobagReleaseGestureProbe = { probe, surface, listeners };
    })()`);
    await this.page.send("Input.dispatchMouseEvent", {
      type: "mouseMoved", x: start.x, y: start.y, button: "none", pointerType: "mouse",
    });
    await this.page.send("Input.dispatchMouseEvent", {
      type: "mousePressed", x: start.x, y: start.y, button: "left", buttons: 1,
      clickCount: 1, pointerType: "mouse", pointerId: 1,
    });
    for (let index = 1; index <= 10; index += 1) {
      await this.page.send("Input.dispatchMouseEvent", {
        type: "mouseMoved",
        x: start.x + (end.x - start.x) * index / 10,
        y: start.y + (end.y - start.y) * index / 10,
        button: "left",
        buttons: 1,
        pointerType: "mouse",
        pointerId: 1,
      });
      await new Promise((resolve) => setTimeout(resolve, 12));
    }
    await this.page.send("Input.dispatchMouseEvent", {
      type: "mouseReleased", x: end.x, y: end.y, button: "left", buttons: 0,
      clickCount: 1, pointerType: "mouse", pointerId: 1,
    });
    return this.page.evaluate(`(() => {
      const state = window.__aerobagReleaseGestureProbe;
      if (!state) return null;
      for (const [type, listener] of Object.entries(state.listeners)) {
        state.surface?.removeEventListener(type, listener, true);
      }
      delete window.__aerobagReleaseGestureProbe;
      return state.probe;
    })()`);
  }

  async wheel(selector, amount) {
    const point = await this.elementPoint(selector, 0.5, 0.5);
    await this.page.send("Input.dispatchMouseEvent", {
      type: "mouseWheel", x: point.x, y: point.y, deltaX: 0, deltaY: amount,
    });
  }

  async elementPoint(selector, xFraction, yFraction) {
    const rect = await this.page.evaluate(`(() => {
      const element = [...document.querySelectorAll(${expressionArgument(selector)})]
        .find((candidate) => ${RENDERED_ELEMENT_PREDICATE}(candidate));
      if (!element) return null;
      const value = element.getBoundingClientRect();
      return { left: value.left, top: value.top, width: value.width, height: value.height };
    })()`);
    if (!rect) throw new Error(`web surface is missing: ${selector}`);
    return {
      x: rect.left + rect.width * xFraction,
      y: rect.top + rect.height * yFraction,
    };
  }

  async readElement(selector) {
    return this.page.evaluate(`(() => {
      const element = [...document.querySelectorAll(${expressionArgument(selector)})]
        .find((candidate) => ${RENDERED_ELEMENT_PREDICATE}(candidate));
      if (!(element instanceof HTMLElement)) return null;
      const rect = element.getBoundingClientRect();
      return {
        test_id: element.dataset.testid ?? null,
        text: element.textContent?.replace(/\\s+/g, " ").trim() ?? "",
        value: element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement
          ? element.value
          : null,
        enabled: element.getAttribute("aria-disabled") !== "true" &&
          (!(element instanceof HTMLButtonElement || element instanceof HTMLInputElement) || !element.disabled),
        pressed: element.getAttribute("aria-pressed"),
        expanded: element.hasAttribute("aria-expanded")
          ? element.getAttribute("aria-expanded") === "true"
          : null,
        state: element.getAttribute("data-e2e-state"),
        selected: element.getAttribute("aria-selected"),
        checked: element instanceof HTMLInputElement ? element.checked : null,
        disabled_reason: element.getAttribute("title"),
        visible: rect.width > 0 && rect.height > 0,
        bounds: { left: rect.left, top: rect.top, width: rect.width, height: rect.height },
      };
    })()`);
  }

  async revealElement(selector) {
    const revealed = await this.page.evaluate(`(() => {
      const element = [...document.querySelectorAll(${expressionArgument(selector)})]
        .find((candidate) => ${RENDERED_ELEMENT_PREDICATE}(candidate));
      if (!(element instanceof HTMLElement)) return false;
      element.scrollIntoView({ block: "center", inline: "center", behavior: "auto" });
      return true;
    })()`);
    if (!revealed) throw new Error(`web element is missing: ${selector}`);
  }

  async collectTestIds(prefix) {
    if (prefix === "parity:raster-state:") {
      return this.page.evaluate(`(() => {
        const images = [...document.querySelectorAll(".rasterTileLayer .mapTileImage")];
        const planned = document.querySelector(".rasterTileLayer")?.childElementCount ?? 0;
        const loaded = images.filter((image) => image.complete && image.naturalWidth > 0).length;
        const failed = images.filter((image) => image.complete && image.naturalWidth === 0).length;
        return [{ id: \`parity:raster-state:planned:\${planned}:loaded:\${loaded}:failed:\${failed}\`, text: "", enabled: true, pressed: null }];
      })()`);
    }
    const visibleControlsOnly = [
      "tray-option-",
      "plate-folder-tile:",
      "plan-row-",
      "plan-procedure-",
      "plan-procedure-transition-",
      "plan-insert-suggestion-",
    ].some((controlPrefix) => prefix.startsWith(controlPrefix));
    return this.page.evaluate(`(() => [...document.querySelectorAll("[data-testid]")]
      .filter((element) => element.dataset.testid.startsWith(${expressionArgument(prefix)}))
      .filter((element) => {
        if (!${visibleControlsOnly}) return true;
        return ${RENDERED_ELEMENT_PREDICATE}(element);
      })
      .map((element) => ({
        id: element.dataset.testid,
        text: element.textContent?.replace(/\\s+/g, " ").trim() ?? "",
        enabled: element.getAttribute("aria-disabled") !== "true" &&
          (!(element instanceof HTMLButtonElement || element instanceof HTMLInputElement) || !element.disabled),
        pressed: element.getAttribute("aria-pressed"),
        selected: element.getAttribute("aria-selected"),
      })))()`);
  }

  async snapshot() {
    return this.page.evaluate(`(() => ({
      href: location.href,
      title: document.title,
      body: document.body.innerText.slice(0, 20000),
      test_ids: [...document.querySelectorAll("[data-testid]")].map((element) => element.dataset.testid),
      nav_db: window.__aerobagE2e?.navDb?.() ?? null,
      render: window.__aerobagE2e?.render?.() ?? null,
      startup_error: document.querySelector(".startupErrorModal,.startupFailure")?.textContent ?? null,
    }))()`);
  }

  async captureScreenshot(path) {
    const screenshot = await this.page.send("Page.captureScreenshot", { format: "png" });
    await writeFile(path, Buffer.from(screenshot.data, "base64"));
  }
}
