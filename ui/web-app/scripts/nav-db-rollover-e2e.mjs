#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { CdpClient } from "./chrome-cdp.mjs";

import {
  E2E_TIMING,
  observeUntil,
} from "../../../tools/e2e/transition-contract.mjs";

const args = parseArgs(process.argv.slice(2));
const repoRoot = path.resolve(process.env.AEROBAG_REPO_ROOT ?? path.join(import.meta.dirname, "../../.."));
const chromeBin = args.chrome ?? process.env.CHROME_BIN ?? "google-chrome-stable";
const port = positiveInteger(args.port, 18084);
const transitionDelaySeconds = positiveInteger(args["transition-seconds"], 45);
const headed = args.headed === "true";
const record = args["no-record"] !== "true";
const scenarios = scenarioList(args.scenario ?? "both");
const runId = args["run-id"] ?? new Date().toISOString().replace(/[:.]/g, "-");
const artifactRoot = path.resolve(
  args["artifact-root"]
    ?? path.join(os.tmpdir(), "aerobag-nav-db-rollover-e2e", runId),
);
const workRoot = fs.mkdtempSync(path.join(os.tmpdir(), "aerobag-nav-db-rollover-work-"));
const publicationRoot = path.join(workRoot, "publication");
const fixtureRoot = materializeFixture(path.join(workRoot, "fixtures"));
const viteLogPath = path.join(artifactRoot, "vite.log");
const baseUrl = `http://127.0.0.1:${port}/`;

async function main() {
  fs.mkdirSync(artifactRoot, { recursive: true });
  const results = [];
  try {
    for (const scenario of scenarios) {
      generatePublication(scenario, Date.now() + 3_600_000);
      const vite = launchVite();
      try {
        await waitForHttp(baseUrl);
        results.push(await runScenario(scenario));
      } finally {
        await stopProcess(vite);
      }
    }
    const summary = {
      run_id: runId,
      generated_at_utc: new Date().toISOString(),
      fixture_root: fixtureRoot,
      transition_delay_seconds: transitionDelaySeconds,
      results,
    };
    writeJson(path.join(artifactRoot, "summary.json"), summary);
    console.log(JSON.stringify(summary, null, 2));
  } finally {
    fs.rmSync(workRoot, { recursive: true, force: true });
  }
}

