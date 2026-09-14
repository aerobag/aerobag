#!/usr/bin/env node
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdtemp, rm, writeFile } from "node:fs/promises";
import assert from "node:assert/strict";
import os from "node:os";
import path from "node:path";
import { connectToBrowser, launchChrome, stopProcess, waitFor } from "./chrome-cdp.mjs";
import { dismissFirstUseTour } from "./dismiss-first-use-tour.mjs";

const url = process.env.AEROBAG_E2E_URL ?? "http://127.0.0.1:8085/";
const route = process.env.AEROBAG_PERF_ROUTE ?? "KRNT KLVN";
const output = process.env.AEROBAG_PERF_OUTPUT ?? "/tmp/aerobag-airway-routing-perf.json";
const narrow = process.argv.includes("--narrow");
const coldHttp = process.argv.includes("--cold-http");
const profile = await mkdtemp(path.join(os.tmpdir(), "aerobag-routing-perf-"));
let chrome;
let browser;
try {
  chrome = await launchChrome({ userDataDir: profile });
  browser = await connectToBrowser(chrome.endpoint);
  const page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  if (coldHttp) {
    await page.send("Network.enable");
    await page.send("Network.setCacheDisabled", { cacheDisabled: true });
  }
  if (narrow) await page.send("Emulation.setDeviceMetricsOverride", {
    width: 400, height: 850, deviceScaleFactor: 1, mobile: false,
  });
  await page.navigate(url);
  await page.waitForLoad();
  await waitFor(async () => page.evaluate(`(() => {
    const error = document.querySelector('.startupErrorModal');
    if (error) throw new Error(error.textContent);
    const accept = document.querySelector('.disclaimerAcceptButton');
    if (accept) { accept.click(); return false; }
    return Boolean(document.querySelector('[data-testid="map-surface"]'));
  })()`), 60_000, "App startup failed", 100);
  await dismissFirstUseTour(page);
  await page.evaluate(`(async () => {
    globalThis.__routePerfRecords = [];
    const logs = await import('/src/domain/debugLog.ts');
    globalThis.__aerobagDebugLogEnabled = true;
    logs.observeDebugLog(record => globalThis.__routePerfRecords.push(record));
  })()`);
  const click = async (selector) => {
    const point = await waitFor(() => page.evaluate(`(() => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!element || element.disabled) return null;
      element.scrollIntoView({ block: 'nearest' });
      const bounds = element.getBoundingClientRect();
      const x = bounds.x + bounds.width / 2, y = bounds.y + bounds.height / 2;
      return element.contains(document.elementFromPoint(x, y)) ? { x, y } : null;
    })()`),
      10_000, `Missing control: ${selector}`, 50);
    await page.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
    await page.send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", clickCount: 1, ...point });
    await page.send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", clickCount: 1, ...point });
  };
  await click('.primaryNavigationCdi');
  await click('[data-testid="plan-append-route-input"]');
  await page.evaluate(`(() => {
    const input = document.querySelector('[data-testid="plan-append-route-input"]');
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(input, ${JSON.stringify(route)});
    input.dispatchEvent(new Event('input', { bubbles: true }));
  })()`);
  await waitFor(() => page.evaluate("Boolean(document.querySelector('.planEntryInputShell.isReady'))"),
    30_000, "Route did not resolve", 50);
  await page.evaluate("document.querySelector('[data-testid=\"plan-append-route-input\"]').form.requestSubmit()");
  await waitFor(() => page.evaluate(`document.querySelectorAll('button.planStructuredWaypointCell').length === 2`),
    30_000, "Expected exactly two flight plan waypoints", 50);
  const measurements = [];
  for (let i = 0; i < 3; i++) {
    if (i) await click('.primaryNavigationCdi');
    await click('button.planStructuredWaypointCell');
    await page.evaluate("globalThis.__routePerfRecords.length = 0; globalThis.__routePerfStart = performance.now()");
    await click('[data-testid="plan-row-action-find_route"]');
    const timing = await waitFor(() => page.evaluate(`(() => {
      const summary = document.querySelector('[data-testid="parity:airway-routing-summary"]');
      const records = globalThis.__routePerfRecords;
      const click = records.find(r => r.tag === 'plan.row_action.click' && r.data.action_id === 'find_route');
      const commit = records.find(r => r.tag === 'airway_routing.editor.commit');
      const frame = records.find(r => r.tag === 'airway_routing.editor.after_frame');
      const mutation = records.find(r => r.tag === 'plan.row_action.mutation.done');
      return summary && click && commit && frame && mutation ? {
        elapsed_ms: performance.now() - globalThis.__routePerfStart, summary: summary.textContent,
        click_to_commit_ms: commit.ts_ms - click.ts_ms,
        click_to_frame_opportunity_ms: frame.ts_ms - click.ts_ms,
      } : null;
    })()`), 60_000, "Route editor did not arrive", 20);
    const records = await page.evaluate("globalThis.__routePerfRecords");
    const input = records.filter(r => /^plan\.row_action\.(pointer_down|click)$/.test(r.tag));
    assert.equal(input.length, 2, "Probe must use the actual pointer/click path");
    assert.ok(input.every(r => r.data.trusted), "Probe must not bypass pointer routing with DOM click()");
    assert.ok(records.some(r => r.tag === "plan.row_action.decision.done"));
    let graphHttp = null;
    if (i === 0) {
      const frontier = records.find(r => r.tag === "worker.flight_plan.perform_row_action.frontier.start")?.data.data;
      assert.ok(frontier, "Fresh profile must fault the routing graph, not reuse an installed graph");
      const pages = new Set(frontier.resources.map(id => Number(id.slice(id.lastIndexOf('/') + 1))));
      const fetches = records.filter(r => r.tag === "worker.nav_kv.page.fetch_detail")
        .map(r => r.data.data).filter(r => pages.has(r.page));
      graphHttp = {
        pages: pages.size,
        timing_pages: fetches.filter(r => r.transfer_size !== null).length,
        network_pages: fetches.filter(r => r.transfer_size > 0).length,
        cached_pages: fetches.filter(r => r.transfer_size === 0 && r.encoded_body_size > 0).length,
        transfer_bytes: fetches.reduce((sum, r) => sum + (r.transfer_size ?? 0), 0),
      };
      if (coldHttp) assert.equal(graphHttp.network_pages, graphHttp.pages, "Cold run must transfer every graph page over HTTP");
    }
    measurements.push({ ...timing, graph_http: graphHttp, records });
    console.log(JSON.stringify({ sample: i, ...timing, graph_http: graphHttp, timings: records.filter(r => /airway_routing|flight_plan\..*(core_had|frontier)|worker.call.done/.test(r.tag)).map(r => {
      const data = r.data?.data ?? r.data;
      const { resources, ...metrics } = data;
      return { tag: r.tag, ...metrics };
    }) }));
    await click('[data-testid="parity:airway-routing-control-remove"]');
    await waitFor(() => page.evaluate("!document.querySelector('[data-testid=\"airway-routing-editor\"]')"),
      10_000, "Route editor did not close", 20);
  }
  await writeFile(output, JSON.stringify({ url, route, narrow, coldHttp, measurements }, null, 2));
  console.log(`Saved ${output}`);
} finally {
  await browser?.close();
  await stopProcess(chrome?.process);
  await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}
