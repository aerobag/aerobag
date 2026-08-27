#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../../ui/android-app/scripts/emulator_identity.sh
source "$ROOT/ui/android-app/scripts/emulator_identity.sh"
aerobag_source_instance_config "$ROOT/../INSTANCE_CONFIG"
aerobag_configure_emulator_identity
SERIAL="$ANDROID_SERIAL"
PORT="${PACKAGE_SOURCE_PORT:-18093}"
CLOUD_PORT="${AEROBAG_E2E_CLOUD_PORT:-18094}"
ANDROID_PACKAGE_PORT="${AEROBAG_ANDROID_PACKAGE_SOURCE_DEVICE_PORT:-$PORT}"
ANDROID_CLOUD_PORT="${AEROBAG_ANDROID_CLOUD_DEVICE_PORT:-$CLOUD_PORT}"
ARTIFACT_DIR="${AEROBAG_E2E_ARTIFACT_DIR:-/tmp/aerobag-release-journey-results}"
LAB_STATE_DIR="${AEROBAG_RELEASE_JOURNEY_LAB_STATE_DIR:-/tmp/aerobag-release-journey-lab-${PORT}}"
TEST_ARTIFACTS_ROOT="${AEROBAG_TEST_ARTIFACTS_ROOT:-/tmp/aerobag-release-journey-test-artifacts}"
TARGET_ROOT_RELATIVE="$(<"$ROOT/ui/target-root.txt")"
UI_TARGET_ROOT="${AEROBAG_UI_TARGET_ROOT:-$(cd "$ROOT" && realpath "$TARGET_ROOT_RELATIVE")}"
WEB_DIST="${AEROBAG_RELEASE_JOURNEY_WEB_DIST:-$UI_TARGET_ROOT/web/dist}"
SERVE_WEB_DIST="${AEROBAG_RELEASE_JOURNEY_SERVE_WEB_DIST:-0}"
REUSE_FIXTURE="${AEROBAG_RELEASE_JOURNEY_REUSE_FIXTURE:-1}"
APP_ARTIFACTS_DIR="${AEROBAG_RELEASE_JOURNEY_APP_ARTIFACTS_DIR:-/tmp/release-e2e-apps-final}"
ANDROID_JOURNEY_TIMEOUT_SECONDS="${AEROBAG_ANDROID_JOURNEY_TIMEOUT_SECONDS:-600}"
ANDROID_BASELINE_SNAPSHOT="${AEROBAG_ANDROID_BASELINE_SNAPSHOT:-}"
JOURNEY_REPETITIONS="${AEROBAG_RELEASE_JOURNEY_REPETITIONS:-1}"
[[ "$JOURNEY_REPETITIONS" =~ ^[1-9][0-9]*$ ]] || {
  echo "AEROBAG_RELEASE_JOURNEY_REPETITIONS must be a positive integer" >&2
  exit 2
}

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
  android-baseline-save NAME   Save the prepared emulator state for fast journey resets.
  android-baseline-restore NAME
                               Restore a prepared emulator state and host port mappings.
  android-suite [p0|p1|all] [START_AT]
                               Run Android journeys, optionally resuming at START_AT.
  android-suite-shard [p0|p1|all] SHARD COUNT
                               Run every COUNTth Android journey assigned to SHARD.
  android-shard-list [p0|p1|all] SHARD COUNT
                               Print the journeys assigned to one shard.
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
  AEROBAG_TEST_ARTIFACTS_ROOT, PACKAGE_SOURCE_PORT, AEROBAG_E2E_CLOUD_PORT,
  AEROBAG_ANDROID_JOURNEY_TIMEOUT_SECONDS,
  AEROBAG_RELEASE_JOURNEY_REPETITIONS, AEROBAG_RELEASE_JOURNEY_REUSE_FIXTURE.
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
    --cloud-origin "http://127.0.0.1:${CLOUD_PORT}" \
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
  local current_web_dist_sha256=""
  local requested_web_dist_sha256=""
  if [[ "$SERVE_WEB_DIST" == "1" ]]; then
    requested_web_dist_sha256="$(sha256sum "$WEB_DIST/index.html" | awk '{print $1}')"
  fi
  health="$(curl -fsS --max-time 2 "http://127.0.0.1:${PORT}/__health" 2>/dev/null || true)"
  if [[ -n "$health" ]]; then
    read -r current serves_web current_web_dist_sha256 < <(
      python3 -c 'import json, sys; value = json.load(sys.stdin); print(value.get("live_feed_profile", ""), str(bool(value.get("serves_web_app"))).lower(), value.get("web_dist_index_sha256") or "-")' \
        <<<"$health" 2>/dev/null || true
    )
  fi
  if [[ "$REUSE_FIXTURE" == "1" && "$current" == "$profile" && \
    ( "$SERVE_WEB_DIST" != "1" || \
      ( "$serves_web" == "true" && "$current_web_dist_sha256" == "$requested_web_dist_sha256" ) ) ]]; then
    return
  fi
  if [[ "$SERVE_WEB_DIST" == "1" || "$serves_web" == "true" ]]; then
    fixture_start "$profile" "$WEB_DIST"
  else
    fixture_start "$profile"
  fi
}

