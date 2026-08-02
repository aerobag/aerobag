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
        panels: Array.from(cloud.querySelectorAll('.cloudAccountColumn .cloudFlowPanel'))
          .map((panel) => panel.getAttribute('data-testid')),
        setupEnabled: !cloud.querySelector('[data-testid="cloud-action-begin_setup"]')?.disabled,
        createEnabled: !cloud.querySelector('[data-testid="cloud-action-begin_create"]')?.disabled,
        overallTitle: cloud.querySelector('[data-testid="cloud-overall-status"] h2')?.textContent ?? null,
        overallDetail: cloud.querySelector('[data-testid="cloud-overall-status"] p')?.textContent ?? null,
      };
    })()`),
    10_000,
    "Cloud page did not open",
  );

  if (JSON.stringify(state) !== JSON.stringify({
    panels: ["cloud-panel-get_started"],
    setupEnabled: true,
    createEnabled: true,
    overallTitle: "Cloud not active",
    overallDetail: "No Sync Account linked yet.",
  })) {
    throw new Error(`unexpected initial Cloud page state: ${JSON.stringify(state)}`);
  }
  await click(page, '[data-testid="cloud-action-begin_create"]');
  const providerState = await waitFor(
    async () => page.evaluate(`(() => {
      const activePanel = document.querySelector('.pageLayer.isActive .cloudFlowPanel.is-active')
        ?.getAttribute('data-testid');
      const drive = document.querySelector('[data-testid="cloud-action-select_provider_google_drive"]');
      const aerobag = document.querySelector('[data-testid="cloud-action-select_provider_aerobag_cloud"]');
      if (activePanel !== 'cloud-panel-provider' || !(drive instanceof HTMLButtonElement) || !(aerobag instanceof HTMLButtonElement)) {
        return null;
      }
      return {
        activePanel,
        driveEnabled: !drive.disabled,
        aerobagEnabled: !aerobag.disabled,
      };
    })()`),
    10_000,
    "Cloud provider selection did not appear",
  );
  if (JSON.stringify(providerState) !== JSON.stringify({
    activePanel: "cloud-panel-provider",
    driveEnabled: true,
    aerobagEnabled: true,
  })) {
    throw new Error(`unexpected provider selection state: ${JSON.stringify(providerState)}`);
  }
  await click(page, '[data-testid="cloud-action-select_provider_aerobag_cloud"]');
  const splitState = await waitFor(
    async () => page.evaluate(`(() => {
      const accountPanel = document.querySelector(
        '.pageLayer.isActive .cloudAccountColumn .cloudFlowPanel.is-active'
      );
      const providerCard = document.querySelector(
        '.pageLayer.isActive [data-testid="cloud-provider-card"]'
      );
      const create = document.querySelector('[data-testid="cloud-action-create_account"]');
      const authorize = document.querySelector('[data-testid="cloud-action-authorize_provider"]');
      if (!accountPanel || !providerCard || !(create instanceof HTMLButtonElement)) {
        return null;
      }
      return {
        accountPanel: accountPanel.getAttribute('data-testid'),
        providerTitle: providerCard.querySelector('h2')?.textContent ?? null,
        createEnabled: !create.disabled,
        authorizePresent: authorize instanceof HTMLButtonElement,
      };
    })()`),
    10_000,
    "Cloud provider card did not separate from Sync Account flow",
  );
  if (JSON.stringify(splitState) !== JSON.stringify({
    accountPanel: "cloud-panel-create_account",
    providerTitle: "Aerobag Cloud",
    createEnabled: true,
    authorizePresent: false,
  })) {
    throw new Error(`unexpected split Cloud state: ${JSON.stringify(splitState)}`);
  }
  await click(page, '[data-testid="cloud-action-create_account"]');
  const linkedState = await waitFor(
    async () => page.evaluate(`(() => {
      const linked = document.querySelector('[data-testid="cloud-panel-linked"]');
      const overall = document.querySelector('[data-testid="cloud-overall-status"]');
      if (!linked || !overall) return null;
      return {
        linkedTitle: linked.querySelector('h2')?.textContent ?? null,
        overallTitle: overall.querySelector('h2')?.textContent ?? null,
        overallDetail: overall.querySelector('p')?.textContent ?? null,
      };
    })()`),
    10_000,
    "Aerobag Cloud account was not created",
  );
  if (linkedState.overallTitle !== "Cloud active") {
    throw new Error(`unexpected linked Cloud state: ${JSON.stringify(linkedState)}`);
  }
  process.stdout.write(`cloud page smoke passed: ${JSON.stringify({ state, splitState, linkedState })}\n`);
} finally {
  await browser?.close();
  await stopProcess(chrome?.process);
  await rm(userDataDir, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
}

async function click(page, selector) {
  await waitFor(async () => page.evaluate(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!(element instanceof HTMLElement) || element.hasAttribute('disabled')) return false;
    element.click();
    return true;
  })()`), 10_000, `could not click ${selector}`);
}
