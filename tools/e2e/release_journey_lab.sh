#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SERIAL="${ANDROID_SERIAL:-emulator-5564}"
PORT="${PACKAGE_SOURCE_PORT:-18093}"
CLOUD_PORT="${AEROBAG_E2E_CLOUD_PORT:-18094}"
ARTIFACT_DIR="${AEROBAG_E2E_ARTIFACT_DIR:-/tmp/aerobag-release-journey-results}"
LAB_STATE_DIR="${AEROBAG_RELEASE_JOURNEY_LAB_STATE_DIR:-/tmp/aerobag-release-journey-lab}"
TEST_ARTIFACTS_ROOT="${AEROBAG_TEST_ARTIFACTS_ROOT:-/tmp/aerobag-release-journey-test-artifacts}"
TARGET_ROOT_RELATIVE="$(<"$ROOT/ui/target-root.txt")"
UI_TARGET_ROOT="${AEROBAG_UI_TARGET_ROOT:-$(cd "$ROOT" && realpath "$TARGET_ROOT_RELATIVE")}"
WEB_DIST="${AEROBAG_RELEASE_JOURNEY_WEB_DIST:-$UI_TARGET_ROOT/web/dist}"
SERVE_WEB_DIST="${AEROBAG_RELEASE_JOURNEY_SERVE_WEB_DIST:-0}"
APP_ARTIFACTS_DIR="${AEROBAG_RELEASE_JOURNEY_APP_ARTIFACTS_DIR:-/tmp/release-e2e-apps-final}"

latest_fixture() {
  find /tmp -maxdepth 2 -path '/tmp/release-journey-publication-v*-materialized/fixture.json' \
    -print 2>/dev/null | sort -V | tail -1
}

FIXTURE="${AEROBAG_RELEASE_JOURNEY_FIXTURE:-$(latest_fixture)}"

usage() {
  cat <<'EOF'
usage: tools/e2e/release_journey_lab.sh COMMAND [ARG]

Commands:
  status                       Show fixture, server, emulator, and app status.
  foundation                   Run release-journey unit/foundation tests.
  test-artifacts-fetch         Fetch pinned fixtures needed by legacy resilience tests.
  fixture-build OUTPUT         Build and materialize a compact fixture.
  fixture-start [PROFILE]      Restart the deterministic fixture server.
  fixture-start-web [PROFILE]  Restart fixture server with the built web app.
  fixture-stop                 Stop the managed fixture server.
  cloud-start                  Restart an empty disposable Aerobag Cloud server.
  cloud-stop                   Stop the managed Aerobag Cloud server.
  apps-build [OUTPUT]          Build one immutable web/APK/cloud-server bundle.
  web-restart                  Rebuild and restart the release-lab web app on 8085.
  web-build                    Build the optimized, E2E-enabled web bundle.
  web-dist-test JOURNEY        Run a web journey against the fixture-served bundle.
  web-dist-suite [p0|p1|all]   Run implemented web journeys against that bundle.
  android-compile              Compile the Android app and semantic probes.
  android-deploy               Install the current debug APK without clearing app data.
  android-install [JOURNEY]    Clean-install/sync, then run one journey.
  android-install-apk [APK]    Clean-install an immutable APK and sync all fixture packages.
  android-upgrade-apk [APK]    Install a rebuilt immutable APK while preserving app data.
  android-test JOURNEY         Run one journey using the installed app.
  android-suite [p0|p1|all] [START_AT]
                               Run Android journeys, optionally resuming at START_AT.
  android-implementation-suite [p0|p1|all] [START_AT]
                               Run only registry implementations, optionally resuming.
  android-offline              Run the Android offline cold-start journey.
  android-open-page PAGE       Navigate the installed app through the semantic driver.
  android-action ID            Perform one Android semantic action.
  android-enter-text ID VALUE  Enter text through the Android semantic driver.
  android-zoom SURFACE AMOUNT  Apply one semantic zoom gesture to the current page.
  android-ui                   Capture compressed UI XML and screenshot.
  android-log [PATTERN]        Capture logcat and print matching diagnostics.
  web-test JOURNEY             Run one web journey against port 8085.
  web-suite [p0|p1|all]        Run implemented web journeys.

Environment overrides:
  AEROBAG_RELEASE_JOURNEY_FIXTURE, AEROBAG_RELEASE_JOURNEY_ORIGIN,
  AEROBAG_E2E_URL, AEROBAG_E2E_ARTIFACT_DIR, ANDROID_SERIAL,
  AEROBAG_TEST_ARTIFACTS_ROOT, PACKAGE_SOURCE_PORT, AEROBAG_E2E_CLOUD_PORT.
EOF
}

