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

const urlIndex = process.argv.indexOf("--url");
const url = urlIndex >= 0 ? process.argv[urlIndex + 1] : null;
if (!url) {
  throw new Error("usage: release-web-smoke.mjs --url <staging-url>");
}

const userDataDir = await mkdtemp(path.join(os.tmpdir(), "aerobag-release-smoke-"));
let chrome;
let browser;
try {
  chrome = await launchChrome({ userDataDir, width: 1200, height: 900 });
  browser = await connectToBrowser(chrome.wsUrl);
  const page = await browser.createPage();
  await page.navigate(url);
  const result = await waitFor(
    async () => await page.evaluate(`(() => {
      const text = document.body?.innerText ?? "";
      if (/Startup failed|generated wasm module is missing|required exports/i.test(text)) {
        return { state: "failed", text: text.slice(0, 1000) };
      }
      const useful = document.querySelector("canvas, button, [role=button]");
      return useful && text.trim().length > 0
        ? { state: "ready", text: text.slice(0, 1000) }
        : null;
    })()`),
    30000,
    "staged web app did not reach a useful first paint",
    100,
  );
  if (result.state !== "ready") {
    throw new Error(`staged web app reported startup failure: ${result.text}`);
  }
  process.stdout.write(`${JSON.stringify({ url, status: "PASS" })}\n`);
} finally {
  if (browser) await browser.close();
  if (chrome) await stopProcess(chrome.process);
  await rm(userDataDir, { recursive: true, force: true });
}