async function runScenario(scenario) {
  const scenarioRoot = path.join(artifactRoot, scenario);
  const frameRoot = path.join(scenarioRoot, "frames");
  fs.mkdirSync(frameRoot, { recursive: true });
  const lab = JSON.parse(fs.readFileSync(path.join(publicationRoot, "lab.json"), "utf8"));
  const initialCycle = lab.initial?.cycle;
  const candidateCycle = lab.candidate?.cycle;
  assert(typeof initialCycle === "string", "generated lab has no initial cycle");
  assert(typeof candidateCycle === "string", "generated lab has no candidate cycle");
  const transitionEpochMs = Date.parse(lab.transition_at);
  assert(Number.isFinite(transitionEpochMs), `invalid generated transition_at ${lab.transition_at}`);

  const userDataDir = fs.mkdtempSync(path.join(workRoot, `chrome-${scenario}-`));
  const browserLogPath = path.join(scenarioRoot, "browser.log");
  const chrome = await launchChrome(userDataDir, browserLogPath);
  const browser = await connectToBrowser(chrome.wsUrl);
  let page;
  const consoleRows = [];
  const requestUrls = new Map();
  try {
    page = await browser.createTarget();
    await page.send("Page.enable");
    await page.send("Runtime.enable");
    await page.send("Log.enable");
    await page.send("Network.enable");
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `try {
        localStorage.setItem("aerobag.web.debugLogToDeveloperServer.v1", "1");
      } catch {}
      globalThis.__aerobagNavDbRolloverE2eRunId = ${JSON.stringify(`${runId}:${scenario}`)};`,
    });
    page.onEvent("Runtime.consoleAPICalled", (event) => {
      consoleRows.push({
        type: event.type,
        timestamp: event.timestamp,
        args: event.args?.map((arg) => arg.value ?? arg.description ?? arg.type),
      });
    });
    page.onEvent("Runtime.exceptionThrown", (event) => {
      consoleRows.push({
        type: "exception",
        timestamp: event.timestamp,
        detail: event.exceptionDetails,
      });
    });
    page.onEvent("Network.requestWillBeSent", (event) => {
      requestUrls.set(event.requestId, event.request?.url ?? null);
    });
    page.onEvent("Network.loadingFailed", (event) => {
      consoleRows.push({
        type: "network-loading-failed",
        timestamp: event.timestamp,
        request_id: event.requestId,
        url: requestUrls.get(event.requestId) ?? null,
        error_text: event.errorText,
        canceled: event.canceled ?? false,
        blocked_reason: event.blockedReason ?? null,
      });
    });
    page.onEvent("Network.responseReceived", (event) => {
      if ((event.response?.status ?? 0) < 400) return;
      consoleRows.push({
        type: "network-error-response",
        timestamp: event.timestamp,
        request_id: event.requestId,
        status: event.response.status,
        url: event.response.url,
      });
    });
    await page.navigate(`${baseUrl}?navDbRolloverE2e=${scenario}&run=${encodeURIComponent(runId)}`);
    await observePage(
      "document ready",
      () => page.evalValue("document.readyState === 'complete'"),
      E2E_TIMING.startupMs,
    );
    await observePage(
      "NAVDB E2E probe",
      () => page.evalValue("typeof window.__aerobagE2e?.navDb === 'function'"),
      E2E_TIMING.startupMs,
    );
    await waitForProbe(
      page,
      (probe) => probe.active_nav_db?.cycle === initialCycle,
      E2E_TIMING.resourceMs,
      `cycle ${initialCycle} startup`,
    );
    await acceptDisclaimer(page);
    await buildRichFlightPlan(page);
    assert(
      !(await page.evalValue("Boolean(document.querySelector('.disclaimerAcceptButton'))")),
      "disclaimer returned after persisted acceptance",
    );
    const before = await navDbProbe(page);
    assert(
      before.active_nav_db?.cycle === initialCycle,
      `expected ${initialCycle} before transition, got ${before.active_nav_db?.cycle}`,
    );
    assertRichPlan(before);
    const planFingerprint = stablePlanFingerprint(before);

    await installStatusOverlay(page, scenario, transitionEpochMs);
    await capturePng(page, path.join(scenarioRoot, "before.png"));
    const frames = [];
    if (record) {
      const framePath = path.join(frameRoot, `${String(frames.length).padStart(4, "0")}.jpg`);
      await captureJpeg(page, framePath);
      frames.push(framePath);
    }
    await observeUntil(`NAVDB ${scenario} ready`, async () => {
      const probe = await navDbProbe(page);
      return probe?.active_nav_db?.cycle === initialCycle ? probe : null;
    }, { timeoutMs: E2E_TIMING.localReadyMs });
    await page.evalValue(
      `window.__aerobagE2e.navDbMaintainAt(${Math.trunc(transitionEpochMs + 1)})`,
    );
    const after = (await observeUntil(`NAVDB ${scenario} transaction`, async () => {
      const probe = await navDbProbe(page);
      const complete = scenario === "success"
        ? probe?.active_nav_db?.cycle === candidateCycle
        : probe?.advance_warning !== null;
      return complete ? probe : null;
    }, {
      timeoutMs: E2E_TIMING.bulkOperationMs,
      intervalMs: E2E_TIMING.resourcePollIntervalMs,
    })).value;
    const warningUi = scenario === "reject"
      ? await revealRejectedWarning(page)
      : null;
    if (record) {
      const framePath = path.join(frameRoot, `${String(frames.length).padStart(4, "0")}.jpg`);
      await captureJpeg(page, framePath);
      frames.push(framePath);
    }
    await capturePng(page, path.join(scenarioRoot, "after.png"));

    const assertions = assertScenario(
      scenario,
      before,
      after,
      planFingerprint,
      warningUi,
      initialCycle,
      candidateCycle,
    );
    writeJson(path.join(scenarioRoot, "assertions.json"), {
      scenario,
      transition_at: new Date(transitionEpochMs).toISOString(),
      before,
      after,
      assertions,
    });
    writeJson(path.join(scenarioRoot, "browser-console.json"), consoleRows);
    const recording = record ? renderRecording(frames, scenarioRoot) : null;
    return {
      scenario,
      passed: true,
      active_cycle_before: before.active_nav_db?.cycle ?? null,
      active_cycle_after: after.active_nav_db?.cycle ?? null,
      nav_data_epoch_before: before.nav_data_epoch,
      nav_data_epoch_after: after.nav_data_epoch,
      warning_after: after.advance_warning?.value ?? null,
      recording,
      artifact_dir: scenarioRoot,
    };
  } catch (error) {
    if (page) {
      await capturePng(page, path.join(scenarioRoot, "failure.png")).catch(() => {});
    }
    writeJson(path.join(scenarioRoot, "browser-console.json"), consoleRows);
    writeJson(path.join(scenarioRoot, "failure.json"), {
      scenario,
      error: error instanceof Error ? error.stack ?? error.message : String(error),
    });
    throw error;
  } finally {
    await browser.close();
    await stopProcess(chrome.process);
  }
}

