#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP_DIR="$ROOT/ui/android-app"
# shellcheck source=emulator_identity.sh
source "$APP_DIR/scripts/emulator_identity.sh"
# shellcheck source=e2e_app_data.sh
source "$APP_DIR/scripts/e2e_app_data.sh"
TARGET_ROOT_FILE="$ROOT/ui/target-root.txt"
INSTANCE_CONFIG="$ROOT/../INSTANCE_CONFIG"
SKIP_INSTALL=0
APK_PATH=""
DRIVER_APK_PATH="${AEROBAG_E2E_DRIVER_APK:-}"
CLEAR_APP_DATA=0
SYNC_OFFLINE_PACKAGES=1
SYNC_ALL_AVAILABLE_PACKAGES=0
TEST_ID=""
RELEASE_FIXTURE="${AEROBAG_RELEASE_JOURNEY_FIXTURE:-}"

usage() {
  cat <<'EOF'
usage: run_e2e.sh [--apk PATH|--skip-install] [--driver-apk PATH] [--clear-app-data] [--serial SERIAL] [--route "KRNT KPWT"] [--release-fixture fixture.json] [--no-sync-offline-packages] [--sync-all-available-packages] [--test TEST_ID]

Builds and installs the Android app, then runs Android end-to-end UI tests.
Installed package data is preserved; the test runner clears only volatile UI
state before launch. Pass --clear-app-data for a clean-device package sync,
--apk to install an immutable APK built by an earlier job, or --skip-install
to run against an already-installed app.
If a clean device starts on Offline Packages, the runner syncs the NW package
set unless --no-sync-offline-packages is supplied.
EOF
}

ROUTE="KRNT KPWT"
ENV_ANDROID_SERIAL_SET="${ANDROID_SERIAL+x}"
ENV_ANDROID_SERIAL_VALUE="${ANDROID_SERIAL-}"
ENV_ANDROID_PACKAGE_SOURCE_BASE_URL_SET="${ANDROID_PACKAGE_SOURCE_BASE_URL+x}"
ENV_ANDROID_PACKAGE_SOURCE_BASE_URL_VALUE="${ANDROID_PACKAGE_SOURCE_BASE_URL-}"
ENV_ANDROID_LIVE_FEED_SOURCE_BASE_URL_SET="${ANDROID_LIVE_FEED_SOURCE_BASE_URL+x}"
ENV_ANDROID_LIVE_FEED_SOURCE_BASE_URL_VALUE="${ANDROID_LIVE_FEED_SOURCE_BASE_URL-}"
ENV_ANDROID_CLOUD_SERVER_BASE_URL_SET="${ANDROID_CLOUD_SERVER_BASE_URL+x}"
ENV_ANDROID_CLOUD_SERVER_BASE_URL_VALUE="${ANDROID_CLOUD_SERVER_BASE_URL-}"
ENV_PACKAGE_SOURCE_PORT_SET="${PACKAGE_SOURCE_PORT+x}"
ENV_PACKAGE_SOURCE_PORT_VALUE="${PACKAGE_SOURCE_PORT-}"

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --apk)
      APK_PATH="${2:-}"
      shift
      ;;
    --driver-apk)
      DRIVER_APK_PATH="${2:-}"
      shift
      ;;
    --skip-install)
      SKIP_INSTALL=1
      ;;
    --clear-app-data)
      CLEAR_APP_DATA=1
      ;;
    --serial)
      ANDROID_SERIAL="${2:-}"
      shift
      ;;
    --route)
      ROUTE="${2:-}"
      shift
      ;;
    --no-sync-offline-packages)
      SYNC_OFFLINE_PACKAGES=0
      ;;
    --sync-all-available-packages)
      SYNC_ALL_AVAILABLE_PACKAGES=1
      ;;
    --test)
      TEST_ID="${2:-}"
      shift
      ;;
    --release-fixture)
      RELEASE_FIXTURE="${2:-}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
  shift
done

aerobag_source_instance_config "$INSTANCE_CONFIG"
if [[ -n "$ENV_ANDROID_SERIAL_SET" ]]; then
  ANDROID_SERIAL="$ENV_ANDROID_SERIAL_VALUE"
fi
if [[ -n "$ENV_ANDROID_PACKAGE_SOURCE_BASE_URL_SET" ]]; then
  ANDROID_PACKAGE_SOURCE_BASE_URL="$ENV_ANDROID_PACKAGE_SOURCE_BASE_URL_VALUE"
fi
if [[ -n "$ENV_ANDROID_LIVE_FEED_SOURCE_BASE_URL_SET" ]]; then
  ANDROID_LIVE_FEED_SOURCE_BASE_URL="$ENV_ANDROID_LIVE_FEED_SOURCE_BASE_URL_VALUE"
