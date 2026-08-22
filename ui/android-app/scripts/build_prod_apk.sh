#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$APP_DIR/../.." && pwd)"
source "$SCRIPT_DIR/require_android_jdk.sh"
cleanup_repo_local_tool_dirs() {
  rm -rf "$APP_DIR/.gradle" "$APP_DIR/.kotlin"
}

trap cleanup_repo_local_tool_dirs EXIT

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
GIT_COMMIT="$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || true)"
if [ -z "$GIT_COMMIT" ]; then
  GIT_COMMIT="unknown"
fi
SHORT_COMMIT="$(git -C "$REPO_ROOT" rev-parse --short=8 HEAD 2>/dev/null || true)"
if [ -z "$SHORT_COMMIT" ]; then
  SHORT_COMMIT="$GIT_COMMIT"
fi
if [ "$SHORT_COMMIT" != "unknown" ]; then
  SHORT_COMMIT="${SHORT_COMMIT:0:8}"
fi
BUILT_AT_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
BUILD_STAMP_UTC="$(date -u +%Y%m%d%H%M)"
DIRTY_STATUS="$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null || true)"
if [ -n "$DIRTY_STATUS" ]; then
  printf 'Android APK build sees dirty checkout:\n%s\n' "$DIRTY_STATUS" >&2
  BUILD_DIRTY=1
  BUILD_DIRTY_JSON=true
  BUILD_ID="$SHORT_COMMIT.dirty"
else
  BUILD_DIRTY=0
  BUILD_DIRTY_JSON=false
  BUILD_ID="$SHORT_COMMIT"
fi

ANDROID_VERSION_CODE="${ANDROID_VERSION_CODE:-$(($(date -u +%s) / 60))}"
ANDROID_VERSION_NAME="${ANDROID_VERSION_NAME:-0.1.$BUILD_STAMP_UTC+$BUILD_ID}"
ANDROID_PACKAGE_SOURCE_BASE_URL="${ANDROID_PACKAGE_SOURCE_BASE_URL:-https://aerobag.org/packages/}"
ANDROID_LIVE_FEED_SOURCE_BASE_URL="${ANDROID_LIVE_FEED_SOURCE_BASE_URL:-https://aerobag.org}"
ANDROID_CLOUD_SERVER_BASE_URL="${ANDROID_CLOUD_SERVER_BASE_URL:-https://aerobag.org/cloud/}"
ANDROID_APK_PUBLIC_BASE_URL="${ANDROID_APK_PUBLIC_BASE_URL:-/downloads}"
ANDROID_TARGET_ABIS="${ANDROID_TARGET_ABIS:-arm64-v8a}"
ANDROID_BUILD_RUST_RELEASE="${ANDROID_BUILD_RUST_RELEASE:-1}"
AEROBAG_ANDROID_KEYSTORE="${AEROBAG_ANDROID_KEYSTORE:-/root/aerobag-credentials/android/aerobag-app.keystore}"
AEROBAG_ANDROID_KEYSTORE_PASSWORD="${AEROBAG_ANDROID_KEYSTORE_PASSWORD:-android}"
AEROBAG_ANDROID_KEY_ALIAS="${AEROBAG_ANDROID_KEY_ALIAS:-androiddebugkey}"
AEROBAG_ANDROID_KEY_PASSWORD="${AEROBAG_ANDROID_KEY_PASSWORD:-android}"
AEROBAG_ANDROID_EXPECTED_CERT_SHA256="${AEROBAG_ANDROID_EXPECTED_CERT_SHA256:-09d7edbf70e51b1b6296097876bd39d19b4e71364e82166030228b5674224be1}"
ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-/usr/lib/android-sdk}}"
ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$ANDROID_HOME}"
GRADLE_USER_HOME="${GRADLE_USER_HOME:-$UI_TARGET_ROOT/android/gradle-user-home}"
PROJECT_CACHE_DIR="${PROJECT_CACHE_DIR:-$UI_TARGET_ROOT/android/project-cache}"

mkdir -p "$DOWNLOAD_DIR" "$GRADLE_USER_HOME" "$PROJECT_CACHE_DIR"

