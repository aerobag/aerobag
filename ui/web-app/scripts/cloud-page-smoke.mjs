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

const url = process.argv.find((value) => value.startsWith("http"))
  ?? process.env.AEROBAG_E2E_URL
  ?? "http://127.0.0.1:8083/";
const userDataDir = await mkdtemp(path.join(os.tmpdir(), "aerobag-cloud-page-"));
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

  await waitFor(async () => {
    const state = await page.evaluate(`(() => ({
      disclaimer: Boolean(document.querySelector('[data-testid="parity:disclaimer-accept-button"]')),
      map: Boolean(document.querySelector('[data-testid="map-surface"]')),
      error: document.querySelector('.startupErrorModal')?.textContent ?? null,
    }))()`);
    if (state.error) {
      throw new Error(state.error);
    }
    if (state.disclaimer) {
      await page.evaluate(`document.querySelector('[data-testid="parity:disclaimer-accept-button"]').click()`);
      return false;
    }
    return state.map;
  }, 60_000, "app did not reach the map");

  await click(page, '.pageLayer.isActive [data-testid="page-button-home"]');
  await click(page, '.pageLayer.isActive [data-testid="home-button-cloud"]');
  const state = await waitFor(
    async () => page.evaluate(`(() => {
      const cloud = document.querySelector('.pageLayer.isActive [data-testid="cloud-page"]');
      if (!cloud) return null;
      return {
        provider: cloud.querySelector('[data-testid="cloud-provider-google_drive"]')?.textContent?.trim(),
        providerSelected: cloud.querySelector('[data-testid="cloud-provider-google_drive"]')?.getAttribute('aria-pressed'),
        connection: cloud.querySelector('[data-testid="cloud-connection-state"]')?.textContent?.trim(),
        account: cloud.querySelector('[data-testid="cloud-account-state"]')?.textContent?.trim(),
        connectEnabled: !cloud.querySelector('[data-testid="cloud-action-connect"]')?.disabled,
        createDisabled: Boolean(cloud.querySelector('[data-testid="cloud-action-create_account"]')?.disabled),
      };
    })()`),
    10_000,
    "Cloud page did not open",
  );

  if (JSON.stringify(state) !== JSON.stringify({
    provider: "Google Drive",
    providerSelected: "true",
    connection: "DISCONNECTED",
    account: "NOT LINKED",
    connectEnabled: true,
    createDisabled: true,
  })) {
    throw new Error(`unexpected initial Cloud page state: ${JSON.stringify(state)}`);
  }
  process.stdout.write(`cloud page smoke passed: ${JSON.stringify(state)}\n`);
} finally {
  await browser?.close();
  await stopProcess(chrome?.process);
  await rm(userDataDir, { recursive: true, force: true });
}

async function click(page, selector) {
  await waitFor(async () => page.evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!(element instanceof HTMLElement) || element.hasAttribute('disabled')) return false;
    element.click();
    return true;
  })()`), 10_000, `could not click ${selector}`);
}
