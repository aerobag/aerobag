#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT="${1:?usage: build_release_e2e_apps.sh OUTPUT_DIR}"
TARGET_ROOT_FILE="$ROOT/ui/target-root.txt"
DEFAULT_TARGET_ROOT="$(python3 - <<'PY' "$ROOT" "$TARGET_ROOT_FILE"
from pathlib import Path
import sys
root = Path(sys.argv[1])
print((root / Path(sys.argv[2]).read_text().strip()).resolve())
PY
)"
AEROBAG_UI_TARGET_ROOT="${AEROBAG_UI_TARGET_ROOT:-$DEFAULT_TARGET_ROOT}"
GRADLE_USER_HOME="${GRADLE_USER_HOME:-$AEROBAG_UI_TARGET_ROOT/android/gradle-user-home}"
GIT_COMMIT="$(git -C "$ROOT" rev-parse HEAD)"
SHORT_COMMIT="$(git -C "$ROOT" rev-parse --short=8 HEAD)"
BUILT_AT_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
BUILD_STAMP_UTC="$(date -u +%Y%m%d%H%M)"
# Immutable APKs always address stable emulator-local ports. Parallel lanes
# isolate their host daemons with adb reverse; host ports must never leak into
# the built application.
ANDROID_PACKAGE_SOURCE_DEVICE_PORT="${AEROBAG_ANDROID_PACKAGE_SOURCE_DEVICE_PORT:-18093}"
ANDROID_CLOUD_DEVICE_PORT="${AEROBAG_ANDROID_CLOUD_DEVICE_PORT:-18094}"
PACKAGE_ORIGIN="http://127.0.0.1:${ANDROID_PACKAGE_SOURCE_DEVICE_PORT}"
CLOUD_BASE_URL="http://127.0.0.1:${ANDROID_CLOUD_DEVICE_PORT}/cloud/"

if [[ -n "$(git -C "$ROOT" status --porcelain)" && -n "${CI:-}" ]]; then
  echo "release E2E apps must be built from a clean CI checkout" >&2
  exit 1
fi

AEROBAG_ANDROID_KEYSTORE="${AEROBAG_ANDROID_KEYSTORE:-/root/aerobag-credentials/android/aerobag-app.keystore}"
AEROBAG_ANDROID_KEYSTORE_PASSWORD="${AEROBAG_ANDROID_KEYSTORE_PASSWORD:-android}"
AEROBAG_ANDROID_KEY_ALIAS="${AEROBAG_ANDROID_KEY_ALIAS:-androiddebugkey}"
AEROBAG_ANDROID_KEY_PASSWORD="${AEROBAG_ANDROID_KEY_PASSWORD:-android}"
test -f "$AEROBAG_ANDROID_KEYSTORE"

mkdir -p "$OUTPUT" "$GRADLE_USER_HOME"

env \
  AEROBAG_UI_TARGET_ROOT="$AEROBAG_UI_TARGET_ROOT" \
  GRADLE_USER_HOME="$GRADLE_USER_HOME" \
  AEROBAG_E2E_ENABLED=1 \
  AEROBAG_GIT_COMMIT="$GIT_COMMIT" \
  AEROBAG_SHORT_COMMIT="$SHORT_COMMIT" \
  AEROBAG_BUILT_AT_UTC="$BUILT_AT_UTC" \
  AEROBAG_BUILD_STAMP_UTC="$BUILD_STAMP_UTC" \
  AEROBAG_BUILD_DIRTY=0 \
  npm --prefix "$ROOT/ui/web-app" run build:optimized

