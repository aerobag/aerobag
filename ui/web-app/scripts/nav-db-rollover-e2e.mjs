#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";

const require = createRequire(path.join(process.cwd(), "package.json"));
const WebSocket = require("ws");

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
  generatePublication("success", Date.now() + 3_600_000);
  const vite = launchVite();
  const results = [];
  try {
    await waitForHttp(baseUrl, 120_000);
    for (const scenario of scenarios) {
      results.push(await runScenario(scenario));
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
    await stopProcess(vite);
    fs.rmSync(workRoot, { recursive: true, force: true });
  }
}

async function runScenario(scenario) {
  const scenarioRoot = path.join(artifactRoot, scenario);
  const frameRoot = path.join(scenarioRoot, "frames");
  fs.mkdirSync(frameRoot, { recursive: true });
  generatePublication(scenario, null);
  const lab = JSON.parse(fs.readFileSync(path.join(publicationRoot, "lab.json"), "utf8"));
  const transitionEpochMs = Date.parse(lab.transition_at);
  assert(Number.isFinite(transitionEpochMs), `invalid generated transition_at ${lab.transition_at}`);

  const userDataDir = fs.mkdtempSync(path.join(workRoot, `chrome-${scenario}-`));
  const browserLogPath = path.join(scenarioRoot, "browser.log");
  const chrome = await launchChrome(userDataDir, browserLogPath);
  const browser = await connectToBrowser(chrome.wsUrl);
  let page;
  const consoleRows = [];
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
    await page.navigate(`${baseUrl}?navDbRolloverE2e=${scenario}&run=${encodeURIComponent(runId)}`);
    await waitFor(() => page.evalValue("document.readyState === 'complete'"), 30_000, "document ready");
    await waitFor(
      () => page.evalValue("typeof window.__aerobagE2e?.navDb === 'function'"),
      90_000,
      "NAVDB E2E probe",
    );
    await waitForProbe(page, (probe) => probe.active_nav_db?.cycle === "2607", 90_000, "cycle 2607 startup");
    await acceptDisclaimer(page);
    await buildRichFlightPlan(page);
    assert(
      !(await page.evalValue("Boolean(document.querySelector('.disclaimerAcceptButton'))")),
      "disclaimer returned after persisted acceptance",
    );
    const before = await navDbProbe(page);
    assert(before.active_nav_db?.cycle === "2607", `expected 2607 before transition, got ${before.active_nav_db?.cycle}`);
    assertRichPlan(before);
    const planFingerprint = stablePlanFingerprint(before.active_plan);

    await installStatusOverlay(page, scenario, transitionEpochMs);
    await capturePng(page, path.join(scenarioRoot, "before.png"));
    const frames = [];
    let nextFrameAt = 0;
    const transitionDeadline = transitionEpochMs + 120_000;
    let after = before;
    while (Date.now() < transitionDeadline) {
      after = await navDbProbe(page);
      if (
        scenario === "success"
          ? after.active_nav_db?.cycle === "2608"
          : after.advance_warning !== null
      ) {
        break;
      }
      if (record && Date.now() >= nextFrameAt) {
        const framePath = path.join(frameRoot, `${String(frames.length).padStart(4, "0")}.jpg`);
        await captureJpeg(page, framePath);
        frames.push(framePath);
        nextFrameAt = Date.now() + 1_000;
      }
      await sleep(200);
    }
    const warningUi = scenario === "reject"
      ? await revealRejectedWarning(page)
      : null;
    if (record) {
      const framePath = path.join(frameRoot, `${String(frames.length).padStart(4, "0")}.jpg`);
      await captureJpeg(page, framePath);
      frames.push(framePath);
    }
    await capturePng(page, path.join(scenarioRoot, "after.png"));

    const assertions = assertScenario(scenario, before, after, planFingerprint, warningUi);
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
  await click(page, '.pageLayer.isActive [data-testid="page-button-home"]');
  await click(page, '.pageLayer.isActive [data-testid="home-button-flight-plan"]');
  await waitFor(
    () => page.evalValue("Boolean(document.querySelector('.pageLayer.isActive [data-testid=\"plan-append-route-input\"]'))"),
    10_000,
    "flight-plan route input",
  );
  await page.evalValue(`(() => {
    const input = document.querySelector('.pageLayer.isActive [data-testid="plan-append-route-input"]');
    if (!(input instanceof HTMLTextAreaElement)) return false;
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set;
    setter.call(input, "KRNT SEA KPAE");
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  await waitFor(
    () => page.evalValue("Boolean(document.querySelector('.pageLayer.isActive .planEntryInputShell.isReady'))"),
    30_000,
    "qualified KRNT SEA KPAE route",
  );
  await page.evalValue(`(() => {
    const input = document.querySelector('.pageLayer.isActive [data-testid="plan-append-route-input"]');
    if (!(input instanceof HTMLTextAreaElement)) return false;
    input.form?.requestSubmit();
    return true;
  })()`);
  await waitForProbe(
    page,
    (probe) => probe.active_plan?.route_components?.length >= 3,
    30_000,
    "three-component route",
  );

  await clickButtonByText(page, '[data-testid^="plan-row-"]', "KPAE");
  await click(page, '.pageLayer.isActive [data-testid="plan-row-action-select_procedure"]');
  await waitFor(
    () => page.evalValue("document.querySelectorAll('.pageLayer.isActive .procedureChoiceButton').length > 0"),
    30_000,
    "KPAE procedures",
  );
  await clickButtonByText(page, ".pageLayer.isActive .procedureChoiceButton", "VOR-A");
  await waitFor(
    () => page.evalValue(`Array.from(document.querySelectorAll(".pageLayer.isActive .airwayChoiceButton"))
      .some((button) => button.textContent?.trim() === "ECEPO")`),
    30_000,
    "ECEPO transition",
  );
  await clickButtonByText(page, ".pageLayer.isActive .airwayChoiceButton", "ECEPO");
  await waitForProbe(
    page,
    (probe) => probe.plan_ui_state?.components?.some(
      (component) => component.procedure_id?.includes("VOR-A"),
    ),
    30_000,
    "KPAE VOR-A ECEPO insertion",
  );
}

function assertScenario(scenario, before, after, expectedPlanFingerprint, warningUi) {
  assert(
    stablePlanFingerprint(after.active_plan) === expectedPlanFingerprint,
    "flight plan changed across NAVDB adoption attempt",
  );
  assertRichPlan(after);
  if (scenario === "success") {
    assert(after.active_nav_db?.cycle === "2608", `success scenario remained on ${after.active_nav_db?.cycle}`);
    assert(
      after.nav_data_epoch === before.nav_data_epoch + 1,
      `success scenario epoch changed ${before.nav_data_epoch} -> ${after.nav_data_epoch}`,
    );
    assert(after.advance_warning === null, "success scenario raised NAVDB advance warning");
  } else {
    assert(after.active_nav_db?.cycle === "2607", `reject scenario changed to ${after.active_nav_db?.cycle}`);
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
  await click(page, '.pageLayer.isActive [data-testid="page-button-return-chart"]');
  await waitFor(
    () => page.evalValue(
      "Boolean(document.querySelector('.pageLayer.isActive [data-testid=\"map-surface\"]'))",
    ),
    10_000,
    "chart page after rejected NAVDB advance",
  );
  await waitFor(
    () => page.evalValue(
      "Boolean(document.querySelector('.pageLayer.isActive [data-testid=\"data-status-launcher\"]'))",
    ),
    10_000,
    "data status warning launcher",
  );
  await click(page, '.pageLayer.isActive [data-testid="data-status-launcher"]');
  await waitFor(
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
    10_000,
    "visible NAVDB rejection warning and reload action",
  );
  return { visible: true };
}

function assertRichPlan(probe) {
  const serialized = JSON.stringify(probe.active_plan);
  assert(serialized.includes('"Navaid":"SEA"'), "flight plan does not contain SEA navaid");
  assert(serialized.includes("VOR-A"), "flight plan does not contain VOR-A procedure");
  assert(serialized.includes("ECEPO"), "flight plan does not contain ECEPO transition");
}

function stablePlanFingerprint(plan) {
  if (!plan) {
    return "null";
  }
  return JSON.stringify({
    route_components: plan.route_components,
    route_component_uids: plan.route_component_uids,
    route_component_uid_counter: plan.route_component_uid_counter,
    guidance: plan.guidance,
    departure: plan.departure,
    destination: plan.destination,
    alternate: plan.alternate,
    cruise_altitude_ft: plan.cruise_altitude_ft,
    notes: plan.notes,
  });
}

async function acceptDisclaimer(page) {
  const visible = await page.evalValue("Boolean(document.querySelector('.disclaimerAcceptButton'))");
  if (!visible) {
    return;
  }
  await waitFor(
    () => page.evalValue("document.querySelector('.disclaimerAcceptButton')?.disabled === false"),
    30_000,
    "disclaimer acceptance button",
  );
  await click(page, ".disclaimerAcceptButton");
  await waitFor(
    () => page.evalValue("!document.querySelector('.disclaimerAcceptButton')"),
    10_000,
    "disclaimer dismissal",
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
  return waitFor(async () => {
    const probe = await navDbProbe(page);
    return probe && predicate(probe) ? probe : false;
  }, timeoutMs, description);
}

async function click(page, selector) {
  const clicked = await page.evalValue(`(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!(element instanceof HTMLElement)) return false;
    element.click();
    return true;
  })()`);
  assert(clicked, `could not click ${selector}`);
}

async function clickButtonByText(page, selector, text) {
  const clicked = await page.evalValue(`(() => {
    const element = Array.from(document.querySelectorAll(${JSON.stringify(selector)}))
      .find((candidate) => candidate.textContent?.trim() === ${JSON.stringify(text)});
    if (!(element instanceof HTMLElement)) return false;
    element.click();
    return true;
  })()`);
  assert(clicked, `could not find ${selector} with text ${text}`);
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
  const relative = path.join("nav-db", "advance-2607-to-2608");
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
  const logFd = fs.openSync(viteLogPath, "w");
  const viteBin = path.join(process.cwd(), "node_modules", ".bin", "vite");
  const child = spawn(viteBin, ["--host", "127.0.0.1", "--port", String(port), "--strictPort"], {
    cwd: process.cwd(),
    env: {
      ...process.env,
      AEROBAG_REPO_ROOT: repoRoot,
      AEROBAG_ARTIFACT_READ_PATH: publicationRoot,
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

class CdpClient {
  constructor(wsUrl) {
    this.wsUrl = wsUrl;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
  }

  open() {
    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(this.wsUrl);
      this.ws.addEventListener("open", resolve, { once: true });
      this.ws.addEventListener("error", reject, { once: true });
      this.ws.addEventListener("message", (event) => this.handleMessage(event.data));
    });
  }

  close() {
    this.ws?.close();
  }

  send(method, params = {}, sessionId = undefined) {
    const id = this.nextId++;
    const message = { id, method, params };
    if (sessionId) {
      message.sessionId = sessionId;
    }
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify(message));
    });
  }

  onEvent(sessionId, method, handler) {
    const key = `${sessionId}:${method}`;
    const handlers = this.listeners.get(key) ?? [];
    handlers.push(handler);
    this.listeners.set(key, handlers);
  }

  handleMessage(data) {
    const message = JSON.parse(data);
    if (message.id && this.pending.has(message.id)) {
      const pending = this.pending.get(message.id);
      this.pending.delete(message.id);
      if (message.error) {
        pending.reject(new Error(JSON.stringify(message.error)));
      } else {
        pending.resolve(message.result ?? {});
      }
      return;
    }
    const key = `${message.sessionId ?? ""}:${message.method ?? ""}`;
    for (const handler of this.listeners.get(key) ?? []) {
      handler(message.params ?? {});
    }
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

async function waitFor(check, timeoutMs, description) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      const value = await check();
      if (value) {
        return value;
      }
    } catch (error) {
      lastError = error;
    }
    await sleep(200);
  }
  throw new Error(`timed out waiting for ${description}${lastError ? `: ${lastError}` : ""}`);
}

async function waitForHttp(url, timeoutMs) {
  await waitFor(async () => {
    try {
      const response = await fetch(url);
      return response.ok;
    } catch {
      return false;
    }
  }, timeoutMs, `Vite at ${url}`);
}

async function stopProcess(process) {
  if (!process || process.exitCode !== null || process.killed) {
    return;
  }
  process.kill("SIGTERM");
  const exited = await Promise.race([
    new Promise((resolve) => process.once("exit", () => resolve(true))),
    sleep(3_000).then(() => false),
  ]);
  if (!exited && process.exitCode === null) {
    process.kill("SIGKILL");
  }
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

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

await main();