env \
  GRADLE_USER_HOME="$GRADLE_USER_HOME" \
  JAVA_TOOL_OPTIONS="$JAVA_TOOL_OPTIONS" \
  ANDROID_HOME="$ANDROID_HOME" \
  ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" \
  AEROBAG_UI_TARGET_ROOT="$UI_TARGET_ROOT" \
  AEROBAG_GIT_COMMIT="$GIT_COMMIT" \
  AEROBAG_SHORT_COMMIT="$SHORT_COMMIT" \
  AEROBAG_BUILT_AT_UTC="$BUILT_AT_UTC" \
  AEROBAG_BUILD_STAMP_UTC="$BUILD_STAMP_UTC" \
  AEROBAG_BUILD_DIRTY="$BUILD_DIRTY" \
  ANDROID_VERSION_CODE="$ANDROID_VERSION_CODE" \
  ANDROID_VERSION_NAME="$ANDROID_VERSION_NAME" \
  ANDROID_PACKAGE_SOURCE_BASE_URL="$ANDROID_PACKAGE_SOURCE_BASE_URL" \
  ANDROID_LIVE_FEED_SOURCE_BASE_URL="$ANDROID_LIVE_FEED_SOURCE_BASE_URL" \
  ANDROID_CLOUD_SERVER_BASE_URL="$ANDROID_CLOUD_SERVER_BASE_URL" \
  ANDROID_TARGET_ABIS="$ANDROID_TARGET_ABIS" \
  ANDROID_BUILD_RUST_RELEASE="$ANDROID_BUILD_RUST_RELEASE" \
  AEROBAG_ANDROID_KEYSTORE="$AEROBAG_ANDROID_KEYSTORE" \
  AEROBAG_ANDROID_KEYSTORE_PASSWORD="$AEROBAG_ANDROID_KEYSTORE_PASSWORD" \
  AEROBAG_ANDROID_KEY_ALIAS="$AEROBAG_ANDROID_KEY_ALIAS" \
  AEROBAG_ANDROID_KEY_PASSWORD="$AEROBAG_ANDROID_KEY_PASSWORD" \
  "$APP_DIR/gradlew" --project-cache-dir "$PROJECT_CACHE_DIR" --no-daemon -p "$APP_DIR" :app:assembleRelease

APK_SOURCE="$UI_TARGET_ROOT/android/build/app/outputs/apk/release/app-release.apk"
if [ ! -f "$APK_SOURCE" ]; then
  echo "expected APK not found: $APK_SOURCE" >&2
  exit 1
fi

APKSIGNER="$ANDROID_HOME/build-tools/34.0.0/apksigner"
if [ ! -x "$APKSIGNER" ]; then
  echo "missing apksigner: $APKSIGNER" >&2
  exit 1
fi
CERT_SHA256="$("$APKSIGNER" verify --print-certs "$APK_SOURCE" | awk -F': ' '/SHA-256 digest/ { gsub(":", "", $2); print tolower($2); exit }')"
if [ -z "$CERT_SHA256" ]; then
  echo "failed to read APK signing certificate from $APK_SOURCE" >&2
  exit 1
fi
if [ "$CERT_SHA256" != "$AEROBAG_ANDROID_EXPECTED_CERT_SHA256" ]; then
  echo "APK signing certificate mismatch: got $CERT_SHA256 expected $AEROBAG_ANDROID_EXPECTED_CERT_SHA256" >&2
  exit 1
fi

APK_FILENAME="aerobag-android-$SHORT_COMMIT.apk"
APK_URL="${ANDROID_APK_PUBLIC_BASE_URL%/}/$APK_FILENAME"
cp "$APK_SOURCE" "$DOWNLOAD_DIR/$APK_FILENAME"
APK_SIZE_BYTES="$(stat -c%s "$DOWNLOAD_DIR/$APK_FILENAME")"

cat > "$DOWNLOAD_DIR/android-apk.json" <<EOF
{
  "apk_url": "$APK_URL",
  "filename": "$APK_FILENAME",
  "apk_size_bytes": $APK_SIZE_BYTES,
  "git_commit": "$GIT_COMMIT",
  "cert_sha256": "$CERT_SHA256",
  "dirty": $BUILD_DIRTY_JSON,
  "version_code": $ANDROID_VERSION_CODE,
  "version_name": "$ANDROID_VERSION_NAME",
  "built_at_utc": "$BUILT_AT_UTC"
}
EOF

echo "published $APK_URL"
