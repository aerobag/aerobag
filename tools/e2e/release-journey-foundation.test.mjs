// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
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
  rasterPlanIsDisplayReady,
  selectChartSearchSuggestion,
} from "./release-journey-implementations.mjs";
import {
  decodeReleaseJourneyFixturePath,
  liveFeedEventsFromCurrent,
} from "./serve-release-journey-fixture.mjs";
import {
  androidActionCandidates, androidElementEnabled, androidElementFallback,
  androidElementMayRequireHorizontalScroll, androidElementMayRequireVerticalScroll,
  androidPageTag, androidProjectionMayRequireVerticalScan, androidSemanticTag,
  androidZoomKeyCode, findTagOrPrefix, SEMANTIC_DRIVER_OPERATIONS, SemanticJourneyDriver,
  validateSemanticDriver, WebSemanticJourneyDriver,
} from "./semantic-journey-driver.mjs";
import { advancingVirtualClockScript } from "./web-semantic-transport.mjs";

test("release journey registry owns every assertion exactly once", () => {
  const index = validateJourneyRegistry();
  assert.equal(index.journey_ids.size, RELEASE_JOURNEYS.length);
  assert.ok(index.assertion_owners.size > 100);
});

test("grouped P2 journeys leave destructive contract failure last", () => {
  const p2 = RELEASE_JOURNEYS.filter((journey) => journey.priority === "p2");
  assert.equal(p2.at(-1)?.id, "shared.contract-failures");
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
  ]) {
    assert.match(workflow, new RegExp(`^  ${job}`, "m"));
  }
  assert.match(workflow, /schedule\) value='\["p2"\]'/);
  assert.match(workflow, /workflow_dispatch\) value='\["p0","p1","p2"\]'/);
  assert.match(workflow, /AEROBAG_RELEASE_JOURNEY_IMPLEMENTATIONS_ONLY: "1"/);
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
  const androidHarness = readFileSync(
    new URL("./android-harness.mjs", import.meta.url),
    "utf8",
  );
  assert.match(androidHarness, /DEBUG_CLEAR_CORE_SETTINGS_EXTRA/);
  assert.match(androidHarness, /DEBUG_CLEAR_UI_PREFS_EXTRA/);
  assert.doesNotMatch(androidHarness, /run-as[^\n]+core-settings-v1\.json/);
  assert.doesNotMatch(androidHarness, /run-as[^\n]+aerobag_ui\.xml/);
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
});

test("Android Cloud actions use their exact semantic selector without generic scroll probes", () => {
  assert.deepEqual(
    androidActionCandidates("copy_setup_code"),
    ["parity:cloud-action:copy_setup_code"],
  );
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
    id: "metars:wx-v1",
    payload: {
      schema_version: 3,
      product: "metars",
      version: "wx-v1",
      version_manifest_url: "versions/metars/wx-v1.json",
      state_url: "states/metars/wx-v1.json.xz",
      state_sha256: "abc",
      published_at_utc: null,
      collected_at_utc: "2026-08-20T00:00:00Z",
      history: [],
    },
  }]);
});
