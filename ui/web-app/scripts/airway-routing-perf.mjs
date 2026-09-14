#!/usr/bin/env node
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { connectToBrowser, launchChrome, stopProcess, waitFor } from "./chrome-cdp.mjs";

const url = process.env.AEROBAG_E2E_URL ?? "http://127.0.0.1:8085/";
const route = process.env.AEROBAG_PERF_ROUTE ?? "KRNT KLVN";
const output = process.env.AEROBAG_PERF_OUTPUT ?? "/tmp/aerobag-airway-routing-perf.json";
const narrow = process.argv.includes("--narrow");
const profile = await mkdtemp(path.join(os.tmpdir(), "aerobag-routing-perf-"));
let chrome;
let browser;
try {
  chrome = await launchChrome({ userDataDir: profile });
  browser = await connectToBrowser(chrome.endpoint);
  const page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
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
  await page.evaluate(`(async () => {
    globalThis.__routePerfRecords = [];
    const logs = await import('/src/domain/debugLog.ts');
    globalThis.__aerobagDebugLogEnabled = true;
    logs.observeDebugLog(record => globalThis.__routePerfRecords.push(record));
  })()`);
  const click = async (selector) => {
    await waitFor(() => page.evaluate(`Boolean(document.querySelector(${JSON.stringify(selector)}))`),
      10_000, `Missing control: ${selector}`, 50);
    await page.evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`);
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
      return summary ? { elapsed_ms: performance.now() - globalThis.__routePerfStart, summary: summary.textContent } : null;
    })()`), 60_000, "Route editor did not arrive", 20);
    const records = await page.evaluate("globalThis.__routePerfRecords");
    measurements.push({ ...timing, records });
    console.log(JSON.stringify({ sample: i, ...timing, timings: records.filter(r => /airway_routing|flight_plan\..*(core_had|frontier)|worker.call.done/.test(r.tag)).map(r => {
      const data = r.data?.data ?? r.data;
      const { resources, ...metrics } = data;
      return { tag: r.tag, ...metrics };
    }) }));
    await click('[data-testid="parity:airway-routing-control-remove"]');
    await waitFor(() => page.evaluate("!document.querySelector('[data-testid=\"airway-routing-editor\"]')"),
      10_000, "Route editor did not close", 20);
  }
  await writeFile(output, JSON.stringify({ url, route, narrow, measurements }, null, 2));
  console.log(`Saved ${output}`);
} finally {
  await browser?.close();
  await stopProcess(chrome?.process);
  await rm(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}
