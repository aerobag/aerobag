// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  connectToBrowser, launchChrome, stopProcess,
} from "../../ui/web-app/scripts/chrome-cdp.mjs";
import { editSemanticText, WebSemanticJourneyDriver } from "./semantic-journey-driver.mjs";
import { E2E_TIMING, observeUntil, performTransition } from "./transition-contract.mjs";
import { advancingVirtualClockScript } from "./virtual-clock.mjs";
import { WebSemanticTransport } from "./web-semantic-transport.mjs";

export function rewriteRequestOrigin(url, sourceOrigin, targetOrigin) {
  const original = new URL(url);
  if (original.origin !== new URL(sourceOrigin).origin) return original.toString();
  return `${new URL(targetOrigin).origin}${original.pathname}${original.search}${original.hash}`;
}

export async function launchCloudJourneyPeer({ url, referenceEpochMs, requestOriginRoutes = [] }) {
  const userDataDir = await mkdtemp(join(tmpdir(), "aerobag-cloud-journey-peer-"));
  const chrome = await launchChrome({ userDataDir, width: 1000, height: 900 });
  const browser = await connectToBrowser(chrome.endpoint);
  const page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  if (referenceEpochMs != null) {
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: advancingVirtualClockScript(referenceEpochMs),
    });
  }
  for (const route of requestOriginRoutes) {
    await page.routeOrigin(route.sourceOrigin, route.targetOrigin);
  }
  await page.navigate(url);
  await page.waitForLoad();
  const transport = new WebSemanticTransport(page, { url });
  const driver = new WebSemanticJourneyDriver(transport);
  const readStartupState = () => page.evaluate(`(() => ({
      disclaimer: Boolean(document.querySelector('[data-testid="parity:disclaimer-accept-button"]')),
      map: Boolean(document.querySelector('[data-testid="parity:page:map"]')),
      error: document.querySelector('.startupErrorModal')?.textContent ?? null,
    }))()`);
  const startup = await observeUntil("cloud journey peer startup surface", async () => {
    const state = await readStartupState();
    if (state.error) throw new Error(state.error);
    return state.disclaimer || state.map ? state : null;
  }, { timeoutMs: E2E_TIMING.startupMs });
  if (startup.value.disclaimer) {
    await performTransition("cloud journey peer disclaimer", {
      ready: () => driver.readElement("disclaimer-accept-button"),
      act: (readyElement) => driver.performAction("disclaimer-accept-button", readyElement),
      complete: async () => {
        const state = await readStartupState();
        return !state.disclaimer && state.map ? state : null;
      },
    });
  }

  return {
    page,
    driver,

    async state() {
      return page.evaluate("window.__aerobagE2e?.cloud?.state() ?? null");
    },

    async waitForState(predicate, description, timeoutMs = E2E_TIMING.cloudConsistencyMs) {
      const result = await observeUntil(description, async () => {
        const state = await page.evaluate("window.__aerobagE2e?.cloud?.state() ?? null");
        return state && predicate(state) ? state : null;
      }, { timeoutMs });
      return result.value;
    },

    async acceptSetupCode(setupCode) {
      await driver.openPage("cloud");
      await performTransition("cloud journey peer begin setup", {
        ready: () => driver.readElement("cloud-action-begin_setup"),
        act: (readyElement) => driver.performAction("begin_setup", readyElement),
        complete: () => driver.readElement("cloud-setup-code-input"),
      });
      await editSemanticText(
        driver,
        "cloud journey peer enter setup code",
        "cloud-setup-code-input",
        setupCode,
      );
      await performTransition("cloud journey peer link", {
        ready: () => driver.readElement("cloud-action-accept_setup_code"),
        act: (readyElement) => driver.performAction("accept_setup_code", readyElement),
        complete: () => driver.readElement("cloud-panel-linked"),
      });
      await this.waitForState(
        (state) => Boolean(state.event_stream_id),
        "cloud journey peer event stream",
      );
    },

    async appendRoute(route) {
      await driver.openPage("flight_plan");
      await editSemanticText(
        driver,
        `cloud journey peer enter route ${route}`,
        "plan-append-route-input",
        route,
      );
      await observeUntil(
        `cloud journey peer route ${route} committable`,
        () => page.evaluate(
          "Boolean(document.querySelector('.pageLayer.isActive .planEntryInputShell.isReady'))",
        ),
        { timeoutMs: E2E_TIMING.resourceMs },
      );
      const expected = route.trim().split(/\s+/);
      const result = await performTransition(`cloud journey peer commit ${route}`, {
        ready: () => driver.readElement("plan-append-route-input"),
        act: () => driver.submit("plan-append-route-input"),
        complete: async () => {
          const state = await page.evaluate("window.__aerobagE2e?.cloud?.state() ?? null");
          return state && expected.every((ident) => state.flight_plan_rows.includes(ident))
            ? state
            : null;
        },
      });
      return result.value;
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