fi
if [[ -n "$ENV_ANDROID_CLOUD_SERVER_BASE_URL_SET" ]]; then
  ANDROID_CLOUD_SERVER_BASE_URL="$ENV_ANDROID_CLOUD_SERVER_BASE_URL_VALUE"
fi
if [[ -n "$ENV_PACKAGE_SOURCE_PORT_SET" ]]; then
  PACKAGE_SOURCE_PORT="$ENV_PACKAGE_SOURCE_PORT_VALUE"
fi

source "$APP_DIR/scripts/require_android_jdk.sh"
cleanup_repo_local_tool_dirs() {
  rm -rf "$APP_DIR/.gradle" "$APP_DIR/.kotlin"
}

trap cleanup_repo_local_tool_dirs EXIT

DEFAULT_UI_TARGET_ROOT="$(python3 - <<'PY' "$ROOT" "$TARGET_ROOT_FILE"
from pathlib import Path
import sys
repo_root = Path(sys.argv[1])
target_root_file = Path(sys.argv[2])
print((repo_root / target_root_file.read_text().strip()).resolve())
PY
)"
ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-/usr/lib/android-sdk}"
ANDROID_HOME="${ANDROID_HOME:-$ANDROID_SDK_ROOT}"
AEROBAG_UI_TARGET_ROOT="${AEROBAG_UI_TARGET_ROOT:-$DEFAULT_UI_TARGET_ROOT}"
GRADLE_USER_HOME="${GRADLE_USER_HOME:-$AEROBAG_UI_TARGET_ROOT/android/gradle-user-home}"
PROJECT_CACHE_DIR="${PROJECT_CACHE_DIR:-$AEROBAG_UI_TARGET_ROOT/android/project-cache}"
aerobag_configure_emulator_identity
PACKAGE_SOURCE_PORT="${PACKAGE_SOURCE_PORT:-8083}"
ANDROID_PACKAGE_SOURCE_DEVICE_PORT="${ANDROID_PACKAGE_SOURCE_DEVICE_PORT:-$PACKAGE_SOURCE_PORT}"
ANDROID_DEV_SERVER_BASE_URL="${ANDROID_DEV_SERVER_BASE_URL:-http://127.0.0.1:${PACKAGE_SOURCE_PORT}}"
ANDROID_PACKAGE_SOURCE_BASE_URL="${ANDROID_PACKAGE_SOURCE_BASE_URL:-http://127.0.0.1:${PACKAGE_SOURCE_PORT}/packages/}"
ANDROID_LIVE_FEED_SOURCE_BASE_URL="${ANDROID_LIVE_FEED_SOURCE_BASE_URL:-}"
ANDROID_CLOUD_SERVER_BASE_URL="${ANDROID_CLOUD_SERVER_BASE_URL:-}"
AEROBAG_ANDROID_KEYSTORE="${AEROBAG_ANDROID_KEYSTORE:-/root/aerobag-credentials/android/aerobag-app.keystore}"
AEROBAG_ANDROID_KEYSTORE_PASSWORD="${AEROBAG_ANDROID_KEYSTORE_PASSWORD:-android}"
AEROBAG_ANDROID_KEY_ALIAS="${AEROBAG_ANDROID_KEY_ALIAS:-androiddebugkey}"
AEROBAG_ANDROID_KEY_PASSWORD="${AEROBAG_ANDROID_KEY_PASSWORD:-android}"

mkdir -p "$GRADLE_USER_HOME" "$PROJECT_CACHE_DIR"

echo "target=$ANDROID_SERIAL"
echo "package_source=$ANDROID_PACKAGE_SOURCE_BASE_URL"
echo "live_feed_source=${ANDROID_LIVE_FEED_SOURCE_BASE_URL:-<package-source-root>}"
echo "cloud_server=${ANDROID_CLOUD_SERVER_BASE_URL:-<not-configured>}"
echo "route=$ROUTE"

adb -s "$ANDROID_SERIAL" wait-for-device
adb -s "$ANDROID_SERIAL" reverse \
  "tcp:${ANDROID_PACKAGE_SOURCE_DEVICE_PORT}" "tcp:${PACKAGE_SOURCE_PORT}" >/dev/null || true

if [[ "$CLEAR_APP_DATA" -eq 1 && "$SKIP_INSTALL" -eq 0 ]]; then
  echo "remove installed app for clean E2E state"
  adb -s "$ANDROID_SERIAL" uninstall org.aerobag.app >/dev/null 2>&1 || true
fi