require_fixture() {
  if [[ -z "$FIXTURE" || ! -f "$FIXTURE" ]]; then
    echo "release journey fixture not found; set AEROBAG_RELEASE_JOURNEY_FIXTURE" >&2
    exit 1
  fi
}

fixture_origin() {
  printf '%s' "${AEROBAG_RELEASE_JOURNEY_ORIGIN:-http://127.0.0.1:${PORT}}"
}

fixture_start() {
  local profile="$1"
  local web_dist="${2:-}"
  local -a web_args=()
  require_fixture
  mkdir -p "$LAB_STATE_DIR"
  if [[ -f "$LAB_STATE_DIR/fixture.pid" ]]; then
    kill "$(<"$LAB_STATE_DIR/fixture.pid")" >/dev/null 2>&1 || true
  fi
  if curl -fsS --max-time 1 "http://127.0.0.1:${PORT}/__health" >/dev/null 2>&1; then
    fuser -k "${PORT}/tcp" >/dev/null 2>&1 || true
  fi
  if [[ -n "$web_dist" ]]; then
    [[ -f "$web_dist/index.html" ]] || { echo "web dist is missing: $web_dist" >&2; exit 1; }
    web_args=(--web-dist "$web_dist")
  fi
  setsid node "$ROOT/tools/e2e/serve-release-journey-fixture.mjs" \
    --fixture "$FIXTURE" --live-feed-profile "$profile" --port "$PORT" \
    "${web_args[@]}" \
    >"$LAB_STATE_DIR/fixture.log" 2>&1 &
  echo "$!" >"$LAB_STATE_DIR/fixture.pid"
  for _ in $(seq 1 50); do
    if curl -fsS --max-time 2 "http://127.0.0.1:${PORT}/__health" >/dev/null 2>&1; then
      echo "fixture server ready: $(fixture_origin) profile=$profile fixture=$FIXTURE${web_dist:+ web_dist=$web_dist}"
      return
    fi
    sleep 0.1
  done
  cat "$LAB_STATE_DIR/fixture.log" >&2
  exit 1
}

cloud_base_url() {
  printf 'http://127.0.0.1:%s/cloud/' "$CLOUD_PORT"
}

cloud_stop() {
  if [[ -f "$LAB_STATE_DIR/cloud.pid" ]]; then
    kill "$(cat "$LAB_STATE_DIR/cloud.pid")" >/dev/null 2>&1 || true
    rm -f "$LAB_STATE_DIR/cloud.pid"
  fi
}