reset_fixture_state() {
  curl -fsS --max-time 2 \
    -H 'Content-Type: application/json' \
    --data '{"reset":true}' \
    "http://127.0.0.1:${PORT}/__control" >/dev/null
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

android_shard_journeys() {
  local priority="$1"
  local shard="$2"
  local shard_count="$3"
  local implementations_only="${AEROBAG_RELEASE_JOURNEY_IMPLEMENTATIONS_ONLY:-0}"
  node --input-type=module - "$priority" "$shard" "$shard_count" "$implementations_only" <<'JS'
import { RELEASE_JOURNEYS } from './tools/e2e/release-journey-registry.mjs';
import { releaseJourneyImplementation } from './tools/e2e/release-journey-implementations.mjs';

const [priority, shardText, shardCountText, implementationsOnly] = process.argv.slice(2);
const shard = Number(shardText);
const shardCount = Number(shardCountText);
const journeys = RELEASE_JOURNEYS.filter((journey) => {
  if (!journey.platforms.includes('android')) return false;
  if (priority !== 'all' && journey.priority !== priority) return false;
  return Boolean(releaseJourneyImplementation(journey.id)) ||
    (implementationsOnly !== '1' && Boolean(journey.existing_test));
});
const isolated = journeys.filter((journey) => journey.android_isolated);
if (isolated.length >= shardCount) {
  throw new Error(`${isolated.length} isolated Android journeys need fewer than ${shardCount} shards`);
}
isolated.forEach((journey, index) => {
  if (index === shard) console.log(journey.id);
});
const regularShardCount = shardCount - isolated.length;
journeys.filter((journey) => !journey.android_isolated).forEach((journey, index) => {
  const assignedShard = isolated.length + (index % regularShardCount);
  if (assignedShard === shard) console.log(journey.id);
});
JS
}

android_baseline_save() {
  local name="$1"
  adb -s "$SERIAL" shell am force-stop org.aerobag.app >/dev/null
  adb -s "$SERIAL" emu avd snapshot save "$name" >/dev/null
  echo "Android journey baseline saved: $name"
}

android_baseline_restore() {
  local name="$1"
  adb -s "$SERIAL" emu avd snapshot load "$name" >/dev/null
  for _ in $(seq 1 100); do
    if adb -s "$SERIAL" get-state >/dev/null 2>&1 &&
      [[ "$(adb -s "$SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == "1" ]]; then
      adb -s "$SERIAL" reverse "tcp:${ANDROID_PACKAGE_PORT}" "tcp:${PORT}" >/dev/null
      return
    fi
    sleep 0.1
  done
  echo "Android emulator did not recover after restoring snapshot $name" >&2
  adb -s "$SERIAL" get-state >&2 || true
  adb -s "$SERIAL" shell getprop sys.boot_completed >&2 || true
  exit 1
}

run_android_test() {
  local journey="$1"
  local profile
  local -a artifact_env=()
  local run_artifact_dir="$ARTIFACT_DIR"
  if [[ "$JOURNEY_REPETITIONS" -gt 1 ]]; then
    run_artifact_dir="$ARTIFACT_DIR/repeat-${AEROBAG_E2E_REPEAT_INDEX:-1}"
  fi
  require_fixture
  profile="$(journey_profile "$journey")"
  ensure_journey_profile "$profile"
  reset_fixture_state
  if [[ -n "$ANDROID_BASELINE_SNAPSHOT" ]]; then
    android_baseline_restore "$ANDROID_BASELINE_SNAPSHOT"
  fi
  if [[ "$journey" == "shared.cloud-crossfill" ]]; then
    cloud_start
    adb -s "$SERIAL" reverse "tcp:${ANDROID_CLOUD_PORT}" "tcp:${CLOUD_PORT}" >/dev/null
  fi
  if [[ -d "$TEST_ARTIFACTS_ROOT/e2e/android-rotation-live-feed" ]]; then
    artifact_env+=("AEROBAG_TEST_ARTIFACTS_ROOT=$TEST_ARTIFACTS_ROOT")
  fi
  local -a state_args=(--clear-app-data --sync-all-available-packages)
  if [[ -n "$ANDROID_BASELINE_SNAPSHOT" ]]; then
    state_args=()
  fi
  timeout --foreground --kill-after=15s "${ANDROID_JOURNEY_TIMEOUT_SECONDS}s" \
    env \
    "${artifact_env[@]}" \
    PACKAGE_SOURCE_PORT="$PORT" \
    ANDROID_PACKAGE_SOURCE_DEVICE_PORT="$ANDROID_PACKAGE_PORT" \
    ANDROID_SERIAL="$SERIAL" \
    ANDROID_PACKAGE_SOURCE_BASE_URL="http://127.0.0.1:${ANDROID_PACKAGE_PORT}/packages/" \
    ANDROID_LIVE_FEED_SOURCE_BASE_URL="http://127.0.0.1:${ANDROID_PACKAGE_PORT}/live-feeds/" \
    ANDROID_CLOUD_SERVER_BASE_URL="http://127.0.0.1:${ANDROID_CLOUD_PORT}/cloud/" \
    AEROBAG_E2E_PEER_URL="${AEROBAG_E2E_PEER_URL:-http://127.0.0.1:8085/}" \
    AEROBAG_E2E_CLOUD_PORT="$CLOUD_PORT" \
    AEROBAG_E2E_ARTIFACT_DIR="$run_artifact_dir" \
    AEROBAG_ANDROID_SEMANTIC_DRIVER_REQUIRED="${AEROBAG_ANDROID_SEMANTIC_DRIVER_REQUIRED:-0}" \
    "$ROOT/ui/android-app/scripts/run_e2e.sh" \
      --skip-install \
      "${state_args[@]}" \
      --release-fixture "$FIXTURE" \
      --test "$journey" </dev/null
}

run_web_test() {
  local journey="$1"
  local profile
  local run_artifact_dir="$ARTIFACT_DIR"
  if [[ "$JOURNEY_REPETITIONS" -gt 1 ]]; then
    run_artifact_dir="$ARTIFACT_DIR/repeat-${AEROBAG_E2E_REPEAT_INDEX:-1}"
  fi
  require_fixture
  profile="$(journey_profile "$journey")"
  ensure_journey_profile "$profile"
  reset_fixture_state
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
    --artifact-dir "$run_artifact_dir/$journey/web"
}

run_repetitions() {
  local platform="$1"
  local journey="$2"
  local iteration
  for iteration in $(seq 1 "$JOURNEY_REPETITIONS"); do
    echo "=== $journey ($platform repeat $iteration/$JOURNEY_REPETITIONS) ==="
    if [[ "$platform" == "android" ]]; then
      if AEROBAG_E2E_REPEAT_INDEX="$iteration" run_android_test "$journey"; then
        :
      else
        return $?
      fi
    else
      if AEROBAG_E2E_REPEAT_INDEX="$iteration" run_web_test "$journey"; then
        :
      else
        return $?
      fi
    fi
  done
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
      ANDROID_PACKAGE_SOURCE_BASE_URL="http://127.0.0.1:${ANDROID_PACKAGE_PORT}/packages/" \
      ANDROID_LIVE_FEED_SOURCE_BASE_URL="http://127.0.0.1:${ANDROID_PACKAGE_PORT}/live-feeds/" \
      ANDROID_CLOUD_SERVER_BASE_URL="http://127.0.0.1:${ANDROID_CLOUD_PORT}/cloud/" \
      ./ui/android-app/scripts/test.sh :app:installDebug
    ;;
  android-install)
    require_fixture
    journey="${2:-shared.startup-navigation}"
    if [[ "$journey" == "shared.cloud-crossfill" ]]; then
      cloud_start
      adb -s "$SERIAL" reverse "tcp:${ANDROID_CLOUD_PORT}" "tcp:${CLOUD_PORT}" >/dev/null
    fi
    cd "$ROOT"
    env \
      PACKAGE_SOURCE_PORT="$PORT" \
      ANDROID_PACKAGE_SOURCE_DEVICE_PORT="$ANDROID_PACKAGE_PORT" \
      ANDROID_SERIAL="$SERIAL" \
      ANDROID_PACKAGE_SOURCE_BASE_URL="http://127.0.0.1:${ANDROID_PACKAGE_PORT}/packages/" \
      ANDROID_LIVE_FEED_SOURCE_BASE_URL="http://127.0.0.1:${ANDROID_PACKAGE_PORT}/live-feeds/" \
      ANDROID_CLOUD_SERVER_BASE_URL="http://127.0.0.1:${ANDROID_CLOUD_PORT}/cloud/" \
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
      ANDROID_PACKAGE_SOURCE_DEVICE_PORT="$ANDROID_PACKAGE_PORT" \
      ANDROID_SERIAL="$SERIAL" \
      ANDROID_PACKAGE_SOURCE_BASE_URL="http://127.0.0.1:${ANDROID_PACKAGE_PORT}/packages/" \
      ANDROID_LIVE_FEED_SOURCE_BASE_URL="http://127.0.0.1:${ANDROID_PACKAGE_PORT}/live-feeds/" \
      ANDROID_CLOUD_SERVER_BASE_URL="http://127.0.0.1:${ANDROID_CLOUD_PORT}/cloud/" \
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
    run_repetitions android "$2"
    ;;
  android-baseline-save)
    name="${2:-$ANDROID_BASELINE_SNAPSHOT}"
    [[ -n "$name" ]] || { echo "android-baseline-save requires NAME" >&2; exit 2; }
    android_baseline_save "$name"
    ;;
  android-baseline-restore)
    name="${2:-$ANDROID_BASELINE_SNAPSHOT}"
    [[ -n "$name" ]] || { echo "android-baseline-restore requires NAME" >&2; exit 2; }
    android_baseline_restore "$name"
    echo "Android journey baseline restored: $name"
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
      run_repetitions android "$journey" || exit $?
    done < <(implemented_journeys android "$priority")
    [[ "$started" != "0" ]] || { echo "START_AT journey not found: $start_at" >&2; exit 2; }
    ;;
  android-suite-shard)
    require_fixture
    priority="${2:-all}"
    shard="${3:-}"
    shard_count="${4:-}"
    [[ "$shard" =~ ^[0-9]+$ && "$shard_count" =~ ^[1-9][0-9]*$ && "$shard" -lt "$shard_count" ]] || {
      echo "android-suite-shard requires 0 <= SHARD < COUNT" >&2
      exit 2
    }
    selected=0
    cd "$ROOT"
    while IFS= read -r journey; do
      selected=1
      run_repetitions android "$journey" || exit $?
    done < <(android_shard_journeys "$priority" "$shard" "$shard_count")
    [[ "$selected" == "1" ]] || echo "android shard $shard/$shard_count has no $priority journeys"
    ;;
  android-shard-list)
    priority="${2:-all}"
    shard="${3:-}"
    shard_count="${4:-}"
    [[ "$shard" =~ ^[0-9]+$ && "$shard_count" =~ ^[1-9][0-9]*$ && "$shard" -lt "$shard_count" ]] || {
      echo "android-shard-list requires 0 <= SHARD < COUNT" >&2
      exit 2
    }
    cd "$ROOT"
    android_shard_journeys "$priority" "$shard" "$shard_count"
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
      run_repetitions android "$journey" || exit $?
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
    run_repetitions web "$2"
    ;;
  web-dist-test)
    [[ -n "${2:-}" ]] || { usage >&2; exit 2; }
    cd "$ROOT"
    SERVE_WEB_DIST=1
    AEROBAG_E2E_URL="$(fixture_origin)" run_repetitions web "$2"
    ;;
  web-suite)
    require_fixture
    priority="${2:-all}"
    cd "$ROOT"
    while IFS= read -r journey; do
      run_repetitions web "$journey" || exit $?
    done < <(implemented_journeys web "$priority")
    ;;
  web-dist-suite)
    require_fixture
    priority="${2:-all}"
    cd "$ROOT"
    SERVE_WEB_DIST=1
    while IFS= read -r journey; do
      AEROBAG_E2E_URL="$(fixture_origin)" run_repetitions web "$journey" || exit $?
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
