// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { runInNewContext } from "node:vm";
import {
  createJourneyResult, finishJourneyResult, recordJourneyCheck,
  recordJourneyStep, validateJourneyResult,
} from "./journey-result.mjs";
import { releaseJourneyFixtureUrl } from "./release-journey-runtime.mjs";
import { verifyProductSurfaceCoverage } from "./product-surface-coverage.mjs";
import { RELEASE_JOURNEYS, validateJourneyRegistry } from "./release-journey-registry.mjs";
import { validateReleaseJourneyFixture } from "./release-journey-fixture.mjs";
import {
  openAndDismissDataStatus,
  offlineSyncButtonIsIdle,
  rasterPlanIsDisplayReady,
  selectChartSearchSuggestion,
  selectProcedure,
} from "./release-journey-implementations.mjs";
import {
  decodeReleaseJourneyFixturePath,
  liveFeedEventsFromCurrent,
  webDistIndexSha256,
} from "./serve-release-journey-fixture.mjs";
import {
  androidActionCandidates, androidElementEnabled, androidElementFallback,
  androidActionUsesSubmit, AndroidSemanticJourneyDriver,
  androidElementMayRequireHorizontalScroll, androidElementMayRequireVerticalScroll,
  androidPageTag, androidProjectionMayRequireVerticalScan, androidSemanticTag,
  androidTextControlNeedsTap,
  androidZoomKeyCode, findTagOrPrefix, retryVerifiedAndroidTextEntry,
  SEMANTIC_DRIVER_OPERATIONS, SemanticJourneyDriver,
  validateSemanticDriver, WebSemanticJourneyDriver,
} from "./semantic-journey-driver.mjs";
import { advancingVirtualClockScript } from "./virtual-clock.mjs";
import { clampDragEndpoint, timelineSeekDeltaX } from "./gesture-geometry.mjs";
import { WebSemanticTransport } from "./web-semantic-transport.mjs";
import { summarizeFixtureRequests } from "./release-journey-runtime.mjs";

test("release journey registry owns every assertion exactly once", () => {
  const index = validateJourneyRegistry();
  assert.equal(index.journey_ids.size, RELEASE_JOURNEYS.length);
  assert.ok(index.assertion_owners.size > 100);
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
  });
  assert.equal(shardZero.status, 0, shardZero.stderr);
  assert.equal(shardZero.stdout.trim(), "shared.cloud-crossfill");

  for (const shard of [1, 2, 3]) {
    const result = spawnSync(
      "bash",
      [script.pathname, "android-shard-list", "p1", String(shard), "4"],
      { cwd: new URL("../..", import.meta.url), encoding: "utf8" },
    );
    assert.equal(result.status, 0, result.stderr);
    assert.doesNotMatch(result.stdout, /shared\.cloud-crossfill/);
  }
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
  const runtime = {
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
  };

  assert.deepEqual(await openAndDismissDataStatus(runtime), { text: "1" });
  assert.deepEqual(events, [
    "read:data-status-launcher:false",
    "action:data-status-launcher",
    "eventually:data status popup opened",
    "read:data-status-panel:true",
    "back:true",
    "eventually:data status popup dismissed",
    "read:data-status-panel:false",
  ]);
});

test("verified Android text entry replaces a dropped first injection", async () => {
  const observed = ["stale-prefill", "fixture-url"];
  const attempts = [];
  const result = await retryVerifiedAndroidTextEntry("fixture-url", {
    enter: async (attempt) => attempts.push(attempt),
    read: async () => observed.shift(),
  });
  assert.deepEqual(result, { matched: true, observed: "fixture-url", attempts: 2 });
  assert.deepEqual(attempts, [0, 1]);
});

