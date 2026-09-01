// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { PassThrough } from "node:stream";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import {
  createJourneyResult, finishJourneyResult, recordJourneyCheck,
  recordJourneyStep, validateJourneyResult,
} from "./journey-result.mjs";
import {
  createJourneyRuntime, releaseJourneyFixtureUrl, semanticOptionSelected,
  summarizeFixtureRequests,
} from "./release-journey-runtime.mjs";
import { verifyProductSurfaceCoverage } from "./product-surface-coverage.mjs";
import { RELEASE_JOURNEYS, validateJourneyRegistry } from "./release-journey-registry.mjs";
import { validateAndroidSmokeFixture } from "./android-smoke-fixture.mjs";
import { validateReleaseJourneyFixture } from "./release-journey-fixture.mjs";
import {
  chooseForecastWindModel, dismissPlanRowTray,
  openAndDismissDataStatus,
  offlineSyncButtonIsIdle,
  publicationArtifactRequestCount,
  publicationCatalogRequestCount,
  rasterPlanHasVisiblePaint,
  rasterPlanIsDisplayReady,
  rasterStateFromProjection,
  selectChartSearchSuggestion,
  selectProcedure,
  selectTfrFromPreparedMap,
} from "./release-journey-implementations.mjs";
import {
  decodeReleaseJourneyFixturePath,
  liveFeedEventsFromCurrent,
  webDistIndexSha256, webDistRelativeCandidates,
} from "./serve-release-journey-fixture.mjs";
import {
  androidActionCandidates, androidElementEnabled, androidMapSelectionEntryFromState,
  AndroidSemanticJourneyDriver,
  androidElementMayRequireHorizontalScroll, androidElementMayRequireVerticalScroll,
  androidElementSemanticTag,
  androidDataStatusRowsFromStateTag, androidPageTag, androidProjectionMayRequireVerticalScan,
  androidPageIdFromStartupStateTag,
  androidSemanticTag, androidSessionRevisionFromStateTag,
  androidZoomKeyCode, editSemanticText, findTagOrPrefix,
  establishRevealedElement, navigateSemanticPage,
  semanticActionReadinessSamples,
  semanticTransitionCompletionSamples,
  SEMANTIC_DRIVER_OPERATIONS, SemanticJourneyDriver,
  validateSemanticDriver, WebSemanticJourneyDriver, webTestIdSelector,
} from "./semantic-journey-driver.mjs";
import { advancingVirtualClockScript } from "./virtual-clock.mjs";
import { clampDragEndpoint, timelineSeekDeltaX } from "./gesture-geometry.mjs";
import { WebSemanticTransport } from "./web-semantic-transport.mjs";
import {
  assertConditionRemains, E2E_TIMING, observeChangedValueUntilStable,
  ObservationTimeoutError, observeUntil, observeValueUntilStable, performTransition,
  TerminalObservationError, TransientObservationError,
} from "./transition-contract.mjs";
import {
  auditJourneyStructure,
  auditQualificationJourneys,
  webWorkspaceDirectory,
} from "./journey-structure-audit.mjs";
import { rewriteRequestOrigin } from "./cloud-journey-peer.mjs";
import { CdpClient, CdpPage } from "../../ui/web-app/scripts/chrome-cdp.mjs";
import {
  androidSemanticReadinessStateMatches,
  androidSemanticTargetStateMatches,
  semanticDriverActionRequest,
  semanticDriverObservationRequest,
  setAndroidWallClockAndWait,
} from "./android-harness.mjs";
import { establishChromeRuntime } from "./run-android-chrome-livefeed-e2e.mjs";

const LAB_METADATA_POISON_CONFIG = join(
  tmpdir(),
  `aerobag-release-lab-metadata-${process.pid}.sh`,
);
writeFileSync(LAB_METADATA_POISON_CONFIG, "exit 97\n");
test.after(() => rmSync(LAB_METADATA_POISON_CONFIG, { force: true }));

function labMetadataEnvironment() {
  const environment = {
    AEROBAG_INSTANCE_CONFIG: LAB_METADATA_POISON_CONFIG,
    AEROBAG_RELEASE_JOURNEY_FIXTURE_SEARCH_ROOT: join(
      tmpdir(),
      `aerobag-missing-fixture-search-root-${process.pid}`,
    ),
    AEROBAG_RELEASE_JOURNEY_REPETITIONS: "1",
    AEROBAG_UI_TARGET_ROOT: tmpdir(),
    VNC_PORT: "5900",
  };
  for (const name of ["HOME", "PATH", "TMPDIR"]) {
    if (process.env[name]) {
      environment[name] = process.env[name];
    }
  }
  return environment;
}

test("web tooling resolves dependencies only from an explicit web workspace", () => {
  assert.equal(
    webWorkspaceDirectory(
      { AEROBAG_UI_TARGET_ROOT: "/tmp/aerobag-ui-target" },
      "/checkout",
    ),
    "/checkout/ui/web-app",
  );
  assert.equal(
    webWorkspaceDirectory(
      {
        AEROBAG_UI_TARGET_ROOT: "/tmp/ignored-ui-target",
        AEROBAG_WEB_WORKSPACE_DIR: "/tmp/explicit-web-workspace",
      },
      "/checkout",
    ),
    "/tmp/explicit-web-workspace",
  );
  assert.equal(webWorkspaceDirectory({}, "/checkout"), "/checkout/ui/web-app");

  const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
  const runner = readFileSync(join(repoRoot, "ui/web-app/scripts/run-target-workspace.sh"), "utf8");
  assert.match(runner, /cp -a "\$WEB_SOURCE_DIR\/scripts" "\$WORKSPACE_DIR\/scripts"/);
  assert.doesNotMatch(runner, /ln -sfn "\$WEB_SOURCE_DIR\/scripts"/);
  assert.match(runner, /AEROBAG_WEB_WORKSPACE_DIR="\$WORKSPACE_DIR"/);

  const qualifier = readFileSync(
    join(repoRoot, "tools/ci/local_candidate_qualification.py"),
    "utf8",
  );
  assert.doesNotMatch(qualifier, /npm[^\n]*ci[^\n]*--prefix[^\n]*ui\/web-app/);
});

function withActionContract(runtime) {
  runtime.openPage ??= (pageId) => runtime.driver.openPage(pageId);
  runtime.reset ??= () => runtime.driver.reset();
  runtime.resetApplicationData ??= () => runtime.driver.resetApplicationData();
  runtime.resetApplicationDataExpectingStartupFailure ??= () =>
    runtime.driver.resetApplicationDataExpectingStartupFailure();
  runtime.reload ??= () => runtime.driver.reload();
  runtime.revealElement ??= (elementId) => runtime.driver.revealElement(elementId);
  runtime.revealProjectionMatching ??= (probe, needle) =>
    runtime.driver.revealProjectionMatching(probe, needle);
  runtime.editText ??= (_description, controlId, value, options = {}) =>
    runtime.driver.enterText(controlId, value, options);
  runtime.transition = async (description, contract) => {
    const readyElement = await runtime.eventually(`${description} ready`, contract.ready);
    await contract.act(readyElement);
    return runtime.eventually(`${description} complete`, contract.complete);
  };
  runtime.action = async (description, actionId, contract) => {
    return runtime.transition(description, {
      ...contract,
      ready: contract.ready ?? (() => runtime.driver.readAction
        ? runtime.driver.readAction(actionId)
        : runtime.driver.readElement(actionId)),
      act: (readyElement) => runtime.driver.performAction(actionId, readyElement),
    });
  };
  return runtime;
}

test("release journey registry owns every assertion exactly once", () => {
  const index = validateJourneyRegistry();
  assert.equal(index.journey_ids.size, RELEASE_JOURNEYS.length);
  assert.ok(index.assertion_owners.size > 100);
});

test("Android fixture clock setup observes the device clock before returning", async () => {
  const targetEpochMs = 1_787_905_620_000;
  const commands = [];
  let clockReads = 0;
  let waitObserved = false;
  await setAndroidWallClockAndWait("emulator-test", targetEpochMs, {
    now: () => 10_000,
    adbCommand: (_serial, args) => {
      commands.push(args);
      if (args.at(-1) === "+%s%3N") {
        clockReads += 1;
        return clockReads === 1 ? "1\n" : `${targetEpochMs}\n`;
      }
      return "";
    },
    wait: async (probe) => {
      assert.equal(await probe(), false);
      assert.equal(await probe(), true);
      waitObserved = true;
    },
  });
  assert.equal(waitObserved, true);
  assert.deepEqual(commands[0], ["shell", "cmd", "alarm", "set-time", String(targetEpochMs)]);
  assert.equal(clockReads, 2);
});