cloud_start() {
  local target_dir="$UI_TARGET_ROOT/services"
  local binary="${AEROBAG_CLOUD_SERVER_BIN:-$target_dir/debug/aerobag-cloud-serverd}"
  mkdir -p "$LAB_STATE_DIR"
  cloud_stop
  if curl -fsS --max-time 1 "http://127.0.0.1:${CLOUD_PORT}/cloud/v1/health" >/dev/null 2>&1; then
    fuser -k "${CLOUD_PORT}/tcp" >/dev/null 2>&1 || true
  fi
  if [[ ! -x "$binary" ]]; then
    CARGO_TARGET_DIR="$target_dir" \
      cargo build --manifest-path "$ROOT/services/Cargo.toml" -p aerobag-cloud-server
  fi
  rm -rf "$LAB_STATE_DIR/cloud-storage"
  mkdir -p "$LAB_STATE_DIR/cloud-storage"
  head -c 32 /dev/urandom >"$LAB_STATE_DIR/cloud-secret.bin"
  setsid "$binary" serve \
    --storage-root "$LAB_STATE_DIR/cloud-storage" \
    --server-secret "$LAB_STATE_DIR/cloud-secret.bin" \
    --policy "$ROOT/deploy/aerobag-cloud-policy.json" \
    --listen "127.0.0.1:${CLOUD_PORT}" \
    >"$LAB_STATE_DIR/cloud.log" 2>&1 &
  echo "$!" >"$LAB_STATE_DIR/cloud.pid"
  for _ in $(seq 1 100); do
    if curl -fsS --max-time 2 "http://127.0.0.1:${CLOUD_PORT}/cloud/v1/health" >/dev/null 2>&1; then
      echo "cloud server ready: $(cloud_base_url)"
      return
    fi
    sleep 0.1
  done
  cat "$LAB_STATE_DIR/cloud.log" >&2
  exit 1
}

journey_profile() {
  local journey="$1"
  node --input-type=module - "$journey" <<'JS'
import { journeyById } from './tools/e2e/release-journey-registry.mjs';
const journey = journeyById(process.argv[2]);
if (!journey) throw new Error(`unknown release journey ${process.argv[2]}`);
console.log(journey.live_feed_profile ?? 'fresh');
JS
}

ensure_journey_profile() {
  local profile="$1"
  local health=""
  local current=""
  local serves_web="false"
  health="$(curl -fsS --max-time 2 "http://127.0.0.1:${PORT}/__health" 2>/dev/null || true)"
  if [[ -n "$health" ]]; then
    read -r current serves_web < <(
      python3 -c 'import json, sys; value = json.load(sys.stdin); print(value.get("live_feed_profile", ""), str(bool(value.get("serves_web_app"))).lower())' \
        <<<"$health" 2>/dev/null || true
    )
  fi
  if [[ "$current" == "$profile" && ( "$SERVE_WEB_DIST" != "1" || "$serves_web" == "true" ) ]]; then
    return
  fi
  if [[ "$SERVE_WEB_DIST" == "1" || "$serves_web" == "true" ]]; then
    fixture_start "$profile" "$WEB_DIST"
  else
    fixture_start "$profile"
  fi
}

implemented_journeys() {
  local platform="$1"
  local priority="$2"
  local implementations_only="${AEROBAG_RELEASE_JOURNEY_IMPLEMENTATIONS_ONLY:-0}"
  node --input-type=module - "$platform" "$priority" "$implementations_only" <<'JS'
import { RELEASE_JOURNEYS } from './tools/e2e/release-journey-registry.mjs';
import { releaseJourneyImplementation } from './tools/e2e/release-journey-implementations.mjs';
const [platform, priority, implementationsOnly] = process.argv.slice(2);
for (const journey of RELEASE_JOURNEYS) {
  if (!journey.platforms.includes(platform)) continue;
  if (priority !== 'all' && journey.priority !== priority) continue;
  if (releaseJourneyImplementation(journey.id)) {
    console.log(journey.id);
  } else if (implementationsOnly !== '1' && platform === 'android' && journey.existing_test) {
    console.log(journey.existing_test);
  }
}
JS
}

