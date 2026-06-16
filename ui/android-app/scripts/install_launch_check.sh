#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
INSTANCE_CONFIG="$ROOT/../INSTANCE_CONFIG"
CLEAR_INSTALLED_PACKAGES=0

usage() {
  cat <<'EOF'
usage: install_launch_check.sh [--clear-installed-packages]

Installs, launches, and checks the Android app. By default, installed package
data is preserved. Pass --clear-installed-packages to delete installed package
directories before installing.
EOF
}

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --clear-installed-packages)
      CLEAR_INSTALLED_PACKAGES=1
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

ENV_ANDROID_SERIAL_SET="${ANDROID_SERIAL+x}"
ENV_ANDROID_SERIAL_VALUE="${ANDROID_SERIAL-}"
ENV_ANDROID_PACKAGE_SOURCE_BASE_URL_SET="${ANDROID_PACKAGE_SOURCE_BASE_URL+x}"
ENV_ANDROID_PACKAGE_SOURCE_BASE_URL_VALUE="${ANDROID_PACKAGE_SOURCE_BASE_URL-}"
ENV_ANDROID_LIVE_FEED_SOURCE_BASE_URL_SET="${ANDROID_LIVE_FEED_SOURCE_BASE_URL+x}"
ENV_ANDROID_LIVE_FEED_SOURCE_BASE_URL_VALUE="${ANDROID_LIVE_FEED_SOURCE_BASE_URL-}"
ENV_PACKAGE_SOURCE_PORT_SET="${PACKAGE_SOURCE_PORT+x}"
ENV_PACKAGE_SOURCE_PORT_VALUE="${PACKAGE_SOURCE_PORT-}"
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

APP_DIR="$ROOT/ui/android-app"
APP_ID="org.aerobag.app"
ACTIVITY="$APP_ID/.MainActivity"
TARGET_ROOT_FILE="$ROOT/ui/target-root.txt"
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
WAIT_SECONDS="${WAIT_SECONDS:-2}"
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

mkdir -p "$GRADLE_USER_HOME" "$PROJECT_CACHE_DIR"

if [[ "$CLEAR_INSTALLED_PACKAGES" -eq 1 ]]; then
  echo "[0/5] clear installed package dirs"
  adb -s "$ANDROID_SERIAL" shell run-as "$APP_ID" rm -rf \
    files/packages \
    files/chart-packages \
    files/plate-packages \
    files/data-packages >/dev/null 2>&1 || true
else
  echo "[0/5] preserve installed package dirs"
fi

echo "[1/5] installDebug"
adb -s "$ANDROID_SERIAL" reverse "tcp:${PACKAGE_SOURCE_PORT}" "tcp:${PACKAGE_SOURCE_PORT}" >/dev/null
(
  cd "$ROOT"
  env GRADLE_USER_HOME="$GRADLE_USER_HOME" ANDROID_HOME="$ANDROID_HOME" ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" AEROBAG_UI_TARGET_ROOT="$AEROBAG_UI_TARGET_ROOT" ANDROID_SERIAL="$ANDROID_SERIAL" ANDROID_PACKAGE_SOURCE_BASE_URL="$ANDROID_PACKAGE_SOURCE_BASE_URL" ANDROID_LIVE_FEED_SOURCE_BASE_URL="$ANDROID_LIVE_FEED_SOURCE_BASE_URL" "$APP_DIR/gradlew" --project-cache-dir "$PROJECT_CACHE_DIR" -p "$APP_DIR" installDebug
)

echo "[2/5] clear logcat"
adb -s "$ANDROID_SERIAL" logcat -c

echo "[3/5] force-stop"
adb -s "$ANDROID_SERIAL" shell am force-stop "$APP_ID"

echo "[4/5] launch"
adb -s "$ANDROID_SERIAL" shell am start -W -n "$ACTIVITY"

echo "[5/5] wait ${WAIT_SECONDS}s and inspect"
sleep "$WAIT_SECONDS"

RESUMED="$(adb -s "$ANDROID_SERIAL" shell dumpsys activity activities | grep -E 'topResumedActivity|ResumedActivity' || true)"
CRASH_LINES="$(adb -s "$ANDROID_SERIAL" logcat -d | grep -E 'AndroidRuntime|FATAL EXCEPTION|FileNotFoundException|app died|Force removing|OutOfMemory|SQLite|Exception|libc|tombstoned|DEBUG' || true)"

echo
echo "Resumed activity:"
echo "$RESUMED"
echo
echo "Crash lines:"
echo "$CRASH_LINES"

if grep -q "$APP_ID/.MainActivity" <<<"$RESUMED"; then
  echo
  echo "RESULT: app is resumed"
  exit 0
fi

echo
echo "RESULT: app is not resumed"
exit 1