async function buildRichFlightPlan(page) {
  await clickOnce(page, '.pageLayer.isActive [data-testid="page-button-home"]');
  await clickOnce(page, '.pageLayer.isActive [data-testid="home-button-flight_plan"]');
  await observePage(
    "flight-plan route input",
    () => page.evalValue("Boolean(document.querySelector('.pageLayer.isActive [data-testid=\"plan-append-route-input\"]'))"),
    E2E_TIMING.localReadyMs,
  );
  await page.evalValue(`(() => {
    const input = document.querySelector('.pageLayer.isActive [data-testid="plan-append-route-input"]');
    if (!(input instanceof HTMLTextAreaElement)) return false;
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set;
    setter.call(input, "KRNT SEA KPAE");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await observePage(
    "qualified KRNT SEA KPAE route",
    () => page.evalValue("Boolean(document.querySelector('.pageLayer.isActive .planEntryInputShell.isReady'))"),
    E2E_TIMING.resourceMs,
  );
  await page.evalValue(`(() => {
    const input = document.querySelector('.pageLayer.isActive [data-testid="plan-append-route-input"]');
    if (!(input instanceof HTMLTextAreaElement)) return false;
    input.form?.requestSubmit();
    return true;
  })()`);
  await waitForProbe(
    page,
    (probe) => {
      const labels = probe.plan_ui_state?.display_rows?.map((row) => row.label) ?? [];
      return ["KRNT", "SEA", "KPAE"].every((label) => labels.includes(label));
    },
    E2E_TIMING.localReadyMs,
    "three-component route",
  );

  await clickButtonByTextOnce(page, '[data-testid^="plan-row-"]', "KPAE");
  await clickOnce(page, '.pageLayer.isActive [data-testid="plan-row-action-select_approach"]');
  await observePage(
    "KPAE procedures",
    () => page.evalValue("document.querySelectorAll('.pageLayer.isActive .procedureChoiceButton').length > 0"),
    E2E_TIMING.resourceMs,
  );
  await clickButtonByTextOnce(page, ".pageLayer.isActive .procedureChoiceButton", "VOR-A");
  await clickOnce(page, '.pageLayer.isActive [data-testid="plan-procedure-transition-ECEPO"]');
  await waitForProbe(
    page,
    (probe) => probe.plan_ui_state?.display_rows?.some((row) => row.procedure_id?.includes("VOR-A")),
    E2E_TIMING.localReadyMs,
    "KPAE VOR-A ECEPO insertion",
  );
}

function assertScenario(
  scenario,
  before,
  after,
  expectedPlanFingerprint,
  warningUi,
  initialCycle,
  candidateCycle,
) {
  assert(
    stablePlanFingerprint(after) === expectedPlanFingerprint,
    "flight plan changed across NAVDB adoption attempt",
  );
  assertRichPlan(after);
  if (scenario === "success") {
    assert(
      after.active_nav_db?.cycle === candidateCycle,
      `success scenario remained on ${after.active_nav_db?.cycle}`,
    );
    assert(
      after.nav_data_epoch === before.nav_data_epoch + 1,
      `success scenario epoch changed ${before.nav_data_epoch} -> ${after.nav_data_epoch}`,
    );
    assert(after.advance_warning === null, "success scenario raised NAVDB advance warning");
  } else {
    assert(
      after.active_nav_db?.cycle === initialCycle,
      `reject scenario changed to ${after.active_nav_db?.cycle}`,
    );
    assert(
      after.nav_data_epoch === before.nav_data_epoch,
      `reject scenario epoch changed ${before.nav_data_epoch} -> ${after.nav_data_epoch}`,
    );
    assert(after.advance_warning?.id === "nav_db:advance", "reject scenario did not raise nav_db:advance");
    assert(
      after.advance_warning?.actions?.some((action) => action.id === "app:reload"),
      "reject scenario warning has no reload action",
    );
    assert(
      after.next_nav_db_maintenance_epoch_ms === null,
      "reject scenario did not lock out repeated NAVDB adoption",
    );
    assert(warningUi?.visible === true, "reject scenario did not visibly open the warning panel");
  }
  return {
    plan_preserved: true,
    rich_plan_preserved: true,
    expected_disposition: scenario === "success" ? "adopted" : "rejected",
    warning_ui_visible: warningUi?.visible ?? null,
  };
}

async function revealRejectedWarning(page) {
  await clickOnce(page, '.pageLayer.isActive [data-testid="page-button-return-chart"]');
  await observePage(
    "chart page after rejected NAVDB advance",
    () => page.evalValue(
      "Boolean(document.querySelector('.pageLayer.isActive [data-testid=\"map-surface\"]'))",
    ),
    E2E_TIMING.localReadyMs,
  );
  await observePage(
    "data status warning launcher",
    () => page.evalValue(
      "Boolean(document.querySelector('.pageLayer.isActive [data-testid=\"data-status-launcher\"]'))",
    ),
    E2E_TIMING.localReadyMs,
  );
  await clickOnce(page, '.pageLayer.isActive [data-testid="data-status-launcher"]');
  await observePage(
    "visible NAVDB rejection warning and reload action",
    () => page.evalValue(`(() => {
      const panel = document.querySelector('[data-testid="data-status-panel"]');
      const warning = document.querySelector('[data-testid="data-status-box-nav_db:advance"]');
      const reload = document.querySelector('[data-testid="data-status-action-nav_db:advance-app:reload"]');
      if (!(panel instanceof HTMLElement) || !(warning instanceof HTMLElement)) return false;
      if (!(reload instanceof HTMLButtonElement) || reload.disabled) return false;
      const text = warning.textContent ?? "";
      const panelRect = panel.getBoundingClientRect();
      const panelStyle = window.getComputedStyle(panel);
      return panelRect.width > 0
        && panelRect.height > 0
        && panelStyle.display !== "none"
        && panelStyle.visibility !== "hidden"
        && text.includes("NAV DB")
        && text.includes("ADVANCE FAILED")
        && text.includes("Reload application when not flying")
        && reload.textContent?.trim() === "Reload application";
    })()`),
    E2E_TIMING.localReadyMs,
  );
  return { visible: true };
}

function assertRichPlan(probe) {
  const rows = probe.plan_ui_state?.display_rows ?? [];
  assert(rows.some((row) => row.label === "SEA"), "flight plan does not contain SEA navaid");
  assert(
    rows.some((row) => row.procedure_id?.includes("VOR-A")),
    "flight plan does not contain VOR-A procedure",
  );
  assert(rows.some((row) => row.label === "ECEPO"), "flight plan does not contain ECEPO transition");
}

function stablePlanFingerprint(probe) {
  if (!probe.active_plan || !probe.plan_ui_state) {
    return "null";
  }
  return JSON.stringify({
    plan_id: probe.active_plan.plan_id,
    plan_version: probe.active_plan.plan_version,
    display_rows: probe.plan_ui_state.display_rows,
    guidance: probe.plan_ui_state.guidance,
  });
}

async function acceptDisclaimer(page) {
  const visible = await page.evalValue("Boolean(document.querySelector('.disclaimerAcceptButton'))");
  if (!visible) {
    return;
  }
  await observePage(
    "disclaimer acceptance button",
    () => page.evalValue("document.querySelector('.disclaimerAcceptButton')?.disabled === false"),
    E2E_TIMING.localReadyMs,
  );
  await clickOnce(page, ".disclaimerAcceptButton");
  await observePage(
    "disclaimer dismissal",
    () => page.evalValue("!document.querySelector('.disclaimerAcceptButton')"),
    E2E_TIMING.localReadyMs,
  );
}

async function installStatusOverlay(page, scenario, transitionEpochMs) {
  await page.evalValue(`(() => {
    document.querySelector("#nav-db-rollover-e2e-status")?.remove();
    const panel = document.createElement("pre");
    panel.id = "nav-db-rollover-e2e-status";
    Object.assign(panel.style, {
      position: "fixed",
      left: "16px",
      bottom: "16px",
      zIndex: "2147483647",
      margin: "0",
      padding: "14px 18px",
      maxWidth: "42vw",
      whiteSpace: "pre-wrap",
      color: "#fff",
      background: "rgba(0, 20, 34, 0.92)",
      border: "3px solid #fff",
      borderRadius: "8px",
      font: "700 18px/1.35 monospace",
      pointerEvents: "none",
    });
    document.body.append(panel);
    const scenario = ${JSON.stringify(scenario)};
    const transition = ${transitionEpochMs};
    const render = () => {
      const state = window.__aerobagE2e?.navDb?.() ?? {};
      const remaining = Math.max(0, transition - Date.now());
      panel.textContent = [
        "NAVDB ROLLOVER E2E: " + scenario.toUpperCase(),
        "active cycle: " + (state.active_nav_db?.cycle ?? "---"),
        "nav epoch: " + (state.nav_data_epoch ?? "---"),
        "transition in: " + (remaining / 1000).toFixed(1) + "s",
        "plan: KRNT SEA KPAE / KPAE VOR-A ECEPO",
        "warning: " + (state.advance_warning?.value ?? "none"),
      ].join("\\n");
    };
    render();
    window.setInterval(render, 250);
    return true;
  })()`);
}

async function navDbProbe(page) {
  return page.evalValue("window.__aerobagE2e?.navDb?.() ?? null");
}

async function waitForProbe(page, predicate, timeoutMs, description) {
  return observePage(description, async () => {
    const probe = await navDbProbe(page);
    return probe && predicate(probe) ? probe : false;
  }, timeoutMs);
}

async function clickOnce(page, selector) {
  await observePage(
    `clickable ${selector}`,
    () => page.evalValue(`(() => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!(element instanceof HTMLElement)) return false;
      if (element instanceof HTMLButtonElement && element.disabled) return false;
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return rect.width > 0 && rect.height > 0
        && style.visibility !== "hidden" && style.display !== "none";
    })()`),
    E2E_TIMING.localReadyMs,
  );
  const acted = await page.evalValue(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!(element instanceof HTMLElement)) return false;
    element.click();
    return true;
  })()`);
  assert(acted, `click target vanished before action: ${selector}`);
}