if [[ -n "$APK_PATH" ]]; then
  if [[ ! -f "$APK_PATH" ]]; then
    echo "prebuilt APK is missing: $APK_PATH" >&2
    exit 1
  fi
  echo "[1/2] install prebuilt APK: $APK_PATH"
  adb -s "$ANDROID_SERIAL" install -r "$APK_PATH" >/dev/null
elif [[ "$SKIP_INSTALL" -eq 0 ]]; then
  echo "[1/2] installRelease"
  (
    cd "$ROOT"
    env \
      GRADLE_USER_HOME="$GRADLE_USER_HOME" \
      ANDROID_HOME="$ANDROID_HOME" \
      ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" \
      AEROBAG_UI_TARGET_ROOT="$AEROBAG_UI_TARGET_ROOT" \
      AEROBAG_E2E_ENABLED=1 \
      ANDROID_BUILD_RUST_RELEASE=1 \
      ANDROID_SERIAL="$ANDROID_SERIAL" \
      ANDROID_DEV_SERVER_BASE_URL="$ANDROID_DEV_SERVER_BASE_URL" \
      ANDROID_PACKAGE_SOURCE_BASE_URL="$ANDROID_PACKAGE_SOURCE_BASE_URL" \
      ANDROID_LIVE_FEED_SOURCE_BASE_URL="$ANDROID_LIVE_FEED_SOURCE_BASE_URL" \
      ANDROID_CLOUD_SERVER_BASE_URL="$ANDROID_CLOUD_SERVER_BASE_URL" \
      AEROBAG_ANDROID_KEYSTORE="$AEROBAG_ANDROID_KEYSTORE" \
      AEROBAG_ANDROID_KEYSTORE_PASSWORD="$AEROBAG_ANDROID_KEYSTORE_PASSWORD" \
      AEROBAG_ANDROID_KEY_ALIAS="$AEROBAG_ANDROID_KEY_ALIAS" \
      AEROBAG_ANDROID_KEY_PASSWORD="$AEROBAG_ANDROID_KEY_PASSWORD" \
      "$APP_DIR/gradlew" --project-cache-dir "$PROJECT_CACHE_DIR" --no-daemon -p "$APP_DIR" \
        installRelease :app:assembleReleaseAndroidTest
  )
else
  echo "[1/2] skip installRelease"
fi

if [[ -z "$DRIVER_APK_PATH" && -n "$APK_PATH" ]]; then
  sibling_driver="$(dirname "$APK_PATH")/aerobag-e2e-driver.apk"
  if [[ -f "$sibling_driver" ]]; then
    DRIVER_APK_PATH="$sibling_driver"
  fi
fi
if [[ -z "$DRIVER_APK_PATH" && "$SKIP_INSTALL" -eq 0 ]]; then
  DRIVER_APK_PATH="$AEROBAG_UI_TARGET_ROOT/android/build/app/outputs/apk/androidTest/release/app-release-androidTest.apk"
fi
if [[ -n "$DRIVER_APK_PATH" ]]; then
  if [[ ! -f "$DRIVER_APK_PATH" ]]; then
    echo "Android E2E semantic driver APK is missing: $DRIVER_APK_PATH" >&2
    exit 1
  fi
  echo "install persistent Android semantic driver: $DRIVER_APK_PATH"
  adb -s "$ANDROID_SERIAL" install -r "$DRIVER_APK_PATH" >/dev/null
elif ! adb -s "$ANDROID_SERIAL" shell pm path org.aerobag.app.test | grep -q '^package:'; then
  echo "Android E2E requires its semantic driver; supply --driver-apk or install it first" >&2
  exit 1
fi

if [[ "$CLEAR_APP_DATA" -eq 1 && "$SKIP_INSTALL" -eq 1 ]]; then
  echo "clear app data for clean E2E state"
  aerobag_e2e_clear_app_data "$ANDROID_SERIAL"
fi

echo "[2/2] android e2e"
E2E_ARGS=(
  --serial "$ANDROID_SERIAL"
  --route "$ROUTE"
  --package-source-port "$PACKAGE_SOURCE_PORT"
)
if [[ "$SYNC_OFFLINE_PACKAGES" -eq 0 ]]; then
  E2E_ARGS+=(--no-sync-offline-packages)
fi
if [[ "$SYNC_ALL_AVAILABLE_PACKAGES" -eq 1 ]]; then
  E2E_ARGS+=(--sync-all-available-packages)
fi
if [[ -n "$TEST_ID" ]]; then
  E2E_ARGS+=(--test "$TEST_ID")
fi
if [[ -n "$RELEASE_FIXTURE" ]]; then
  E2E_ARGS+=(--release-fixture "$RELEASE_FIXTURE")
fi
node "$ROOT/tools/e2e/run-android-e2e-suite.mjs" "${E2E_ARGS[@]}"
