// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import test from "node:test";
import { requireWebDependency } from "../../ui/web-app/scripts/web-workspace-require.mjs";
import { beginBrowserInput, finishBrowserInput, cancelBrowserInput } from "./browser-input-observation.mjs";

const { JSDOM } = requireWebDependency("jsdom");
function page(t) {
  const dom = new JSDOM('<div id="surface"><img><button>overlay</button></div><div id="scrim"></div>', { runScripts: "outside-only" });
  t.after(() => dom.window.close());
  const { document } = dom.window;
  const surface = document.querySelector("#surface");
  const image = document.querySelector("img");
  const bounds = { left: 0, top: 0, width: 1000, height: 900 };
  surface.getBoundingClientRect = () => bounds;
  document.elementFromPoint = () => image;
  return {
    dom, document, image, surface, bounds,
    begin: (eventTypes = ["wheel"], expectedBounds = bounds) => dom.window.eval(`(${beginBrowserInput.toString()})(${JSON.stringify({
      selector: "#surface", point: { x: 500, y: 450 }, bounds: expectedBounds, eventTypes, timeoutMs: 3000,
    })})`),
    finish: () => dom.window.eval(`(${finishBrowserInput.toString()})()`),
    cancel: () => dom.window.eval(`(${cancelBrowserInput.toString()})()`),
    event: (target, type) => target.dispatchEvent(new dom.window.MouseEvent(type, { bubbles: true, clientX: 500, clientY: 450 })),
  };
}

test("delivery receipt waits for the browser event and runs no application action itself", async (t) => {
  const p = page(t);
  let zoom = 1;
  p.surface.addEventListener("wheel", () => { zoom++; });
  p.begin();
  const result = p.finish();
  assert.equal(zoom, 1);
  // CDP acknowledgement can precede this dispatch by any amount of time.
  p.event(p.image, "wheel");
  assert.equal(zoom, 2);
  const receipt = await result;
  assert.equal(receipt.events[0].delivered, true);
  assert.equal(receipt.events[0].target, "IMG");
  assert.equal(p.dom.window.__aerobagInputReceipt, undefined);
});

test("wrong-target delivery fails at the action boundary, not a later state timeout", async (t) => {
  const p = page(t); p.begin();
  const result = p.finish();
  p.event(p.document.querySelector("#scrim"), "wheel");
  await assert.rejects(result, /input wheel missed #surface/);
});

test("nested controls, covering scrims, and changed geometry cannot authorize input", (t) => {
  const p = page(t);
  for (const target of ["button", "#scrim"]) {
    p.document.elementFromPoint = () => p.document.querySelector(target);
    assert.throws(() => p.begin(), /obstructed/);
  }
  p.document.elementFromPoint = () => p.image;
  assert.throws(() => p.begin(["wheel"], { ...p.bounds, left: 20 }), /moved after readiness/);
  assert.equal(p.dom.window.__aerobagInputReceipt, undefined);
});

test("pointer receipts wait for release and are disposed when transport fails", async (t) => {
  const p = page(t); p.begin(["pointerdown", "pointermove", "pointerup"]);
  p.event(p.image, "pointerdown"); p.event(p.image, "pointermove");
  const result = p.finish();
  p.event(p.surface, "pointerup");
  assert.deepEqual(Array.from((await result).events, (event) => event.type), ["pointerdown", "pointermove", "pointerup"]);
  p.begin(); p.cancel();
  assert.equal(p.dom.window.__aerobagInputReceipt, undefined);
  p.begin(); p.cancel(); // A failed operation cannot poison the next one.
});