async function clickButtonByTextOnce(page, selector, text) {
  await observePage(
    `${selector} with text ${text}`,
    () => page.evalValue(`(() => {
      const element = Array.from(document.querySelectorAll(${JSON.stringify(selector)}))
        .find((candidate) => candidate.textContent?.trim() === ${JSON.stringify(text)});
      if (!(element instanceof HTMLElement)) return false;
      if (element instanceof HTMLButtonElement && element.disabled) return false;
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return rect.width > 0 && rect.height > 0
        && style.visibility !== "hidden" && style.display !== "none";
    })()`),
    E2E_TIMING.localReadyMs,
  );
  const acted = await page.evalValue(`(() => {
    const element = Array.from(document.querySelectorAll(${JSON.stringify(selector)}))
      .find((candidate) => candidate.textContent?.trim() === ${JSON.stringify(text)});
    if (!(element instanceof HTMLElement)) return false;
    element.click();
    return true;
  })()`);
  assert(acted, `button vanished before action: ${selector} with text ${text}`);
}

function generatePublication(scenario, transitionEpochMs) {
  const timingArgs = transitionEpochMs === null
    ? ["--transition-delay-seconds", String(transitionDelaySeconds)]
    : ["--transition-at", new Date(transitionEpochMs).toISOString()];
  runChecked(
    "cargo",
    [
      "run",
      "--quiet",
      "--manifest-path",
      path.join(repoRoot, "product/preprocessor/Cargo.toml"),
      "-p",
      "preprocessor-cli",
      "--bin",
      "nav_db_rollover_lab",
      "--",
      "--fixture-root",
      fixtureRoot,
      "--output-root",
      publicationRoot,
      ...timingArgs,
      "--scenario",
      scenario,
    ],
    { cwd: repoRoot },
  );
}

