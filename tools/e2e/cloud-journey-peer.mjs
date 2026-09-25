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
import { E2E_TIMING, observeUntil, performTransition, TerminalObservationError } from "./transition-contract.mjs";
import { advancingVirtualClockScript } from "./virtual-clock.mjs";
import { WebSemanticTransport } from "./web-semantic-transport.mjs";
import { acceptDisclaimer } from "./first-use-startup.mjs";
import { createJourneyRuntime } from "./release-journey-runtime.mjs";

export function rewriteRequestOrigin(url, sourceOrigin, targetOrigin) {
  const original = new URL(url);
  if (original.origin !== new URL(sourceOrigin).origin) return original.toString();
  return `${new URL(targetOrigin).origin}${original.pathname}${original.search}${original.hash}`;
}

export async function linkCloudJourneyPeer(driver, { scheduler, onTiming } = {}) {
  const read = async () => ({
    linked: await driver.readElement("cloud-panel-linked"),
    linking: await driver.readElement("cloud-panel-link_account"),
  });
  const feedback = (state) => {
    if (state.linking?.state === "error") {
      throw new TerminalObservationError("cloud account link", state.linking.text);
    }
    return Boolean(state.linked || state.linking?.state === "working");
  };
  // UI feedback and the provider's network completion have separate owners.
  // Neither a working panel alone nor a silently stalled UI is success.
  await performTransition("cloud journey peer link", {
    ready: () => driver.readElement("cloud-action-accept_setup_code"),
    act: (readyElement) => driver.performAction("accept_setup_code", readyElement),
    complete: read,
    completionSatisfied: feedback,
    scheduler, onTiming,
  });
  return observeUntil("cloud journey peer provider verification", read, {
    timeoutMs: E2E_TIMING.cloudConsistencyMs,
    scheduler,
    accept: (state) => feedback(state) && Boolean(state.linked),
  });
}

export async function launchCloudJourneyPeer({ url, referenceEpochMs, requestOriginRoutes = [], netLogPath = null }) {
  const userDataDir = await mkdtemp(join(tmpdir(), "aerobag-cloud-journey-peer-"));
  let chrome, browser;
  const close = async () => {
    await browser?.close();
    await stopProcess(chrome?.process);
    await rm(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  };
  try {
    // Peers are separate Chrome processes: never let them overwrite the main
    // browser's netlog (inherited through AEROBAG_CHROME_NET_LOG).
    chrome = await launchChrome({ userDataDir, width: 1000, height: 900, netLogPath });
    browser = await connectToBrowser(chrome.endpoint);
    const page = await browser.createPage();
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    // Keep transport diagnostics passive; netlogs cover page/worker traffic
    // without enabling DevTools Network instrumentation during qualification.
    await page.send("Log.enable");
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
    await acceptDisclaimer(createJourneyRuntime({
      journey: { id: "cloud-peer-startup", assertions: [] },
      platform: "web", driver, artifactDir: userDataDir,
    }), { required: true });

    return {
      page,
      driver,

      async state() {
        const state = await page.evaluate(`(() => {
          const state = window.__aerobagE2e?.cloud?.state() ?? null;
          if (!state) return null;
          const cloud = JSON.parse(localStorage.getItem("aerobag.core.settings.v1"))?.cloud;
          return {
            ...state,
            local_sync: cloud ? {
              pending_keys: cloud.records.pending_keys,
              records_format: cloud.records_format,
              workflow: cloud.workflow?.state ?? null,
              last_provider_failure: cloud.last_provider_failure,
              next_retry_epoch_ms: cloud.next_retry_epoch_ms,
              root_revision: cloud.account?.acs?.root_revision,
            } : null,
            application_scripts: Array.from(document.scripts, (script) => script.src)
              .filter(Boolean),
          };
        })()`);
        return { ...state, net_log: netLogPath, browser_diagnostics: page.diagnostics.slice(-200) };
      },

      async waitForState(predicate, description, timeoutMs = E2E_TIMING.cloudConsistencyMs) {
        const result = await observeUntil(description,
          () => page.evaluate("window.__aerobagE2e?.cloud?.state() ?? null"),
          { timeoutMs, accept: state => Boolean(state && predicate(state)) });
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
        await linkCloudJourneyPeer(driver);
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
          ready: async () => {
            const input = await driver.readElement("plan-append-route-input");
            return input?.focused ? input : null;
          },
          act: (readyElement) => driver.submit("plan-append-route-input", readyElement),
          complete: async () => {
            const state = await page.evaluate("window.__aerobagE2e?.cloud?.state() ?? null");
            return state && expected.every((ident) => state.flight_plan_rows.includes(ident))
              ? state
              : null;
          },
        });
        await page.evaluate("window.__aerobagE2e.cloud.awaitProviderIdle()");
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

      close,
    };
  } catch (error) {
    // Setup owns the process/profile until a fully ready peer is returned.
    chrome ??= error.chrome;
    error.chrome = chrome;
    await close().catch((cleanupError) => { error.cleanup_error = cleanupError.message; });
    throw error;
  }
}