test("Android emulator readiness waits for the final display cutout", () => {
  const launcher = readFileSync(
    new URL("../../ui/android-app/scripts/start_emulator_stack.sh", import.meta.url),
    "utf8",
  );
  assert.match(launcher, /waiting for final Android display configuration/);
  assert.match(launcher, /mAppBounds=Rect\\\(0, \[1-9\]/);
  assert.match(launcher, /emulator_ready_deadline=\$\(\(SECONDS \+ EMULATOR_READY_TIMEOUT\)\)/);
  assert.doesNotMatch(launcher, /DISPLAY_CONFIGURATION_READY_TIMEOUT|display_configuration_deadline/);
  assert.doesNotMatch(launcher, /wait-for-broadcast-idle/);
  assert.ok(
    launcher.indexOf("waiting for final Android display configuration") <
      launcher.indexOf('echo "PACKAGE_SOURCE_REVERSE='),
  );
});

test("CDP transport rejects late work without writing after pipe shutdown", async () => {
  const pipeWrite = new PassThrough();
  const pipeRead = new PassThrough();
  pipeWrite.resume();
  const client = new CdpClient({ pipeWrite, pipeRead });
  await client.open();
  const pending = client.send("Runtime.enable");
  client.close();
  await assert.rejects(pending, /connection closed/);
  await assert.rejects(client.send("Runtime.enable"), /request Runtime\.enable rejected/);
  pipeRead.write(`${JSON.stringify({ method: "Runtime.event" })}\0`);
  assert.equal(pipeWrite.writableEnded, true);
});

test("web release journeys observe workers without attaching a debugger during startup", () => {
  const runner = readFileSync(new URL("./run-release-journey.mjs", import.meta.url), "utf8");
  assert.doesNotMatch(runner, /page\.enableChildTargetDiagnostics\(\)/);
  assert.match(runner, /this\.addEventListener\("error"/);
  assert.match(runner, /this\.addEventListener\("messageerror"/);
});

test("grouped P2 journeys leave destructive contract failure last", () => {
  const p2 = RELEASE_JOURNEYS.filter((journey) => journey.priority === "p2");
  assert.equal(p2.at(-1)?.id, "shared.contract-failures");
});

test("Android cloud crossfill executes alone in its release shard", () => {
  const script = new URL("./release_journey_lab.sh", import.meta.url);
  const shardZero = spawnSync("bash", [script.pathname, "android-shard-list", "p1", "0", "4"], {
    cwd: new URL("../..", import.meta.url),
    encoding: "utf8",
    env: labMetadataEnvironment(),
  });
  assert.equal(shardZero.status, 0, shardZero.stderr);
  assert.equal(shardZero.stdout.trim(), "shared.cloud-crossfill");

  for (const shard of [1, 2, 3]) {
    const result = spawnSync(
      "bash",
      [script.pathname, "android-shard-list", "p1", String(shard), "4"],
      {
        cwd: new URL("../..", import.meta.url),
        encoding: "utf8",
        env: labMetadataEnvironment(),
      },
    );
    assert.equal(result.status, 0, result.stderr);
    assert.doesNotMatch(result.stdout, /shared\.cloud-crossfill/);
  }
});

test("cloud Sync Now establishes quiescence after revealing its control", () => {
  const source = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  const journey = source.slice(
    source.indexOf("async function cloudCrossfill"),
    source.indexOf("export const RELEASE_JOURNEY_IMPLEMENTATIONS"),
  );
  const reveal = journey.indexOf('revealCloudAction(runtime, "sync_now"');
  const quiescence = journey.indexOf('runtime.stable(\n    "settled cloud state before manual synchronization"');
  const action = journey.indexOf('runtime.action("synchronize cloud state now"');
  assert.ok(reveal >= 0, "cloud journey must reveal Sync Now");
  assert.ok(quiescence > reveal, "cloud quiescence must follow UI preparation");
  assert.ok(action > quiescence, "Sync Now must follow the quiescence barrier");
  assert.match(journey, /runtime\.driver\.readCloudActionRevision\(\)/);
});

test("web semantic drags remain inside their target surface", () => {
  assert.deepEqual(
    clampDragEndpoint(
      { x: 720, y: 720 },
      { x: 360, y: 260 },
      { x: 20, y: 20 },
      { x: 980, y: 980 },
    ),
    { x: 980, y: 980 },
  );
  assert.deepEqual(
    clampDragEndpoint(
      { x: 280, y: 280 },
      { x: -360, y: -360 },
      { x: 20, y: 20 },
      { x: 980, y: 980 },
    ),
    { x: 20, y: 20 },
  );
});

test("replay seek gestures move toward the open side of the timeline", () => {
  assert.equal(timelineSeekDeltaX(1, 5), 320);
  assert.equal(timelineSeekDeltaX(4.4, 5), -320);
});

test("status popup dismissal waits until the popup can receive Back", async () => {
  const events = [];
  let panelOpen = false;
  const runtime = withActionContract({
    driver: {
      async readElement(id) {
        events.push(`read:${id}:${panelOpen}`);
        if (id === "data-status-launcher") return { text: "1" };
        if (id === "data-status-panel") return panelOpen ? { test_id: id } : null;
        return null;
      },
      async performAction(id) {
        events.push(`action:${id}`);
        panelOpen = true;
      },
      async back() {
        events.push(`back:${panelOpen}`);
        assert.equal(panelOpen, true);
        panelOpen = false;
      },
    },
    async eventually(label, probe) {
      events.push(`eventually:${label}`);
      const value = await probe();
      assert.ok(value, `${label} did not satisfy its postcondition`);
      return value;
    },
  });

  assert.deepEqual(await openAndDismissDataStatus(runtime), { text: "1" });
  assert.deepEqual(events, [
    "read:data-status-launcher:false",
    "eventually:open data status popup ready",
    "read:data-status-launcher:false",
    "action:data-status-launcher",
    "eventually:open data status popup complete",
    "read:data-status-panel:true",
    "eventually:dismiss data status popup ready",
    "read:data-status-panel:true",
    "back:true",
    "eventually:dismiss data status popup complete",
    "read:data-status-panel:false",
  ]);
});

test("chart search selection completes only after its inspector is rendered", () => {
  const journeys = readFileSync(new URL("./release-journey-implementations.mjs", import.meta.url), "utf8");
  const method = journeys.slice(
    journeys.indexOf("export async function selectChartSearchSuggestion"),
    journeys.indexOf("async function revealRequiredElement"),
  );
  assert.match(method, /readProjection\(selectedProjection\)/);
  assert.match(method, /readElement\("map-selection-tray"\)/);
  assert.match(method, /complete: completedSelection/);
});

test("Android chart search relinquishes the IME before opening an inspector", () => {
  const charts = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/ChartsPage.kt", import.meta.url),
    "utf8",
  );
  const search = charts.slice(charts.indexOf("internal fun AndroidChartSearchBox("));
  assert.match(
    search,
    /\.clickable \{\s*focusManager\.clearFocus\(force = true\)\s*keyboardController\?\.hide\(\)\s*onSuggestionClick/,
  );

  const native = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const dismiss = native.slice(
    native.indexOf("async function dismissMapSelection("),
    native.indexOf("async function inspectRawTerrainSpot("),
  );
  assert.match(dismiss, /androidImeShown\(serial\)/);
  assert.match(dismiss, /readElement\("map-selection-tray"\)/);
});

test("TFR selection observes the target overlay before taking its final inspector snapshot", async () => {
  const events = [];
  let selections = 0;
  let overlayProbes = 0;
  let trayOpen = false;
  const runtime = withActionContract({
    platform: "web",
    driver: {
      async openPage(id) { events.push(`page:${id}`); },
      async enterText(id, value) { events.push(`text:${id}:${value}`); },
      async readElement(id) {
        events.push(`read:${id}`);
        if (id === "map-selection-tray") return trayOpen ? { id } : null;
        return id === "chart-search-suggestion-27W" ? { enabled: true } : null;
      },
      async performAction(id) {
        events.push(`action:${id}`);
        selections += 1;
        trayOpen = true;
      },
      async readProjection(probe) {
        events.push(`projection:${probe}`);
        if (probe === "parity:map-selection-selected:27W") {
          return selections > 0 ? [{ id: `${probe}:ready` }] : [];
        }
        if (probe === "parity:live-overlay:") {
          overlayProbes += 1;
          return [{ id: `parity:live-overlay:metars:0:pireps:0:obstacles:0:tfrs:${overlayProbes > 1 ? 1 : 0}` }];
        }
        return [];
      },
      async back() {
        events.push("back");
        trayOpen = false;
      },
    },
    async eventually(label, probe) {
      events.push(`eventually:${label}`);
      for (let attempt = 0; attempt < 3; attempt += 1) {
        const value = await probe();
        if (value) return value;
      }
      throw new Error(`${label} did not satisfy its postcondition`);
    },
  });

  const result = await selectTfrFromPreparedMap(runtime, "27W");
  assert.equal(result.overlay.tfrs, 1);
  assert.equal(selections, 2);
  assert.equal(events.filter((event) => event === "back").length, 1);
  assert.ok(
    events.indexOf("projection:parity:live-overlay:") <
      events.lastIndexOf("action:chart-search-suggestion-27W"),
    events.join("\n"),
  );
});

test("a transition performs its user action exactly once while observing delayed completion", async () => {
  let actions = 0;
  let completionProbes = 0;
  const readyEvidence = { test_id: "button", enabled: true };
  let actionEvidence = null;
  const result = await performTransition("delayed state", {
    ready: async () => readyEvidence,
    act: async (evidence) => { actions += 1; actionEvidence = evidence; },
    complete: async () => {
      completionProbes += 1;
      return completionProbes >= 3 ? { committed: true } : null;
    },
    readyTimeoutMs: 100,
    responseTimeoutMs: 100,
    intervalMs: 1,
  });
  assert.equal(actions, 1);
  assert.equal(actionEvidence, readyEvidence);
  assert.equal(completionProbes, 4);
  assert.deepEqual(result.value, { committed: true });
});

test("a transition waits for semantic control geometry to settle before acting", async () => {
  const readiness = [
    { test_id: "button", enabled: true, actionable: true, bounds: { left: 10, top: 10, width: 80, height: 40 } },
    { test_id: "button", enabled: true, actionable: true, bounds: { left: 10, top: 30, width: 80, height: 40 } },
    { test_id: "button", enabled: true, actionable: true, bounds: { left: 10, top: 30, width: 80, height: 40 } },
  ];
  let readinessIndex = 0;
  let actedWith = null;
  const result = await performTransition("settling button", {
    ready: async () => readiness[Math.min(readinessIndex++, readiness.length - 1)],
    act: async (evidence) => { actedWith = evidence; },
    complete: async () => actedWith ? { committed: true } : null,
    readyTimeoutMs: 100,
    responseTimeoutMs: 100,
    intervalMs: 1,
  });
  assert.equal(readinessIndex, 3);
  assert.deepEqual(actedWith.bounds, readiness[2].bounds);
  assert.deepEqual(result.value, { committed: true });
});

test("a transition may act after one readiness sample when delivery revalidates the target", async () => {
  let readinessProbes = 0;
  let actedWith = null;
  const readyEvidence = {
    test_id: "button",
    enabled: true,
    actionable: true,
    bounds: { left: 10, top: 30, width: 80, height: 40 },
  };
  const result = await performTransition("revalidated semantic button", {
    readinessSamples: 1,
    ready: async () => {
      readinessProbes += 1;
      return readyEvidence;
    },
    act: async (evidence) => { actedWith = evidence; },
    complete: async () => actedWith ? { committed: true } : null,
    readyTimeoutMs: 100,
    responseTimeoutMs: 100,
    intervalMs: 1,
  });
  assert.equal(readinessProbes, 1);
  assert.equal(actedWith, readyEvidence);
  assert.deepEqual(result.value, { committed: true });
});

test("a transition rejects a postcondition that was already true before its action", async () => {
  let actions = 0;
  await assert.rejects(
    () => performTransition("already complete", {
      ready: async () => true,
      act: async () => { actions += 1; },
      complete: async () => ({ stale: true }),
      intervalMs: 1,
    }),
    /completion was already satisfied before the action/,
  );
  assert.equal(actions, 0);
});

test("an ensure transition accepts state that converges before its action", async () => {
  let actions = 0;
  const result = await performTransition("ensure focused", {
    ready: async () => ({ test_id: "editor", focused: false }),
    act: async () => { actions += 1; },
    complete: async () => ({ test_id: "editor", focused: true }),
    acceptPreexistingCompletion: true,
    intervalMs: 1,
  });
  assert.equal(actions, 0);
  assert.equal(result.value.focused, true);
  assert.deepEqual(result.timing.action_result, { skipped: "already-complete" });
});

test("only text-focus convergence may accept an already-complete transition", () => {
  const journeys = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  const driver = readFileSync(
    new URL("./semantic-journey-driver.mjs", import.meta.url),
    "utf8",
  );
  const editText = driver.slice(
    driver.indexOf("export async function editSemanticText"),
    driver.indexOf("export async function inspectSemanticMapAt"),
  );
  assert.doesNotMatch(journeys, /acceptPreexistingCompletion/);
  assert.equal((driver.match(/acceptPreexistingCompletion/g) ?? []).length, 1);
  assert.match(editText, /acceptPreexistingCompletion: true/);
});

test("a user transition cannot borrow a long-running operation budget", async () => {
  await assert.rejects(
    performTransition("slow button", {
      ready: async () => true,
      act: async () => {},
      complete: async () => true,
      responseTimeoutMs: E2E_TIMING.localResourceMs,
    }),
    /user transitions are capped.*separate named phase/s,
  );
});

test("action delivery and completion share one user-response budget", async () => {
  await assert.rejects(
    performTransition("slow action delivery", {
      ready: async () => true,
      act: async () => new Promise((resolve) => setTimeout(resolve, 8)),
      complete: async () => false,
      readyTimeoutMs: 20,
      responseTimeoutMs: 10,
      intervalMs: 1,
    }),
    /timed out|exceeded the 10ms user-response budget/,
  );
});

test("a transition does not accept a one-sample completion glitch", async () => {
  let probes = 0;
  const result = await performTransition("glitchy completion", {
    ready: async () => true,
    act: async () => {},
    complete: async () => {
      probes += 1;
      return probes === 2 || probes >= 4 ? { committed: true } : null;
    },
    readyTimeoutMs: 100,
    responseTimeoutMs: 100,
    intervalMs: 1,
  });
  assert.equal(probes, 5);
  assert.deepEqual(result.value, { committed: true });
});

test("an exact platform postcondition may complete after one observation", async () => {
  let completed = false;
  const result = await performTransition("exact completion", {
    ready: async () => true,
    act: async () => { completed = true; },
    complete: async () => completed,
    completionSamples: 1,
  });
  assert.equal(result.value, true);
});

test("a transition can wait on platform events instead of hot polling", async () => {
  let probes = 0;
  let waits = 0;
  const result = await performTransition("event-driven completion", {
    ready: async () => true,
    act: async () => {},
    complete: async () => (++probes >= 3 ? { rendered: true } : null),
    waitForObservation: async () => { waits += 1; },
    readyTimeoutMs: 100,
    responseTimeoutMs: 100,
    intervalMs: 1,
  });
  assert.deepEqual(result.value, { rendered: true });
  assert.equal(waits, 3);
});

test("scroll-like observations wait for a changed projection to settle", async () => {
  const samples = ["before", "moving-1", "moving-2", "settled", "settled", "settled"];
  let index = 0;
  const result = await observeChangedValueUntilStable(
    "animated projection",
    async () => samples[index++],
    {
      initialValue: "before",
      intervalMs: 0,
      timeoutMs: 100,
      stableSamples: 3,
    },
  );
  assert.equal(result.value, "settled");
  assert.equal(index, samples.length);
});

test("baseline observations reject moving values until they settle", async () => {
  const samples = ["moving-1", "moving-2", "settled", "settled", "settled"];
  let index = 0;
  const result = await observeValueUntilStable(
    "viewport baseline",
    async () => samples[index++],
    { timeoutMs: 100, intervalMs: 0, stableSamples: 3 },
  );
  assert.equal(result.value, "settled");
  assert.equal(index, samples.length);
});

test("a blocked probe cannot report success after its observation budget", async () => {
  let error = null;
  try {
    await observeUntil("blocked probe", async () => {
      await new Promise((resolve) => setTimeout(resolve, 20));
      return true;
    }, { timeoutMs: 5, intervalMs: 1 });
  } catch (caught) {
    error = caught;
  }
  assert.match(error?.message ?? "", /blocked probe timed out/);
  assert.equal(error.name, "ObservationTimeoutError");
  assert.equal(error.diagnostics.attempts, 1);
  assert.equal(error.diagnostics.last_value, true);
});

test("terminal observation failures abort without consuming the readiness budget", async () => {
  const started = performance.now();
  await assert.rejects(
    observeUntil("startup", async () => {
      throw new TerminalObservationError("startup failed", "adapter import failed");
    }, { timeoutMs: 10_000 }),
    /startup failed: adapter import failed/,
  );
  assert.ok(performance.now() - started < 100);
});

test("unexpected observation errors are terminal instead of becoming slow timeouts", async () => {
  await assert.rejects(
    observeUntil("driver state", async () => {
      throw new Error("semantic driver disconnected");
    }, { timeoutMs: 10_000 }),
    /driver state: semantic driver disconnected/,
  );
});

test("only explicitly transient observation errors may be retried", async () => {
  let attempts = 0;
  const result = await observeUntil("expected reconnect", async () => {
    attempts += 1;
    if (attempts < 3) throw new TransientObservationError("connection pending");
    return "connected";
  }, { timeoutMs: 100, intervalMs: 0 });
  assert.equal(result.value, "connected");
  assert.equal(attempts, 3);
});

test("a failed transition records its phase and final diagnostic state", async () => {
  const timings = [];
  let state = "before";
  await assert.rejects(
    performTransition("missing render", {
      ready: async () => true,
      act: async () => { state = "model-updated"; return { accepted: true }; },
      complete: async () => null,
      diagnose: async () => ({ state }),
      readyTimeoutMs: 30,
      responseTimeoutMs: 20,
      intervalMs: 1,
      onTiming: (timing) => timings.push(timing),
    }),
    /missing render completed timed out/,
  );
  assert.equal(timings.length, 1);
  assert.equal(timings[0].outcome, "fail");
  assert.equal(timings[0].failure_phase, "completion");
  assert.equal(timings[0].ready_state, true);
  assert.deepEqual(timings[0].action_result, { accepted: true });
  assert.deepEqual(timings[0].diagnostic_state, { state: "model-updated" });
  assert.ok(timings[0].observation.attempts > 0);
});

test("temporal behavior is sampled instead of hidden behind a sleep", async () => {
  let samples = 0;
  const result = await assertConditionRemains(
    "stable state",
    async () => ++samples,
    (sample) => sample > 0,
    { durationMs: 8, intervalMs: 1 },
  );
  assert.ok(result.samples >= 2);
});

test("shared journeys contain no mutations inside observation loops or fixed UI sleeps", () => {
  const path = new URL("./release-journey-implementations.mjs", import.meta.url);
  const violations = auditJourneyStructure(readFileSync(path, "utf8"), path.pathname);
  assert.deepEqual(violations, []);
});

test("standalone Android journeys keep host and baked device package ports distinct", () => {
  const source = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  assert.match(source, /packageSourceDevicePort: process\.env\.AEROBAG_ANDROID_PACKAGE_SOURCE_DEVICE_PORT/);
  assert.match(
    source,
    /`tcp:\$\{packageSourceDevicePort\}`,[\s\S]*?`tcp:\$\{packageSourcePort\}`/,
  );
  assert.doesNotMatch(
    source,
    /\["reverse", `tcp:\$\{packageSourcePort\}`, `tcp:\$\{packageSourcePort\}`\]/,
  );
});

test("Android behavior reset preserves bootstrap disclaimer agreement", () => {
  const source = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const sharedJourney = source.slice(
    source.indexOf("async function runSharedReleaseJourney"),
    source.indexOf("async function runOfflineColdStart"),
  );
  assert.match(
    sharedJourney,
    /resetApp:[\s\S]*?clearUiPrefs: true, clearCoreSettings: false/,
  );
  assert.match(
    sharedJourney,
    /resetApplicationData:[\s\S]*?pm", "clear"/,
  );
});

test("cloud peer follows the deterministic journey structure", () => {
  const source = readFileSync(new URL("./cloud-journey-peer.mjs", import.meta.url), "utf8");
  assert.deepEqual(auditJourneyStructure(source, "cloud-journey-peer.mjs"), []);
  assert.match(source, /cloud\.awaitProviderIdle\(\)/);
  assert.doesNotMatch(source, /pumpCloudProvider/);
});

test("Android pairing keeps its provider descriptor while the browser peer routes loopback", () => {
  assert.equal(
    rewriteRequestOrigin(
      "http://127.0.0.1:18094/cloud/v1/events?cursor=4",
      "http://127.0.0.1:18094",
      "http://127.0.0.1:18134",
    ),
    "http://127.0.0.1:18134/cloud/v1/events?cursor=4",
  );
  assert.equal(
    rewriteRequestOrigin(
      "https://example.test/cloud/v1/events",
      "http://127.0.0.1:18094",
      "http://127.0.0.1:18134",
    ),
    "https://example.test/cloud/v1/events",
  );
});

test("native Android journeys contain no mutations inside observation loops or fixed UI sleeps", () => {
  const path = new URL("./run-android-e2e-suite.mjs", import.meta.url);
  const violations = auditJourneyStructure(readFileSync(path, "utf8"), path.pathname);
  assert.deepEqual(violations, []);
});

test("native airport search transitions use indexed semantics instead of full hierarchy dumps", () => {
  const source = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  for (const [name, nextName] of [
    ["centerChartOnDestination", "inspectAirportFromChartSearch"],
    ["inspectAirportFromChartSearch", "dismissMapSelection"],
  ]) {
    const body = source.slice(
      source.indexOf(`async function ${name}`),
      source.indexOf(`async function ${nextName}`),
    );
    assert.doesNotMatch(body, /dumpAndroid\(/, `${name} must not walk the full accessibility tree`);
    assert.match(body, /readProjection\("parity:map-selection-state:"\)/);
  }
});

test("native plate opening does not click an already-selected exact airport twice", () => {
  const source = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const body = source.slice(
    source.indexOf("async function openPlateFromAirportInspector"),
    source.indexOf("async function ensureMapFollowEngaged"),
  );
  assert.doesNotMatch(body, /selected in inspector|map-selection-item:airport/);
  assert.match(body, /plate folder opened for \$\{airportId\}/);
});

test("native map selection dismissal observes the fixed projection", () => {
  const native = readFileSync("tools/e2e/run-android-e2e-suite.mjs", "utf8");
  const body = native.slice(
    native.indexOf("async function dismissMapSelection"),
    native.indexOf("async function inspectRawTerrainSpot"),
  );
  assert.match(body, /readProjection\("parity:map-selection-state:"\)/);
  assert.match(body, /readElement\("map-selection-tray"\)/);
  assert.match(body, /androidImeShown\(serial\)/);
  assert.doesNotMatch(body, /dumpAndroid\(/);
});

test("Android map-selection state uses one fixed bounded projection", () => {
  const map = readFileSync(new URL(
    "../../ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt",
    import.meta.url,
  ), "utf8");
  const ids = readFileSync(new URL(
    "../../ui/android-app/app/src/main/res/values/e2e_projection_ids.xml",
    import.meta.url,
  ), "utf8");
  const driver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  assert.match(map, /viewId = R\.id\.e2e_map_selection_projection/);
  assert.equal(
    [...map.matchAll(/viewId = R\.id\.e2e_map_selection_projection/g)].length,
    2,
    "the map and its separate popup accessibility window must expose the same projection",
  );
  assert.match(ids, /name="e2e_map_selection_projection"/);
  assert.match(
    driver,
    /\["parity:map-selection-state:", "org\.aerobag\.app:id\/e2e_map_selection_projection"\]/,
  );
  assert.match(map, /detail:\$\{detailId\?\.let\(::rasterSemanticToken\) \?: "none"\}/);
  assert.match(map, /mapSelectionDetailProjectionId\(mapSelection\?\.detailModal\)/);
  assert.match(
    driver,
    /async readModal\(modalId\)[\s\S]*readScalarProjection\("parity:map-selection-state:"\)/,
  );
});

test("inspector SPOT gestures ignore moving-ownship map rotation", () => {
  const source = readFileSync(new URL("./release-journey-implementations.mjs", import.meta.url), "utf8");
  const journey = source.slice(
    source.indexOf("async function inspectorDetails"),
    source.indexOf("async function flightPlanEditing"),
  );
  const spotPhase = journey.slice(
    journey.indexOf('await runtime.openPage("map");', journey.indexOf("inspector.plates")),
    journey.indexOf('runtime.check("inspector.spot-fallback"'),
  );
  assert.match(spotPhase, /viewportGeometryId\(await runtime\.driver\.readProjection\("parity:viewport:"\)\)/);
  assert.doesNotMatch(spotPhase, /selectStationaryPlanPreview/);
});

test("map selection actions wait through asynchronous inspector materialization", async () => {
  const { waitForMapSelectionAction } = await import("./release-journey-implementations.mjs");
  const reads = [];
  let attempts = 0;
  const runtime = {
    platform: "android",
    driver: {
      async readElement(id) {
        reads.push(id);
        attempts += 1;
        return attempts < 3 ? null : { test_id: id, enabled: true };
      },
    },
    async eventually(description, observe) {
      assert.equal(description, "inspector weather action");
      for (let attempt = 0; attempt < 3; attempt += 1) {
        const result = await observe();
        if (result) return result;
      }
      throw new Error("action did not materialize");
    },
  };

  const action = await waitForMapSelectionAction(runtime, "wx", "inspector weather action");
  assert.equal(action.enabled, true);
  assert.deepEqual(reads, [
    "map-selection-action:wx",
    "map-selection-action:wx",
    "map-selection-action:wx",
  ]);
});

test("tray selection avoids a redundant click when the target materializes selected", async () => {
  const { selectTrayOptionMatching } = await import("./release-journey-implementations.mjs");
  const calls = [];
  let optionsOpen = false;
  const option = { id: "parity:tray-option:KPAE", text: "KPAE", enabled: true };
  const runtime = {
    platform: "android",
    driver: {
      async readElement(id) {
        assert.equal(id, "plate-airport-button");
        return { text: optionsOpen ? "KPAE" : "SELECT AIRPORT" };
      },
      async readProjection(prefix) {
        assert.equal(prefix, "parity:tray-option:");
        return optionsOpen ? [option] : [];
      },
      async back() {
        calls.push("dismiss");
        optionsOpen = false;
      },
    },
    async action(description, id, { complete }) {
      calls.push(description);
      assert.equal(id, "plate-airport-button");
      optionsOpen = true;
      return complete();
    },
    async revealProjectionMatching(prefix, needle) {
      assert.equal(prefix, "parity:tray-option:");
      assert.equal(needle, "KPAE");
      return option;
    },
    async transition(description, { ready, act, complete }) {
      calls.push(description);
      assert.ok(await ready());
      await act();
      assert.equal(await complete(), true);
    },
  };

  const selected = await selectTrayOptionMatching(runtime, "plate-airport-button", "KPAE");
  assert.equal(selected.text, "KPAE");
  assert.deepEqual(calls, [
    "open plate-airport-button options",
    "dismiss already-selected plate-airport-button options",
    "dismiss",
  ]);
});

test("viewport geometry identity excludes orientation but preserves pan changes", async () => {
  const {
    viewportGeometryId,
    viewportZoomLevel,
  } = await import("./release-journey-implementations.mjs");
  assert.equal(
    viewportGeometryId([{ id: "parity:viewport:center-x-milli:41100:center-y-milli:89100:zoom:11392:up:27" }]),
    "parity:viewport:center-x-milli:41100:center-y-milli:89100:zoom:11392",
  );
  assert.notEqual(
    viewportGeometryId([{ id: "parity:viewport:center-x-milli:41100:center-y-milli:89100:zoom:11392:up:27" }]),
    viewportGeometryId([{ id: "parity:viewport:center-x-milli:41350:center-y-milli:89100:zoom:11392:up:27" }]),
  );
  assert.equal(
    viewportZoomLevel([{ id: "parity:viewport:center-x-milli:41154:center-y-milli:89484:zoom:11392:up:0" }]),
    viewportZoomLevel([{ id: "parity:viewport:center-x-milli:41157:center-y-milli:89482:zoom:11392:up:0" }]),
    "residual pan settling must not satisfy a zoom transition",
  );
  assert.notEqual(
    viewportZoomLevel([{ id: "parity:viewport:center-x-milli:41157:center-y-milli:89482:zoom:11392:up:0" }]),
    viewportZoomLevel([{ id: "parity:viewport:center-x-milli:41157:center-y-milli:89482:zoom:12092:up:0" }]),
  );
});

test("native Android journeys cannot bypass transition contracts with raw adb input", () => {
  const source = `
    async function broken(serial) {
      adb(serial, ["shell", "input", "tap", "10", "20"]);
    }
  `;
  assert.deepEqual(
    auditJourneyStructure(source, "run-android-e2e-suite.mjs")
      .map((violation) => violation.message),
    ["raw adb input must be the act phase of a semantic transition"],
  );
});

test("semantic driver read APIs cannot traverse or mutate the rendered UI", () => {
  const path = new URL("./semantic-journey-driver.mjs", import.meta.url);
  const violations = auditJourneyStructure(readFileSync(path, "utf8"), path.pathname);
  assert.deepEqual(violations, []);
});

test("Android Chrome journey uses the shared named timing policy", () => {
  const path = new URL("./run-android-chrome-livefeed-e2e.mjs", import.meta.url);
  const violations = auditJourneyStructure(readFileSync(path, "utf8"), path.pathname);
  assert.deepEqual(violations, []);
});

test("Android Chrome launch recovers when a clean emulator kills its first browser process", async () => {
  const attempts = [];
  const failures = [];
  const result = await establishChromeRuntime({
    launchAttempt: async (attempt) => {
      attempts.push(attempt);
      if (attempt === 1) {
        throw new TerminalObservationError(
          "Android Chrome launch",
          "browser process exited before its DevTools socket became ready",
        );
      }
      return "ready";
    },
    onAttemptFailure: async (error, attempt) => failures.push([attempt, error.message]),
  });

  assert.equal(result, "ready");
  assert.deepEqual(attempts, [1, 2]);
  assert.deepEqual(failures, [[
    1,
    "Android Chrome launch: browser process exited before its DevTools socket became ready",
  ]]);
});

test("Android Chrome launch fails after the bounded retry budget", async () => {
  const attempts = [];
  await assert.rejects(
    establishChromeRuntime({
      attempts: 2,
      launchAttempt: async (attempt) => {
        attempts.push(attempt);
        throw new Error(`process loss ${attempt}`);
      },
    }),
    /failed to become ready after 2 attempts \(attempt 1: process loss 1; attempt 2: process loss 2\)/,
  );
  assert.deepEqual(attempts, [1, 2]);
});

test("NAVDB rollover journey follows the deterministic journey structure", () => {
  const path = new URL("../../ui/web-app/scripts/nav-db-rollover-e2e.mjs", import.meta.url);
  const violations = auditJourneyStructure(readFileSync(path, "utf8"), path.pathname);
  assert.deepEqual(violations, []);
});

test("every qualification journey entrypoint passes the structural audit", () => {
  const repoRoot = fileURLToPath(new URL("../..", import.meta.url));
  assert.deepEqual(auditQualificationJourneys(repoRoot), []);
});

test("journey structure audit follows helper calls into observation loops", () => {
  const source = `
    async function clickThing(runtime) {
      await runtime.driver.performAction("thing");
    }
    async function broken(runtime) {
      await runtime.eventually("bad", async () => {
        await clickThing(runtime);
        return null;
      }, 5000);
      await delay(500);
    }
  `;
  const violations = auditJourneyStructure(source, "broken-journey.mjs");
  assert.deepEqual(
    violations.map((violation) => violation.message),
    [
      "performAction must be the single act phase of a semantic transition",
      "raw eventually deadline is forbidden; use a named E2E_TIMING class",
      "eventually callback invokes mutating operation clickThing",
      "fixed delay is forbidden in journey function broken",
    ],
  );
});

test("journey structure audit rejects text entry combined with submission", () => {
  const source = `
    async function broken(runtime) {
      await runtime.driver.enterText("route", "KSEA KPAE", { submit: true });
    }
  `;
  assert.deepEqual(
    auditJourneyStructure(source, "combined-entry.mjs").map((violation) => violation.message),
    [
      "enterText must be the act phase of a semantic transition",
      "enterText must not also submit; model editing and submission as separate user actions",
    ],
  );
});

test("journey structure audit rejects generic mutation steps", () => {
  const source = `
    async function broken(runtime) {
      await runtime.step("maybe click", () => runtime.driver.performAction("button"));
    }
  `;
  assert.deepEqual(
    auditJourneyStructure(source, "generic-step.mjs").map((violation) => violation.message),
    [
      "generic journey steps are forbidden; use a semantic transition or a typed runtime phase",
      "performAction must be the single act phase of a semantic transition",
    ],
  );
});

test("journey structure audit requires raw gestures to name their completion", () => {
  const source = `
    async function broken(runtime) {
      await runtime.driver.drag("map-surface", { x: 10, y: 20 });
    }
    async function sound(runtime) {
      await runtime.transition("pan", {
        ready: () => runtime.driver.readElement("map-surface"),
        act: () => runtime.driver.drag("map-surface", { x: 10, y: 20 }),
        complete: () => runtime.driver.readProjection("parity:viewport:"),
      });
    }
  `;
  assert.deepEqual(
    auditJourneyStructure(source, "gesture-journey.mjs").map((violation) => violation.message),
    ["drag must be the act phase of a semantic transition"],
  );
});

test("journey structure audit rejects Android action rediscovery", () => {
  const violations = auditJourneyStructure(`
    async function broken(result, serial) {
      await nativeTransition(result, "toggle", {
        ready: async () => readNode(serial),
        act: async () => tapTag(serial, "parity:toggle"),
        complete: async () => readState(serial),
      });
    }
  `);
  assert.ok(violations.some(({ message }) =>
    message.includes("must accept its observed readiness evidence")));
  assert.ok(violations.some(({ message }) =>
    message.includes("must not rediscover UI state through tapTag")));
});

test("journey structure audit requires action completion contracts", () => {
  const source = `
    async function broken(runtime) {
      await runtime.action("open", "button", {});
    }
  `;
  assert.deepEqual(
    auditJourneyStructure(source, "action-contract.mjs").map((violation) => violation.message),
    ["action must declare a semantic completion condition"],
  );
});

test("journey structure audit rejects custom semantic-action readiness", () => {
  const source = `
    async function broken(runtime) {
      await runtime.action("open row", "plan-row:123", {
        ready: () => runtime.driver.findProjectionMatching("row:", "KSEA"),
        complete: () => runtime.driver.readElement("row-tray"),
      });
    }
  `;
  assert.deepEqual(
    auditJourneyStructure(source, "custom-action-readiness.mjs")
      .map((violation) => violation.message),
    ["action readiness must come from driver.readAction(actionId)"],
  );
});

test("journey structure audit recognizes DOM actions hidden inside page evaluation", () => {
  const source = `
    async function broken(page) {
      await observeUntil("button", async () =>
        page.evalValue("document.querySelector('button')?.click(); true"));
    }
  `;
  const violations = auditJourneyStructure(source, "dom-mutation.mjs");
  assert.deepEqual(
    violations.map((violation) => violation.message),
    ["observeUntil callback invokes mutating operation evalValue"],
  );
});

test("journey structure audit rejects long-running budgets on user transitions", () => {
  const source = `
    async function broken(runtime) {
      await runtime.transition("slow button", {
        ready: async () => true,
        act: async () => runtime.driver.performAction("button"),
        complete: async () => true,
        responseTimeoutMs: E2E_TIMING.resourceMs,
      });
    }
  `;
  assert.deepEqual(
    auditJourneyStructure(source, "slow-transition.mjs").map((violation) => violation.message),
    ["user transition responseTimeoutMs must use E2E_TIMING.userResponseMs"],
  );
});

test("NAVDB rollover scenarios never replace a publication under a live Vite server", () => {
  const source = readFileSync(
    new URL("../../ui/web-app/scripts/nav-db-rollover-e2e.mjs", import.meta.url),
    "utf8",
  );
  const scenarioLoop = source.slice(
    source.indexOf("for (const scenario of scenarios)"),
    source.indexOf("const summary ="),
  );
  assert.match(
    scenarioLoop,
    /generatePublication\(scenario,[\s\S]*const vite = launchVite\(\)[\s\S]*await runScenario\(scenario\)[\s\S]*await stopProcess\(vite\)/,
  );
  const runScenario = source.slice(
    source.indexOf("async function runScenario"),
    source.indexOf("async function buildRichFlightPlan"),
  );
  assert.doesNotMatch(runScenario, /generatePublication\(/);
});

test("NAVDB rollover uses the shared pipe-based Chrome launcher", () => {
  const rollover = readFileSync(
    new URL("../../ui/web-app/scripts/nav-db-rollover-e2e.mjs", import.meta.url),
    "utf8",
  );
  assert.match(rollover, /import \{ CdpClient, launchChrome, stopProcess \} from "\.\/chrome-cdp\.mjs"/);
  assert.match(rollover, /const chrome = await launchChrome\(\{[\s\S]*?chromeBin,[\s\S]*?userDataDir,/);
  assert.match(rollover, /headless: !headed/);
  assert.match(rollover, /connectToBrowser\(chrome\.endpoint\)/);
  assert.match(rollover, /fs\.writeFileSync\(browserLogPath, chrome\.getStderr\(\), "utf8"\)/);
  assert.doesNotMatch(rollover, /remote-debugging-port|DevTools listening on/);
});

test("hosted CI pins and fans out immutable release inputs", () => {
  const lock = JSON.parse(readFileSync(
    new URL("../../test-artifacts.lock.json", import.meta.url),
    "utf8",
  ));
  const fixture = lock.fixtures["release-journey-publication"];
  assert.equal(fixture.contract_version, 1);
  assert.deepEqual(fixture.required_globs, [
    "published/**/packaged/*.zip",
    "live-feeds/fresh/current.json",
    "replay/*.json",
  ]);

  const workflow = readFileSync(
    new URL("../../.github/workflows/e2e-ci.yml", import.meta.url),
    "utf8",
  );
  for (const job of [
    "build-release-apps:",
    "release-journey-fixture:",
    "release-journey-web:",
    "release-journey-android-baseline:",
    "release-journey-android:",
    "qualification-result:",
  ]) {
    assert.match(workflow, new RegExp(`^  ${job}`, "m"));
  }
  assert.match(workflow, /schedule\) value='\["p0","p1","p2"\]'/);
  assert.match(workflow, /RELEASE_CANDIDATE.*value='\["p0","p1","p2"\]'/);
  assert.match(workflow, /REF_TYPE" == "tag".*value='\["p0","p1","p2"\]'/);
  assert.match(workflow, /tags:\n\s+- "20\*"/);
  assert.match(workflow, /Release qualification \{0\}/);
  assert.match(workflow, /Candidate qualification \{0\}/);
  assert.match(workflow, /AEROBAG_RELEASE_JOURNEY_REPETITIONS/);
  assert.match(workflow, /AEROBAG_RELEASE_JOURNEY_IMPLEMENTATIONS_ONLY: "1"/);
  assert.doesNotMatch(workflow, /ANDROID_SERIAL:\s*emulator-/);
  assert.equal(workflow.match(/Install browser harness dependencies/g)?.length, 4);
  for (const job of [
    "release-journey-web",
    "release-journey-android-baseline",
    "release-journey-android",
    "android-native",
  ]) {
    const start = workflow.indexOf(`  ${job}:`);
    const rest = workflow.slice(start + 2);
    const nextJob = rest.search(/^  [a-z0-9-]+:/m);
    const section = nextJob < 0 ? rest : rest.slice(0, nextJob);
    assert.match(section, /AEROBAG_WEB_WORKSPACE_DIR:/, `${job} names its web workspace`);
    assert.match(section, /Install browser harness dependencies/, `${job} installs web dependencies`);
  }

  const lab = readFileSync(
    new URL("./release_journey_lab.sh", import.meta.url),
    "utf8",
  );
  assert.match(
    lab,
    /local -a state_args=\(--clear-app-data --sync-all-available-packages\)/,
  );
  assert.match(lab, /android_baseline_restore "\$ANDROID_BASELINE_ARCHIVE"/);
  assert.match(lab, /AEROBAG_RELEASE_JOURNEY_REUSE_FIXTURE:-1/);
  assert.match(lab, /--data '\{"reset":true\}'/);
  assert.match(lab, /aerobag-release-journey-lab-\$\{PORT\}/);
  assert.match(lab, /current_web_dist_sha256.*requested_web_dist_sha256/);
  const androidRunner = lab.slice(lab.indexOf("run_android_test()"), lab.indexOf("run_web_test()"));
  const webRunner = lab.slice(lab.indexOf("run_web_test()"), lab.indexOf("run_repetitions()"));
  assert.doesNotMatch(androidRunner, /require_web_fixture_origin/);
  assert.match(webRunner, /require_web_fixture_origin/);
  assert.match(lab, /web journey fixture mismatch: managed=/);
  assert.match(
    lab,
    /ANDROID_PACKAGE_PORT="\$\{AEROBAG_ANDROID_PACKAGE_SOURCE_DEVICE_PORT:-\$PORT\}"/,
  );
  assert.match(
    lab,
    /ANDROID_CLOUD_PORT="\$\{AEROBAG_ANDROID_CLOUD_DEVICE_PORT:-\$CLOUD_PORT\}"/,
  );
  assert.match(
    lab,
    /reverse "tcp:\$\{ANDROID_PACKAGE_PORT\}" "tcp:\$\{PORT\}"/,
  );
  assert.match(
    lab,
    /reverse "tcp:\$\{ANDROID_CLOUD_PORT\}" "tcp:\$\{CLOUD_PORT\}"/,
  );
  const runAndroidTest = lab.slice(
    lab.indexOf("run_android_test()"),
    lab.indexOf("run_web_test()"),
  );
  assert.equal(runAndroidTest.match(/--release-fixture "\$FIXTURE"/g)?.length, 1);
  assert.match(lab, /run_e2e\.sh" \\\n\s+--skip-install \\\n\s+"\$\{state_args\[@\]\}" \\/);
  assert.match(lab, /--test "\$journey" <\/dev\/null/);
  assert.doesNotMatch(lab, /setTimeout\(resolve,\s*750\)/);
  assert.match(lab, /observeUntil\(`\$\{surface\} zoom`/);
});

test("fixture web identity changes with the exact built application", () => {
  const temp = mkdtempSync(join(tmpdir(), "aerobag-release-web-identity-"));
  try {
    writeFileSync(join(temp, "index.html"), "candidate one\n");
    const first = webDistIndexSha256(temp);
    writeFileSync(join(temp, "index.html"), "candidate two\n");
    const second = webDistIndexSha256(temp);

    assert.match(first, /^[0-9a-f]{64}$/);
    assert.match(second, /^[0-9a-f]{64}$/);
    assert.notEqual(first, second);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test("production-shaped web serving resolves document routes before SPA fallback", () => {
  assert.deepEqual(webDistRelativeCandidates("/"), ["index.html"]);
  assert.deepEqual(webDistRelativeCandidates("/about"), ["about", "about.html", "index.html"]);
  assert.deepEqual(webDistRelativeCandidates("/assets/app.js"), ["assets/app.js"]);
  const about = readFileSync(new URL("../../ui/web-app/about.html", import.meta.url), "utf8");
  assert.match(about, /data-testid="parity:page:about"/);
});

test("failure diagnostics summarize fixture traffic without discarding anomalies", () => {
  const requests = Array.from({ length: 120 }, (_, index) => ({
    url: `/packages/page-${index}`,
    status: 200,
    outcome: "finished",
  }));
  requests.push({ url: "/packages/broken", status: 503, outcome: "finished" });
  requests.push({ url: "/packages/closed", status: 200, outcome: "closed" });
  requests.push({ url: "/live-feeds/v3/events", status: 200, outcome: "active" });

  const summary = summarizeFixtureRequests(requests);

  assert.equal(summary.count, 123);
  assert.deepEqual(summary.outcomes, { finished: 121, closed: 1, active: 1 });
  assert.equal(summary.tail.length, 100);
  assert.deepEqual(summary.anomalies.map((entry) => entry.url), [
    "/packages/broken",
    "/packages/closed",
  ]);
});

test("web reset replaces the old app target before clearing persistent origin state", async () => {
  const calls = [];
  const oldPage = {};
  const replacementPage = {
    navigate: async (url) => calls.push(["navigate", url]),
    waitForLoad: async () => calls.push(["waitForLoad"]),
    send: async (method, args) => calls.push(["send", method, args]),
  };
  const transport = new WebSemanticTransport(oldPage, {
    url: "http://fixture.test/app",
    origin: "http://fixture.test",
    recreatePage: async (page) => {
      calls.push(["recreatePage", page]);
      return replacementPage;
    },
  });

  await transport.reset();

  assert.deepEqual(calls, [
    ["recreatePage", oldPage],
    ["send", "Storage.clearDataForOrigin", {
      origin: "http://fixture.test",
      storageTypes: "all",
    }],
    ["send", "Browser.grantPermissions", {
      origin: "http://fixture.test",
      permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
    }],
    ["navigate", "http://fixture.test/app"],
    ["waitForLoad"],
  ]);
});

test("web reset recovers one browser-canceled startup module without replaying an app action", async () => {
  let resets = 0;
  const transport = {
    async reset() { resets += 1; },
    async readElement() { return null; },
    async collectTestIds(prefix) {
      return resets === 2 && prefix === "parity:startup-state:"
        ? [{ id: "parity:startup-state:ready:true:page:Home" }]
        : [];
    },
    hasCanceledStartupModuleRequest() { return resets === 1; },
  };
  const driver = new WebSemanticJourneyDriver(transport);

  await driver.reset();

  assert.equal(resets, 2);
});

test("web reset fails after two browser-canceled startup modules", async () => {
  let resets = 0;
  const transport = {
    async reset() { resets += 1; },
    async readElement() { return null; },
    async collectTestIds() { return []; },
    hasCanceledStartupModuleRequest() { return true; },
  };
  const driver = new WebSemanticJourneyDriver(transport);

  await assert.rejects(
    driver.reset(),
    /browser canceled the startup module request twice/,
  );
  assert.equal(resets, 2);
});

test("web reset does not retry an application startup failure", async () => {
  let resets = 0;
  const transport = {
    async reset() { resets += 1; },
    async readElement(selector) {
      return selector.includes("startup-fatal-error")
        ? { visible: true, text: "generated WASM module is missing required exports" }
        : null;
    },
    async collectTestIds() { return []; },
    hasCanceledStartupModuleRequest() { return false; },
  };
  const driver = new WebSemanticJourneyDriver(transport);

  await assert.rejects(
    driver.reset(),
    /application startup failed: generated WASM module is missing required exports/,
  );
  assert.equal(resets, 1);
});

test("closing a web page for reset observes destruction of its dedicated workers", async () => {
  const requests = [];
  let targetReads = 0;
  const client = {
    onEvent() {},
    send: async (method, args) => {
      requests.push([method, args]);
      if (method === "Target.closeTarget") return { success: true };
      if (method !== "Target.getTargets") throw new Error(`unexpected ${method}`);
      targetReads += 1;
      return targetReads === 1 ? {
        targetInfos: [
          { targetId: "page-1", type: "page" },
          { targetId: "worker-1", type: "worker" },
        ],
      } : { targetInfos: [] };
    },
  };
  const page = new CdpPage(client, "session-1", "page-1");

  await page.closeForReset(E2E_TIMING.localReadyMs);

  assert.deepEqual(requests[0], ["Target.closeTarget", { targetId: "page-1" }]);
  assert.equal(targetReads, 2);
});

test("CDP navigation errors fail immediately instead of becoming UI readiness timeouts", async () => {
  const listeners = new Map();
  const client = {
    onEvent(_session, method, callback) { listeners.set(method, callback); },
    offEvent(_session, method, callback) {
      if (listeners.get(method) === callback) listeners.delete(method);
    },
    send: async (method) => {
      if (method === "Page.navigate") return { errorText: "net::ERR_CONNECTION_REFUSED" };
      throw new Error(`unexpected ${method}`);
    },
  };
  const page = new CdpPage(client, "session-1", "page-1");

  await assert.rejects(
    page.navigate("http://fixture.test/"),
    /Page\.navigate failed.*ERR_CONNECTION_REFUSED/,
  );
  assert.equal(listeners.has("Page.loadEventFired"), false);
  await assert.rejects(page.waitForLoad(), /without a successful navigation/);
});

test("CDP semantic activation can carry browser user-gesture authority", async () => {
  const requests = [];
  const client = {
    onEvent() {},
    send: async (method, args) => {
      requests.push([method, args]);
      return { result: { value: true } };
    },
  };
  const page = new CdpPage(client, "session-1", "page-1");

  assert.equal(await page.evaluate("true", { userGesture: true }), true);
  assert.equal(requests[0][0], "Runtime.evaluate");
  assert.equal(requests[0][1].userGesture, true);
});

test("web reload replaces its page target without clearing persisted state", async () => {
  const calls = [];
  const oldPage = {};
  const replacementPage = {
    navigate: async (url) => calls.push(["navigate", url]),
    waitForLoad: async () => calls.push(["waitForLoad"]),
    send: async (method, args) => calls.push(["send", method, args]),
  };
  const transport = new WebSemanticTransport(oldPage, {
    url: "http://fixture.test/app",
    origin: "http://fixture.test",
    recreatePage: async (page) => {
      calls.push(["recreatePage", page]);
      return replacementPage;
    },
  });

  await transport.reload();

  assert.deepEqual(calls, [
    ["recreatePage", oldPage],
    ["send", "Browser.grantPermissions", {
      origin: "http://fixture.test",
      permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
    }],
    ["navigate", "http://fixture.test/app"],
    ["waitForLoad"],
  ]);
});

test("web controls reacquire exposed semantic targets and activate atomically without a coordinate round trip", async () => {
  const evaluations = [];
  const page = {
    evaluate: async (expression, options) => {
      evaluations.push([expression, options]);
      return {
        status: "activated",
        probe: { click: 1, matched: 1, actionable_clicks: 1 },
      };
    },
    send: async () => assert.fail("semantic control activation must not use CDP pointer coordinates"),
  };
  const transport = new WebSemanticTransport(page, { url: "http://fixture.test/app" });

  assert.equal(await transport.clickIfVisible('[data-testid="button"]', {
    test_id: "button",
    bounds: { left: 0, top: 0, width: 100, height: 50 },
    action_point: { x: 50, y: 25 },
  }), true);
  assert.equal(evaluations.length, 1);
  assert.match(evaluations[0][0], /elementFromPoint/);
  assert.match(evaluations[0][0], /element\.dataset\.testid !== expected\.test_id/);
  assert.doesNotMatch(evaluations[0][0], /expected\.bounds/);
  assert.doesNotMatch(evaluations[0][0], /expected\.action_point/);
  assert.match(evaluations[0][0], /fractions = \[0\.5, 0\.1, 0\.9, 0\.3, 0\.7\]/);
  assert.match(evaluations[0][0], /element\.click\(\)/);
  assert.deepEqual(evaluations[0][1], { userGesture: true });
});

test("web text actions retain exact readiness evidence", () => {
  const driverSource = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const driver = driverSource.slice(
    driverSource.indexOf("export class WebSemanticJourneyDriver"),
    driverSource.indexOf("export class AndroidSemanticJourneyDriver"),
  );
  const transport = readFileSync(new URL("./web-semantic-transport.mjs", import.meta.url), "utf8");
  assert.doesNotMatch(driver, /_readyElement/);
  assert.match(driver, /transport\.enterText\(webTestIdSelector\(controlId\), value, readyElement\)/);
  assert.match(driver, /transport\.submit\(webTestIdSelector\(controlId\), readyElement\)/);
  assert.match(transport, /input\.dataset\.testid !== expected\.test_id/);
  assert.match(transport, /document\.activeElement === input/);
});

test("web text focus uses an editable-control contract instead of button activation", () => {
  const transport = readFileSync(new URL("./web-semantic-transport.mjs", import.meta.url), "utf8");
  const driver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  assert.match(transport, /async focusText\(selector, readyElement\)/);
  assert.match(transport, /input instanceof HTMLInputElement \|\| input instanceof HTMLTextAreaElement/);
  assert.match(transport, /document\.activeElement === input/);
  assert.match(driver, /transport\.focusText\(webTestIdSelector\(controlId\), readyElement\)/);
});

test("web layer toggles expose the same selected state they paint", () => {
  const web = readFileSync(new URL("../../ui/web-app/src/App.tsx", import.meta.url), "utf8");
  assert.match(
    web,
    /aria-pressed=\{\(option\.toggleState\?\.visible \?\? option\.active\) \? "true" : "false"\}/,
  );
});

test("release journey stability runs require every repetition to pass", () => {
  const temp = mkdtempSync(join(tmpdir(), "aerobag-release-suite-repetitions-"));
  try {
    const fixture = join(temp, "fixture.json");
    const count = join(temp, "count");
    const fakeBin = join(temp, "bin");
    writeFileSync(fixture, "{}\n");
    writeFileSync(count, "0\n");
    mkdirSync(fakeBin);
    const node = join(fakeBin, "node");
    writeFileSync(node, `#!/usr/bin/env bash
if [[ "$1" == "-e" ]]; then printf 'http://127.0.0.1:18093'; exit 0; fi
if [[ "$1" == *run-release-journey.mjs ]]; then
  value=$(( $(cat "${count}") + 1 ))
  echo "$value" >"${count}"
  [[ "$value" == 2 ]] && exit 37
  exit 0
fi
if [[ "$#" == "5" ]]; then echo fake.journey; else echo fresh; fi
`);
    chmodSync(node, 0o755);
    const curl = join(fakeBin, "curl");
    writeFileSync(curl, `#!/usr/bin/env bash
echo '{"live_feed_profile":"fresh","serves_web_app":false}'
`);
    chmodSync(curl, 0o755);
    const result = spawnSync("bash", [
      new URL("./release_journey_lab.sh", import.meta.url).pathname,
      "web-suite",
      "p0",
    ], {
      cwd: new URL("../..", import.meta.url).pathname,
      env: {
        ...process.env,
        AEROBAG_RELEASE_JOURNEY_FIXTURE: fixture,
        AEROBAG_RELEASE_JOURNEY_REPETITIONS: "3",
        AEROBAG_RELEASE_JOURNEY_ORIGIN: "http://127.0.0.1:18093",
        AEROBAG_E2E_URL: "http://127.0.0.1:18093",
        PATH: `${fakeBin}:${process.env.PATH}`,
      },
    });
    const observedCount = readFileSync(count, "utf8").trim();
    assert.equal(observedCount, "2", `stdout=${result.stdout}\nstderr=${result.stderr}`);
    assert.notEqual(result.status, 0, `stdout=${result.stdout}\nstderr=${result.stderr}`);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test("release journey suites reject a journey id mistaken for a priority", () => {
  const result = spawnSync("bash", [
    new URL("./release_journey_lab.sh", import.meta.url).pathname,
    "web-suite",
    "shared.other-documents",
  ], {
    cwd: new URL("../..", import.meta.url).pathname,
    env: labMetadataEnvironment(),
  });
  assert.equal(result.status, 2, `stdout=${result.stdout}\nstderr=${result.stderr}`);
  assert.match(result.stderr.toString(), /invalid journey priority/);
});

test("release journey suite priority validation accepts the p2 release lane", () => {
  const lab = readFileSync(new URL("./release_journey_lab.sh", import.meta.url), "utf8");
  assert.match(lab, /p0\|p1\|p2\|all/);
  assert.match(lab, /web-suite \[p0\|p1\|p2\|all\]/);
});

test("release journey suites propagate a failed journey process", () => {
  const temp = mkdtempSync(join(tmpdir(), "aerobag-release-suite-failure-"));
  try {
    const fixture = join(temp, "fixture.json");
    const fakeBin = join(temp, "bin");
    writeFileSync(fixture, "{}\n");
    mkdirSync(fakeBin);
    const node = join(fakeBin, "node");
    writeFileSync(node, `#!/usr/bin/env bash
if [[ "$1" == "-e" ]]; then printf 'http://127.0.0.1:18093'; exit 0; fi
if [[ "$1" == *run-release-journey.mjs ]]; then exit 37; fi
if [[ "$#" == "5" ]]; then echo fake.journey; else echo fresh; fi
`);
    chmodSync(node, 0o755);
    const curl = join(fakeBin, "curl");
    writeFileSync(curl, `#!/usr/bin/env bash
echo '{"live_feed_profile":"fresh","serves_web_app":false}'
`);
    chmodSync(curl, 0o755);
    const result = spawnSync("bash", [
      new URL("./release_journey_lab.sh", import.meta.url).pathname,
      "web-suite",
      "p0",
    ], {
      cwd: new URL("../..", import.meta.url).pathname,
      env: {
        ...process.env,
        AEROBAG_RELEASE_JOURNEY_FIXTURE: fixture,
        AEROBAG_RELEASE_JOURNEY_ORIGIN: "http://127.0.0.1:18093",
        AEROBAG_E2E_URL: "http://127.0.0.1:18093",
        PATH: `${fakeBin}:${process.env.PATH}`,
      },
    });
    assert.equal(result.status, 37, result.stderr.toString());
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test("Android journey suites bound blocked driver processes", () => {
  const lab = readFileSync(
    new URL("./release_journey_lab.sh", import.meta.url),
    "utf8",
  );
  assert.match(lab, /AEROBAG_ANDROID_JOURNEY_TIMEOUT_SECONDS:-600/);
  assert.match(lab, /android-suite-shard\)/);
  assert.match(lab, /android_shard_journeys/);
  assert.match(lab, /journey\.android_isolated/);
  assert.doesNotMatch(lab, /while IFS= read -r journey/);
  assert.match(lab, /mapfile -t journeys/);
  assert.match(
    lab,
    /timeout --foreground --kill-after=15s "\$\{ANDROID_JOURNEY_TIMEOUT_SECONDS\}s"/,
  );

  const workflow = readFileSync(
    new URL("../../.github/workflows/e2e-ci.yml", import.meta.url),
    "utf8",
  );
  assert.match(
    workflow,
    /release-journey-android:[\s\S]*?runs-on: ubuntu-latest\n\s+timeout-minutes: 45/,
  );
  assert.match(workflow, /shard: \[0, 1, 2, 3\]/);
  assert.match(workflow, /android-suite-shard all "\$\{\{ matrix\.shard \}\}" 4/);
  const androidJob = workflow.match(
    /  release-journey-android:[\s\S]*?\n  android-native:/,
  )?.[0] ?? "";
  assert.doesNotMatch(androidJob, /matrix\.priority|priority:/);
  assert.match(
    workflow,
    /android-native:[\s\S]*?runs-on: ubuntu-latest\n\s+timeout-minutes: 30/,
  );
});

test("Android emulator launchers and journey lab derive one device identity", () => {
  const root = new URL("../..", import.meta.url).pathname;
  const helper = new URL("../../ui/android-app/scripts/emulator_identity.sh", import.meta.url).pathname;
  const identity = (vncPort) => {
    const env = { ...process.env, VNC_PORT: String(vncPort) };
    for (const name of [
      "DISPLAY_NUM", "EMULATOR_CONSOLE_PORT", "EMULATOR_ADB_PORT",
      "ANDROID_SERIAL", "AVD_INSTANCE_NAME", "EMULATOR_READ_ONLY",
    ]) {
      delete env[name];
    }
    const result = spawnSync("bash", [helper], { cwd: root, env });
    assert.equal(result.status, 0, result.stderr.toString());
    return result.stdout.toString();
  };

  assert.match(identity(5900), /ANDROID_SERIAL=emulator-5554/);
  assert.match(identity(5905), /ANDROID_SERIAL=emulator-5564/);

  const lab = readFileSync(new URL("./release_journey_lab.sh", import.meta.url), "utf8");
  assert.match(lab, /source "\$ROOT\/ui\/android-app\/scripts\/emulator_identity\.sh"/);
  assert.doesNotMatch(lab, /emulator-5564/);

  const stopStack = readFileSync(
    new URL("../../ui/android-app/scripts/stop_emulator_stack.sh", import.meta.url),
    "utf8",
  );
  assert.match(stopStack, /qemu-system\.\*-ports \$\{EMULATOR_CONSOLE_PORT\},\$\{EMULATOR_ADB_PORT\}/);
  assert.doesNotMatch(stopStack, /qemu-system\.\*@\$AVD_INSTANCE_NAME/);

  const startStack = readFileSync(
    new URL("../../ui/android-app/scripts/start_emulator_stack.sh", import.meta.url),
    "utf8",
  );
  assert.match(startStack, /service check package/);
  assert.match(startStack, /\^Service package: found\$/);
  assert.match(
    startStack,
    /EMULATOR_DATA_PARTITION_SIZE="\$\{EMULATOR_DATA_PARTITION_SIZE:-4294967296\}"/,
  );
});

test("Android qualification isolates semantic tests from Google service instability", () => {
  const workflow = readFileSync(
    new URL("../../.github/workflows/e2e-ci.yml", import.meta.url),
    "utf8",
  );
  const aggregate = workflow.match(/  release-journey-android:[\s\S]*?\n  android-native:/)?.[0] ?? "";
  const native = workflow.match(/  android-native:[\s\S]*?\n  android-chrome-live-feed:/)?.[0] ?? "";
  assert.match(aggregate, /AVD_PACKAGE_PATH: system-images;android-34;aosp_atd;x86_64/);
  assert.match(native, /matrix\.test == 'android\.plate-first-render-smoke'/);
  assert.match(native, /system-images;android-34;google_apis;x86_64/);
  assert.match(native, /system-images;android-34;aosp_atd;x86_64/);
});

test("Android qualification archives packages without leaking live-feed state between profiles", () => {
  const workflow = readFileSync(
    new URL("../../.github/workflows/e2e-ci.yml", import.meta.url),
    "utf8",
  );
  const localQualification = readFileSync(
    new URL("../ci/local_candidate_qualification.py", import.meta.url),
    "utf8",
  );
  const baselineJob = workflow.match(
    /  release-journey-android-baseline:[\s\S]*?\n  release-journey-android:/,
  )?.[0] ?? "";
  const shardJob = workflow.match(
    /  release-journey-android:[\s\S]*?\n  android-native:/,
  )?.[0] ?? "";
  assert.match(baselineJob, /fixture-start-web empty[\s\S]+shared\.startup-navigation/);
  assert.match(baselineJob, /android-baseline-save/);
  assert.match(shardJob, /release-journey-android-baseline-\$\{\{ github\.sha \}\}/);
  assert.match(shardJob, /android-boot-install/);
  assert.doesNotMatch(shardJob, /shared\.startup-navigation|android-baseline-save/);
  assert.equal(workflow.match(/android-baseline-save/g)?.length, 1);
  assert.match(localQualification, /fixture-start-web empty/);
  assert.doesNotMatch(workflow, /AEROBAG_ANDROID_BASELINE_SNAPSHOT/);
  assert.match(workflow, /AEROBAG_ANDROID_BASELINE_ARCHIVE/);
  const lab = readFileSync(
    new URL("./release_journey_lab.sh", import.meta.url),
    "utf8",
  );
  assert.match(lab, /android_baseline_save\(\)[\s\S]+android_clear_baseline_live_feeds/);
  assert.match(lab, /android_baseline_restore\(\)[\s\S]+android_clear_baseline_live_feeds/);
  assert.match(lab, /find files\/live-feeds -type f -print/);
  assert.match(lab, /exec-out run-as org\.aerobag\.app tar -C \. -cf - \./);
  assert.match(lab, /aerobag_e2e_clear_app_data "\$SERIAL"/);
  assert.doesNotMatch(lab, /shell pm clear org\.aerobag\.app/);
  const appData = readFileSync(
    new URL("../../ui/android-app/scripts/e2e_app_data.sh", import.meta.url),
    "utf8",
  );
  assert.match(appData, /shell am stop-app "\$AEROBAG_E2E_APP_PACKAGE"/);
  assert.match(appData, /find \. -mindepth 1 -delete/);
  assert.doesNotMatch(appData, /shell pm clear|shell am force-stop/);
  assert.doesNotMatch(lab, /emu avd snapshot (?:save|load)/);
});

test("procedure replacement waits for the picker transaction before reopening its row", async () => {
  let transitionSelected = false;
  let pickerReadCount = 0;
  let rowOpen = false;
  const staleRow = { id: "parity:plan-procedure-row:I16R:uid:old" };
  const currentRow = { id: "parity:plan-procedure-row:I16R:uid:new" };
  const runtime = withActionContract({
    platform: "web",
    driver: {
      async findProjectionMatching(prefix, label) {
        assert.equal(prefix, "parity:plan-row:");
        assert.equal(label, "KPAE");
        return { id: "parity:plan-row:airport-row", text: "KPAE" };
      },
      async revealProjectionMatching(prefix, label) {
        return this.findProjectionMatching(prefix, label);
      },
      async readElement(id) {
        if (id === "plan-row-tray-scrim") return rowOpen ? { id } : null;
        if (id === "plan-row-action-select_approach") return { enabled: true };
        if (id === "plan-procedure-picker") {
          pickerReadCount += 1;
          return pickerReadCount < 2 ? { id } : null;
        }
        return null;
      },
      async readAction(id) {
        return { test_id: id, enabled: true };
      },
      async readProjection(prefix) {
        if (prefix === "parity:plan-procedure:") {
          return [{ id: "parity:plan-procedure:I16R" }];
        }
        if (prefix === "parity:plan-procedure-transition:") {
          return [{ id: "parity:plan-procedure-transition:VECTORS", enabled: true }];
        }
        if (prefix === "parity:plan-procedure-row:I16R:uid:") {
          return [transitionSelected && pickerReadCount >= 2 ? currentRow : staleRow];
        }
        return [];
      },
      async performAction(id) {
        if (id.startsWith("plan-row:")) rowOpen = true;
        else if (id !== "plan-row-tray-scrim") rowOpen = false;
        if (id === "plan-procedure-transition:VECTORS") transitionSelected = true;
      },
    },
    async step(_label, action) {
      return action();
    },
    async eventually(label, probe) {
      for (let attempt = 0; attempt < 4; attempt += 1) {
        const value = await probe();
        if (value) return value;
      }
      throw new Error(`eventually failed: ${label}`);
    },
  });

  const selected = await selectProcedure(runtime, {
    airportId: "KPAE",
    actionId: "select_approach",
    procedureId: "I16R",
  });
  assert.equal(selected, currentRow);
  assert.equal(transitionSelected, true);
  assert.equal(pickerReadCount, 2);
});

test("flight-plan row tray dismissal uses the supported Back action", async () => {
  let rowOpen = true;
  let backCount = 0;
  const runtime = withActionContract({
    driver: {
      async readElement(id) {
        assert.equal(id, "plan-row-tray-scrim");
        return rowOpen ? { id } : null;
      },
      async back() {
        backCount += 1;
        rowOpen = false;
      },
      async performAction() {
        assert.fail("full-screen scrims must not be tapped through their obscured center");
      },
    },
    async eventually(label, probe) {
      for (let attempt = 0; attempt < 2; attempt += 1) {
        const value = await probe();
        if (value) return value;
      }
      throw new Error(`eventually failed: ${label}`);
    },
  });

  await dismissPlanRowTray(runtime);
  assert.equal(backCount, 1);
  assert.equal(rowOpen, false);
});

test("forecast choice waits for one coherent action state before branching", async () => {
  let calculationReads = 0;
  let readyReads = 0;
  let readySelected = false;
  let selectionCalculationInFlight = false;
  const actions = [];
  const runtime = withActionContract({
    platform: "web",
    driver: {
      async readElement(id) {
        if (id === "altitude-comparison-loading") {
          if (selectionCalculationInFlight) return { text: "Calculating…" };
          calculationReads += 1;
          return calculationReads < 3 ? { text: "Calculating…" } : null;
        }
        if (id === "altitude-planner-wind-action-no_wind") return { enabled: true };
        if (id === "altitude-planner-wind-action-latest_forecast") return null;
        if (id === "altitude-planner-wind-action-ready_forecast") {
          readyReads += 1;
          if (readyReads === 1) return null;
          return { enabled: true, pressed: readySelected ? "true" : "false" };
        }
        return null;
      },
      async performAction(id) {
        actions.push(id);
        if (id === "altitude-planner-wind-action-ready_forecast") {
          selectionCalculationInFlight = true;
        }
      },
    },
    async eventually(label, probe) {
      if (label === "ready wind forecast selected") {
        selectionCalculationInFlight = false;
        readySelected = true;
      }
      for (let attempt = 0; attempt < 3; attempt += 1) {
        const value = await probe();
        if (value) return value;
      }
      throw new Error("forecast state did not settle");
    },
  });

  const result = await chooseForecastWindModel(runtime);
  assert.equal(result.downloaded, false);
  assert.equal(calculationReads, 3);
  assert.equal(result.selected.pressed, "true");
  assert.deepEqual(actions, ["altitude-planner-wind-action-ready_forecast"]);
});

test("altitude choices do not treat accessibility whitespace as a state change", () => {
  const source = readFileSync(new URL("./release-journey-implementations.mjs", import.meta.url), "utf8");
  const choice = source.slice(
    source.indexOf("function semanticTextSignature"),
    source.indexOf("function altitudeWindActionId"),
  );
  assert.match(choice, /replace\(\/\\s\+\/g, " "\)\.trim\(\)/);
  assert.match(choice, /semanticTextSignature\(directAfter\.text\) !== semanticTextSignature\(before\?\.text\)/);
  assert.match(choice, /semanticTextSignature\(value\.text\) !== semanticTextSignature\(before\?\.text\)/);
});

test("local lab and immutable app builds agree on fixed service ports", () => {
  const lab = readFileSync(
    new URL("./release_journey_lab.sh", import.meta.url),
    "utf8",
  );
  const builder = readFileSync(
    new URL("../ci/build_release_e2e_apps.sh", import.meta.url),
    "utf8",
  );
  assert.match(lab, /PACKAGE_SOURCE_PORT:-18093/);
  assert.match(lab, /AEROBAG_E2E_CLOUD_PORT:-18094/);
  assert.match(builder, /AEROBAG_ANDROID_PACKAGE_SOURCE_DEVICE_PORT:-18093/);
  assert.match(builder, /AEROBAG_ANDROID_CLOUD_DEVICE_PORT:-18094/);
  assert.doesNotMatch(builder, /PACKAGE_ORIGIN="http:\/\/127\.0\.0\.1:\$\{PACKAGE_SOURCE_PORT\}"/);
  assert.match(builder, /AEROBAG_E2E_ENABLED=1/);
  assert.match(builder, /ANDROID_DEV_SERVER_BASE_URL="\$PACKAGE_ORIGIN"/);
  assert.match(builder, /cp -a "\$ROOT\/ui\/icons" "\$OUTPUT\/web-dist\/icons"/);
  const webBuild = builder.slice(
    builder.indexOf("env \\\n  AEROBAG_UI_TARGET_ROOT"),
    builder.indexOf("env \\\n  AEROBAG_UI_TARGET_ROOT", builder.indexOf("env \\\n  AEROBAG_UI_TARGET_ROOT") + 1),
  );
  assert.doesNotMatch(webBuild, /AEROBAG_LIVE_FEEDS_ORIGIN|AEROBAG_CLOUD_SERVER_BASE_URL/);
  const fixtureServer = readFileSync(
    new URL("./serve-release-journey-fixture.mjs", import.meta.url),
    "utf8",
  );
  assert.match(fixtureServer, /pathname\.startsWith\("\/cloud\/"\)/);
  assert.match(fixtureServer, /proxyCloudRequest\(request, response, args\.cloudOrigin\)/);
  const androidBuild = readFileSync(
    new URL("../../ui/android-app/app/build.gradle.kts", import.meta.url),
    "utf8",
  );
  assert.match(androidBuild, /release\s*\{[\s\S]*?isDebuggable = androidE2eEnabled/);
  const androidHarness = readFileSync(
    new URL("./android-harness.mjs", import.meta.url),
    "utf8",
  );
  assert.match(androidHarness, /DEBUG_CLEAR_CORE_SETTINGS_EXTRA/);
  assert.match(androidHarness, /DEBUG_CLEAR_UI_PREFS_EXTRA/);
  assert.doesNotMatch(androidHarness, /run-as[^\n]+core-settings-v1\.json/);
  assert.doesNotMatch(androidHarness, /run-as[^\n]+aerobag_ui\.xml/);
});

test("Android E2E can map an immutable APK port to an isolated host fixture", () => {
  const source = readFileSync(
    new URL("../../ui/android-app/scripts/run_e2e.sh", import.meta.url),
    "utf8",
  );
  const emulatorSource = readFileSync(
    new URL("../../ui/android-app/scripts/start_emulator_stack.sh", import.meta.url),
    "utf8",
  );
  assert.match(
    source,
    /ANDROID_PACKAGE_SOURCE_DEVICE_PORT="\$\{ANDROID_PACKAGE_SOURCE_DEVICE_PORT:-\$PACKAGE_SOURCE_PORT\}"/,
  );
  assert.match(
    source,
    /"tcp:\$\{ANDROID_PACKAGE_SOURCE_DEVICE_PORT\}" "tcp:\$\{PACKAGE_SOURCE_PORT\}"/,
  );
  assert.match(source, /installRelease :app:assembleReleaseAndroidTest/);
  assert.match(source, /ANDROID_BUILD_RUST_RELEASE=1/);
  assert.match(source, /outputs\/apk\/androidTest\/release\/app-release-androidTest\.apk/);
  assert.doesNotMatch(source, /assembleDebugAndroidTest|app-debug-androidTest\.apk/);
  assert.match(
    emulatorSource,
    /ANDROID_PACKAGE_SOURCE_DEVICE_PORT="\$\{ANDROID_PACKAGE_SOURCE_DEVICE_PORT:-\$PACKAGE_SOURCE_PORT\}"/,
  );
  assert.match(
    emulatorSource,
    /"tcp:\$\{ANDROID_PACKAGE_SOURCE_DEVICE_PORT\}" "tcp:\$\{PACKAGE_SOURCE_PORT\}"/,
  );
});

test("persistent Android semantic requests separate probes from bounded actions", () => {
  const source = readFileSync(
    new URL("./android-harness.mjs", import.meta.url),
    "utf8",
  );
  assert.match(source, /SEMANTIC_OBSERVATION_REQUEST_TIMEOUT_SECONDS = 0\.9/);
  assert.match(source, /SEMANTIC_OBSERVATION_RECOVERY_TIMEOUT_SECONDS = 2\.25/);
  assert.match(source, /SEMANTIC_ACTION_REQUEST_TIMEOUT_SECONDS = 2\.25/);
  assert.match(
    source,
    /semanticDriverObservationRequest\(state\.port, "\/dump"\)/,
  );
  assert.match(
    source,
    /semanticDriverActionRequest\(port, `\/tap\?\$\{query\}`\)/,
  );
  assert.match(source, /"--fail-with-body"/);
  assert.match(source, /Android semantic driver IME registration/);
  assert.match(source, /\["shell", "ime", "list", "-a", "-s"\]/);
  assert.match(
    source,
    /semanticDriverObservationUnavailable\(response\)[\s\S]*new TransientObservationError/,
  );
  const observationTimeoutSeconds = Number(
    source.match(/SEMANTIC_OBSERVATION_REQUEST_TIMEOUT_SECONDS = ([0-9.]+)/)?.[1],
  );
  assert.ok(
    observationTimeoutSeconds * 1_000 <= E2E_TIMING.userResponseMs / 3,
    "one semantic observation must leave room for retries inside a user transition",
  );
  const hierarchyDump = source.slice(
    source.indexOf("function semanticDriverDump"),
    source.indexOf("export async function ensureAndroidSemanticDriver"),
  );
  assert.match(
    hierarchyDump,
    /semanticDriverObservationUnavailable\(response\)[\s\S]*new TransientObservationError/,
  );
});

test("persistent Android progress actions resolve exact readiness evidence before bounded fallbacks", () => {
  const source = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  assert.match(source, /value\.put\("semantic-path", semanticPath\)/);
  assert.match(source, /private AccessibilityNodeInfo nodeAtPath\(String semanticPath\)/);
  assert.match(source, /private AccessibilityNodeInfo resolveRenderedNode\(/);
  assert.match(source, /private static AccessibilityNodeInfo findRenderedNodeAtPoint\(/);
  assert.match(
    source,
    /bounds\.contains\(expectedBounds\.centerX\(\), expectedBounds\.centerY\(\)\)/,
  );
  assert.match(source, /tag\.equals\(node\.getViewIdResourceName\(\)\)/);
  assert.match(source, /for \(int attempt = 0; attempt < 3; attempt\+\+\)/);
  assert.match(source, /if \(setMatchingNodeProgress\(node, tag, value, expectedBounds\)\) return true/);
  assert.match(source, /ProviderProjection projection = providerProjection\(tag, true\)/);
  const actionResolution = source.slice(
    source.indexOf("private AccessibilityNodeInfo resolveRenderedNode("),
    source.indexOf("private static boolean matchesRenderedTarget("),
  );
  assert.ok(
    actionResolution.indexOf("findIndexedRenderedNode(tag, expectedBounds)") <
      actionResolution.indexOf("nodeAtPath(semanticPath)"),
    "indexed exact action lookup must precede path and point fallbacks",
  );
  assert.doesNotMatch(actionResolution, /collectMatchingNodes/);
});

test("rapid Android scalar projections use stable IDs instead of full-tree prefix scans", () => {
  const driver = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const playback = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/PlaybackWidget.kt", import.meta.url),
    "utf8",
  );
  const journeyDriver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const mainActivity = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt", import.meta.url),
    "utf8",
  );
  const mapExplorer = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt", import.meta.url),
    "utf8",
  );
  const charts = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/ChartsPage.kt", import.meta.url),
    "utf8",
  );
  const projectionProvider = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/E2eProjectionProvider.kt", import.meta.url),
    "utf8",
  );
  const projectionView = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/E2eProjectionView.kt", import.meta.url),
    "utf8",
  );
  const manifest = readFileSync(
    new URL("../../ui/android-app/app/src/main/AndroidManifest.xml", import.meta.url),
    "utf8",
  );
  assert.match(driver, /case "\/exact-projection"/);
  assert.match(
    driver,
    /ProviderProjection providerProjection = renderedOnly[\s\S]*ProviderProjection\.unhandled\(\)[\s\S]*providerProjection\(tag, verifyReachable, avoidNavigation\)/,
  );
  assert.match(driver, /if \(providerProjection\.handled\) return providerProjection\.values/);
  assert.ok(
    driver.indexOf("ProviderProjection providerProjection = renderedOnly") <
      driver.indexOf("List<AccessibilityNodeInfo> roots = targetRoots(true)", driver.indexOf("private JSONArray renderExactProjection")),
    "fixed projections must bypass accessibility before requesting any rendered roots",
  );
  assert.match(driver, /findAccessibilityNodeInfosByViewId\(tag\)/);
  assert.match(driver, /exactNodePaths\.get\(tag\)/);
  assert.match(driver, /nodeAtPath\(cachedPath\)/);
  assert.match(driver, /tag\.equals\(cached\.getViewIdResourceName\(\)\)/);
  assert.match(
    driver,
    /appendExactProjectionAtPoint\([\s\S]*tag,[\s\S]*cachedBounds,[\s\S]*output,[\s\S]*includeDescendantText/,
  );
  assert.match(driver, /bounds\.contains\(expectedBounds\.centerX\(\), expectedBounds\.centerY\(\)\)/);
  assert.match(driver, /exactNodePaths\.put\(tag, semanticPath\)/);
  assert.match(playback, /\.testTag\("parity:playback-widget"\)/);
  assert.match(journeyDriver, /e2e_live_overlay_projection/);
  assert.match(journeyDriver, /e2e_nexrad_state_projection/);
  assert.match(
    journeyDriver,
    /ANDROID_EXACT_SCALAR_PROJECTIONS\.has\(prefix\)[\s\S]*queryAndroidExactProjection/,
  );
  assert.match(journeyDriver, /"parity:ownship-state:"/);
  assert.match(journeyDriver, /"parity:playback-widget:"/);
  assert.match(journeyDriver, /"parity:viewport:"/);
  assert.match(journeyDriver, /"parity:map-follow-state:"/);
  assert.match(journeyDriver, /"parity:plate-viewport:"/);
  assert.match(journeyDriver, /e2e_playback_widget_projection/);
  assert.match(mapExplorer, /R\.id\.e2e_map_follow_projection/);
  assert.match(mapExplorer, /R\.id\.e2e_map_family_projection/);
  assert.match(mapExplorer, /R\.id\.e2e_raster_state_projection/);
  assert.match(mapExplorer, /R\.id\.e2e_vector_state_projection/);
  assert.doesNotMatch(mapExplorer, /\.testTag\("parity:map-family:/);
  assert.doesNotMatch(mapExplorer, /\.testTag\(\s*"parity:raster-state:/);
  assert.doesNotMatch(mapExplorer, /\.testTag\("parity:vector-state:/);
  assert.doesNotMatch(mapExplorer, /\.testTag\(mapFollowProbeTag/);
  assert.match(projectionView, /E2eProjectionRegistry\.publish\(resourceId, state, owner\)/);
  assert.match(projectionView, /E2eProjectionRegistry\.remove\(resourceId, owner\)/);
  assert.match(projectionProvider, /ConcurrentHashMap<String, Entry>/);
  assert.match(projectionProvider, /viewId !in E2eProjectionRegistry\.KnownViewIds/);
  assert.match(manifest, /android:enabled="\$\{e2eProjectionProviderEnabled\}"/);
  assert.match(manifest, /android:readPermission="org\.aerobag\.app\.permission\.READ_E2E_PROJECTIONS"/);
  assert.match(charts, /R\.id\.e2e_plate_viewport_projection/);
  assert.match(
    charts,
    /MapCenterButton\([\s\S]*e2eIndexedControl\([\s\S]*semanticTag = "parity:center-here-button"/,
  );
  assert.doesNotMatch(charts, /\.testTag\(\s*"parity:plate-viewport:/);
  assert.match(mainActivity, /R\.id\.e2e_startup_state_projection/);
  assert.match(mainActivity, /R\.id\.e2e_flight_plan_rows_projection/);
  assert.match(mainActivity, /sessionPlanUiState\.displayRows\.joinToString/);
  assert.match(mainActivity, /R\.id\.e2e_flight_plan_state_projection/);
  assert.match(mainActivity, /flightPlanStateProjection\(sessionPlanUiState\)/);
  assert.match(mapExplorer, /R\.id\.e2e_flight_plan_route_overlay_projection/);
  assert.match(mapExplorer, /flightPlanRouteOverlayProjectionState\(/);
  assert.match(mainActivity, /R\.id\.e2e_flight_plan_overlay_projection/);
  assert.match(mainActivity, /flightPlanOverlayController\.state is FlightPlanOverlayState\.RowTray/);
  assert.match(mainActivity, /parity:startup-state:ready:/);
  assert.match(
    harness,
    /STARTUP_PROJECTION_ID = "org\.aerobag\.app:id\/e2e_startup_state_projection"/,
  );
  assert.match(
    harness,
    /queryAndroidStartupProjection[\s\S]*queryAndroidExactProjection\(serial, STARTUP_PROJECTION_ID/,
  );
  assert.match(journeyDriver, /e2e_flight_plan_rows_projection/);
  assert.match(journeyDriver, /e2e_flight_plan_state_projection/);
  assert.match(journeyDriver, /e2e_flight_plan_route_overlay_projection/);
  assert.match(journeyDriver, /e2e_flight_plan_overlay_projection/);
  assert.match(journeyDriver, /e2e_flight_plan_route_entry_projection/);
});

test("plate journeys rendezvous with the selected chart instead of arbitrary viewport stability", () => {
  const implementations = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  assert.match(
    implementations,
    /initializedPlateViewport[\s\S]*plateViewport\(runtime, chartId\)/,
  );
  assert.match(implementations, /const chartId = plateChartId\(chart\)/);
  assert.match(implementations, /const multiId = plateChartId\(multi\)/);
  assert.match(implementations, /const legendChartId = plateChartId\(legendOption\)/);
  const multiPageScroll = implementations.slice(
    implementations.indexOf("const firstPageViewport"),
    implementations.indexOf('runtime.check("plate.first-last-page"'),
  );
  assert.match(
    multiPageScroll,
    /zoom multi-page plate for scrolling[\s\S]*scroll multi-page plate/,
    "a fitted multi-page plate must be zoomed before scrolling can be required to move it",
  );
  assert.match(multiPageScroll, /value !== scrollableViewport/);
  assert.doesNotMatch(implementations, /settled (?:initial plate|legend) viewport/);
});

test("Android coordinate actions settle geometry while atomic web actions use one readiness sample", () => {
  const source = readFileSync(new URL("./release-journey-runtime.mjs", import.meta.url), "utf8");
  const actionBody = source.slice(
    source.indexOf("async action(description"),
    source.indexOf("async repeatableAction(description"),
  );
  const repeatBody = source.slice(
    source.indexOf("async repeatAction(description"),
    source.indexOf("async openOption(description"),
  );
  assert.match(actionBody, /readinessSamples: semanticActionReadinessSamples\(driver\)/);
  assert.equal(semanticActionReadinessSamples({ platform: "android" }), E2E_TIMING.stableObservationSamples);
  assert.equal(semanticActionReadinessSamples({ platform: "web" }), 1);
  assert.equal(semanticTransitionCompletionSamples({ platform: "android" }), 1);
  assert.equal(
    semanticTransitionCompletionSamples({ platform: "web" }),
    E2E_TIMING.transitionCompletionSamples,
  );
  assert.match(source, /semanticTransitionCompletionSamples\(driver\)/);
  assert.doesNotMatch(repeatBody, /readinessSamples:/);
  assert.match(repeatBody, /performRepeatedAction/);
});

test("mandatory disclaimer response and application startup use separate budgets", () => {
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const implementation = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  assert.match(
    harness,
    /application startup after accepting mandatory disclaimer[\s\S]*E2E_TIMING\.startupMs/,
  );
  assert.match(
    harness,
    /accept mandatory disclaimer[\s\S]*?complete:[\s\S]*?disclaimer_required === "false"/,
  );
  assert.doesNotMatch(
    harness.slice(
      harness.indexOf("export async function acceptDisclaimerIfPresent"),
      harness.indexOf("export function assertRuntimeIsAvailable"),
    ),
    /complete:[\s\S]*?parity:disclaimer-accept-button/,
  );
  assert.match(
    implementation,
    /accept mandatory disclaimer[\s\S]*const completed = await startupState\(runtime\)/,
  );
});

test("Android app restart observes a stable process node without dumping the UI", () => {
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const activity = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt", import.meta.url),
    "utf8",
  );
  const launch = harness.slice(
    harness.indexOf("export async function launchFreshAndroidApp"),
    harness.indexOf("export async function acceptDisclaimerIfPresent"),
  );
  assert.match(launch, /restartAndroidAppAcrossSemanticLifecycle\(/);
  assert.match(launch, /"am", "stop-app", ANDROID_PACKAGE/);
  assert.doesNotMatch(launch, /"am", "force-stop", ANDROID_PACKAGE/);
  assert.match(launch, /firstAerobagProcessNode\(serial\)/);
  assert.doesNotMatch(launch, /dumpAndroid\(/);
  const processProbe = harness.slice(
    harness.indexOf("function firstAerobagProcessNode"),
    harness.indexOf("function semanticNodeIdentity"),
  );
  assert.match(processProbe, /parity:app-process:/);
  assert.match(processProbe, /prefix: true, first: true/);
  assert.match(processProbe, /TransientObservationError/);
  assert.match(activity, /AndroidProcessSemanticId = UUID\.randomUUID\(\)\.toString\(\)/);
  assert.match(activity, /testTag\("parity:app-process:\$AndroidProcessSemanticId"\)/);
  const lifecycle = harness.slice(
    harness.indexOf("export async function restartAndroidAppAcrossSemanticLifecycle"),
    harness.indexOf("export async function launchFreshAndroidApp"),
  );
  assert.match(lifecycle, /previous Aerobag process, task, window, and semantic UI removed/);
  assert.match(lifecycle, /new Aerobag semantic UI visible/);
  assert.match(lifecycle, /consecutiveSuccesses: E2E_TIMING\.transitionCompletionSamples/g);
  assert.match(launch, /androidAppLifecyclePresent\(serial\)/);
  assert.doesNotMatch(
    launch.slice(launch.indexOf("readStoppedState:")),
    /firstAerobagProcessNode\(serial\)[\s\S]*?androidAppLifecyclePresent\(serial\)/,
  );
});

test("package refresh completion observes a new catalog request", () => {
  const requests = [
    { method: "GET", url: "/__health" },
    { method: "HEAD", url: "/packages/current_artifacts.json" },
    { method: "GET", url: "/packages/current_artifacts.json" },
    { method: "GET", url: "http://fixture.test/packages/" },
  ];
  assert.equal(publicationCatalogRequestCount(requests), 2);
});

test("package sync completion observes artifact requests instead of transient labels", () => {
  const requests = [
    { method: "GET", url: "/packages/published/packages/updated%20artifact.zip" },
    { method: "GET", url: "/packages/current_artifacts.json" },
  ];
  assert.equal(publicationArtifactRequestCount(requests, "updated artifact.zip"), 1);
  const source = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  const maintenance = source.slice(
    source.indexOf("async function androidPackageMaintenance"),
    source.indexOf("const DEBUG_ASSERTIONS"),
  );
  assert.match(maintenance, /start interrupted offline package sync[\s\S]*publicationArtifactRequestCount/);
  assert.match(maintenance, /start successful offline package sync[\s\S]*publicationArtifactRequestCount/);
  assert.doesNotMatch(maintenance, /APPLYING\|CANCELING/);
  assert.ok(
    maintenance.indexOf('runtime.openPage("offline_packages")') <
      maintenance.indexOf('publication: "updated"'),
  );
});

test("fixture reads tolerate transport resets but fixture mutations remain single-shot", () => {
  const source = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  const controls = source.slice(
    source.indexOf("async function setFixtureControl"),
    source.indexOf("export function publicationCatalogRequestCount"),
  );
  assert.match(controls, /fixtureHealth[\s\S]*transientNetworkErrors: true/);
  assert.match(controls, /fixtureRequests[\s\S]*transientNetworkErrors: true/);
  const mutation = controls.slice(0, controls.indexOf("async function fixtureHealth"));
  assert.doesNotMatch(mutation, /transientNetworkErrors/);
  assert.match(source, /headers\.set\("connection", "close"\)/);
});

test("journey actions require a semantic completion condition", async () => {
  const actions = [];
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-action-contract-"));
  const runtime = createJourneyRuntime({
    journey: { id: "action-contract", assertions: [] },
    platform: "web",
    driver: {
      async readElement(id) { return id === "button" ? { test_id: id } : null; },
      async readAction(id) { return id === "button" ? { test_id: id } : null; },
      async performAction(id) { actions.push(id); },
      async captureFrame() {},
    },
    fixture: null,
    artifactDir,
  });
  try {
    await assert.rejects(
      runtime.action("incomplete action", "button", {}),
      /must declare a semantic completion condition/,
    );
    await assert.rejects(
      runtime.action("extra timeout", "button", { complete: async () => true }, 10_000),
      /unexpected positional arguments/,
    );
    await assert.rejects(
      runtime.action("custom readiness", "button", {
        ready: async () => ({ test_id: "other" }),
        complete: async () => true,
      }),
      /action readiness must come from driver\.readAction/,
    );
    let completionProbes = 0;
    const value = await runtime.action("complete action", "button", {
      complete: async () => {
        completionProbes += 1;
        return actions.length > 0 ? { state: "done" } : null;
      },
    });
    assert.deepEqual(actions, ["button"]);
    assert.deepEqual(value, { state: "done" });
    assert.equal(completionProbes, 3);
  } finally {
    rmSync(artifactDir, { recursive: true, force: true });
  }
});

test("repeatable actions retain one proven target and reject forged handles", async () => {
  const target = {
    test_id: "parity:playback-play-toggle",
    enabled: true,
    bounds: "[10,20][30,40]",
    semantic_path: "0/1/2",
  };
  const actions = [];
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-repeatable-action-"));
  const runtime = createJourneyRuntime({
    journey: { id: "repeatable-action-contract", assertions: [] },
    platform: "android",
    driver: {
      async readAction() { return target; },
      async performAction(actionId, evidence) { actions.push(["initial", actionId, evidence]); },
      async readRepeatedAction(actionId, evidence) {
        assert.equal(actionId, "playback-play-toggle");
        assert.equal(evidence, target);
        return evidence;
      },
      async performRepeatedAction(actionId, retained, evidence) {
        actions.push(["repeat", actionId, retained, evidence]);
      },
      async captureFrame() {},
    },
    fixture: null,
    artifactDir,
  });
  try {
    const first = await runtime.repeatableAction("play", "playback-play-toggle", {
      complete: async () => actions.length === 1 ? "playing" : null,
    });
    assert.equal(first.value, "playing");
    assert.equal(actions[0][2], target);

    const second = await runtime.repeatAction("pause", first.handle, {
      complete: async () => actions.length === 2 ? "paused" : null,
    });
    assert.equal(second, "paused");
    assert.deepEqual(actions[1], [
      "repeat", "playback-play-toggle", target, target,
    ]);
    await assert.rejects(
      runtime.repeatAction("forged", { actionId: "playback-play-toggle" }, {
        complete: async () => true,
      }),
      /unknown repeatable action handle/,
    );
  } finally {
    rmSync(artifactDir, { recursive: true, force: true });
  }
});

test("semantic text editing acts on the exact readiness evidence", async () => {
  const readyElement = { test_id: "route-input", enabled: true, value: "" };
  let currentValue = "";
  let focused = false;
  let focusEvidence = null;
  let actionEvidence = null;
  const result = await editSemanticText({
    async readElement() {
      return { ...readyElement, focused, value: currentValue };
    },
    async focusText(_controlId, evidence) {
      focusEvidence = evidence;
      focused = true;
    },
    async enterText(_controlId, value, _options, evidence) {
      actionEvidence = evidence;
      currentValue = value;
    },
  }, "edit route", "route-input", "KSEA KPAE");
  assert.equal(focusEvidence.test_id, readyElement.test_id);
  assert.equal(actionEvidence.test_id, readyElement.test_id);
  assert.equal(actionEvidence.focused, true);
  assert.equal(result.value, "KSEA KPAE");
});

test("semantic text editing uses action-ready reads before every mutation", async () => {
  const readyElement = { test_id: "route-input", enabled: true, value: "" };
  let focused = false;
  let currentValue = "";
  let broadReads = 0;
  const focusEvidence = [];
  const actionEvidence = [];
  const result = await editSemanticText({
    async readElement() {
      broadReads += 1;
      return { ...readyElement, focused: false, value: "stale" };
    },
    async readTextElement() {
      return { ...readyElement, focused, value: currentValue };
    },
    async focusText(_controlId, evidence) {
      focusEvidence.push(evidence);
      focused = true;
    },
    async enterText(_controlId, value, _options, evidence) {
      actionEvidence.push(evidence);
      currentValue = value;
    },
  }, "edit route", "route-input", "KSEA KPAE");
  assert.equal(broadReads, 0);
  assert.equal(focusEvidence.length, 1);
  assert.equal(actionEvidence.length, 1);
  assert.equal(result.value, "KSEA KPAE");

  const source = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const method = source.slice(
    source.indexOf("export async function editSemanticText"),
    source.indexOf("export async function inspectSemanticMapAt"),
  );
  assert.doesNotMatch(method, /discoverTextElement/);
  assert.match(method, /let current = await readTextElement\(controlId\)/);
  assert.match(method, /readinessSamples: semanticActionReadinessSamples\(driver\)/);
  assert.match(method, /ready: async \(\) => \{[\s\S]*readTextElement\(controlId\)/);
  assert.match(method, /complete: async \(\) => \{[\s\S]*readTextElement\(controlId\)/);
});

test("Android text completion reacquires an exact input moved by the keyboard", () => {
  const source = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const method = source.slice(
    source.lastIndexOf("  async readTextElement(elementId)"),
    source.lastIndexOf("  async readModal(modalId)"),
  );
  assert.match(method, /queryAndroidExactProjection/);
  assert.match(method, /verifyReachable: true/);
  assert.match(
    method,
    /candidate\.visible === "true" && candidate\["center-reachable"\] === "true"/,
  );
  assert.doesNotMatch(method, /boundedOnly: true/);
  assert.doesNotMatch(method, /queryFirstAndroidSemanticNode|dumpAndroid/);
  const service = readFileSync(new URL(
    "../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java",
    import.meta.url,
  ), "utf8");
  assert.match(service, /value\.put\("focused", Boolean\.toString\(node\.isFocused\(\)\)\)/);
  assert.match(service, /value\.put\("semantic-path", semanticPath\)/);
});

test("Android text focus uses the same verified timed gesture path as a user", () => {
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const journeyDriver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const service = readFileSync(new URL(
    "../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java",
    import.meta.url,
  ), "utf8");
  assert.match(harness, /semanticDriverActionRequest\(port, `\/tap\?\$\{query\}`\)/);
  assert.match(journeyDriver, /async focusText[\s\S]*focusAndroidSemanticNode\(/);
  assert.match(service, /case "\/tap"/);
  assert.match(service, /handleTap[\s\S]*renderedTapBounds\(tag, bounds, semanticPath\)/);
  assert.match(service, /dispatchTapGesture[\s\S]*dispatchGesture/);
  assert.doesNotMatch(harness, /"shell", "input", "tap"/);
  assert.doesNotMatch(service, /case "\/focus"|focusRenderedNode|focusMatchingNode/);
});

test("journey failures retain bounded observation diagnostics", async () => {
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-observation-diagnostic-"));
  const runtime = createJourneyRuntime({
    journey: { id: "observation-diagnostic", assertions: [] },
    platform: "test",
    driver: { async captureFrame() {} },
    fixture: null,
    artifactDir,
  });
  try {
    const error = new ObservationTimeoutError("selected viewport", 3000, {
      attempts: 12,
      last_value: "chart:expected",
    });
    const result = await runtime.finish(error);
    assert.deepEqual(result.diagnostics.failure_observation, error.diagnostics);
  } finally {
    rmSync(artifactDir, { recursive: true, force: true });
  }
});

test("journey runtime exposes typed phases instead of a generic step callback", async () => {
  const calls = [];
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-typed-phases-"));
  const runtime = createJourneyRuntime({
    journey: { id: "typed-phases", assertions: [] },
    platform: "web",
    driver: {
      async reset() { calls.push("reset"); },
      async resetApplicationData() { calls.push("reset-data"); },
      async resetApplicationDataExpectingStartupFailure() { calls.push("reset-data-failure"); },
      async reload() { calls.push("reload"); },
      async revealElement(id) { calls.push(`reveal:${id}`); return { id }; },
      async revealProjectionMatching(probe, needle) {
        calls.push(`projection:${probe}:${needle}`);
        return { probe, needle };
      },
      async captureFrame() {},
    },
    fixture: null,
    artifactDir,
  });
  try {
    assert.equal(runtime.step, undefined);
    await runtime.reset();
    await runtime.resetApplicationData();
    await runtime.resetApplicationDataExpectingStartupFailure();
    await runtime.reload();
    await runtime.revealElement("button");
    await runtime.revealProjectionMatching("row:", "KSEA");
    assert.deepEqual(calls, [
      "reset", "reset-data", "reset-data-failure", "reload", "reveal:button", "projection:row::KSEA",
    ]);
  } finally {
    rmSync(artifactDir, { recursive: true, force: true });
  }
});

test("journey choices retry transient discovery before launcher and selection actions", async () => {
  const calls = [];
  let state = "closed";
  let optionReads = 0;
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-choice-contract-"));
  const runtime = createJourneyRuntime({
    journey: { id: "choice-contract", assertions: [] },
    platform: "web",
    driver: {
      async readAction(id) { return id === "launcher" && state === "closed" ? { id } : null; },
      async openChooser(id) { calls.push(["open", id]); state = "open"; },
      async readOption(launcher, option) {
        optionReads += 1;
        if (optionReads === 1) {
          throw new TransientObservationError("semantic tree busy");
        }
        return launcher === "launcher" && option === "choice" && state === "open"
          ? { launcher, option }
          : null;
      },
      async selectOption(launcher, option) {
        calls.push(["select", launcher, option]);
        state = "selected";
      },
      async captureFrame() {},
    },
    fixture: null,
    artifactDir,
  });
  try {
    const selected = await runtime.chooseOption("choose value", "launcher", "choice", {
      complete: async () => state === "selected" ? { state } : null,
    });
    assert.deepEqual(calls, [
      ["open", "launcher"],
      ["select", "launcher", "choice"],
    ]);
    assert.ok(optionReads >= 3);
    assert.deepEqual(selected, { state: "selected" });
  } finally {
    rmSync(artifactDir, { recursive: true, force: true });
  }
});

test("Android offline startup completion uses fixed and exact projections instead of hierarchy dumps", () => {
  const source = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const bootstrap = source.slice(
    source.indexOf("async function ensureOfflinePackagesReady"),
    source.indexOf("async function waitForRuntime"),
  );
  assert.match(bootstrap, /queryAndroidStartupProjection/);
  assert.match(bootstrap, /queryAndroidRuntimeReadyForJourney/);
  assert.match(bootstrap, /queryExactAndroidNode/);
  assert.match(bootstrap, /state\?\.page === "OfflinePackages"/);
  assert.doesNotMatch(
    bootstrap,
    /dumpAndroid|queryAndroidSemanticNodes|androidRuntimeReadyForJourney/,
  );
});

test("Android plate first-open journey uses exact projections for semantic rendezvous", () => {
  const source = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const firstPaint = source.slice(
    source.indexOf("async function waitForPlateImagePainted"),
    source.indexOf("function labelContainsWords"),
  );
  const plateOpen = source.slice(
    source.indexOf("async function openPlateFromAirportInspector"),
    source.indexOf("async function ensureMapFollowEngaged"),
  );
  for (const implementation of [firstPaint, plateOpen]) {
    assert.match(implementation, /queryExactAndroidNode/);
    assert.doesNotMatch(implementation, /dumpAndroid|queryAndroidSemanticNodes/);
  }
});

test("Android plate raster qualification isolates the noisy Google image from GNSS", () => {
  const source = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const journey = source.slice(
    source.indexOf("async function runPlateFirstRenderSmoke"),
    source.indexOf("async function launchReleaseJourneyAndroidApp"),
  );
  assert.match(
    journey,
    /"cmd", "location", "set-location-enabled", "false"[\s\S]*launchFreshAndroidApp/,
  );
});

test("Android disclaimer bootstrap observes startup before querying its popup action", () => {
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const disclaimer = harness.slice(
    harness.indexOf("export async function acceptDisclaimerIfPresent"),
    harness.indexOf("export function assertRuntimeIsAvailable"),
  );
  assert.match(disclaimer, /observeUntil\("initial mandatory disclaimer state"/);
  assert.match(disclaimer, /queryAndroidSemanticNodes/);
  assert.doesNotMatch(disclaimer, /dumpAndroid/);
});

test("toggle choices acknowledge state on their visible chooser surface", async () => {
  const calls = [];
  let state = "closed";
  const artifactDir = mkdtempSync(join(tmpdir(), "aerobag-toggle-choice-contract-"));
  const runtime = createJourneyRuntime({
    journey: { id: "toggle-choice-contract", assertions: [] },
    platform: "android",
    driver: {
      async readAction(id) { return id === "launcher" && state === "closed" ? { id } : null; },
      async openChooser(id) { calls.push(["open", id]); state = "open"; },
      async readOption(launcher, option) {
        if (launcher !== "launcher" || option !== "choice" || state === "closed") return null;
        return { launcher, option, checked: state === "selected" };
      },
      async selectOption(launcher, option) {
        calls.push(["select", launcher, option]);
        state = "selected";
      },
      async captureFrame() {},
    },
    fixture: null,
    artifactDir,
  });
  try {
    const selected = await runtime.toggleOption("toggle value", "launcher", "choice", true);
    assert.equal(semanticOptionSelected(selected), true);
    assert.deepEqual(calls, [
      ["open", "launcher"],
      ["select", "launcher", "choice"],
    ]);
  } finally {
    rmSync(artifactDir, { recursive: true, force: true });
  }
});

test("mutable flight-plan column labels are not part of semantic identities", () => {
  const android = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/PlanDisplayWidgets.kt", import.meta.url),
    "utf8",
  );
  const web = readFileSync(new URL("../../ui/web-app/src/App.tsx", import.meta.url), "utf8");
  assert.match(android, /testTag\("parity:plan-column:\$\{column\.id\}"\)/);
  assert.match(android, /stateDescription = column\.label/);
  assert.doesNotMatch(android, /parity:plan-column:\$\{column\.id\}:\$\{column\.label\}/);
  assert.match(web, /data-testid=\{`parity:plan-column:\$\{column\.id\}`\}/);
  assert.match(web, /data-e2e-state=\{column\.label\}/);
  assert.doesNotMatch(web, /parity:plan-column:\$\{column\.id\}:\$\{column\.label\}/);
});

test("chart search selection delivers one platform tap and observes its result", async () => {
  let selected = false;
  let taps = 0;
  const runtime = {
    driver: {
      async readProjection(id) {
        if (id === "parity:map-selection-selected:KSEA") return selected ? [{ id }] : [];
        if (id === "chart-search-suggestion-KSEA") return selected ? [] : [{ id }];
        return [];
      },
      async readElement(id) {
        return id === "map-selection-tray" && selected ? { id } : null;
      },
      async performAction(id) {
        assert.equal(id, "chart-search-suggestion-KSEA");
        taps += 1;
        selected = true;
      },
    },
    async action(_label, actionId, { complete }) {
      const ready = { test_id: actionId, enabled: true };
      await this.driver.performAction(actionId, ready);
      return complete();
    },
  };
  assert.deepEqual(
    await selectChartSearchSuggestion(runtime, "KSEA"),
    { id: "parity:map-selection-selected:KSEA" },
  );
  assert.equal(taps, 1);
});

test("raster readiness measures painted coverage without hiding reported failures", () => {
  assert.deepEqual(
    rasterStateFromProjection([{
      id: "parity:raster-state:plan:42:maps:tac%3Anw,tac-reference:planned:18:loaded:14:failed:0",
    }]),
    {
      planId: "42",
      mapIds: ["tac:nw", "tac-reference"],
      planned: 18,
      loaded: 14,
      failed: 0,
    },
  );
  assert.equal(rasterPlanHasVisiblePaint({ planned: 18, loaded: 1, failed: 0 }), true);
  assert.equal(rasterPlanHasVisiblePaint({ planned: 18, loaded: 0, failed: 0 }), false);
  assert.equal(rasterPlanIsDisplayReady({ planned: 18, loaded: 17, failed: 0 }), true);
  assert.equal(rasterPlanIsDisplayReady({ planned: 18, loaded: 2, failed: 0 }), false);
  assert.equal(rasterPlanIsDisplayReady({ planned: 18, loaded: 17, failed: 1 }), true);
});

test("offline package sync completion rejects every in-flight label", () => {
  assert.equal(offlineSyncButtonIsIdle({ enabled: true, text: "APPLY CHANGES" }), true);
  assert.equal(offlineSyncButtonIsIdle({ enabled: true, text: "APPLYING (cancel)" }), false);
  assert.equal(offlineSyncButtonIsIdle({ enabled: true, text: "SYNCING" }), false);
  assert.equal(offlineSyncButtonIsIdle({ enabled: true, text: "CANCELING" }), false);
  assert.equal(offlineSyncButtonIsIdle({ enabled: false, text: "APPLY CHANGES" }), false);
});

test("offline package maintenance covers a transport that remains open without progress", () => {
  const implementation = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  const fixtureServer = readFileSync(
    new URL("./serve-release-journey-fixture.mjs", import.meta.url),
    "utf8",
  );
  assert.match(implementation, /artifact_fault: "stall-once"/);
  assert.match(implementation, /stalled artifact transfer failed closed/);
  assert.match(fixtureServer, /\["none", "drop", "stall-once"\]/);
  assert.match(fixtureServer, /stallThisRequest/);
});

test("Android track-up memory is owned above the disposable map page", () => {
  const retained = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/RetainedSession.kt", import.meta.url),
    "utf8",
  );
  const mapPage = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt", import.meta.url),
    "utf8",
  );
  assert.match(retained, /val mapOrientationMemory = MapOrientationMemory\(\)/);
  assert.match(mapPage, /mapOrientationMemory: MapOrientationMemory/);
  assert.doesNotMatch(mapPage, /remember \{ MapOrientationMemory\(\) \}/);
});

test("map-mode layer probes restore their state before replay", () => {
  const implementations = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  const journey = implementations.slice(
    implementations.indexOf("async function mapModesAndOverlays"),
    implementations.indexOf("async function inspectorDetails"),
  );
  const restoration = journey.indexOf("restore ${layerId} layer");
  const replay = journey.indexOf("await loadReplayFixture(runtime)");
  assert.ok(restoration >= 0, "map-mode journey must restore changed layers");
  assert.ok(replay > restoration, "layer restoration must complete before replay starts");
  assert.match(journey, /await dismissTrayOptions\(runtime, "dismiss restored layer choices"\)/);
});

test("map warning coverage uses deterministic mixed live-feed health", () => {
  const journey = RELEASE_JOURNEYS.find(({ id }) => id === "shared.map-modes-and-overlays");
  assert.equal(journey?.live_feed_profile, "mixed");
});

test("Android keeps the playback editor above the software keyboard", () => {
  const source = readFileSync(new URL(
    "../../ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt",
    import.meta.url,
  ), "utf8");
  const playbackStart = source.indexOf("private fun MapPlaybackWidgetOverlay");
  const playback = source.slice(playbackStart, source.indexOf("private fun RasterImageLayers", playbackStart));
  assert.match(playback, /WindowInsets\.ime\.getBottom\(this\)\.toDp\(\)/);
  assert.match(playback, /if \(!playbackSourceFocused\)/);
  assert.match(playback, /configuration\.screenHeightDp \* 0\.38f/);
  assert.match(playback, /onSourceFocusChange = \{ focused -> playbackSourceFocused = focused \}/);
  assert.match(playback, /bottom = visiblePlaybackBottomPadding/);
});

test("Android airport-info popups export their semantic identity", () => {
  const mapPage = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt", import.meta.url),
    "utf8",
  );
  const modal = mapPage.slice(mapPage.indexOf("internal fun AirportInfoModal"));
  assert.match(
    modal,
    /\.testTag\("parity:airport-info-modal:\$\{detail\.airportId\}"\)\s*\.semantics \{ testTagsAsResourceId = true \}/,
  );
  assert.match(
    modal,
    /\.testTag\("parity:airport-info-scroll:\$\{scrollState\.value\}"\)\s*\.verticalScroll\(scrollState\)/,
  );
  assert.match(
    modal,
    /E2eProjectionView\(\s*viewId = R\.id\.e2e_airport_info_scroll_projection,\s*state = scrollState\.value\.toString\(\)/,
  );
  const driver = readFileSync(
    new URL("./semantic-journey-driver.mjs", import.meta.url),
    "utf8",
  );
  assert.match(
    driver,
    /\["parity:airport-info-scroll:", "org\.aerobag\.app:id\/e2e_airport_info_scroll_projection"\]/,
  );
  const notamModal = mapPage.slice(mapPage.indexOf("internal fun ProcedureNotamModal"));
  assert.match(
    notamModal,
    /\.testTag\("parity:procedure-notam-modal"\)\s*\.semantics \{ testTagsAsResourceId = true \}/,
  );
});

test("Android status popups export their semantic identity", () => {
  const source = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/DataStatusPage.kt", import.meta.url),
    "utf8",
  );
  const badge = source.slice(source.indexOf("internal fun DataStatusBadge"));
  assert.match(
    badge,
    /\.testTag\("parity:\$testTagPrefix-panel"\)\s*\.semantics \{ testTagsAsResourceId = true \}/,
  );
});

test("Android altitude unavailability is semantically observable", () => {
  const source = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/AltitudePlannerPage.kt", import.meta.url),
    "utf8",
  );
  assert.match(
    source,
    /messages = planner\.unavailableReasons\.map \{ it\.message \},\s*modifier = Modifier\.testTag\("parity:altitude-planner-status"\)/,
  );
});

test("Android altitude departure basis keeps a stable action identity", () => {
  const source = readFileSync(new URL(
    "../../ui/android-app/app/src/main/java/org/aerobag/app/AltitudePlannerPage.kt",
    import.meta.url,
  ), "utf8");
  assert.match(
    source,
    /testTag = "parity:altitude-planner-departure-basis"/,
  );
  assert.doesNotMatch(source, /parity:altitude-planner-departure-basis:\$\{/);
});

test("Android flight-data settings expose their visible selected state", () => {
  const source = readFileSync(new URL(
    "../../ui/android-app/app/src/main/java/org/aerobag/app/FlightDataBanner.kt",
    import.meta.url,
  ), "utf8");
  assert.match(source, /semantics \{ selected = item\.enabled \}/);
});

test("Android semantic discovery scrolls horizontally for clipped control strips", () => {
  assert.equal(androidElementMayRequireHorizontalScroll("plan-control:undo"), true);
  assert.equal(androidElementMayRequireHorizontalScroll("altitude-planner-control:wind_model"), true);
  assert.equal(androidElementMayRequireHorizontalScroll("altitude-planner-departure-basis"), true);
  assert.equal(androidElementMayRequireHorizontalScroll("settings-toggle-debug_internet_adsb"), false);
});

test("Android horizontal controls use rendered hit geometry instead of layout projections", () => {
  const driver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const readAction = driver.slice(
    driver.lastIndexOf("  async readAction(actionId)"),
    driver.lastIndexOf("  async readSessionRevision()"),
  );
  const reveal = driver.slice(
    driver.lastIndexOf("  async revealElement(elementId)"),
    driver.lastIndexOf("  async reload()"),
  );
  assert.match(readAction, /renderedOnly = androidElementMayRequireHorizontalScroll\(actionId\)/);
  assert.match(readAction, /renderedOnly,/);
  assert.match(reveal, /renderedOnly = androidElementMayRequireHorizontalScroll\(elementId\)/);
  assert.match(reveal, /requireReachable: true,[\s\S]*renderedOnly,[\s\S]*avoidNavigation/);
  assert.match(harness, /rendered_only: String\(renderedOnly\)/);
  assert.match(harness, /verify_reachable: String\(verifyReachable\)/);
  assert.match(harness, /avoid_navigation: String\(avoidNavigation\)/);
  assert.match(
    service,
    /renderedOnly\s*\? ProviderProjection\.unhandled\(\)\s*:\s*providerProjection\(tag, verifyReachable, avoidNavigation\)/,
  );
});

test("Android provider readiness never traverses the accessibility tree", () => {
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const reachability = service.slice(
    service.indexOf("private boolean projectedCenterReachable"),
    service.indexOf("private Rect indexedBounds"),
  );
  assert.match(reachability, /physicalDisplayBounds\(\)/);
  assert.match(service, /getDefaultDisplay\(\)\.getRealSize\(size\)/);
  assert.match(reachability, /private boolean projectedCenterClearOfNavigation/);
  assert.match(reachability, /indexedBounds\("parity:primary-navigation"\)/);
  assert.match(reachability, /tag\.startsWith\("parity:button:"\)/);
  assert.doesNotMatch(reachability, /AccessibilityNodeInfo|getRootInActiveWindow|findRendered/);
  assert.doesNotMatch(service, /findRenderedNodeInInputWindow/);
});

test("Android page navigation requires visible semantic pages", () => {
  const source = readFileSync(
    new URL("./semantic-journey-driver.mjs", import.meta.url),
    "utf8",
  );
  assert.match(source, /function visibleAndroidPage/);
  assert.match(source, /visible page before navigation/);
  assert.match(source, /requireVisible: true/);
  assert.match(source, /export async function navigateSemanticPage/);
  const androidDriver = source.slice(source.indexOf("export class AndroidSemanticJourneyDriver"));
  const readPage = androidDriver.slice(
    androidDriver.indexOf("async readPage(pageId)"),
    androidDriver.indexOf("async readNavigationAction(pageId)"),
  );
  const currentPage = source.slice(
    source.indexOf("function visibleAndroidPage"),
    source.indexOf("export function androidZoomKeyCode"),
  );
  assert.match(currentPage, /queryAndroidStartupProjection\(serial\)/);
  assert.doesNotMatch(currentPage, /queryAndroidSemanticNodes/);
  assert.match(readPage, /const current = visibleAndroidPage\(this\.serial\)/);
  assert.match(readPage, /current\?\.pageId !== pageId/);
  assert.match(readPage, /queryFirstAndroidSemanticNode/);
  assert.match(readPage, /includeDescendantText: false/);
});

test("Android first-node probes stop semantic traversal at the first match", () => {
  const source = readFileSync(
    new URL("./semantic-journey-driver.mjs", import.meta.url),
    "utf8",
  );
  const queryFirst = source.slice(
    source.indexOf("function queryFirstAndroidSemanticNode"),
    source.indexOf("function readinessEvidenceMatchesTag"),
  );
  assert.match(queryFirst, /allowPrefix = false/);
  assert.match(queryFirst, /queryAndroidExactProjection/);
  assert.match(
    queryFirst,
    /includeDescendantText,[\s\S]*renderedOnly,[\s\S]*verifyReachable: requireReachable,[\s\S]*avoidNavigation/,
  );
  assert.doesNotMatch(queryFirst, /indexedOnly: true/);
  assert.match(queryFirst, /\{ prefix: true, first: true, includeDescendantText \}/);
});

test("Android exact rendered-control discovery is breadth-first and bounded", () => {
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const lookup = service.slice(
    service.indexOf("private boolean appendFirstExactProjectionBreadthFirst"),
    service.indexOf("private boolean appendExactProjectionAtPoint"),
  );
  assert.match(lookup, /ArrayDeque<PathNode>/);
  assert.match(lookup, /pending\.removeFirst\(\)/);
  assert.match(lookup, /pending\.addLast/);
  assert.match(lookup, /visited < EXACT_PROJECTION_NODE_LIMIT/);
  assert.match(lookup, /System\.nanoTime\(\) < deadlineNanos/);
  assert.equal((lookup.match(/appendFirstExactProjectionBreadthFirst\(/g) ?? []).length, 1);
});

test("Android exact discovery uses bounded breadth-first and depth-first strategies", () => {
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const projection = service.slice(
    service.indexOf("private JSONArray renderExactProjection"),
    service.indexOf("private ProviderProjection providerProjection"),
  );
  assert.match(projection, /appendFirstExactProjectionBreadthFirst/);
  assert.match(projection, /appendFirstExactProjectionDepthFirst/);
  const depthFirst = service.slice(
    service.indexOf("private boolean appendFirstExactProjectionDepthFirst"),
    service.indexOf("private static void recyclePathNodes"),
  );
  assert.match(depthFirst, /System\.nanoTime\(\) >= deadlineNanos/);
  assert.match(depthFirst, /visited\[0\] >= EXACT_PROJECTION_NODE_LIMIT \/ 2/);
});

test("Android journey controls publish indexed geometry through the private E2E provider", () => {
  const projection = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/E2eProjectionView.kt", import.meta.url),
    "utf8",
  );
  const provider = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/E2eProjectionProvider.kt", import.meta.url),
    "utf8",
  );
  const flightPlan = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/FlightPlanPage.kt", import.meta.url),
    "utf8",
  );
  const settings = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/SettingsPage.kt", import.meta.url),
    "utf8",
  );
  const commonWidgets = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/DebugAndCommonWidgets.kt", import.meta.url),
    "utf8",
  );
  const charts = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/ChartsPage.kt", import.meta.url),
    "utf8",
  );
  const mapExplorer = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt", import.meta.url),
    "utf8",
  );
  const playback = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/PlaybackWidget.kt", import.meta.url),
    "utf8",
  );
  const cloud = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/CloudPage.kt", import.meta.url),
    "utf8",
  );
  assert.match(projection, /fun Modifier\.e2eIndexedControl/);
  assert.match(projection, /fun Modifier\.e2eIndexedTextControl/);
  assert.match(projection, /kind:text:text:\$\{Uri\.encode\(text\)\}:enabled:\$enabled:focused:\$focused/);
  assert.match(projection, /positionOnScreen\(\)/);
  assert.doesNotMatch(projection, /boundsInWindow\(\)/);
  assert.match(provider, /snapshot\.bounds/);
  assert.match(provider, /!knownSemanticControl && snapshot == null/);
  assert.match(provider, /if \(snapshot == null\) 0 else 1/);
  assert.match(flightPlan, /semanticTag = "parity:plan-append-route-input"/);
  assert.match(settings, /semanticTag = "parity:settings-section:\$\{section\.id\}"/);
  assert.match(commonWidgets, /semanticTag = resolvedTestTag/);
  assert.match(commonWidgets, /text:\$\{Uri\.encode\(renderedLabel\)\}/);
  assert.match(charts, /semanticTag = testTag/);
  assert.match(charts, /semanticTag = "parity:primary-navigation"/);
  assert.match(charts, /e2eIndexedTextControl\(\s*semanticTag = "parity:chart-search-input"/);
  assert.match(flightPlan, /e2eIndexedTextControl\(\s*semanticTag = "parity:plan-append-route-input"/);
  assert.match(mapExplorer, /e2eIndexedTextControl\(\s*semanticTag = "parity:plan-insert-airport-input"/);
  assert.match(playback, /e2eIndexedTextControl\(\s*semanticTag = "parity:playback-source-input"/);
  assert.match(cloud, /e2eIndexedTextControl\(\s*semanticTag = "parity:cloud-setup-code-input"/);
  assert.match(cloud, /testTag\("parity:cloud-panel:\$\{panel\.id\}"\)/);
  assert.match(cloud, /stateDescription = "state:\$\{panel\.state\.name\.lowercase\(\)\}"/);
  assert.doesNotMatch(cloud, /cloud-panel:\$\{panel\.id\}:state:/);
  assert.match(mapExplorer, /semanticTag = "parity:map-selection-tray"/);
  assert.match(mapExplorer, /semanticTag = "parity:map-surface"/);
  assert.match(provider, /KnownSemanticPrefixes/);
  assert.match(provider, /knownSemanticControl/);
  assert.match(provider, /fun readPrefix\(resourceIdPrefix: String\)/);
  assert.match(provider, /resourceIdPrefix !in E2eProjectionRegistry\.KnownSemanticPrefixes/);
  assert.match(charts, /semanticTag = "parity:chart-search-suggestion:\$\{suggestion\.identifier\}"/);
  assert.match(
    charts,
    /semanticTag = semanticTag,[\s\S]*text:\$\{Uri\.encode\(listOfNotNull\(suggestion\.identifier, friendlyName\)/,
  );
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  assert.match(service, /semanticPath\.startsWith\("projection-provider:"\)/);
  const prefixQuery = service.slice(
    service.indexOf("private void handleQuery"),
    service.indexOf("private void handleExactProjection"),
  );
  assert.match(prefixQuery, /providerProjectionPrefix\(tag\)/);
  assert.ok(
    prefixQuery.indexOf("providerProjectionPrefix(tag)") < prefixQuery.indexOf("renderNodeQuery("),
    "known prefix queries must bypass accessibility traversal",
  );
  assert.match(service, /currentBounds == null \|\| !expectedBounds\.equals\(currentBounds\)/);
  assert.match(service, /projectedCenterReachable\(parsedBounds\)/);
  assert.match(service, /projectedCenterClearOfNavigation\(snapshot\.resourceId, parsedBounds\)/);
  assert.match(service, /indexedBounds\("parity:primary-navigation"\)/);
  assert.match(
    service,
    /return tag\.startsWith\("parity:button:"\) && navigationBounds\.contains\(bounds\)/,
  );
});

test("Android chooser options use the authoritative app-owned control index", () => {
  const harness = readFileSync(new URL("android-harness.mjs", import.meta.url), "utf8");
  const driver = readFileSync(new URL("semantic-journey-driver.mjs", import.meta.url), "utf8");
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  assert.match(harness, /provider_only: String\(providerOnly\)/);
  const option = driver.slice(
    driver.lastIndexOf("  async readOption(launcherId, optionId)"),
    driver.lastIndexOf("  async selectOption(launcherId, optionId, readyElement)"),
  );
  assert.match(option, /queryAndroidExactProjection/);
  assert.match(option, /providerOnly: true/);
  assert.doesNotMatch(option, /queryFirstAndroidSemanticNode/);
  assert.match(service, /if \(tag\.isEmpty\(\) \|\| providerOnly\) return output/);
});

test("Android projection-provider IPC is bounded and leaves failure evidence", () => {
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const harness = readFileSync(new URL("android-harness.mjs", import.meta.url), "utf8");
  const suite = readFileSync(new URL("run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const providerSnapshot = service.slice(
    service.indexOf("private ProviderSnapshot providerSnapshot"),
    service.indexOf("private boolean projectedCenterReachable"),
  );
  assert.match(providerSnapshot, /CancellationSignal cancellationSignal/);
  assert.match(providerSnapshot, /PROVIDER_QUERY_TIMEOUT_MS/);
  assert.match(providerSnapshot, /OperationCanceledException/);
  assert.match(providerSnapshot, /ProviderSnapshot\.handledAbsent\(\)/);
  assert.match(service, /case "\/request-state"/);
  const captures = harness.slice(
    harness.indexOf("export function captureAndroidFailureDiagnostics"),
    harness.indexOf("export function decodeXml"),
  );
  assert.ok(captures.indexOf('"semantic-driver.json"') < captures.indexOf('"logcat.txt"'));
  assert.ok(captures.indexOf('"logcat.txt"') < captures.indexOf('"ui.xml"'));
  assert.match(suite, /captureAndroidFailureDiagnostics\(args\.serial, artifactDir, journey\.id\)/);
  assert.match(suite, /persistJourneyResult\(error\.journeyResult, artifactDir\)/);
});

test("Android semantic taps validate current controls before one timed input gesture", () => {
  const harness = readFileSync(
    new URL("android-harness.mjs", import.meta.url),
    "utf8",
  );
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const click = harness.slice(
    harness.indexOf("export function clickAndroidSemanticNode"),
    harness.indexOf("export function focusAndroidSemanticNode"),
  );
  assert.match(click, /new URLSearchParams\(\{\s*tag,\s*bounds: expectedBounds,\s*path: semanticPath/s);
  assert.match(service, /renderedTapBounds\(tag, bounds, semanticPath\)/);
  assert.match(service, /resolveRenderedNode\(tag, expectedBounds, semanticPath\)/);
  const providerTapStart = service.indexOf(
    'if (semanticPath.startsWith("projection-provider:"))',
  );
  const providerTap = service.slice(
    providerTapStart,
    service.indexOf('AccessibilityNodeInfo node = resolveRenderedNode', providerTapStart),
  );
  assert.match(providerTap, /providerProjection\(tag, true\)/);
  assert.match(providerTap, /return currentBounds/);
  assert.doesNotMatch(providerTap, /resolveRenderedNode|AccessibilityNodeInfo/);
  assert.doesNotMatch(click, /"shell", "input", "tap"/);
  assert.equal((click.match(/target = androidPhysicalTapTarget/g) ?? []).length, 1);
  assert.match(click, /queryAndroidExactProjection\([\s\S]*providerOnly: true/);
  assert.match(click, /for \(let attempt = 0; attempt < 4; attempt \+= 1\)/);
  assert.match(click, /waitForAndroidSemanticEvent\(serial, 250\)/);
  assert.match(click, /if \(!refreshed\) continue/);
  assert.match(service, /new GestureDescription\.StrokeDescription\(path, 0, 80\)/);
  assert.match(service, /dispatchGesture/);
  assert.doesNotMatch(service, /GestureResultCallback/);
  assert.match(service, /ACTION_UP receipt is the authoritative completion signal/);
  assert.doesNotMatch(service, /ACTION_CLICK/);
});

test("Android stale indexed targets separate semantic state from temporary actionability", () => {
  const current = {
    "resource-id": "parity:button:HOME",
    "semantic-path": "projection-provider:42",
    bounds: "[10,20][30,40]",
    visible: "true",
    "center-reachable": "true",
    enabled: "true",
    selected: "false",
    checked: "false",
    "state-description": "enabled:true:selected:false:text:HOME:window-focus:true",
  };
  const expected = {
    enabled: true,
    selected: false,
    checked: false,
    stateDescription: "enabled:true:selected:false:text:HOME:window-focus:false",
  };
  assert.equal(
    androidSemanticTargetStateMatches("parity:button:HOME", current, expected),
    true,
    "window focus is delivery metadata, not a semantic action-state change",
  );
  assert.equal(
    androidSemanticReadinessStateMatches("parity:button:HOME", current, expected),
    true,
  );
  assert.equal(
    androidSemanticTargetStateMatches(
      "parity:button:HOME", { ...current, selected: "true" }, expected,
    ),
    false,
  );
  assert.equal(
    androidSemanticTargetStateMatches(
      "parity:button:HOME", { ...current, "resource-id": "parity:button:PLATE" }, expected,
    ),
    false,
  );
  assert.equal(
    androidSemanticTargetStateMatches(
      "parity:button:HOME",
      { ...current, "state-description": "enabled:true:selected:false:text:PLATE:window-focus:true" },
      expected,
    ),
    false,
  );
  assert.equal(
    androidSemanticTargetStateMatches(
      "parity:button:HOME", { ...current, "center-reachable": "false" }, expected,
    ),
    true,
  );
  assert.equal(
    androidSemanticReadinessStateMatches(
      "parity:button:HOME", { ...current, "center-reachable": "false" }, expected,
    ),
    false,
  );
});

test("Android current-page discovery follows the visible page before persisted state", () => {
  assert.equal(
    androidPageIdFromStartupStateTag(
      "parity:startup-state:ready:true:page:Home:persisted_page:Map:session_revision:44",
    ),
    "home",
  );
  assert.equal(
    androidPageIdFromStartupStateTag(
      "parity:startup-state:ready:true:persisted_page:Plan:session_revision:45",
    ),
    "flight_plan",
  );
  assert.equal(androidPageIdFromStartupStateTag("parity:startup-state:ready:false"), null);
});

test("shared settings-choice probes translate to Android's core-owned row IDs", () => {
  assert.equal(
    androidSemanticTag("settings-choice-flight_data_visibility"),
    "parity:settings-choice:flight_data_visibility",
  );
});

test("Android identifies projections whose visible cards can be explicitly scanned", () => {
  assert.equal(androidProjectionMayRequireVerticalScan("parity:data-status-row:"), true);
  assert.equal(androidProjectionMayRequireVerticalScan("parity:offline-"), true);
  assert.equal(androidProjectionMayRequireVerticalScan("parity:offline-region:"), true);
  assert.equal(androidProjectionMayRequireVerticalScan("parity:offline-product:"), true);
  assert.equal(androidProjectionMayRequireVerticalScan("parity:plan-row:"), false);
});

test("Android exposes virtualized Data Status state without scrolling every card", () => {
  assert.deepEqual(
    androidDataStatusRowsFromStateTag(
      "parity:data-status-state:client=ok|live_feed:notams=stale",
    ).map((row) => row.id),
    [
      "parity:data-status-row:client:severity:ok",
      "parity:data-status-row:live_feed:notams:severity:stale",
    ],
  );
  const page = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/DataStatusPage.kt", import.meta.url),
    "utf8",
  );
  const driver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  assert.match(page, /E2eProjectionView\([\s\S]*R\.id\.e2e_data_status_projection/);
  assert.doesNotMatch(page, /prefix = "parity:data-status-state:"/);
  assert.match(
    driver,
    /\["parity:data-status-state:", "org\.aerobag\.app:id\/e2e_data_status_projection"\]/,
  );
  assert.match(
    driver,
    /readScalarProjection\("parity:data-status-state:"\)/,
  );
});

test("projection observations are distinct from explicit UI traversal", () => {
  assert.ok(SEMANTIC_DRIVER_OPERATIONS.includes("readProjection"));
  assert.ok(SEMANTIC_DRIVER_OPERATIONS.includes("scanProjection"));
  assert.ok(SEMANTIC_DRIVER_OPERATIONS.includes("findProjectionMatching"));
  assert.ok(SEMANTIC_DRIVER_OPERATIONS.includes("revealProjectionMatching"));
});

test("Android map-selection projections preserve dynamic visible inspector text", () => {
  const kseaState =
    "selected:KSEA:category:airport:text:KSEA · Elev 433 · 7nm Seattle%2C WA:centered:KSEA:offset-px:2";
  assert.deepEqual(androidMapSelectionEntryFromState(kseaState, "KSEA"), {
    id: "parity:map-selection-selected:KSEA",
    text: "KSEA · Elev 433 · 7nm Seattle, WA",
    enabled: true,
    pressed: null,
    state: kseaState,
  });
  assert.equal(androidMapSelectionEntryFromState(kseaState, "KPAE"), null);

  const spotState =
    "selected:SPOT:category:spot:text:SPOT · 2nm Terrain elevation 125 ft:centered:none:offset-px:none";
  assert.equal(
    androidMapSelectionEntryFromState(spotState)?.text,
    "SPOT · 2nm Terrain elevation 125 ft",
  );
  assert.equal(
    androidMapSelectionEntryFromState(
      "selected:none:category:none:text::centered:none:offset-px:none",
    ),
    null,
  );
});

test("Android observes a named map selection through its bounded scalar projection", () => {
  const source = readFileSync(
    new URL("./semantic-journey-driver.mjs", import.meta.url),
    "utf8",
  );
  const selectionBranch = source.slice(
    source.indexOf('if (prefix.startsWith("parity:map-selection-selected:"))'),
    source.indexOf('if (ANDROID_EXACT_SCALAR_PROJECTIONS.has(prefix))'),
  );
  assert.match(selectionBranch, /this\.readScalarProjection\("parity:map-selection-state:"\)/);
  assert.doesNotMatch(selectionBranch, /queryAndroidExactProjection\(this\.serial, prefix/);
});

test("Android modal presence and absence use only the fixed scalar projection", () => {
  const source = readFileSync(
    new URL("./semantic-journey-driver.mjs", import.meta.url),
    "utf8",
  );
  const method = source.slice(
    source.lastIndexOf("  async readModal(modalId)"),
    source.lastIndexOf("  async revealElement(elementId)"),
  );
  assert.match(method, /this\.readScalarProjection\("parity:map-selection-state:"\)/);
  assert.match(method, /detailId === modalId/);
  assert.doesNotMatch(method, /queryFirstAndroidSemanticNode|dumpAndroid/);
});

test("fixed E2E scalar projections stay above transient Compose overlays", () => {
  const source = readFileSync(new URL(
    "../../ui/android-app/app/src/main/java/org/aerobag/app/E2eProjectionView.kt",
    import.meta.url,
  ), "utf8");
  assert.match(source, /\.requiredSize\(1\.dp\)/);
  assert.match(source, /\.zIndex\(Float\.MAX_VALUE\)/);
  assert.match(source, /val resourceId = "org\.aerobag\.app:id\/\$resourceName"/);
  assert.match(source, /\.testTag\(resourceId\)/);
  assert.match(source, /testTagsAsResourceId = true/);
  assert.match(source, /stateDescription = state/);
});

test("map scalar projections occupy distinct accessibility bounds", () => {
  const source = readFileSync(new URL(
    "../../ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt",
    import.meta.url,
  ), "utf8");
  const mapBody = source.slice(
    source.indexOf("viewId = R.id.e2e_viewport_projection"),
    source.indexOf("MapPlaybackWidgetOverlay("),
  );
  const positions = new Map();
  for (const projection of [
    "viewport", "live_overlay", "nexrad_state", "map_selection",
    "ownship_state", "map_follow", "playback_widget",
  ]) {
    const body = mapBody.slice(mapBody.indexOf(`viewId = R.id.e2e_${projection}_projection`));
    const nextProjection = body.indexOf("viewId = R.id.e2e_", 1);
    const call = nextProjection < 0 ? body : body.slice(0, nextProjection);
    const offset = /\.offset\(x = (\d+)\.dp\)/.exec(call)?.[1];
    assert.ok(offset, `${projection} must have an explicit accessibility slot`);
    assert.equal(positions.has(offset), false, `${projection} overlaps ${positions.get(offset)} at ${offset}.dp`);
    positions.set(offset, projection);
  }
});

test("Android scalar projections traverse once and then use only their proven path", () => {
  const source = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const method = source.slice(
    source.indexOf("  readScalarProjection(prefix)"),
    source.indexOf("  async waitForObservation(intervalMs)"),
  );
  assert.match(method, /this\.seededScalarProjections\.has\(semanticTag\)/);
  assert.match(method, /\{ boundedOnly \}/);
  assert.match(method, /if \(boundedOnly && queried\.length === 0\)/);
  assert.match(method, /this\.seededScalarProjections\.delete\(semanticTag\)/);
  assert.match(method, /if \(queried\.length > 0\) this\.seededScalarProjections\.add\(semanticTag\)/);
});

test("Android action delivery cannot rediscover a different control after readiness", async () => {
  const source = readFileSync(
    new URL("./semantic-journey-driver.mjs", import.meta.url),
    "utf8",
  );
  const action = source.slice(
    source.indexOf("  async performAction(actionId, readyElement = null)"),
    source.indexOf("  async readAction(actionId)"),
  );
  assert.doesNotMatch(action, /scrollUntilTag|tapTag|queryFirstAndroidSemanticNode/);

  const driver = new AndroidSemanticJourneyDriver("test", { resetApp: async () => {} });
  await assert.rejects(
    () => driver.performAction("playback-play-toggle"),
    /has no readiness evidence/,
  );
});

test("Android drag delivery reuses exact readiness geometry", () => {
  const source = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const method = source.slice(
    source.lastIndexOf("  async drag(surfaceId"),
    source.lastIndexOf("  async setProgress(controlId"),
  );
  assert.match(method, /readinessEvidenceMatchesTag\(semanticTag, readyElement\)/);
  assert.match(method, /rectOfBounds\(readyElement\.bounds\)/);
  assert.doesNotMatch(method, /dumpAndroid|findTagOrPrefix|queryAndroid/);
});

test("product surface manifest exactly follows core-projected branches", () => {
  const summary = verifyProductSurfaceCoverage();
  assert.ok(summary.categories >= 10);
  assert.ok(summary.branches >= 80);
});

test("coverage verification rejects newly exposed and stale branches", () => {
  assert.throws(
    () => verifyProductSurfaceCoverage(
      { schema_version: 1, surfaces: { sample: { old: "home.chart" } } },
      { sample: ["new"] },
    ),
    /uncovered new[\s\S]*stale old/,
  );
});

test("journey result records semantic actions and assertions", () => {
  const result = createJourneyResult({ id: "shared.test", platform: "web" });
  recordJourneyStep(result, "open-page", "chart", 12);
  recordJourneyCheck(result, "chart.search", true, "KPAE");
  finishJourneyResult(result);
  assert.equal(validateJourneyResult(result).status, "pass");
  assert.deepEqual(result.steps[0], {
    action_id: "open-page", status: "pass", detail: "chart", duration_ms: 12,
  });
});

test("release fixture is capability-addressed and complete", () => {
  const manifest = {
    schema_version: 1,
    fixture: "release-journey-publication",
    publication_root: "published",
    capabilities: {
      reference_epoch_ms: 1_787_180_400_000,
      initial_viewport: { lat: 47.5, lon: -122.2 },
      replay_trace: "replay.jsonl",
      second_publication: "published-next",
      raster_families: ["none", "sec", "tac", "flyway", "enr-l", "enr-h", "shaded-relief"],
      airport: {
        runway_complex: "KSEA", runway_fallback: "S88",
        published_tpa: "KPAE", derived_tpa: "KPLU",
      },
      airway: { entry: "SEA", airway: "V2", exit: "PAE" },
      procedure: { sid: {}, star: {}, approach: {} },
      plate: { georeferenced: {}, multi_page_rotated: {}, notam: {}, geometry_warning: {}, legend: {}, inset: {} },
      document: { csup: {}, other: {} },
      live_feeds: { fresh: {}, mixed: {}, stale: {}, pirep_target_airport: "KSEA" },
    },
  };
  assert.equal(validateReleaseJourneyFixture(manifest).fixture, "release-journey-publication");
  delete manifest.capabilities.plate.notam;
  assert.throws(() => validateReleaseJourneyFixture(manifest), /plate\.notam/);
});

test("Android smoke fixture names a plate backed by its compact publication", () => {
  const manifest = {
    schema_version: 1,
    fixture: "android-smoke-publication",
    capabilities: {
      plate: {
        georeferenced: {
          airport_id: "KPLU",
          label_contains: "RNAV 35",
        },
      },
    },
  };
  assert.equal(validateAndroidSmokeFixture(manifest), manifest);
  delete manifest.capabilities.plate.georeferenced.label_contains;
  assert.throws(() => validateAndroidSmokeFixture(manifest), /plate label/);
});

test("Android fixture URLs use keyboard-safe reversible paths", () => {
  const relative = "replay/track-gap.json";
  const url = releaseJourneyFixtureUrl("android", relative);
  assert.match(url, /^\/releasejourney\/[0-9a-f]+$/);
  assert.equal(decodeReleaseJourneyFixturePath(url.split("/").at(-1)), relative);
  assert.equal(
    releaseJourneyFixtureUrl("web", relative, "http://fixture.test"),
    "http://fixture.test/release-journey/replay/track-gap.json",
  );
});

test("semantic drivers expose the platform-neutral journey operation set", () => {
  const driver = validateSemanticDriver(new SemanticJourneyDriver("fake"));
  assert.deepEqual(
    SEMANTIC_DRIVER_OPERATIONS.filter((operation) => typeof driver[operation] === "function"),
    SEMANTIC_DRIVER_OPERATIONS,
  );
  assert.rejects(driver.openPage("map"), /does not implement readCurrentPage/);
});

test("shared semantic navigation records each required user gesture separately", async () => {
  let pageId = "map";
  const activated = [];
  const transitions = [];
  const driver = {
    async readCurrentPage() { return { pageId }; },
    async readPage(expectedPageId) {
      return pageId === expectedPageId ? { pageId } : null;
    },
    async readNavigationAction(destination) { return { destination }; },
    async activateNavigation(destination) {
      activated.push(destination);
      pageId = destination;
    },
  };
  const transition = async (description, contract) => {
    transitions.push(description);
    assert.ok(await contract.ready());
    assert.equal(await contract.complete(), null);
    await contract.act();
    return contract.complete();
  };

  assert.deepEqual(
    await navigateSemanticPage(driver, "settings", {
      observe: async (_description, probe) => probe(),
      transition,
    }),
    { pageId: "settings" },
  );
  assert.deepEqual(activated, ["home", "settings"]);
  assert.deepEqual(transitions, ["navigate to Home", "navigate to settings"]);
});

test("shared semantic navigation does not skip an explicit Home request", async () => {
  let pageId = "map";
  const activated = [];
  const driver = {
    async readCurrentPage() { return { pageId }; },
    async readPage(expectedPageId) {
      return pageId === expectedPageId ? { pageId } : null;
    },
    async readNavigationAction(destination) { return { destination }; },
    async activateNavigation(destination) {
      activated.push(destination);
      pageId = destination;
    },
  };
  const transition = async (_description, contract) => {
    assert.equal(await contract.complete(), null);
    await contract.act();
    return contract.complete();
  };

  assert.deepEqual(
    await navigateSemanticPage(driver, "home", {
      observe: async (_description, probe) => probe(),
      transition,
    }),
    { pageId: "home" },
  );
  assert.deepEqual(activated, ["home"]);
});

test("shared semantic navigation separates page selection from rendered readiness", async () => {
  let selectedPage = "home";
  let renderProbes = 0;
  const driver = {
    async readCurrentPage() { return { pageId: selectedPage }; },
    async readPage(expectedPageId) {
      renderProbes += 1;
      return renderProbes >= 2 && selectedPage === expectedPageId
        ? { pageId: expectedPageId, rendered: true }
        : null;
    },
    async readNavigationAction(destination) { return { destination }; },
    async activateNavigation(destination) { selectedPage = destination; },
  };
  const transition = async (_description, contract) => {
    assert.ok(await contract.ready());
    await contract.act();
    return contract.complete();
  };
  const observe = async (_description, probe) => {
    for (let attempt = 0; attempt < 3; attempt += 1) {
      const value = await probe();
      if (value) return value;
    }
    return null;
  };

  assert.deepEqual(
    await navigateSemanticPage(driver, "map", { observe, transition }),
    { pageId: "map", rendered: true },
  );
  assert.equal(renderProbes, 2);
});

test("web back tolerates a tray that closes between discovery and click", async () => {
  const keyEvents = [];
  const driver = new WebSemanticJourneyDriver({
    firstExisting: async () => ".trayScrim",
    clickIfVisible: async () => false,
    page: { send: async (method, payload) => keyEvents.push({ method, payload }) },
  });
  await driver.back();
  assert.deepEqual(keyEvents.map(({ payload }) => payload.type), ["keyDown", "keyUp"]);
});

test("web action readiness and delivery share the same semantic selector", async () => {
  const selectors = [];
  const clicks = [];
  const transport = {
    async firstExisting(candidates) {
      selectors.push(candidates);
      return '[data-testid="cloud-action-begin_setup"]';
    },
    async readElement(selector) {
      return {
        test_id: "cloud-action-begin_setup",
        enabled: true,
        visible: true,
        actionable: true,
      };
    },
    async click(selector) { clicks.push(selector); },
  };
  const driver = new WebSemanticJourneyDriver(transport);

  const ready = await driver.readAction("begin_setup");
  assert.equal(ready?.enabled, true);
  await driver.performAction("begin_setup", ready);
  assert.equal(selectors.length, 1);
  for (const candidates of selectors) {
    assert.match(candidates.join("\n"), /cloud-action-begin_setup/);
  }
  assert.deepEqual(clicks, ['[data-testid="cloud-action-begin_setup"]']);
});

test("web projection reveal makes an offscreen semantic item actionable before returning", async () => {
  const calls = [];
  const driver = Object.create(WebSemanticJourneyDriver.prototype);
  driver.findProjectionMatching = async (probe, needle) => {
    calls.push(["find", probe, needle]);
    return { id: "tray-option-airport-diagram", text: "AIRPORT DIAGRAM" };
  };
  driver.revealElement = async (elementId) => {
    calls.push(["reveal", elementId]);
    return { test_id: elementId, actionable: true };
  };

  const entry = await driver.revealProjectionMatching("tray-option-", "AIRPORT DIAGRAM");

  assert.equal(entry.id, "tray-option-airport-diagram");
  assert.deepEqual(calls, [
    ["find", "tray-option-", "AIRPORT DIAGRAM"],
    ["reveal", "tray-option-airport-diagram"],
  ]);
});

test("web semantic selectors preserve JSON-valued action identities", () => {
  assert.equal(
    webTestIdSelector('tray-option-{"target":{"kind":"existing"}}'),
    '[data-testid="tray-option-{\\"target\\":{\\"kind\\":\\"existing\\"}}"]',
  );
});

test("Android semantic aliases preserve shared search suggestion ids", () => {
  assert.equal(
    androidSemanticTag("chart-search-suggestion-KSEA"),
    "parity:chart-search-suggestion:KSEA",
  );
});

test("Android aliases the shared ownship launcher to its Compose semantic tag", () => {
  assert.equal(androidElementSemanticTag("ownship-source-button"), "parity:ownship-launcher");
});

test("Android activates every tagged control through one exact semantic action path", () => {
  const driver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  assert.doesNotMatch(driver, /ActionUsesSemanticActivation/);
  assert.doesNotMatch(driver, /androidActionUsesSubmit|Android submit action/);
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  assert.match(
    service,
    /handleTap[\s\S]*bounds != null && !bounds\.isEmpty\(\)[\s\S]*renderedTapBounds\(tag, bounds, semanticPath\)/,
  );
  assert.match(
    driver,
    /activateAndroidSemanticTag[\s\S]*clickAndroidSemanticNode/,
  );
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  assert.doesNotMatch(harness, /"shell", "input", "tap"/);
  assert.match(service, /dispatchTapGesture[\s\S]*dispatchGesture/);
  assert.doesNotMatch(service, /ACTION_CLICK/);
});

test("Android release journeys retain session-work timing evidence", () => {
  const source = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt", import.meta.url),
    "utf8",
  );
  assert.match(
    source,
    /setPerfMetricsEnabled\([\s\S]*perfScenario != null \|\| BuildConfig\.AEROBAG_E2E_ENABLED/,
  );
});

test("Android semantic tree traversal absorbs concurrent Compose child replacement", () => {
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  assert.match(
    service,
    /childAtOrNull[\s\S]*catch \(IndexOutOfBoundsException error\)/,
  );
  assert.doesNotMatch(service, /AccessibilityNodeInfo child = (?:node|current)\.getChild\(/);
});

test("Android semantic driver isolates each client failure from its server loop", () => {
  const source = readFileSync(new URL(
    "../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java",
    import.meta.url,
  ), "utf8");
  const serverLoop = source.slice(source.indexOf("private void serve()"), source.indexOf("private void handleRequest"));
  assert.match(source, /Executors\.newFixedThreadPool\(4\)/);
  assert.match(serverLoop, /clientExecutor\.execute\(\(\) -> handleClient\(client\)\)/);
  assert.match(serverLoop, /try \{\s*handleRequest\(client\);\s*\} catch \(IOException error\)/);
  assert.match(serverLoop, /catch \(RuntimeException error\)[\s\S]*respondFailureBestEffort\(client, error\)/);
  assert.doesNotMatch(serverLoop, /catch \(IOException error\)[\s\S]*throw new RuntimeException\(error\);/);
});

test("Android semantic probes cannot create an accessibility traversal herd", () => {
  const service = readFileSync(new URL(
    "../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java",
    import.meta.url,
  ), "utf8");
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  assert.match(service, /AtomicBoolean semanticRequestActive/);
  assert.match(service, /semanticRequestActive\.compareAndSet\(false, true\)/);
  assert.match(service, /"semantic request busy\\n",\s*503/);
  assert.match(
    service,
    /finally \{\s*if \(ownsSemanticRequest\) \{\s*synchronized \(semanticRequestMonitor\) \{[\s\S]*semanticRequestActive\.set\(false\);[\s\S]*semanticRequestMonitor\.notifyAll\(\)/,
  );
  assert.doesNotMatch(
    service.slice(service.indexOf("private static boolean isSemanticEndpoint")),
    /case [^\n]*"\/await-event"/,
  );
  assert.doesNotMatch(
    service.slice(service.indexOf("private static boolean isSemanticEndpoint")),
    /case [^\n]*"\/await-idle"/,
  );
  assert.match(service, /semanticRequestMonitor\.notifyAll\(\)/);
  assert.match(harness, /semanticDriverObservationUnavailable\(response\)/);
  assert.match(harness, /response\.stdout\.includes\("semantic request busy"\)/);
});

test("Android semantic actions retry only an explicit busy non-delivery", () => {
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const actionRequest = harness.slice(
    harness.indexOf("function semanticDriverActionRequest"),
    harness.indexOf("const ANDROID_TEXT_KEY_COMMANDS"),
  );
  assert.match(actionRequest, /while \(semanticDriverRequestBusy\(response\)\)/);
  assert.match(actionRequest, /semanticDriverIdleRequest/);
  assert.doesNotMatch(actionRequest, /semanticDriverRequestTimedOut/);
  assert.equal(
    (harness.match(/semanticDriverActionRequest\((?:state\.port|port),/g) ?? []).length,
    5,
  );

  const requests = [];
  const responses = [
    { status: 22, stdout: "semantic request busy\n", stderr: "curl: HTTP 503" },
    { status: 0, stdout: "idle\n", stderr: "" },
    { status: 0, stdout: "ok\n", stderr: "" },
  ];
  const completed = semanticDriverActionRequest(
    19191,
    "/tap?bounds=%5B0%2C0%5D%5B1%2C1%5D",
    "POST",
    (...request) => {
      requests.push(request);
      return responses.shift();
    },
  );
  assert.equal(completed.stdout, "ok\n");
  assert.equal(requests.length, 3);
  assert.equal(requests[0][1], requests[2][1]);
  assert.match(requests[1][1], /^\/await-idle\?/);

  let timeoutRequests = 0;
  const timedOut = semanticDriverActionRequest(19191, "/tap?bounds=timeout", "POST", () => {
    timeoutRequests += 1;
    return { status: 28, stdout: "", stderr: "Operation timed out" };
  });
  assert.equal(timedOut.status, 28);
  assert.equal(timeoutRequests, 1);
});

test("Android semantic observations recover one timed-out read without creating a traversal herd", () => {
  const requests = [];
  const responses = [
    { status: 28, stdout: "", stderr: "Operation timed out" },
    { status: 0, stdout: "idle\n", stderr: "" },
    { status: 0, stdout: "[]\n", stderr: "" },
  ];
  const completed = semanticDriverObservationRequest(19191, "/query?tag=map", (...request) => {
    requests.push(request);
    return responses.shift();
  });
  assert.equal(completed.status, 0);
  assert.equal(requests.length, 3);
  assert.equal(requests[0][1], "/query?tag=map");
  assert.match(requests[1][1], /^\/await-idle\?/);
  assert.equal(requests[2][1], "/query?tag=map");
});

test("Android exact semantic observations wait for an action lock", () => {
  const requests = [];
  const responses = [
    { status: 22, stdout: "semantic request busy\n", stderr: "curl: HTTP 503" },
    { status: 0, stdout: "idle\n", stderr: "" },
    { status: 0, stdout: "[]\n", stderr: "" },
  ];
  const path = "/exact-projection?tag=input";
  const completed = semanticDriverObservationRequest(19191, path, (...request) => {
    requests.push(request);
    return responses.shift();
  });
  assert.equal(completed.status, 0);
  assert.equal(completed.stdout, "[]\n");
  assert.deepEqual(
    requests.map((request) => request[1]),
    [path, requests[1][1], path],
  );
  assert.match(requests[1][1], /^\/await-idle\?/);
});

test("Android semantic request watchdog accepts fractional network deadlines", () => {
  const source = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const start = source.indexOf("function semanticDriverRequest(");
  const end = source.indexOf("\nfunction ", start + 1);
  const implementation = source.slice(start, end);
  assert.match(implementation, /timeout: Math\.ceil\(\(timeoutSeconds \+ 1\) \* 1000\)/);
});

test("revealed-element observation handles the initial probe through the bounded observer", () => {
  const source = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const start = source.indexOf("export async function establishRevealedElement");
  const end = source.indexOf("\nexport ", start + 1);
  const implementation = source.slice(start, end < 0 ? undefined : end);
  assert.doesNotMatch(implementation, /const initial = await readReachable\(\)/);
  assert.match(implementation, /await observe\(/);
});

test("Android semantic driver rejects stale protocol artifacts before a journey", () => {
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const bundleBuilder = readFileSync(
    new URL("../ci/build_release_e2e_apps.sh", import.meta.url),
    "utf8",
  );
  const bundleVerifier = readFileSync(
    new URL("../ci/verify_release_e2e_apps.py", import.meta.url),
    "utf8",
  );
  assert.match(harness, /aerobag-semantic-driver\/25/);
  assert.match(service, /aerobag-semantic-driver\/25/);
  assert.match(bundleBuilder, /aerobag-semantic-driver\/25/);
  assert.match(bundleVerifier, /aerobag-semantic-driver\/25/);
  assert.match(harness, /semantic driver protocol mismatch/);
});

test("Android replay sliders use accessible progress actions instead of timed swipes", () => {
  const playbackWidget = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/PlaybackWidget.kt", import.meta.url),
    "utf8",
  );
  const journeys = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  const replaySlice = journeys.slice(
    journeys.indexOf("async function setReplayRate"),
    journeys.indexOf("async function preparedLiveFeeds"),
  );
  assert.equal((playbackWidget.match(/setProgress \{/g) ?? []).length, 2);
  assert.match(replaySlice, /setProgress\(\s*"playback-rate-input"/);
  assert.match(replaySlice, /setProgress\("playback-overview"/);
  assert.doesNotMatch(replaySlice, /drag\("playback-rate-input"/);
});

test("Android exact semantic queries revalidate cached targets before traversing the tree", () => {
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const query = service.slice(
    service.indexOf("private JSONArray renderNodeQuery"),
    service.indexOf("private JSONArray renderExactProjection"),
  );
  assert.match(
    query,
    /if \(!prefix && appendCachedNodeQuery\(tag, output, includeDescendantText\)\) return output;/,
  );
  assert.match(query, /nodeAtPath\(semanticPath\)/);
  assert.match(query, /tag\.equals\(node\.getViewIdResourceName\(\)\) && bounds\.equals\(expectedBounds\)/);
  assert.match(query, /centerReachable\(node\)/);
  assert.match(query, /appendCachedNodeQueryAtPoint\([\s\S]*includeDescendantText[\s\S]*\)/);
  assert.match(query, /bounds\.contains\(expectedBounds\.centerX\(\), expectedBounds\.centerY\(\)\)/);
  assert.match(query, /exactNodePaths\.remove\(tag, semanticPath\)/);
  assert.match(query, /exactNodeBounds\.remove\(tag, expectedBounds\)/);
});

test("Android high-fanout presence probes do not aggregate descendant text", () => {
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const driver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  assert.match(harness, /descendant_text: String\(includeDescendantText\)/);
  assert.match(driver, /includeDescendantText: elementId !== "map-surface"/);
  assert.match(driver, /includeDescendantText: false/);
  assert.match(service, /query\.getOrDefault\("descendant_text", "true"\)/);
  assert.match(service, /includeDescendantText \? nodeLabel\(node\) : directNodeLabel\(node\)/);
});

test("Android keyboard visibility does not traverse the accessibility hierarchy", () => {
  const driver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const keyboard = driver.slice(
    driver.indexOf('if (elementId === "software-keyboard")'),
    driver.indexOf('if (elementId.startsWith("installed-package:"))'),
  );
  assert.match(keyboard, /androidImeShown\(this\.serial\)/);
  assert.doesNotMatch(keyboard, /dumpAndroid\(/);
});

test("Android layer regression uses the shared typed popup-control contract", () => {
  const suite = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const layerHelpers = suite.slice(
    suite.indexOf("function queryLayerToggleNode"),
    suite.indexOf("function rejectedLayerCommandCount"),
  );
  const layerJourney = suite.slice(
    suite.indexOf("async function runLayerToggleNavDbRegression"),
    suite.indexOf("async function runFlightPlanRouteSmoke"),
  );
  assert.match(layerHelpers, /driver\.readOption\("layers-button", layerId\)/);
  assert.match(layerJourney, /driver\.selectOption\("layers-button", "terrain_warning"/);
  assert.match(layerJourney, /driver\.selectOption\("layers-button", "nexrad"/);
  assert.doesNotMatch(layerHelpers + layerJourney, /queryAndroidSemanticNodes|dumpAndroid\(/);
});

test("Android CTR gestures use the exact follow projection instead of traversing the map", () => {
  const suite = readFileSync(new URL("./run-android-e2e-suite.mjs", import.meta.url), "utf8");
  const dragJourney = suite.slice(
    suite.indexOf("async function dragMapWhileFollowing"),
    suite.indexOf("async function zoomMapOneStepWhileFollowing"),
  );
  assert.match(dragJourney, /ready: async \(\) => queryMapFollowProbe\(serial\)/);
  assert.match(dragJourney, /followProbe\.centerX/);
  assert.match(dragJourney, /followProbe\.centerY/);
  assert.doesNotMatch(dragJourney, /queryAndroidSemanticNodes\(/);
  assert.doesNotMatch(dragJourney, /dumpAndroid\(/);
  assert.doesNotMatch(dragJourney, /parity:map-surface/);
});

test("map-family completion compares the fixed projection instead of scanning a derived suffix", () => {
  const implementations = readFileSync(new URL("./release-journey-implementations.mjs", import.meta.url), "utf8");
  const helper = implementations.slice(
    implementations.indexOf("async function selectedMapFamily"),
    implementations.indexOf("async function ensureMapFamily"),
  );
  assert.match(helper, /readProjection\("parity:map-family:"\)/);
  assert.match(helper, /startsWith\(`parity:map-family:\$\{familyId\}:`\)/);
  assert.doesNotMatch(helper, /readProjection\(`parity:map-family:\$\{familyId\}:/);
});

test("Android terrain tile failures terminate the current render pass", () => {
  const map = readFileSync(new URL(
    "../../ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt",
    import.meta.url,
  ), "utf8");
  const batch = map.slice(
    map.indexOf("var batchRendered = 0"),
    map.indexOf("LaunchedEffect(uiSession, viewport", map.indexOf("var batchRendered = 0")),
  );
  assert.match(batch, /catch \(error: Throwable\) \{\s*batchFailed = true/);
  assert.match(batch, /if \(batchFailed\) break/);
});

test("Android exact action discovery uses the accessibility view-id index", () => {
  const service = readFileSync(new URL(
    "../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java",
    import.meta.url,
  ), "utf8");
  assert.match(
    service,
    /if \(!prefix && appendIndexedNodeQuery\(tag, output, includeDescendantText\)\) return output;/,
  );
  assert.match(service, /findAccessibilityNodeInfosByViewId\(tag\)/);
  assert.match(service, /AccessibilityNodeInfo indexed = findIndexedRenderedNode\(tag, expectedBounds\);/);
});

test("Android semantic actions preserve the separator between readiness bounds", () => {
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  assert.match(service, /\.replace\("\]\[", " "\)/);
});

test("Android semantic text delivery revalidates one exact rendered accessibility action", () => {
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const driver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const textDelivery = service.slice(
    service.indexOf("private boolean setRenderedText"),
    service.indexOf("private boolean setRenderedProgress"),
  );
  assert.match(harness, /ime", "set", SEMANTIC_DRIVER_IME/);
  assert.match(
    driver,
    /setAndroidSemanticText\([\s\S]*this\.serial,[\s\S]*semanticTag,[\s\S]*value,[\s\S]*readyElement\.bounds,[\s\S]*readyElement\.semantic_path/,
  );
  assert.match(textDelivery, /resolveRenderedNode\(tag, expectedBounds, semanticPath\)/);
  assert.match(textDelivery, /setMatchingNodeText\(node, tag, value, expectedBounds\)/);
  assert.match(service, /supportsAction\([\s\S]*AccessibilityNodeInfo\.ACTION_SET_TEXT/);
  assert.match(service, /ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE/);
  assert.match(service, /node\.performAction\(AccessibilityNodeInfo\.ACTION_SET_TEXT, arguments\)/);
  assert.match(driver, /providerOnly: true/);
  assert.match(service, /"text"\.equals\(fields\.getOrDefault\("kind", ""\)\)/);
  assert.match(driver, /projected\.focused && !projected\.supports_set_text/);
  assert.match(driver, /readyElement\.focused !== true/);
  assert.doesNotMatch(harness, /\/ime-ready/);
  assert.doesNotMatch(service, /SemanticDriverInputMethodService\.focusedTextReady/);
});

test("Android mandatory disclaimer publishes indexed action geometry", () => {
  const activity = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt", import.meta.url),
    "utf8",
  );
  const disclaimer = activity.slice(
    activity.indexOf("internal fun DisclaimerConsentModal"),
    activity.indexOf("internal data class UiInvalidationRevisions"),
  );
  assert.match(
    disclaimer,
    /e2eIndexedControl\(\s*semanticTag = "parity:disclaimer-accept-button"/,
  );
  assert.match(disclaimer, /testTag\("parity:disclaimer-accept-button"\)/);
});

test("Android playback buttons publish indexed action geometry", () => {
  const playback = readFileSync(
    new URL("../../ui/android-app/app/src/main/java/org/aerobag/app/PlaybackWidget.kt", import.meta.url),
    "utf8",
  );
  const button = playback.slice(
    playback.indexOf("internal fun PlaybackSmallButton"),
    playback.indexOf("internal fun PlaybackButtonIconCanvas"),
  );
  assert.match(
    button,
    /e2eIndexedControl\(\s*semanticTag = testTag/,
  );
  assert.match(button, /state = "enabled:\$enabled:selected:false:checked:false"/);
  assert.match(button, /Modifier\.testTag\(testTag\)/);
});

test("Android click and focus delivery resolves the current Compose virtual node", () => {
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const tap = service.slice(
    service.indexOf("private void handleTap"),
    service.indexOf("private void handleScroll"),
  );
  assert.match(tap, /parseBounds[\s\S]*renderedTapBounds\(tag, bounds, semanticPath\)/);
  assert.match(service, /renderedTapBounds[\s\S]*resolveRenderedNode\(tag, expectedBounds, semanticPath\)/);
  assert.doesNotMatch(service, /clickRenderedNode|focusRenderedNode|clickMatchingNode|focusMatchingNode/);
});

test("Android action readiness requires app-indexed geometry", () => {
  const driver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const methodStart = driver.lastIndexOf("async readAction(actionId)");
  const method = driver.slice(
    methodStart,
    driver.indexOf("async readSessionRevision()", methodStart),
  );
  assert.match(method, /queryFirstAndroidSemanticNode/);
  assert.match(method, /providerOnly: true/);
});

test("Android semantic lookup prefers an exact action over an earlier prefix match", () => {
  const xml = `<hierarchy>` +
    `<node resource-id="parity:chart-search-suggestion:27WA"/>` +
    `<node resource-id="parity:chart-search-suggestion:27W"/>` +
    `</hierarchy>`;
  assert.equal(
    findTagOrPrefix(xml, "parity:chart-search-suggestion:27W")?.["resource-id"],
    "parity:chart-search-suggestion:27W",
  );
});

test("Android semantic aliases map shared flight-plan controls to core enum ids", () => {
  assert.equal(
    androidSemanticTag("plan-control:stop_navigation"),
    "parity:plan-control:StopNavigation",
  );
  assert.equal(
    androidSemanticTag("plan-control:restore_direct_to"),
    "parity:plan-control:RestoreDirectTo",
  );
  assert.equal(
    androidSemanticTag("plan-control:toggle_sequencing_suspension"),
    "parity:plan-control:ToggleSequencingSuspension",
  );
});

test("Android Cloud actions use exact selectors and require visible scroll reachability", () => {
  assert.deepEqual(
    androidActionCandidates("copy_setup_code"),
    ["parity:cloud-action:copy_setup_code"],
  );
  for (const actionId of [
    "begin_setup", "begin_create", "back_setup", "scan_setup_code", "accept_setup_code",
    "create_account", "backup_setup_code", "add_device", "close_linked_detail",
    "begin_unlink", "confirm_unlink", "sync_now", "copy_setup_code",
  ]) {
    assert.equal(androidElementMayRequireVerticalScroll(actionId), true, actionId);
  }
});

test("Android reveal requires reachability and traverses only known scroll collections", () => {
  const source = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const revealMethod = source.slice(
    source.lastIndexOf("  async revealElement(elementId)"),
    source.indexOf("\n  async reload()", source.lastIndexOf("  async revealElement(elementId)")),
  );
  assert.match(revealMethod, /establishRevealedElement/);
  assert.match(revealMethod, /scrollUntilTag\(this\.serial, semanticTag, 20, true, true\)/);
  assert.match(revealMethod, /!androidElementMayRequireVerticalScroll\(elementId\)/);
  assert.match(revealMethod, /traverse: async \(\) => false/);
  const readElementMethod = source.slice(
    source.lastIndexOf("  async readElement(elementId)"),
    source.lastIndexOf("  async revealElement(elementId)"),
  );
  assert.match(readElementMethod, /requireVisible: true/);
});

test("Android vertical reveals settle semantic scrolling before exposing a target", () => {
  const harness = readFileSync(new URL("./android-harness.mjs", import.meta.url), "utf8");
  const method = harness.slice(
    harness.indexOf("async function scrollUntilTagInDirection"),
    harness.indexOf("export function pressKey"),
  );
  assert.match(method, /queryAndroidExactProjection/);
  assert.match(method, /verifyReachable: requireReachable/);
  assert.match(method, /avoidNavigation/);
  assert.match(method, /await scrollAndroidSemanticSurfaceAndAwait/);
  assert.doesNotMatch(method, /scrollAndroidSemanticSurface\(serial, "vertical", direction\)/);
  const settleHelper = harness.slice(
    harness.indexOf("async function scrollAndroidSemanticSurfaceAndAwait"),
    harness.indexOf("export async function findNodeByScrolling"),
  );
  assert.match(settleHelper, /const before = dumpAndroid\(serial\)/);
  assert.match(settleHelper, /awaitAndroidScrollProjectionSettled\(serial, before\)/);
  assert.match(settleHelper, /observeChangedValueUntilStable/);
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  assert.match(service, /scrollFirstRenderedSurface\(orientation, action\)/);
  assert.match(service, /awaitAccessibilityQuietAfter\(eventSequence, 150, 750\)/);
});

test("Android reveal traversal re-rendezvous with exact semantic reachability", async () => {
  let reachable = null;
  let traversals = 0;
  const result = await establishRevealedElement({
    description: "settings-section-debug_diagnostics",
    readReachable: async () => reachable,
    traverse: async () => {
      traversals += 1;
      if (traversals === 2) reachable = { test_id: "settings-section-debug_diagnostics" };
      return true;
    },
    observe: async (_description, probe) => {
      for (let attempt = 0; attempt < 3; attempt += 1) {
        const value = await probe();
        if (value) return { value, durationMs: 0 };
      }
      throw new Error("test probe did not establish reachability");
    },
  });
  assert.equal(traversals, 2);
  assert.equal(result?.test_id, "settings-section-debug_diagnostics");
});

test("session revision acknowledgement is parsed from Android's persistent root projection", () => {
  assert.equal(
    androidSessionRevisionFromStateTag(
      "parity:startup-state:ready:true:persisted_page:Map:session_revision:417",
    ),
    417,
  );
  assert.equal(androidSessionRevisionFromStateTag("parity:startup-state:ready:true"), null);
});

test("Android action readiness uses the same semantic identities as delivery", () => {
  assert.deepEqual(androidActionCandidates("ownship-source-button"), ["parity:ownship-launcher"]);
  assert.deepEqual(androidActionCandidates("plan-row:row-7"), ["parity:plan-row:row-7"]);
  assert.deepEqual(androidActionCandidates("tray-option:Vectors"), ["parity:tray-option:Vectors"]);
});

test("Android plan actions use stable exact semantic identities", () => {
  const source = readFileSync(
    new URL("./semantic-journey-driver.mjs", import.meta.url),
    "utf8",
  );
  const readAction = source.slice(
    source.lastIndexOf("  async readAction(actionId)"),
    source.lastIndexOf("  async readSessionRevision()"),
  );
  assert.doesNotMatch(readAction, /allowPrefix|stateSuffixed/);
  assert.deepEqual(androidActionCandidates("restore_direct_to"), [
    "parity:plan-control:RestoreDirectTo",
  ]);
  assert.deepEqual(androidActionCandidates("move_up"), ["parity:plan-row-action:move_up"]);
  assert.deepEqual(androidActionCandidates("direct_to"), [
    "parity:map-selection-action:direct_to",
    "parity:plan-row-action:direct_to",
  ]);
  assert.match(source, /androidActionCandidateMatches\(actionId, retainedTag\)/);
});

test("native Android leg activation consumes the shared typed action contract", () => {
  const nativeSuite = readFileSync(
    new URL("run-android-e2e-suite.mjs", import.meta.url),
    "utf8",
  );
  const activation = nativeSuite.slice(
    nativeSuite.indexOf("async function activateDestinationLeg"),
    nativeSuite.indexOf("async function ensureChartPage"),
  );
  assert.match(activation, /driver\.readElement\("plan-row-tray-scrim"\)/);
  assert.match(activation, /driver\.readAction\("activate_leg"\)/);
  assert.match(activation, /driver\.performAction\("activate_leg", readyAction\)/);
  assert.doesNotMatch(activation, /activate_leg:enabled:true/);
});

test("native Android deterministic ownship uses shared settings traversal", () => {
  const nativeSuite = readFileSync(
    new URL("run-android-e2e-suite.mjs", import.meta.url),
    "utf8",
  );
  const setup = nativeSuite.slice(
    nativeSuite.indexOf("async function ensureBadAutopilotDebugFlag"),
    nativeSuite.indexOf("async function dismissOwnshipSourceTray"),
  );
  assert.match(setup, /driver\.revealElement\(sectionId\)/);
  assert.match(setup, /driver\.revealElement\(toggleId\)/);
  assert.match(setup, /driver\.performAction\(sectionId, readySection\)/);
  assert.match(setup, /driver\.performAction\(toggleId, readyToggle\)/);
  assert.doesNotMatch(setup, /scrollUntilTag|dumpAndroid/);
});

test("native Android deterministic ownship uses the shared typed chooser contract", () => {
  const nativeSuite = readFileSync(
    new URL("run-android-e2e-suite.mjs", import.meta.url),
    "utf8",
  );
  const chooser = nativeSuite.slice(
    nativeSuite.indexOf("async function openBadAutopilotSourceTray"),
    nativeSuite.indexOf("function cropPlateSurface"),
  );
  assert.match(chooser, /driver\.readAction\("ownship-source-button"\)/);
  assert.match(chooser, /driver\.openChooser\("ownship-source-button", readyLauncher\)/);
  assert.match(
    chooser,
    /driver\.readOption\("ownship-source-button", "__bad_autopilot__"\)/,
  );
  assert.match(
    chooser,
    /driver\.readOption\("ownship-source-button", "__direct_situation__"\)/,
  );
  assert.match(chooser, /if \(await driver\.readOption/);
  assert.doesNotMatch(chooser, /readAction\("ownship-source-button"\)\) === null/);
  assert.match(chooser, /driver\.selectOption\(/);
  assert.doesNotMatch(chooser, /dumpAndroid|BAD_AUTOPILOT_SOURCE_TAG/);
});

test("Android offline-package state probes can reach lazy-list rows", () => {
  assert.equal(
    androidElementMayRequireVerticalScroll("parity:offline-product:terrain:selection:pause"),
    true,
  );
  assert.equal(androidElementMayRequireVerticalScroll("cloud-overall-status"), false);
  assert.equal(
    androidElementMayRequireVerticalScroll("parity:settings-slider:display_dim_timeout:2m"),
    true,
  );
  assert.equal(androidElementMayRequireVerticalScroll("offline-refresh-button"), false);
  assert.equal(androidElementMayRequireVerticalScroll("offline-sync-button"), false);
  assert.equal(androidElementMayRequireVerticalScroll("cloud-setup-code-output"), true);
  assert.equal(androidElementMayRequireVerticalScroll("plan-append-route-input"), true);
  assert.equal(androidElementMayRequireVerticalScroll("plan-airway-entry:MEDEA"), true);
  assert.equal(androidElementMayRequireVerticalScroll("plan-airway-exit:KPAE"), true);
});

test("route append journeys reject an editor that traversal did not reveal", () => {
  const source = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  const appendRoute = source.slice(
    source.indexOf("async function appendRoute(runtime, route)"),
    source.indexOf("\nasync function", source.indexOf("async function appendRoute(runtime, route)") + 1),
  );
  assert.match(
    appendRoute,
    /revealRequiredElement\(runtime, "plan-append-route-input", "flight-plan route editor"\)/,
  );
  assert.doesNotMatch(appendRoute, /runtime\.revealElement\("plan-append-route-input"/);
});

test("Android Back remains idempotent when one dismissal closes nested overlays", async () => {
  let presses = 0;
  const driver = new AndroidSemanticJourneyDriver("test", {
    resetApp: async () => {},
    pressBack: () => { presses += 1; },
    softwareKeyboardShown: () => false,
  });
  await driver.back();
  await driver.back();
  assert.equal(presses, 2);
});

test("Android semantic Back cannot be intercepted by a retained software keyboard", () => {
  const source = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const method = source.slice(
    source.lastIndexOf("  async back()"),
    source.lastIndexOf("  async captureFrame(path)"),
  );
  assert.match(method, /software keyboard hidden before Android Back/);
  assert.match(method, /this\.softwareKeyboardShown\(\) \? null : true/);
  assert.ok(method.indexOf("observeUntil") < method.indexOf("this.pressBackCallback()"));

  const journeys = readFileSync(
    new URL("./release-journey-implementations.mjs", import.meta.url),
    "utf8",
  );
  assert.doesNotMatch(journeys, /dismiss retained chart search keyboard/);
});

test("Android flight-plan readiness uses its fixed page marker", () => {
  assert.equal(androidPageTag("flight_plan"), "parity:page:flight_plan");
});

test("Android semantic probes distinguish logical enablement from help clickability", () => {
  assert.equal(androidElementEnabled({
    "resource-id": "parity:plan-row-action:move_down:enabled:false",
    enabled: "true",
  }), false);
  assert.equal(androidElementEnabled({
    "resource-id": "parity:plan-row-action:move_down:enabled:true",
    enabled: "true",
  }), true);
});

test("Android semantic action readiness requires a center-reachable control", () => {
  const source = readFileSync(
    new URL("./semantic-journey-driver.mjs", import.meta.url),
    "utf8",
  );
  const readAction = source.slice(
    source.lastIndexOf("  async readAction(actionId)"),
    source.lastIndexOf("  async readSessionRevision()"),
  );
  assert.match(readAction, /requireVisible: true,\s*requireReachable: true/);
  const queryFirst = source.slice(
    source.indexOf("function queryFirstAndroidSemanticNode"),
    source.indexOf("function readinessEvidenceMatchesTag"),
  );
  assert.match(queryFirst, /verifyReachable: requireReachable/);
  const service = readFileSync(
    new URL(
      "../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java",
      import.meta.url,
    ),
    "utf8",
  );
  assert.match(service, /getCurrentWindowMetrics\(\)[\s\S]*getBounds\(\)/);
  assert.match(service, /displayBounds\.contains\(centerX, centerY\)/);
  assert.match(service, /ancestorClip\.contains\(bounds\.centerX\(\), bounds\.centerY\(\)\)/);
  assert.match(service, /"center-reachable"/);
  assert.doesNotMatch(service, /awaitAccepted(?:Click|Text)Action|ACTION_RETRY/);
});

test("Android indexed taps revalidate app state and rendered reachability before delivery", () => {
  const service = readFileSync(
    new URL("../../ui/android-app/app/src/androidTest/java/org/aerobag/app/e2e/SemanticDriverService.java", import.meta.url),
    "utf8",
  );
  const renderedTapBounds = service.slice(
    service.indexOf("private Rect renderedTapBounds"),
    service.indexOf("private static boolean scrollNode"),
  );
  const indexedBranch = renderedTapBounds.slice(
    renderedTapBounds.indexOf('semanticPath.startsWith("projection-provider:")'),
    renderedTapBounds.indexOf("AccessibilityNodeInfo node"),
  );
  assert.match(indexedBranch, /providerProjection\(tag, true\)/);
  assert.match(indexedBranch, /semanticPath\.equals\(value\.optString\("semantic-path"/);
  assert.match(indexedBranch, /expectedBounds\.equals\(currentBounds\)/);
  assert.match(indexedBranch, /return currentBounds/);
  assert.doesNotMatch(indexedBranch, /resolveRenderedNode|AccessibilityNodeInfo/);
  assert.match(renderedTapBounds, /!centerReachable\(node\)/);
});

test("Android zoom key direction matches web wheel semantics", () => {
  assert.equal(androidZoomKeyCode(-360), "KEYCODE_PLUS");
  assert.equal(androidZoomKeyCode(360), "KEYCODE_MINUS");
  const driver = readFileSync(new URL("./semantic-journey-driver.mjs", import.meta.url), "utf8");
  const androidZoom = driver.slice(
    driver.indexOf("async zoom(surfaceId, amount, readyElement = null)"),
    driver.indexOf("async readProjection(probe)", driver.indexOf("async zoom(surfaceId, amount, readyElement = null)")),
  );
  assert.match(androidZoom, /readinessEvidenceMatchesTag\(semanticTag, readyElement\)/);
  assert.doesNotMatch(androidZoom, /dumpAndroid\(/);
});

test("web virtual clock advances from the fixture epoch", () => {
  const source = advancingVirtualClockScript(123_000);
  assert.match(source, /referenceEpochMs = 123000/);
  assert.match(source, /performance\.now\(\) - startedAt/);
  let monotonicMs = 10.25;
  const context = {
    Date,
    performance: { now: () => monotonicMs },
  };
  runInNewContext(source, context);
  monotonicMs = 10.80;
  assert.equal(context.Date.now(), 123_000);
  assert.equal(Number.isInteger(context.Date.now()), true);
});

test("fixture server emits daemon-shaped live-feed events", () => {
  const events = liveFeedEventsFromCurrent({
    schema_version: 3,
    generated_at_utc: "2026-08-20T00:00:00Z",
    products: {
      metars: {
        current: "wx-v1",
        version_manifest_url: "versions/metars/wx-v1.json",
        state_url: "states/metars/wx-v1.json.xz",
        state_sha256: "abc",
        collected_at_utc: "2026-08-20T00:00:00Z",
      },
    },
  });
  assert.deepEqual(events, [{
    id: "catalog:2026-08-20T00:00:00Z",
    event: "live-feed-catalog",
    payload: {
      schema_version: 3,
      generated_at_utc: "2026-08-20T00:00:00Z",
      products: {
        metars: {
          current: "wx-v1",
          version_manifest_url: "versions/metars/wx-v1.json",
          state_url: "states/metars/wx-v1.json.xz",
          state_sha256: "abc",
          collected_at_utc: "2026-08-20T00:00:00Z",
        },
      },
    },
  }]);
});