run_android_test() {
  local journey="$1"
  local profile
  local -a artifact_env=()
  require_fixture
  profile="$(journey_profile "$journey")"
  ensure_journey_profile "$profile"
  if [[ "$journey" == "shared.cloud-crossfill" ]]; then
    cloud_start
    adb -s "$SERIAL" reverse "tcp:${CLOUD_PORT}" "tcp:${CLOUD_PORT}" >/dev/null
  fi
  if [[ -d "$TEST_ARTIFACTS_ROOT/e2e/android-rotation-live-feed" ]]; then
    artifact_env+=("AEROBAG_TEST_ARTIFACTS_ROOT=$TEST_ARTIFACTS_ROOT")
  fi
  env \
    "${artifact_env[@]}" \
    PACKAGE_SOURCE_PORT="$PORT" \
    ANDROID_SERIAL="$SERIAL" \
    ANDROID_PACKAGE_SOURCE_BASE_URL="http://127.0.0.1:${PORT}/packages/" \
    ANDROID_LIVE_FEED_SOURCE_BASE_URL="http://127.0.0.1:${PORT}/live-feeds/" \
    AEROBAG_E2E_PEER_URL="${AEROBAG_E2E_PEER_URL:-http://127.0.0.1:8085/}" \
    AEROBAG_E2E_CLOUD_PORT="$CLOUD_PORT" \
    AEROBAG_E2E_ARTIFACT_DIR="$ARTIFACT_DIR" \
    "$ROOT/ui/android-app/scripts/run_e2e.sh" \
      --skip-install \
      --release-fixture "$FIXTURE" \
      --test "$journey"
}

run_web_test() {
  local journey="$1"
  local profile
  require_fixture
  profile="$(journey_profile "$journey")"
  ensure_journey_profile "$profile"
  if [[ "$journey" == "shared.cloud-crossfill" ]]; then
    cloud_start
  fi
  AEROBAG_E2E_PEER_URL="${AEROBAG_E2E_PEER_URL:-${AEROBAG_E2E_URL:-http://127.0.0.1:8085/}}" \
  node "$ROOT/tools/e2e/run-release-journey.mjs" \
    --platform web \
    --journey "$journey" \
    --url "${AEROBAG_E2E_URL:-http://127.0.0.1:8085/}" \
    --fixture "$FIXTURE" \
    --fixture-origin "$(fixture_origin)" \
    --artifact-dir "$ARTIFACT_DIR/$journey/web"
}

