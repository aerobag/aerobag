#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$APP_DIR/../.." && pwd)"

if [ -n "${AEROBAG_UI_TARGET_ROOT:-}" ]; then
  UI_TARGET_ROOT="$AEROBAG_UI_TARGET_ROOT"
else
  TARGET_ROOT_RELATIVE="$(cat "$REPO_ROOT/ui/target-root.txt")"
  if [[ "$TARGET_ROOT_RELATIVE" = /* ]]; then
    UI_TARGET_ROOT="$TARGET_ROOT_RELATIVE"
  else
    UI_TARGET_ROOT="$REPO_ROOT/$TARGET_ROOT_RELATIVE"
  fi
fi

WEB_DIST="${AEROBAG_WEB_DIST:-$UI_TARGET_ROOT/web/dist}"
DOWNLOAD_DIR="$WEB_DIST/downloads"
GIT_COMMIT="$(git -C "$REPO_ROOT" rev-parse HEAD)"
SHORT_COMMIT="$(git -C "$REPO_ROOT" rev-parse --short=8 HEAD)"
BUILT_AT_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

ANDROID_VERSION_CODE="${ANDROID_VERSION_CODE:-$(($(date -u +%s) / 60))}"
ANDROID_VERSION_NAME="${ANDROID_VERSION_NAME:-0.1.0-$SHORT_COMMIT}"
ANDROID_PACKAGE_SOURCE_BASE_URL="${ANDROID_PACKAGE_SOURCE_BASE_URL:-https://aerobag.org/packages/}"
ANDROID_LIVE_FEED_SOURCE_BASE_URL="${ANDROID_LIVE_FEED_SOURCE_BASE_URL:-https://aerobag.org}"
ANDROID_TARGET_ABIS="${ANDROID_TARGET_ABIS:-arm64-v8a}"
ANDROID_BUILD_RUST_RELEASE="${ANDROID_BUILD_RUST_RELEASE:-1}"

mkdir -p "$DOWNLOAD_DIR"

env \
  AEROBAG_UI_TARGET_ROOT="$UI_TARGET_ROOT" \
  ANDROID_VERSION_CODE="$ANDROID_VERSION_CODE" \
  ANDROID_VERSION_NAME="$ANDROID_VERSION_NAME" \
  ANDROID_PACKAGE_SOURCE_BASE_URL="$ANDROID_PACKAGE_SOURCE_BASE_URL" \
  ANDROID_LIVE_FEED_SOURCE_BASE_URL="$ANDROID_LIVE_FEED_SOURCE_BASE_URL" \
  ANDROID_TARGET_ABIS="$ANDROID_TARGET_ABIS" \
  ANDROID_BUILD_RUST_RELEASE="$ANDROID_BUILD_RUST_RELEASE" \
  "$APP_DIR/gradlew" -p "$APP_DIR" :app:assembleDebug

APK_SOURCE="$UI_TARGET_ROOT/android/build/app/outputs/apk/debug/app-debug.apk"
if [ ! -f "$APK_SOURCE" ]; then
  echo "expected APK not found: $APK_SOURCE" >&2
  exit 1
fi

APK_FILENAME="aerobag-android-$SHORT_COMMIT.apk"
APK_URL="/downloads/$APK_FILENAME"
cp "$APK_SOURCE" "$DOWNLOAD_DIR/$APK_FILENAME"

cat > "$DOWNLOAD_DIR/android-apk.json" <<EOF
{
  "apk_url": "$APK_URL",
  "filename": "$APK_FILENAME",
  "git_commit": "$GIT_COMMIT",
  "version_code": $ANDROID_VERSION_CODE,
  "version_name": "$ANDROID_VERSION_NAME",
  "built_at_utc": "$BUILT_AT_UTC"
}
EOF

echo "published $APK_URL"
