// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import { waitFor } from "./chrome-cdp.mjs";

// Standalone browser probes start in fresh profiles too. Close the actual
// introduction through pointer input before exercising unrelated app controls.
export async function dismissFirstUseTour(page) {
  const evaluate = (page.evaluate ?? page.evalValue).bind(page);
  await waitFor(() => evaluate(`Boolean(document.querySelector('[data-testid^="parity:startup-state:"][data-testid*="tour_pending:false"]'))`), 60000, "first-use introduction decision");
  if (!await evaluate(`Boolean(document.querySelector('[data-testid="guided-tour-panel"]'))`)) return;
  const click = async selector => {
    const point = await evaluate(`(() => { const el=document.querySelector(${JSON.stringify(selector)}); if(!el) return null; const b=el.getBoundingClientRect(); return {x:b.x+b.width/2,y:b.y+b.height/2}; })()`);
    if (!point) throw new Error(`Missing first-use control: ${selector}`);
    await page.send("Input.dispatchMouseEvent", {type:"mousePressed",button:"left",clickCount:1,...point});
    await page.send("Input.dispatchMouseEvent", {type:"mouseReleased",button:"left",clickCount:1,...point});
  };
  await click('[data-testid="guided-tour-close"]');
  await waitFor(() => evaluate(`!document.querySelector('[data-testid="guided-tour-panel"]') && Boolean(document.querySelector('.pageLayer.isActive [data-testid="home-button-chart"]'))`), 10000, "close introduction to Home");
  await click('.pageLayer.isActive [data-testid="home-button-chart"]');
  await waitFor(() => evaluate(`Boolean(document.querySelector('.pageLayer.isActive [data-testid="map-surface"]'))`), 10000, "return to chart after introduction");
}
