#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP_DIR="$ROOT/ui/android-app"
TARGET_ROOT_FILE="$ROOT/ui/target-root.txt"
INSTANCE_CONFIG="$ROOT/../INSTANCE_CONFIG"
SKIP_INSTALL=0
SYNC_OFFLINE_PACKAGES=1

usage() {
  cat <<'EOF'
usage: run_e2e.sh [--skip-install] [--serial SERIAL] [--route "KRNT KPWT"] [--no-sync-offline-packages]

Builds and installs the Android app, then runs Android end-to-end UI tests.
Installed package data is preserved; the test runner clears only volatile UI
state before launch. Pass --skip-install to run against an already-installed app.
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
ENV_PACKAGE_SOURCE_PORT_SET="${PACKAGE_SOURCE_PORT+x}"
ENV_PACKAGE_SOURCE_PORT_VALUE="${PACKAGE_SOURCE_PORT-}"

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --skip-install)
      SKIP_INSTALL=1
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

if [[ -f "$INSTANCE_CONFIG" ]]; then
  # shellcheck source=/dev/null
  source "$INSTANCE_CONFIG"
fi
if [[ -n "$ENV_ANDROID_SERIAL_SET" ]]; then
  ANDROID_SERIAL="$ENV_ANDROID_SERIAL_VALUE"
fi
if [[ -n "$ENV_ANDROID_PACKAGE_SOURCE_BASE_URL_SET" ]]; then
  ANDROID_PACKAGE_SOURCE_BASE_URL="$ENV_ANDROID_PACKAGE_SOURCE_BASE_URL_VALUE"
fi
if [[ -n "$ENV_ANDROID_LIVE_FEED_SOURCE_BASE_URL_SET" ]]; then
  ANDROID_LIVE_FEED_SOURCE_BASE_URL="$ENV_ANDROID_LIVE_FEED_SOURCE_BASE_URL_VALUE"
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
VNC_PORT="${VNC_PORT:-5900}"
DEFAULT_EMULATOR_CONSOLE_PORT="$(python3 - <<'PY' "$VNC_PORT"
import sys
port = int(sys.argv[1])
index = max(port - 5900, 0)
print(5554 + index * 2)
PY
)"
EMULATOR_CONSOLE_PORT="${EMULATOR_CONSOLE_PORT:-$DEFAULT_EMULATOR_CONSOLE_PORT}"
ANDROID_SERIAL="${ANDROID_SERIAL:-emulator-${EMULATOR_CONSOLE_PORT}}"
PACKAGE_SOURCE_PORT="${PACKAGE_SOURCE_PORT:-8083}"
ANDROID_PACKAGE_SOURCE_BASE_URL="${ANDROID_PACKAGE_SOURCE_BASE_URL:-http://127.0.0.1:${PACKAGE_SOURCE_PORT}/packages/}"
ANDROID_LIVE_FEED_SOURCE_BASE_URL="${ANDROID_LIVE_FEED_SOURCE_BASE_URL:-}"
AEROBAG_ANDROID_KEYSTORE="${AEROBAG_ANDROID_KEYSTORE:-/root/aerobag-secrets/android/aerobag-app.keystore}"
AEROBAG_ANDROID_KEYSTORE_PASSWORD="${AEROBAG_ANDROID_KEYSTORE_PASSWORD:-android}"
AEROBAG_ANDROID_KEY_ALIAS="${AEROBAG_ANDROID_KEY_ALIAS:-androiddebugkey}"
AEROBAG_ANDROID_KEY_PASSWORD="${AEROBAG_ANDROID_KEY_PASSWORD:-android}"

mkdir -p "$GRADLE_USER_HOME" "$PROJECT_CACHE_DIR"

echo "target=$ANDROID_SERIAL"
echo "package_source=$ANDROID_PACKAGE_SOURCE_BASE_URL"
echo "live_feed_source=${ANDROID_LIVE_FEED_SOURCE_BASE_URL:-<package-source-root>}"
echo "route=$ROUTE"

adb -s "$ANDROID_SERIAL" wait-for-device
adb -s "$ANDROID_SERIAL" reverse "tcp:${PACKAGE_SOURCE_PORT}" "tcp:${PACKAGE_SOURCE_PORT}" >/dev/null || true

if [[ "$SKIP_INSTALL" -eq 0 ]]; then
  echo "[1/2] installDebug"
  (
    cd "$ROOT"
    env \
      GRADLE_USER_HOME="$GRADLE_USER_HOME" \
      ANDROID_HOME="$ANDROID_HOME" \
      ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" \
      AEROBAG_UI_TARGET_ROOT="$AEROBAG_UI_TARGET_ROOT" \
      ANDROID_SERIAL="$ANDROID_SERIAL" \
      ANDROID_PACKAGE_SOURCE_BASE_URL="$ANDROID_PACKAGE_SOURCE_BASE_URL" \
      ANDROID_LIVE_FEED_SOURCE_BASE_URL="$ANDROID_LIVE_FEED_SOURCE_BASE_URL" \
      AEROBAG_ANDROID_KEYSTORE="$AEROBAG_ANDROID_KEYSTORE" \
      AEROBAG_ANDROID_KEYSTORE_PASSWORD="$AEROBAG_ANDROID_KEYSTORE_PASSWORD" \
      AEROBAG_ANDROID_KEY_ALIAS="$AEROBAG_ANDROID_KEY_ALIAS" \
      AEROBAG_ANDROID_KEY_PASSWORD="$AEROBAG_ANDROID_KEY_PASSWORD" \
      "$APP_DIR/gradlew" --project-cache-dir "$PROJECT_CACHE_DIR" --no-daemon -p "$APP_DIR" installDebug
  )
else
  echo "[1/2] skip installDebug"
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
node "$ROOT/tools/e2e/run-android-e2e-suite.mjs" "${E2E_ARGS[@]}"
