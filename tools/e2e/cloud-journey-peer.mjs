// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  connectToBrowser, launchChrome, stopProcess, waitFor,
} from "../../ui/web-app/scripts/chrome-cdp.mjs";
import { WebSemanticJourneyDriver } from "./semantic-journey-driver.mjs";
import { advancingVirtualClockScript, WebSemanticTransport } from "./web-semantic-transport.mjs";

export async function launchCloudJourneyPeer({ url, referenceEpochMs }) {
  const userDataDir = await mkdtemp(join(tmpdir(), "aerobag-cloud-journey-peer-"));
  const chrome = await launchChrome({ userDataDir, width: 1000, height: 900 });
  const browser = await connectToBrowser(chrome.wsUrl);
  const page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  if (referenceEpochMs != null) {
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: advancingVirtualClockScript(referenceEpochMs),
    });
  }
  await page.navigate(url);
  await page.waitForLoad();
  await waitFor(async () => {
    const state = await page.evaluate(`(() => ({
      disclaimer: Boolean(document.querySelector('[data-testid="parity:disclaimer-accept-button"]')),
      map: Boolean(document.querySelector('[data-testid="parity:page:map"]')),
      error: document.querySelector('.startupErrorModal')?.textContent ?? null,
    }))()`);
    if (state.error) throw new Error(state.error);
    if (state.disclaimer) {
      await page.evaluate(
        `document.querySelector('[data-testid="parity:disclaimer-accept-button"]').click()`,
      );
      return false;
    }
    return state.map;
  }, 60_000, "cloud journey peer did not reach the map");

  const transport = new WebSemanticTransport(page, { url });
  const driver = new WebSemanticJourneyDriver(transport);
  return {
    page,
    driver,

    async state() {
      return page.evaluate("window.__aerobagE2e?.cloud?.state() ?? null");
    },

    async waitForState(predicate, description, timeoutMs = 20_000) {
      return waitFor(async () => {
        const state = await page.evaluate("window.__aerobagE2e?.cloud?.state() ?? null");
        return state && predicate(state) ? state : false;
      }, timeoutMs, description);
    },

    async acceptSetupCode(setupCode) {
      await driver.openPage("cloud");
      await driver.performAction("begin_setup");
      await waitFor(
        () => driver.readElement("cloud-setup-code-input"),
        10_000,
        "cloud journey peer setup-code input did not appear",
      );
      await driver.enterText("cloud-setup-code-input", setupCode);
      await driver.performAction("accept_setup_code");
      await this.waitForState(
        (state) => Boolean(state.event_stream_id),
        "cloud journey peer did not link and connect",
      );
    },

    async appendRoute(route) {
      await driver.openPage("flight_plan");
      await driver.enterText("plan-append-route-input", route);
      await waitFor(
        () => page.evaluate(
          "Boolean(document.querySelector('.pageLayer.isActive .planEntryInputShell.isReady'))",
        ),
        20_000,
        `cloud journey peer route ${route} did not become committable`,
      );
      await page.evaluate(
        "document.querySelector('.pageLayer.isActive .planEntryForm').requestSubmit()",
      );
      const expected = route.trim().split(/\s+/);
      return this.waitForState(
        (state) => expected.every((ident) => state.flight_plan_rows.includes(ident)),
        `cloud journey peer did not commit ${route}`,
      );
    },

    async setOfflinePackagePreferences(preferences) {
      await page.evaluate(
        `window.__aerobagE2e.cloud.setOfflinePackagePreferences(${JSON.stringify(preferences)})`,
      );
      return this.waitForState(
        (state) => JSON.stringify(state.offline_package_preferences) === JSON.stringify(preferences),
        "cloud journey peer did not record offline-package preferences",
      );
    },

    async close() {
      await browser.close();
      await stopProcess(chrome.process);
      await rm(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    },
  };
}
