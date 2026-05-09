#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$APP_DIR/../.." && pwd)"
DEFAULT_UI_TARGET_ROOT="$(cd "$REPO_ROOT/.." && pwd)/ui-target"

ANDROID_SERIAL="${ANDROID_SERIAL:-10.110.10.232:5555}"
ANDROID_DEV_SERVER_BASE_URL="${ANDROID_DEV_SERVER_BASE_URL:-http://10.110.44.18:8083}"
ANDROID_PACKAGE_SOURCE_BASE_URL="${ANDROID_PACKAGE_SOURCE_BASE_URL:-${ANDROID_DEV_SERVER_BASE_URL}/packages/}"
AEROBAG_UI_TARGET_ROOT="${AEROBAG_UI_TARGET_ROOT:-$DEFAULT_UI_TARGET_ROOT}"
APK="$AEROBAG_UI_TARGET_ROOT/android/build/app/outputs/apk/debug/app-debug.apk"
APP_ID="net.jonh.aerobag.prototype"

echo "target=$ANDROID_SERIAL"
echo "dev_server=$ANDROID_DEV_SERVER_BASE_URL"
echo "package_source=$ANDROID_PACKAGE_SOURCE_BASE_URL"

env \
  AEROBAG_UI_TARGET_ROOT="$AEROBAG_UI_TARGET_ROOT" \
  ANDROID_DEV_SERVER_BASE_URL="$ANDROID_DEV_SERVER_BASE_URL" \
  ANDROID_PACKAGE_SOURCE_BASE_URL="$ANDROID_PACKAGE_SOURCE_BASE_URL" \
  "$APP_DIR/gradlew" -p "$APP_DIR" assembleDebug

adb -s "$ANDROID_SERIAL" install -r "$APK"
adb -s "$ANDROID_SERIAL" shell monkey -p "$APP_ID" 1 >/dev/null

