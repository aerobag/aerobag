#!/usr/bin/env node
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import path from "node:path";
import { connectToBrowser, launchChrome, stopProcess, waitFor } from "./chrome-cdp.mjs";
import { dismissFirstUseTour } from "./dismiss-first-use-tour.mjs";

const repo = process.env.AEROBAG_REPO_ROOT ?? path.resolve("../..");
const { beginBrowserInput, finishBrowserInput } = await import(pathToFileURL(path.join(repo, "tools/e2e/browser-input-observation.mjs")));
const width = Number(process.env.AEROBAG_E2E_WIDTH ?? 1200);
const height = 900;
const prefix = process.env.AEROBAG_E2E_SCREENSHOT_PREFIX ?? `/tmp/flight-plan-control-${width}`;
const profile = await mkdtemp("/tmp/aerobag-workspace-browser-");
let chrome, browser, page;
const scope = ".pageLayer.isActive ";
const testId = id => `${scope}[data-testid="${id}"]`;
const visible = selector => `Boolean(document.querySelector(${JSON.stringify(selector)})?.getClientRects().length)`;
try {
  chrome = await launchChrome({ userDataDir: profile, width, height });
  browser = await connectToBrowser(chrome.endpoint);
  page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  await page.send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
  await page.navigate(process.env.AEROBAG_E2E_URL ?? "http://127.0.0.1:8085/");
  await page.waitForLoad();
  await waitFor(() => page.evaluate(visible('[data-testid="parity:disclaimer-accept-button"]')), 60000, "disclaimer");
  await click('[data-testid="parity:disclaimer-accept-button"]');
  await waitFor(() => page.evaluate(visible('[data-testid="guided-tour-panel"]')), 30000, "fresh-profile introduction rendered");
  await dismissFirstUseTour(page);
  await click(testId("nav-cdi"));
  await waitFor(() => page.evaluate(visible(testId("plan-control-toggle_open_plan"))), 15000, "flight-plan workspace");

  // Seed Active through the same Clear/append/Swap path a pilot uses.
  await command("toggle_open_plan");
  await expectHeading("STANDBY");
  await command("clear_standby");
  await expectHeading("STANDBY");
  await waitFor(() => page.evaluate(`document.activeElement?.dataset.testid === "plan-append-route-input"`), 10000, "Clear focuses route entry");
  await enterRoute("KRNT SEA KUAO");
  await command("swap_plans");
  await expectHeading("ACTIVE");
  await expectRows("KRNT", "SEA", "KUAO");
  await command("copy_active_to_standby");
  await expectHeading("STANDBY");
  await expectRows("KRNT", "SEA", "KUAO");
  assert.match(await page.evaluate(`document.querySelector(${JSON.stringify(testId("plan-collapsed-summary"))}).value`), /KRNT.*SEA.*KUAO/);
  await click(testId("plan-collapsed-summary"));
  await expectHeading("STANDBY");
  await page.send("Input.dispatchKeyEvent", { type: "keyDown", key: "a", code: "KeyA", modifiers: 2, windowsVirtualKeyCode: 65 });
  await page.send("Input.dispatchKeyEvent", { type: "keyUp", key: "a", code: "KeyA", modifiers: 2, windowsVirtualKeyCode: 65 });
  assert.equal(await page.evaluate(`document.activeElement.value.slice(document.activeElement.selectionStart, document.activeElement.selectionEnd)`), "KRNT SEA KUAO");
  assert.equal(await page.evaluate(visible(testId("plan-control-start_navigation"))), false);

  await command("clear_standby");
  await expectRows();
  await click(testId("plan-control-undo"));
  await expectRows("KRNT", "SEA", "KUAO");
  await command("clear_standby");
  await expectRows();
  await enterRoute("KPAE KPLU");
  await screenshot("standby");
  await click(testId("plan-estimate-mode"));
  await waitFor(() => page.evaluate(`document.querySelector(${JSON.stringify(testId("altitude-plan-heading"))})?.textContent === "STANDBY PLAN"`), 10000, "altitude planner identifies Standby");
  await screenshot("altitude-standby");
  await click(testId("nav-cdi"));
  await expectHeading("STANDBY");
  await command("toggle_open_plan");
  await expectHeading("ACTIVE");
  await expectRows("KRNT", "SEA", "KUAO");
  await screenshot("active");
  await inspectTransition("standby");
  await inspectTransition("active");
  await command("swap_plans");
  await expectRows("KPAE", "KPLU");
  await command("toggle_open_plan");
  await expectHeading("STANDBY");
  await expectRows("KRNT", "SEA", "KUAO");
  await page.send("Page.reload", { ignoreCache: true });
  await waitFor(() => page.evaluate(visible(testId("plan-append-route-input"))), 60000, "persisted plan page");
  await expectHeading("STANDBY");
  await expectRows("KRNT", "SEA", "KUAO");
  assert.match(await page.evaluate(`document.querySelector(${JSON.stringify(testId("plan-collapsed-summary"))}).value`), /KPAE.*KPLU/);
  console.log(JSON.stringify({ width, passed: true, checks: ["clear-and-focus", "append", "copy", "separate-editing", "clear-undo", "swap", "reload"], screenshots: prefix }));
} catch (error) {
  console.error(error);
  if (page) {
    try {
      await screenshot("failure");
      await writeFile(`${prefix}-failure.txt`, await page.evaluate("document.body.innerText"));
    } catch (captureError) { console.error("Failure capture:", captureError.message); }
  }
  throw error;
} finally {
  browser?.close();
  await stopProcess(chrome?.process);
  await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}