command="${1:-}"
case "$command" in
  status)
    echo "fixture=${FIXTURE:-<missing>}"
    echo "fixture_origin=$(fixture_origin)"
    echo "android_serial=$SERIAL"
    echo "artifact_dir=$ARTIFACT_DIR"
    echo "test_artifacts_root=$TEST_ARTIFACTS_ROOT"
    echo "cloud_base_url=$(cloud_base_url)"
    curl -fsS --max-time 2 "http://127.0.0.1:${PORT}/__health" || true
    echo
    curl -fsS --max-time 2 "http://127.0.0.1:${CLOUD_PORT}/cloud/v1/health" || true
    echo
    adb -s "$SERIAL" get-state || true
    adb -s "$SERIAL" shell dumpsys activity activities | rg -m1 'mResumedActivity|topResumedActivity' || true
    ;;
  foundation)
    cd "$ROOT"
    node --test \
      tools/e2e/release-journey-foundation.test.mjs \
      tools/e2e/android-harness.test.mjs \
      tools/e2e/live-feed-contract-paths.test.mjs
    ;;
  test-artifacts-fetch)
    if [[ -d "$TEST_ARTIFACTS_ROOT/e2e/android-rotation-live-feed" ]]; then
      echo "test artifacts already ready: $TEST_ARTIFACTS_ROOT"
      exit 0
    fi
    cd "$ROOT"
    python3 tools/ci/fetch_test_artifacts.py \
      --fixture android-rotation-live-feed \
      --destination "$TEST_ARTIFACTS_ROOT"
    ;;
  fixture-build)
    [[ -n "${2:-}" ]] || { usage >&2; exit 2; }
    output="$2"
    materialized="${output}-materialized"
    cd "$ROOT"
    python3 tools/ci/build_release_journey_fixture.py \
      --source-publication "${AEROBAG_SOURCE_PUBLICATION:-/root/aerobag-artifacts/published}" \
      --output "$output" \
      --primary-cycle "${AEROBAG_PRIMARY_CYCLE:-2608}" \
      --had-query "${AEROBAG_HAD_QUERY:-/root/aerobag-artifacts/target/debug/had_query}" \
      --live-feed-source "${AEROBAG_LIVE_FEED_SOURCE:-/root/aerobag-artifacts/dev-stack/live-feeds/v3}"
    python3 tools/ci/materialize_release_journey_fixture.py \
      --source "$output" --output "$materialized"
    echo "AEROBAG_RELEASE_JOURNEY_FIXTURE=$materialized/fixture.json"
    ;;
  fixture-start)
    profile="${2:-fresh}"
    fixture_start "$profile"
    ;;
  fixture-start-web)
    profile="${2:-fresh}"
    fixture_start "$profile" "$WEB_DIST"
    ;;
  fixture-stop)
    if [[ -f "$LAB_STATE_DIR/fixture.pid" ]]; then
      kill "$(cat "$LAB_STATE_DIR/fixture.pid")" >/dev/null 2>&1 || true
      rm -f "$LAB_STATE_DIR/fixture.pid"
    fi
    ;;
  cloud-start)
    cloud_start
    ;;
  cloud-stop)
    cloud_stop
    ;;
  apps-build)
    cd "$ROOT"
    "$ROOT/tools/ci/build_release_e2e_apps.sh" "${2:-$APP_ARTIFACTS_DIR}"
    ;;
  web-restart)
    require_fixture
    cd "$ROOT"
    env \
      AEROBAG_ARTIFACT_READ_PATH="$(dirname "$FIXTURE")/published" \
      AEROBAG_LIVE_FEEDS_ORIGIN="$(fixture_origin)" \
      AEROBAG_CLOUD_SERVER_BASE_URL="$(cloud_base_url)" \
      ./ui/web-app/scripts/restart-vite-dev.sh
    ;;
  web-build)
    cd "$ROOT"
    env \
      AEROBAG_E2E_ENABLED=1 \
      AEROBAG_LIVE_FEEDS_ORIGIN="$(fixture_origin)" \
      AEROBAG_CLOUD_SERVER_BASE_URL="$(cloud_base_url)" \
      npm --prefix ui/web-app run build:optimized
    ;;
  android-compile)
    cd "$ROOT"
    ./ui/android-app/scripts/test.sh :app:compileDebugKotlin
    ;;
  android-deploy)
    require_fixture
    cd "$ROOT"
    env \
      ANDROID_SERIAL="$SERIAL" \
      ANDROID_PACKAGE_SOURCE_BASE_URL="http://127.0.0.1:${PORT}/packages/" \
      ANDROID_LIVE_FEED_SOURCE_BASE_URL="http://127.0.0.1:${PORT}/live-feeds/" \
      ANDROID_CLOUD_SERVER_BASE_URL="$(cloud_base_url)" \
      ./ui/android-app/scripts/test.sh :app:installDebug
    ;;
  android-install)
    require_fixture
    journey="${2:-shared.startup-navigation}"
    if [[ "$journey" == "shared.cloud-crossfill" ]]; then
      cloud_start
      adb -s "$SERIAL" reverse "tcp:${CLOUD_PORT}" "tcp:${CLOUD_PORT}" >/dev/null
    fi
    cd "$ROOT"
    env \
      PACKAGE_SOURCE_PORT="$PORT" \
      ANDROID_SERIAL="$SERIAL" \
      ANDROID_PACKAGE_SOURCE_BASE_URL="http://127.0.0.1:${PORT}/packages/" \
      ANDROID_LIVE_FEED_SOURCE_BASE_URL="http://127.0.0.1:${PORT}/live-feeds/" \
      ANDROID_CLOUD_SERVER_BASE_URL="$(cloud_base_url)" \
      AEROBAG_E2E_ARTIFACT_DIR="$ARTIFACT_DIR" \
      ./ui/android-app/scripts/run_e2e.sh \
        --clear-app-data \
        --sync-all-available-packages \
        --release-fixture "$FIXTURE" \
        --test "$journey"
    ;;
  android-install-apk)
    require_fixture
    apk="${2:-$APP_ARTIFACTS_DIR/aerobag-release-e2e.apk}"
    journey="${3:-shared.startup-navigation}"
    [[ -f "$apk" ]] || { echo "immutable APK is missing: $apk" >&2; exit 1; }
    cd "$ROOT"
    env \
      PACKAGE_SOURCE_PORT="$PORT" \
      ANDROID_SERIAL="$SERIAL" \
      ANDROID_PACKAGE_SOURCE_BASE_URL="http://127.0.0.1:${PORT}/packages/" \
      ANDROID_LIVE_FEED_SOURCE_BASE_URL="http://127.0.0.1:${PORT}/live-feeds/" \
      ANDROID_CLOUD_SERVER_BASE_URL="$(cloud_base_url)" \
      AEROBAG_E2E_ARTIFACT_DIR="$ARTIFACT_DIR" \
      ./ui/android-app/scripts/run_e2e.sh \
        --apk "$apk" \
        --clear-app-data \
        --sync-all-available-packages \
        --release-fixture "$FIXTURE" \
        --test "$journey"
    ;;
  android-upgrade-apk)
    apk="${2:-$APP_ARTIFACTS_DIR/aerobag-release-e2e.apk}"
    [[ -f "$apk" ]] || { echo "immutable APK is missing: $apk" >&2; exit 1; }
    adb -s "$SERIAL" install -r "$apk" >/dev/null
    echo "immutable APK upgraded: $apk"
    ;;
  android-test)
    [[ -n "${2:-}" ]] || { usage >&2; exit 2; }
    cd "$ROOT"
    run_android_test "$2"
    ;;
  android-suite)
    require_fixture
    priority="${2:-all}"
    start_at="${3:-}"
    started="${start_at:+0}"
    cd "$ROOT"
    while IFS= read -r journey; do
      if [[ "$started" == "0" && "$journey" != "$start_at" ]]; then
        continue
      fi
      started="1"
      echo "=== $journey (android) ==="
      run_android_test "$journey"
    done < <(implemented_journeys android "$priority")
    [[ "$started" != "0" ]] || { echo "START_AT journey not found: $start_at" >&2; exit 2; }
    ;;
  android-implementation-suite)
    require_fixture
    priority="${2:-all}"
    start_at="${3:-}"
    started="${start_at:+0}"
    cd "$ROOT"
    AEROBAG_RELEASE_JOURNEY_IMPLEMENTATIONS_ONLY=1
    export AEROBAG_RELEASE_JOURNEY_IMPLEMENTATIONS_ONLY
    while IFS= read -r journey; do
      if [[ "$started" == "0" && "$journey" != "$start_at" ]]; then
        continue
      fi
      started="1"
      echo "=== $journey (android) ==="
      run_android_test "$journey"
    done < <(implemented_journeys android "$priority")
    [[ "$started" != "0" ]] || { echo "START_AT journey not found: $start_at" >&2; exit 2; }
    ;;
  android-offline)
    cd "$ROOT"
    run_android_test android.offline-cold-start
    ;;
  android-open-page)
    [[ -n "${2:-}" ]] || { usage >&2; exit 2; }
    cd "$ROOT"
    node --input-type=module - "$SERIAL" "$2" <<'JS'
