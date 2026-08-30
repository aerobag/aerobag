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
  chrome = await launchChrome({
    userDataDir,
    width: 1200,
    height: 900,
    transport: "pipe",
  });
  browser = await connectToBrowser(chrome);
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

  const aboutUrl = new URL("about", url).href;
  const aboutPage = await browser.createPage();
  await aboutPage.navigate(aboutUrl);
  const aboutResult = await waitFor(
    async () => await aboutPage.evaluate(`(() => {
      const resources = performance.getEntriesByType("resource").map((entry) => entry.name);
      const forbiddenResources = resources.filter((resource) =>
        resource.includes("/packages/")
        || resource.includes(".wasm")
        || resource.includes("appCore.worker")
        || (resource.includes("/assets/index-") && resource.includes(".js"))
      );
      const metadataStatus = document.getElementById("download-status");
      const failure = {
        startup_shell: document.getElementById("startup-shell") !== null,
        app_root: document.getElementById("root") !== null,
        module_script: document.querySelector('script[type="module"]') !== null,
        external_script: document.querySelector("script[src]") !== null,
        forbidden_resources: forbiddenResources,
        metadata_error: metadataStatus?.textContent?.includes("unavailable")
          ? metadataStatus.textContent
          : null,
      };
      if (Object.values(failure).some((value) => Array.isArray(value) ? value.length > 0 : value)) {
        return { state: "failed", failure };
      }
      const ready = document.documentElement.dataset.aerobagPage === "about"
        && document.getElementById("about-page") !== null
        && document.querySelector("article")?.textContent?.trim()
        && document.querySelector("#android-apk[href]") !== null
        && document.getElementById("android-metadata")?.hidden === false;
      return ready ? { state: "ready" } : null;
    })()`),
    10000,
    "standalone About page did not become ready",
    100,
  );
  if (aboutResult.state !== "ready") {
    throw new Error(`standalone About page loaded application resources: ${JSON.stringify(aboutResult.failure)}`);
  }
  process.stdout.write(`${JSON.stringify({ url, about_url: aboutUrl, status: "PASS" })}\n`);
} finally {
  if (browser) await browser.close();
  if (chrome) await stopProcess(chrome.process);
  await rm(userDataDir, { recursive: true, force: true });
}