async function click(selector) {
  await waitFor(() => page.evaluate(`!document.querySelector('.pageLayer.isActive [data-transition="running"]')`), 5000, "workspace animation settled before input");
  await waitFor(() => page.evaluate(visible(selector)), 15000, `visible ${selector}`);
  await page.evaluate(`document.querySelector(${JSON.stringify(selector)}).scrollIntoView({block:"nearest"})`);
  const bounds = await page.evaluate(`(() => { const r=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect(); return {left:r.left,top:r.top,width:r.width,height:r.height}; })()`);
  const point = { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 };
  assert.ok(point.x >= 0 && point.x < width && point.y >= 0 && point.y < height, `${selector} is off screen`);
  await page.evaluate(`(${beginBrowserInput.toString()})(${JSON.stringify({ selector, point, bounds, eventTypes: ["pointerdown", "pointerup", "click"], timeoutMs: 5000 })})`);
  await page.send("Input.dispatchMouseEvent", { type: "mousePressed", ...point, button: "left", clickCount: 1 });
  await page.send("Input.dispatchMouseEvent", { type: "mouseReleased", ...point, button: "left", clickCount: 1 });
  await page.evaluate(`(${finishBrowserInput.toString()})()`);
}
async function command(name) { await click(testId(`plan-control-${name}`)); }
async function expectHeading(text) {
  await waitFor(() => page.evaluate(`document.querySelector('.pageLayer.isActive [data-testid="plan-slot-heading"]')?.textContent === ${JSON.stringify(text + " PLAN")}`), 10000, `open ${text}`);
}
async function expectRows(...labels) {
  await waitFor(() => page.evaluate(`JSON.stringify([...document.querySelectorAll('.pageLayer.isActive button[data-testid^="plan-row-"]')].map(e=>e.textContent.trim())) === ${JSON.stringify(JSON.stringify(labels))}`), 15000, `rows ${labels.join(" ")}`);
}
async function enterRoute(route) {
  await click(testId("plan-append-route-input"));
  await page.send("Input.insertText", { text: route });
  await waitFor(() => page.evaluate(visible(`${scope}[data-testid="parity:plan-append-route-state:can_commit:true:loading:false"]`)), 30000, "route preview ready");
  await page.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  await page.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  await expectRows(...route.split(" "));
}
async function screenshot(name) {
  await page.evaluate(`Promise.all([...document.querySelectorAll('.planWorkspaceDivider')].flatMap(e => e.getAnimations()).map(a => a.finished.catch(() => {})))`);
  const image = await page.send("Page.captureScreenshot", { format: "png" });
  await writeFile(`${prefix}-${name}.png`, Buffer.from(image.data, "base64"));
}

async function inspectTransition(slot) {
  await page.send("Animation.enable");
  await page.send("Animation.setPlaybackRate", { playbackRate: 0.05 });
  await command("toggle_open_plan");
  await waitFor(() => page.evaluate(`Boolean(document.querySelector('.pageLayer.isActive .planWorkspaceLayout.is${slot}[data-transition="running"]'))`), 10000, `opening ${slot}`);
  await page.evaluate(`document.querySelector('.pageLayer.isActive .planWorkspaceLayout').getAnimations({subtree:true}).forEach(a=>{ a.pause(); a.currentTime=64; })`);
  const early = await page.send("Page.captureScreenshot", { format: "png" });
  await writeFile(`${prefix}-closing-${slot}.png`, Buffer.from(early.data, "base64"));
  const geometry = await page.evaluate(`(() => {
    const frame = document.querySelector('.pageLayer.isActive .planWorkspaceLayout');
    for (const animation of frame.getAnimations({subtree:true})) { animation.pause(); animation.currentTime = 160; }
    const live = frame.querySelector('[data-testid="plan-workspace-editor-clip"]');
    const old = frame.querySelector('.planWorkspaceClosingEditor');
    const line = frame.querySelector('.planWorkspaceDivider');
    const bounds = e => { const r = e.getBoundingClientRect(); return {top:r.top, bottom:r.bottom, height:r.height}; };
    return {live:bounds(live), old:bounds(old), line:bounds(line), full:live.firstElementChild.getBoundingClientRect().height,
      oldText:old.textContent, liveText:live.textContent};
  })()`);
  assert.ok(geometry.live.height > 80 && geometry.live.height < geometry.full - 1, JSON.stringify(geometry));
  const upper = slot === "active" ? geometry.live : geometry.old;
  const lower = slot === "active" ? geometry.old : geometry.live;
  assert.ok(Math.abs(upper.bottom - geometry.line.top) < 2, JSON.stringify(geometry));
  assert.ok(Math.abs(lower.top - geometry.line.bottom) < 2, JSON.stringify(geometry));
  assert.ok(geometry.oldText.includes(slot === "active" ? "KPLU" : "KUAO"));
  const shot = await page.send("Page.captureScreenshot", { format: "png" });
  await writeFile(`${prefix}-opening-${slot}.png`, Buffer.from(shot.data, "base64"));
  await page.evaluate(`document.querySelector('.pageLayer.isActive .planWorkspaceLayout').getAnimations({subtree:true}).forEach(a=>a.finish())`);
  await page.send("Animation.setPlaybackRate", { playbackRate: 1 });
  await expectHeading(slot.toUpperCase());
}
