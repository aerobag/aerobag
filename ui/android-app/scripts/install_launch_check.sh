#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP_DIR="$ROOT/ui/android-app"
APP_ID="net.jonh.aerobag.prototype"
ACTIVITY="$APP_ID/.MainActivity"
GRADLE_USER_HOME="${GRADLE_USER_HOME:-$ROOT/.gradle-user-home}"
WAIT_SECONDS="${WAIT_SECONDS:-2}"

echo "[1/5] installDebug"
(
  cd "$APP_DIR"
  env GRADLE_USER_HOME="$GRADLE_USER_HOME" ./gradlew installDebug
)

echo "[2/5] clear logcat"
adb logcat -c

echo "[3/5] force-stop"
adb shell am force-stop "$APP_ID"

echo "[4/5] launch"
adb shell am start -W -n "$ACTIVITY"

echo "[5/5] wait ${WAIT_SECONDS}s and inspect"
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