function materializeFixture(destinationRoot) {
  const fixtureLock = JSON.parse(
    fs.readFileSync(path.join(repoRoot, "test-artifacts.lock.json"), "utf8"),
  );
  const configuredPath = fixtureLock.fixtures?.["nav-db-advance"]?.path;
  assert(typeof configuredPath === "string", "fixture lock has no nav-db-advance path");
  const relative = path.normalize(configuredPath);
  const configured = process.env.AEROBAG_TEST_ARTIFACTS_ROOT;
  const candidates = [
    configured ? path.join(configured, relative) : null,
    path.resolve(repoRoot, "../aerobag-test-artifacts", relative),
  ].filter(Boolean);
  for (const candidate of candidates) {
    if (fs.existsSync(path.join(candidate, "fixture.json"))) {
      return candidate;
    }
  }

  const bareCandidates = [
    process.env.AEROBAG_TEST_ARTIFACTS_GIT,
    "/root/aerobag-test-artifacts.git",
    path.resolve(repoRoot, "../aerobag-test-artifacts.git"),
  ].filter(Boolean);
  const bare = bareCandidates.find((candidate) => fs.existsSync(candidate));
  if (!bare) {
    throw new Error(
      "NAVDB rollover fixtures are unavailable; set AEROBAG_TEST_ARTIFACTS_ROOT or AEROBAG_TEST_ARTIFACTS_GIT",
    );
  }
  fs.mkdirSync(destinationRoot, { recursive: true });
  const archive = path.join(destinationRoot, "fixture.tar");
  runChecked(
    "git",
    [`--git-dir=${bare}`, "archive", "--format=tar", `--output=${archive}`, "HEAD", relative],
    { cwd: repoRoot },
  );
  runChecked("tar", ["-xf", archive, "-C", destinationRoot], { cwd: repoRoot });
  return path.join(destinationRoot, relative);
}