import { AndroidSemanticJourneyDriver } from "./tools/e2e/semantic-journey-driver.mjs";
const [serial, page] = process.argv.slice(2);
const driver = new AndroidSemanticJourneyDriver(serial, { resetApp: async () => {} });
await driver.openPage(page);
console.log(`android page ready: ${page}`);
JS
    ;;
  android-action)
    [[ -n "${2:-}" ]] || { usage >&2; exit 2; }
    cd "$ROOT"
    node --input-type=module - "$SERIAL" "$2" <<'JS'
import { AndroidSemanticJourneyDriver } from "./tools/e2e/semantic-journey-driver.mjs";
const [serial, action] = process.argv.slice(2);
const driver = new AndroidSemanticJourneyDriver(serial, { resetApp: async () => {} });
await driver.performAction(action);
console.log(`android action performed: ${action}`);
JS
    ;;
  android-enter-text)
    [[ -n "${2:-}" && -n "${3:-}" ]] || { usage >&2; exit 2; }
    cd "$ROOT"
    node --input-type=module - "$SERIAL" "$2" "$3" <<'JS'
import { AndroidSemanticJourneyDriver } from "./tools/e2e/semantic-journey-driver.mjs";
const [serial, control, value] = process.argv.slice(2);
const driver = new AndroidSemanticJourneyDriver(serial, { resetApp: async () => {} });
await driver.enterText(control, value);
console.log(`android text entered: control=${control} length=${value.length}`);
JS
    ;;
  android-zoom)
    [[ -n "${2:-}" && -n "${3:-}" ]] || { usage >&2; exit 2; }
    cd "$ROOT"
    node --input-type=module - "$SERIAL" "$2" "$3" <<'JS'
