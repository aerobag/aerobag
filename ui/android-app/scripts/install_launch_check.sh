#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP_DIR="$ROOT/ui/android-app"
APP_ID="net.jonh.aerobag.prototype"
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

mkdir -p "$GRADLE_USER_HOME" "$PROJECT_CACHE_DIR"

echo "[1/6] installDebug"
(
  cd "$ROOT"
  env GRADLE_USER_HOME="$GRADLE_USER_HOME" ANDROID_HOME="$ANDROID_HOME" ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" AEROBAG_UI_TARGET_ROOT="$AEROBAG_UI_TARGET_ROOT" "$APP_DIR/gradlew" --project-cache-dir "$PROJECT_CACHE_DIR" -p "$APP_DIR" installDebug
)

echo "[2/6] seed chart payloads"
(
  cd "$ROOT"
  env GRADLE_USER_HOME="$GRADLE_USER_HOME" ANDROID_HOME="$ANDROID_HOME" ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" AEROBAG_UI_TARGET_ROOT="$AEROBAG_UI_TARGET_ROOT" "$APP_DIR/gradlew" --project-cache-dir "$PROJECT_CACHE_DIR" -p "$APP_DIR" seedPrototypeSectionalPackages seedPrototypeChartPackages
)

echo "[3/6] clear logcat"
adb logcat -c

echo "[4/6] force-stop"
adb shell am force-stop "$APP_ID"

echo "[5/6] launch"
adb shell am start -W -n "$ACTIVITY"

echo "[6/6] wait ${WAIT_SECONDS}s and inspect"
sleep "$WAIT_SECONDS"

RESUMED="$(adb shell dumpsys activity activities | grep -E 'topResumedActivity|ResumedActivity' || true)"
CRASH_LINES="$(adb logcat -d | grep -E 'AndroidRuntime|FATAL EXCEPTION|FileNotFoundException|app died|Force removing|OutOfMemory|SQLite|Exception|libc|tombstoned|DEBUG' || true)"

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