function launchVite() {
  fs.mkdirSync(path.dirname(viteLogPath), { recursive: true });
  const logFd = fs.openSync(viteLogPath, "a");
  const viteBin = path.join(process.cwd(), "node_modules", ".bin", "vite");
  const child = spawn(viteBin, ["--host", "127.0.0.1", "--port", String(port), "--strictPort"], {
    cwd: process.cwd(),
    env: {
      ...process.env,
      AEROBAG_REPO_ROOT: repoRoot,
      AEROBAG_ARTIFACT_READ_PATH: publicationRoot,
      AEROBAG_E2E_ENABLED: "1",
      AEROBAG_LIVE_FEEDS_ORIGIN: "",
      AEROBAG_WEB_DEBUG_LOG_ENABLED: "1",
    },
    stdio: ["ignore", logFd, logFd],
  });
  child.once("exit", () => fs.closeSync(logFd));
  return child;
}

function launchChrome(userDataDir, browserLogPath) {
  return new Promise((resolve, reject) => {
    const logFd = fs.openSync(browserLogPath, "w");
    const chromeArgs = [
      "--no-sandbox",
      "--disable-gpu",
      "--disable-dev-shm-usage",
      "--no-first-run",
      "--no-default-browser-check",
      "--remote-debugging-port=0",
      `--user-data-dir=${userDataDir}`,
      "--window-size=1440,1000",
      "about:blank",
    ];
    if (!headed) {
      chromeArgs.unshift("--headless=new");
    }
    const child = spawn(chromeBin, chromeArgs, { stdio: ["ignore", "ignore", "pipe"] });
    let stderr = "";
    const timer = setTimeout(() => {
      reject(new Error(`timed out waiting for Chrome DevTools endpoint; stderr=${stderr}`));
    }, 20_000);
    child.stderr.on("data", (chunk) => {
      const text = chunk.toString("utf8");
      stderr += text;
      fs.writeSync(logFd, text);
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) {
        clearTimeout(timer);
        resolve({ process: child, wsUrl: match[1] });
      }
    });
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once("exit", (code, signal) => {
      fs.closeSync(logFd);
      if (!stderr.includes("DevTools listening on")) {
        clearTimeout(timer);
        reject(new Error(`Chrome exited before DevTools was ready: code=${code} signal=${signal}`));
      }
    });
  });
}