test("verified Android text entry rejects three corrupted injections", async () => {
  const result = await retryVerifiedAndroidTextEntry("fixture-url", {
    enter: async () => {},
    read: async () => "corrupt",
  });
  assert.deepEqual(result, { matched: false, observed: "corrupt", attempts: 3 });
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
  assert.equal(workflow.match(/Install browser harness dependencies/g)?.length, 2);

  const lab = readFileSync(
    new URL("./release_journey_lab.sh", import.meta.url),
    "utf8",
  );
  assert.match(
    lab,
    /local -a state_args=\(--clear-app-data --sync-all-available-packages\)/,
  );
  assert.match(lab, /android_baseline_restore "\$ANDROID_BASELINE_SNAPSHOT"/);
  assert.match(lab, /AEROBAG_RELEASE_JOURNEY_REUSE_FIXTURE:-1/);
  assert.match(lab, /--data '\{"reset":true\}'/);
  assert.match(lab, /aerobag-release-journey-lab-\$\{PORT\}/);
  assert.match(lab, /current_web_dist_sha256.*requested_web_dist_sha256/);
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

test("web reset stops the app before clearing persistent origin state", async () => {
  const calls = [];
  const page = {
    navigate: async (url) => calls.push(["navigate", url]),
    waitForLoad: async () => calls.push(["waitForLoad"]),
    send: async (method, args) => calls.push(["send", method, args]),
  };
  const transport = new WebSemanticTransport(page, {
    url: "http://fixture.test/app",
    origin: "http://fixture.test",
  });

  await transport.reset();

  assert.deepEqual(calls, [
    ["navigate", "about:blank"],
    ["waitForLoad"],
    ["send", "Storage.clearDataForOrigin", {
      origin: "http://fixture.test",
      storageTypes: "all",
    }],
    ["navigate", "http://fixture.test/app"],
    ["waitForLoad"],
  ]);
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

test("release journey suites propagate a failed journey process", () => {
  const temp = mkdtempSync(join(tmpdir(), "aerobag-release-suite-failure-"));
  try {
    const fixture = join(temp, "fixture.json");
    const fakeBin = join(temp, "bin");
    writeFileSync(fixture, "{}\n");
    mkdirSync(fakeBin);
    const node = join(fakeBin, "node");
    writeFileSync(node, `#!/usr/bin/env bash
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
  assert.match(workflow, /android-suite-shard/);
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

test("procedure replacement waits for the picker transaction before reopening its row", async () => {
  let transitionSelected = false;
  let pickerReadCount = 0;
  const staleRow = { id: "parity:plan-procedure-row:I16R:uid:old" };
  const currentRow = { id: "parity:plan-procedure-row:I16R:uid:new" };
  const runtime = {
    platform: "web",
    driver: {
      async findProjectionMatching(prefix, label) {
        assert.equal(prefix, "parity:plan-row:");
        assert.equal(label, "KPAE");
        return { id: "parity:plan-row:airport-row", text: "KPAE" };
      },
      async readElement(id) {
        if (id === "plan-row-action-select_approach") return { enabled: true };
        if (id === "plan-procedure-picker") {
          pickerReadCount += 1;
          return pickerReadCount < 2 ? { id } : null;
        }
        return null;
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
        if (id === "plan-procedure-transition:VECTORS") transitionSelected = true;
      },
    },
    async eventually(label, probe) {
      for (let attempt = 0; attempt < 4; attempt += 1) {
        const value = await probe();
        if (value) return value;
      }
      throw new Error(`eventually failed: ${label}`);
    },
  };

  const selected = await selectProcedure(runtime, {
    airportId: "KPAE",
    actionId: "select_approach",
    procedureId: "I16R",
  });
  assert.equal(selected, currentRow);
  assert.equal(transitionSelected, true);
  assert.equal(pickerReadCount, 2);
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
  for (const source of [lab, builder]) {
    assert.match(source, /PACKAGE_SOURCE_PORT:-18093/);
    assert.match(source, /AEROBAG_E2E_CLOUD_PORT:-18094/);
  }
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
  assert.match(
    emulatorSource,
    /ANDROID_PACKAGE_SOURCE_DEVICE_PORT="\$\{ANDROID_PACKAGE_SOURCE_DEVICE_PORT:-\$PACKAGE_SOURCE_PORT\}"/,
  );
  assert.match(
    emulatorSource,
    /"tcp:\$\{ANDROID_PACKAGE_SOURCE_DEVICE_PORT\}" "tcp:\$\{PACKAGE_SOURCE_PORT\}"/,
  );
});

test("chart search selection retries a dropped platform tap", async () => {
  let selected = false;
  let taps = 0;
  const runtime = {
    driver: {
      async readProjection(id) {
        if (id === "parity:map-selection-selected:KSEA") return selected ? [{ id }] : [];
        if (id === "chart-search-suggestion-KSEA") return selected ? [] : [{ id }];
        return [];
      },
      async performAction(id) {
        assert.equal(id, "chart-search-suggestion-KSEA");
        taps += 1;
        if (taps === 2) selected = true;
      },
    },
    async eventually(_label, probe) {
      for (let attempt = 0; attempt < 4; attempt += 1) {
        const value = await probe();
        if (value) return value;
      }
      throw new Error("selection did not land");
    },
  };
  assert.deepEqual(
    await selectChartSearchSuggestion(runtime, "KSEA"),
    { id: "parity:map-selection-selected:KSEA" },
  );
  assert.equal(taps, 2);
});

test("raster readiness measures painted coverage without hiding reported failures", () => {
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

test("Android semantic discovery scrolls horizontally for clipped control strips", () => {
  assert.equal(androidElementMayRequireHorizontalScroll("plan-control:undo"), true);
  assert.equal(androidElementMayRequireHorizontalScroll("altitude-planner-control:wind_model"), true);
  assert.equal(androidElementMayRequireHorizontalScroll("altitude-planner-departure-basis"), true);
  assert.equal(androidElementMayRequireHorizontalScroll("settings-toggle-debug_internet_adsb"), false);
});

test("shared settings-choice probes translate to Android's core-owned row IDs", () => {
  assert.equal(
    androidSemanticTag("settings-choice-flight_data_visibility"),
    "parity:settings-choice:flight_data_visibility",
  );
});

test("Android scans lazy Data Status projections across the vertical list", () => {
  assert.equal(androidProjectionMayRequireVerticalScan("parity:data-status-row:"), true);
  assert.equal(androidProjectionMayRequireVerticalScan("parity:plan-row:"), false);
});

test("Android playback uses a direct tap instead of a multi-dump generic action", () => {
  const source = readFileSync(
    new URL("./semantic-journey-driver.mjs", import.meta.url),
    "utf8",
  );
  const action = source.slice(source.indexOf('if (actionId === "playback-play-toggle")'));
  assert.match(action, /findTagOrPrefix\(dumpAndroid\(this\.serial\), "parity:playback-play-toggle"\)/);
  assert.match(action, /adb\(this\.serial, \[/);
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
      live_feeds: { fresh: {}, mixed: {}, stale: {} },
    },
  };
  assert.equal(validateReleaseJourneyFixture(manifest).fixture, "release-journey-publication");
  delete manifest.capabilities.plate.notam;
  assert.throws(() => validateReleaseJourneyFixture(manifest), /plate\.notam/);
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
  assert.rejects(driver.openPage("map"), /does not implement openPage/);
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

test("Android semantic aliases preserve shared search suggestion ids", () => {
  assert.equal(
    androidSemanticTag("chart-search-suggestion-KSEA"),
    "parity:chart-search-suggestion:KSEA",
  );
});

test("Android exact search suggestions use the focused field submit action", () => {
  assert.equal(androidActionUsesSubmit("chart-search-suggestion:KSEA"), true);
  assert.equal(androidActionUsesSubmit("airport_info"), false);
});

test("Android text entry does not retap an auto-focused Compose field", () => {
  assert.equal(androidTextControlNeedsTap({ focused: "true" }), false);
  assert.equal(androidTextControlNeedsTap({ focused: "false" }), true);
  assert.equal(androidTextControlNeedsTap(null), true);
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
  assert.equal(androidElementMayRequireVerticalScroll("plan-airway-entry:MEDEA"), true);
  assert.equal(androidElementMayRequireVerticalScroll("plan-airway-exit:KPAE"), true);
});

test("Android Back remains idempotent when one dismissal closes nested overlays", async () => {
  let presses = 0;
  const driver = new AndroidSemanticJourneyDriver("test", {
    resetApp: async () => {},
    pressBack: () => { presses += 1; },
  });
  await driver.back();
  await driver.back();
  assert.equal(presses, 2);
});

test("Android flight-plan state remains a stable semantic page probe", () => {
  assert.equal(androidPageTag("flight_plan"), "parity:plan-state:");
});

test("Android insert editor falls back only to the dialog's focused text field", () => {
  const xml = `<hierarchy><node resource-id="parity:button:Enter" class="android.view.View" ` +
    `focused="false"/><node resource-id="" class="android.widget.EditText" focused="true" ` +
    `enabled="true" bounds="[1,2][3,4]"/></hierarchy>`;
  assert.equal(androidElementFallback(xml, "plan-insert-airport-input")?.focused, "true");
  assert.equal(androidElementFallback(xml, "chart-search-input"), null);
  assert.equal(androidElementFallback(xml.replace("parity:button:Enter", "other"), "plan-insert-airport-input"), null);
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

test("Android zoom key direction matches web wheel semantics", () => {
  assert.equal(androidZoomKeyCode(-360), "KEYCODE_PLUS");
  assert.equal(androidZoomKeyCode(360), "KEYCODE_MINUS");
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