env \
  AEROBAG_UI_TARGET_ROOT="$AEROBAG_UI_TARGET_ROOT" \
  GRADLE_USER_HOME="$GRADLE_USER_HOME" \
  AEROBAG_GIT_COMMIT="$GIT_COMMIT" \
  AEROBAG_SHORT_COMMIT="$SHORT_COMMIT" \
  AEROBAG_BUILT_AT_UTC="$BUILT_AT_UTC" \
  AEROBAG_BUILD_STAMP_UTC="$BUILD_STAMP_UTC" \
  AEROBAG_BUILD_DIRTY=0 \
  AEROBAG_E2E_ENABLED=1 \
  ANDROID_DEV_SERVER_BASE_URL="$PACKAGE_ORIGIN" \
  ANDROID_PACKAGE_SOURCE_BASE_URL="$PACKAGE_ORIGIN/packages/" \
  ANDROID_LIVE_FEED_SOURCE_BASE_URL="$PACKAGE_ORIGIN" \
  ANDROID_CLOUD_SERVER_BASE_URL="$CLOUD_BASE_URL" \
  ANDROID_TARGET_ABIS="${ANDROID_TARGET_ABIS:-x86_64}" \
  ANDROID_BUILD_RUST_RELEASE=1 \
  AEROBAG_ANDROID_KEYSTORE="$AEROBAG_ANDROID_KEYSTORE" \
  AEROBAG_ANDROID_KEYSTORE_PASSWORD="$AEROBAG_ANDROID_KEYSTORE_PASSWORD" \
  AEROBAG_ANDROID_KEY_ALIAS="$AEROBAG_ANDROID_KEY_ALIAS" \
  AEROBAG_ANDROID_KEY_PASSWORD="$AEROBAG_ANDROID_KEY_PASSWORD" \
  "$ROOT/ui/android-app/gradlew" \
    --project-cache-dir "$AEROBAG_UI_TARGET_ROOT/android/project-cache" \
    --no-daemon -p "$ROOT/ui/android-app" \
    :app:assembleRelease :app:assembleReleaseAndroidTest

env CARGO_TARGET_DIR="$AEROBAG_UI_TARGET_ROOT/services" \
  cargo build --manifest-path "$ROOT/services/Cargo.toml" -p aerobag-cloud-server

WEB_DIST="$AEROBAG_UI_TARGET_ROOT/web/dist"
APK="$AEROBAG_UI_TARGET_ROOT/android/build/app/outputs/apk/release/app-release.apk"
DRIVER_APK="$AEROBAG_UI_TARGET_ROOT/android/build/app/outputs/apk/androidTest/release/app-release-androidTest.apk"
CLOUD_SERVER="$AEROBAG_UI_TARGET_ROOT/services/debug/aerobag-cloud-serverd"
test -f "$WEB_DIST/index.html"
test -f "$APK"
test -f "$DRIVER_APK"
test -x "$CLOUD_SERVER"
rm -rf "$OUTPUT/web-dist"
cp -a "$WEB_DIST" "$OUTPUT/web-dist"
# Production serves this tree at /icons independently of the Vite bundle.
# Bundle it for release journeys so their app origin is self-contained too.
cp -a "$ROOT/ui/icons" "$OUTPUT/web-dist/icons"
cp "$APK" "$OUTPUT/aerobag-release-e2e.apk"
cp "$DRIVER_APK" "$OUTPUT/aerobag-e2e-driver.apk"
cp "$CLOUD_SERVER" "$OUTPUT/aerobag-cloud-serverd"

python3 - <<'PY' "$OUTPUT" "$GIT_COMMIT" "$BUILT_AT_UTC" "$ANDROID_PACKAGE_SOURCE_DEVICE_PORT" "$ANDROID_CLOUD_DEVICE_PORT"
import hashlib
import json
from pathlib import Path
import sys

root = Path(sys.argv[1])
apk = root / "aerobag-release-e2e.apk"
driver_apk = root / "aerobag-e2e-driver.apk"
cloud_server = root / "aerobag-cloud-serverd"
manifest = {
    "schema_version": 1,
    "git_commit": sys.argv[2],
    "built_at_utc": sys.argv[3],
    "endpoints": {
        "package_source_port": int(sys.argv[4]),
        "cloud_port": int(sys.argv[5]),
    },
    "android_apk": {
        "path": apk.name,
        "size_bytes": apk.stat().st_size,
        "sha256": hashlib.sha256(apk.read_bytes()).hexdigest(),
    },
    "android_e2e_driver_apk": {
        "path": driver_apk.name,
        "size_bytes": driver_apk.stat().st_size,
        "sha256": hashlib.sha256(driver_apk.read_bytes()).hexdigest(),
        "protocol": "aerobag-semantic-driver/4",
    },
    "cloud_server": {
        "path": cloud_server.name,
        "size_bytes": cloud_server.stat().st_size,
        "sha256": hashlib.sha256(cloud_server.read_bytes()).hexdigest(),
    },
    "web_dist": {"path": "web-dist"},
}
(root / "build-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
PY

python3 "$ROOT/tools/ci/verify_release_e2e_apps.py" "$OUTPUT"