async function connectToBrowser(wsUrl) {
  const client = new CdpClient(wsUrl);
  await client.open();
  await client.send("Browser.getVersion", {}, undefined, E2E_TIMING.startupMs);
  return {
    close: () => client.close(),
    async createTarget() {
      const created = await client.send("Target.createTarget", { url: "about:blank" });
      const attached = await client.send("Target.attachToTarget", {
        targetId: created.targetId,
        flatten: true,
      });
      return new CdpPage(client, attached.sessionId);
    },
  };
}

class CdpPage {
  constructor(client, sessionId) {
    this.client = client;
    this.sessionId = sessionId;
  }

  send(method, params = {}) {
    return this.client.send(method, params, this.sessionId);
  }

  onEvent(method, handler) {
    this.client.onEvent(this.sessionId, method, handler);
  }

  async navigate(url) {
    await this.send("Page.navigate", { url });
  }

  async evalValue(expression) {
    const response = await this.send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (response.exceptionDetails) {
      throw new Error(response.exceptionDetails.exception?.description ?? response.exceptionDetails.text);
    }
    return response.result?.value;
  }
}

async function capturePng(page, outputPath) {
  const result = await page.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
  fs.writeFileSync(outputPath, Buffer.from(result.data, "base64"));
}

async function captureJpeg(page, outputPath) {
  const result = await page.send("Page.captureScreenshot", {
    format: "jpeg",
    quality: 76,
    captureBeyondViewport: false,
  });
  fs.writeFileSync(outputPath, Buffer.from(result.data, "base64"));
}

function renderRecording(frames, scenarioRoot) {
  if (frames.length === 0) {
    return null;
  }
  const output = path.join(scenarioRoot, "recording.gif");
  const rendered = spawnSync(
    "convert",
    ["-delay", "100", "-loop", "0", ...frames, output],
    { encoding: "utf8" },
  );
  if (rendered.status !== 0) {
    fs.writeFileSync(
      path.join(scenarioRoot, "recording-error.txt"),
      rendered.stderr || rendered.error?.message || "ImageMagick convert failed",
    );
    return null;
  }
  return output;
}

async function observePage(description, probe, timeoutMs) {
  const result = await observeUntil(description, probe, {
    timeoutMs,
    intervalMs: E2E_TIMING.resourcePollIntervalMs,
  });
  return result.value;
}

async function waitForHttp(url) {
  await observePage(`Vite at ${url}`, async () => {
    try {
      const response = await fetch(url);
      return response.ok;
    } catch {
      return false;
    }
  }, E2E_TIMING.startupMs);
}

async function stopProcess(process) {
  if (!process || process.exitCode !== null || process.killed) {
    return;
  }
  process.kill("SIGTERM");
  const exited = await processExitWithin(process, E2E_TIMING.userResponseMs);
  if (!exited && process.exitCode === null) {
    process.kill("SIGKILL");
  }
}

function processExitWithin(process, timeoutMs) {
  return new Promise((resolve) => {
    const onExit = () => {
      clearTimeout(timer);
      resolve(true);
    };
    const timer = setTimeout(() => {
      process.off("exit", onExit);
      resolve(false);
    }, timeoutMs);
    process.once("exit", onExit);
  });
}

function runChecked(command, commandArgs, options = {}) {
  const result = spawnSync(command, commandArgs, {
    ...options,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(
      `${command} ${commandArgs.join(" ")} failed (${result.status}):\n${result.stdout ?? ""}\n${result.stderr ?? ""}`,
    );
  }
  return result.stdout;
}

function scenarioList(value) {
  if (value === "both") {
    return ["success", "reject"];
  }
  if (value === "success" || value === "reject") {
    return [value];
  }
  throw new Error(`invalid --scenario ${value}; expected success, reject, or both`);
}

function parseArgs(values) {
  const parsed = {};
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (!value.startsWith("--")) {
      continue;
    }
    const keyValue = value.slice(2);
    const equals = keyValue.indexOf("=");
    if (equals >= 0) {
      parsed[keyValue.slice(0, equals)] = keyValue.slice(equals + 1);
    } else if (keyValue.startsWith("no-") || keyValue === "headed") {
      parsed[keyValue] = "true";
    } else {
      parsed[keyValue] = values[++index];
    }
  }
  return parsed;
}

function positiveInteger(value, fallback) {
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : fallback;
}

function writeJson(outputPath, value) {
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(value, null, 2)}\n`);
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

await main();
