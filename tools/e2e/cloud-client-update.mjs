// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { execFileSync } from "node:child_process";

export function selectNextTestClient(settingsJson) {
  const settings = JSON.parse(settingsJson);
  if (!settings.cloud?.account || !("test_next_client" in settings.cloud)) {
    throw new Error("Cloud format journey requires a linked account in an E2E-enabled core build");
  }
  settings.cloud.test_next_client = true;
  return JSON.stringify(settings);
}

// Simulate installing a new binary, not upgrading account data. The test-only
// selector is applied with the old process stopped; all migration/consent is
// then performed by the real core and visible UI. Production cores omit it.
export async function simulateCloudClientUpdate(driver) {
  if (driver.platform === "web") {
    const page = driver.transport.page;
    // Leave the app entirely before editing its local test-build selector.
    // The fixture health response is same-origin JSON, with no running core.
    await page.navigate(new URL("/__health", driver.transport.url).href);
    await page.waitForLoad();
    await page.evaluate(`(() => {
        if (document.contentType !== "application/json") {
          throw new Error("Expected the fixture's inert JSON health document");
        }
        const select = ${selectNextTestClient.toString()};
        const key = "aerobag.core.settings.v1";
        localStorage.setItem(key, select(localStorage.getItem(key)));
      })()`);
    await driver.reload();
  } else if (driver.platform === "android") {
    const adb = (args, options = {}) => execFileSync("adb", ["-s", driver.serial, ...args], {
      encoding: "utf8", timeout: 30_000, ...options,
    });
    adb(["shell", "am", "force-stop", "org.aerobag.app"]);
    const settings = adb(["exec-out", "run-as", "org.aerobag.app", "cat", "files/core-settings-v1.json"]);
    adb(["shell", "-T", "run-as org.aerobag.app sh -c 'cat > files/core-settings-v1.json'"], {
      input: selectNextTestClient(settings),
    });
    await driver.reload();
  } else {
    throw new Error(`Unsupported cloud journey platform: ${driver.platform}`);
  }
}
