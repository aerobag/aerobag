#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP_DIR="$ROOT/ui/android-app"
TARGET_ROOT_FILE="$ROOT/ui/target-root.txt"
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
AEROBAG_ANDROID_KEYSTORE="${AEROBAG_ANDROID_KEYSTORE:-/root/aerobag-credentials/android/aerobag-app.keystore}"
AEROBAG_ANDROID_KEYSTORE_PASSWORD="${AEROBAG_ANDROID_KEYSTORE_PASSWORD:-android}"
AEROBAG_ANDROID_KEY_ALIAS="${AEROBAG_ANDROID_KEY_ALIAS:-androiddebugkey}"
AEROBAG_ANDROID_KEY_PASSWORD="${AEROBAG_ANDROID_KEY_PASSWORD:-android}"

mkdir -p "$GRADLE_USER_HOME" "$PROJECT_CACHE_DIR"

python3 "$APP_DIR/scripts/check_slow_ui_calls.py"

GRADLE_TASKS=("$@")
if [[ ${#GRADLE_TASKS[@]} -eq 0 ]]; then
  GRADLE_TASKS=(test)
fi

(
  cd "$ROOT"
  env \
    GRADLE_USER_HOME="$GRADLE_USER_HOME" \
    JAVA_TOOL_OPTIONS="$JAVA_TOOL_OPTIONS" \
    ANDROID_HOME="$ANDROID_HOME" \
    ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT" \
    AEROBAG_UI_TARGET_ROOT="$AEROBAG_UI_TARGET_ROOT" \
    AEROBAG_ANDROID_KEYSTORE="$AEROBAG_ANDROID_KEYSTORE" \
    AEROBAG_ANDROID_KEYSTORE_PASSWORD="$AEROBAG_ANDROID_KEYSTORE_PASSWORD" \
    AEROBAG_ANDROID_KEY_ALIAS="$AEROBAG_ANDROID_KEY_ALIAS" \
    AEROBAG_ANDROID_KEY_PASSWORD="$AEROBAG_ANDROID_KEY_PASSWORD" \
    "$APP_DIR/gradlew" --project-cache-dir "$PROJECT_CACHE_DIR" --no-daemon -p "$APP_DIR" "${GRADLE_TASKS[@]}"
)