import { AndroidSemanticJourneyDriver } from "./tools/e2e/semantic-journey-driver.mjs";
const [serial, surface, amount] = process.argv.slice(2);
const driver = new AndroidSemanticJourneyDriver(serial, { resetApp: async () => {} });
await driver.zoom(surface, Number(amount));
await new Promise((resolve) => setTimeout(resolve, 750));
console.log(`android zoom applied: surface=${surface} amount=${amount}`);
JS
    ;;
  android-ui)
    mkdir -p "$ARTIFACT_DIR/android-ui"
    adb -s "$SERIAL" shell uiautomator dump --compressed /sdcard/aerobag-release-journey.xml >/dev/null
    adb -s "$SERIAL" exec-out cat /sdcard/aerobag-release-journey.xml >"$ARTIFACT_DIR/android-ui/ui.xml"
    adb -s "$SERIAL" exec-out screencap -p >"$ARTIFACT_DIR/android-ui/screenshot.png"
    rg -o 'resource-id="parity:[^"]+' "$ARTIFACT_DIR/android-ui/ui.xml" || true
    ;;
  android-log)
    mkdir -p "$ARTIFACT_DIR/android-log"
    adb -s "$SERIAL" logcat -d -v threadtime >"$ARTIFACT_DIR/android-log/logcat.txt"
    rg -i "${2:-Aerobag|FATAL|exception|nexrad}" "$ARTIFACT_DIR/android-log/logcat.txt" || true
    ;;
  web-test)
    [[ -n "${2:-}" ]] || { usage >&2; exit 2; }
    cd "$ROOT"
    run_web_test "$2"
    ;;
  web-dist-test)
    [[ -n "${2:-}" ]] || { usage >&2; exit 2; }
    cd "$ROOT"
    SERVE_WEB_DIST=1
    AEROBAG_E2E_URL="$(fixture_origin)" run_web_test "$2"
    ;;
  web-suite)
    require_fixture
    priority="${2:-all}"
    cd "$ROOT"
    while IFS= read -r journey; do
      echo "=== $journey (web) ==="
      run_web_test "$journey"
    done < <(implemented_journeys web "$priority")
    ;;
  web-dist-suite)
    require_fixture
    priority="${2:-all}"
    cd "$ROOT"
    SERVE_WEB_DIST=1
    while IFS= read -r journey; do
      echo "=== $journey (web dist) ==="
      AEROBAG_E2E_URL="$(fixture_origin)" run_web_test "$journey"
    done < <(implemented_journeys web "$priority")
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
