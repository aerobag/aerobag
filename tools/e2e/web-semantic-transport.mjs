// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { writeFile } from "node:fs/promises";
import { waitFor } from "../../ui/web-app/scripts/chrome-cdp.mjs";

function expressionArgument(value) {
  return JSON.stringify(value);
}

export class WebSemanticTransport {
  constructor(page, { url, origin = new URL(url).origin } = {}) {
    this.page = page;
    this.url = url;
    this.origin = origin;
  }

  async reset() {
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
      return [...document.querySelectorAll(${expressionArgument(selector)})].some((element) => {
        const rect = element.getBoundingClientRect();
        const style = getComputedStyle(element);
        return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
      });
    })()`);
  }

  async waitFor(selector, description, timeoutMs = 15_000) {
    return waitFor(
      () => this.visible(selector),
      timeoutMs,
      `timed out waiting for ${description}`,
      100,
    );
  }

  async click(selector) {
    if (!await this.clickIfVisible(selector)) {
      throw new Error(`web control is missing: ${selector}`);
    }
  }

  async clickIfVisible(selector) {
    const clicked = await this.page.evaluate(`(() => {
      const element = [...document.querySelectorAll(${expressionArgument(selector)})].find((candidate) => {
        const rect = candidate.getBoundingClientRect();
        const style = getComputedStyle(candidate);
        return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
      });
      if (!(element instanceof HTMLElement)) return false;
      element.click();
      return true;
    })()`);
    return clicked;
  }

  async clickTestId(testId) {
    const clicked = await this.page.evaluate(`(() => {
      const element = [...document.querySelectorAll("[data-testid]")].find((candidate) => {
        if (candidate.dataset.testid !== ${expressionArgument(testId)}) return false;
        const rect = candidate.getBoundingClientRect();
        const style = getComputedStyle(candidate);
        return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
      });
      if (element instanceof HTMLElement) {
        element.click();
      } else if (element instanceof SVGElement) {
        element.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, view: window }));
      } else {
        return false;
      }
      return true;
    })()`);
    if (!clicked) throw new Error(`web control is missing: data-testid=${testId}`);
  }

  async firstExisting(selectors) {
    for (const selector of selectors) {
      if (!await this.exists(selector)) continue;
      await this.page.evaluate(`document.querySelector(${expressionArgument(selector)})
        ?.scrollIntoView({ block: "center", inline: "center" })`);
      if (await this.visible(selector)) return selector;
    }
    return null;
  }

  async enterText(selector, value, { submit = false } = {}) {
    const changed = await this.page.evaluate(`(() => {
      const input = [...document.querySelectorAll(${expressionArgument(selector)})].find((candidate) => {
        const rect = candidate.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0;
      });
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

  async pointerClick(selector, xFraction = 0.5, yFraction = 0.5) {
    const point = await this.elementPoint(selector, xFraction, yFraction);
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
        candidate.getBoundingClientRect().width > 0 &&
        candidate.getBoundingClientRect().height > 0);
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
    const end = { x: start.x + deltaX, y: start.y + deltaY };
    await this.page.evaluate(`(() => {
      const surface = [...document.querySelectorAll(${expressionArgument(selector)})].find((candidate) => {
        const rect = candidate.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0;
      });
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
      const element = [...document.querySelectorAll(${expressionArgument(selector)})].find((candidate) => {
        const rect = candidate.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0;
      });
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
      const element = [...document.querySelectorAll(${expressionArgument(selector)})].find((candidate) => {
        const rect = candidate.getBoundingClientRect();
        const style = getComputedStyle(candidate);
        return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none";
      });
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
        expanded: element.getAttribute("aria-expanded"),
        selected: element.getAttribute("aria-selected"),
        checked: element instanceof HTMLInputElement ? element.checked : null,
        disabled_reason: element.getAttribute("title"),
        visible: rect.width > 0 && rect.height > 0,
        bounds: { left: rect.left, top: rect.top, width: rect.width, height: rect.height },
      };
    })()`);
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
    return this.page.evaluate(`(() => [...document.querySelectorAll("[data-testid]")]
      .filter((element) => element.dataset.testid.startsWith(${expressionArgument(prefix)}))
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
