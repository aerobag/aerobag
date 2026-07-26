#!/usr/bin/env node

// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {
  connectToBrowser,
  launchChrome,
  stopProcess,
  waitFor,
} from "./chrome-cdp.mjs";

const url = process.env.AEROBAG_E2E_URL ?? "http://127.0.0.1:8084/";
const userDataDir = await mkdtemp(path.join(os.tmpdir(), "aerobag-disclaimer-persistence-"));
let chrome;
let browser;

try {
  chrome = await launchChrome({ userDataDir });
  browser = await connectToBrowser(chrome.wsUrl);
  const page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");

  await page.navigate(url);
  await page.waitForLoad();
  await waitFor(
    () => page.evaluate("Boolean(document.querySelector('.disclaimerAcceptButton'))"),
    60_000,
    "disclaimer did not appear",
  );
  await page.evaluate("document.querySelector('.disclaimerAcceptButton').click()");
  await waitFor(
    () => page.evaluate("!document.querySelector('.disclaimerAcceptButton')"),
    10_000,
    "disclaimer did not close after acceptance",
  );

  const settingsJson = await waitFor(
    () => page.evaluate("localStorage.getItem('aerobag.core.settings.v1')"),
    10_000,
    "core settings were not persisted",
  );
  const settings = JSON.parse(settingsJson);
  if (!settings.preferences?.accepted_disclaimer_agreement_ids?.includes("no-warranty-v1")) {
    throw new Error(`persisted settings omit disclaimer agreement: ${settingsJson}`);
  }

  await page.navigate(url);
  await page.waitForLoad();
  await waitFor(
    () => page.evaluate(`(() => {
      const startupFailure = document.querySelector('.startupFailure')?.textContent;
      if (startupFailure) throw new Error(startupFailure);
      return Boolean(
        document.querySelector('[data-testid="map-surface"]')
          || document.querySelector('.disclaimerAcceptButton'),
      );
    })()`),
    60_000,
    "application did not finish reloading",
  );
  if (await page.evaluate("Boolean(document.querySelector('.disclaimerAcceptButton'))")) {
    throw new Error("disclaimer returned after persisted acceptance");
  }

  process.stdout.write("disclaimer persistence e2e passed\n");
} finally {
  await browser?.close();
  await stopProcess(chrome?.process);
  await rm(userDataDir, { recursive: true, force: true });
}
