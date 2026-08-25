#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
ANDROID_APP_DIR="$ROOT/ui/android-app"
# shellcheck source=../../android-app/scripts/emulator_identity.sh
source "$ANDROID_APP_DIR/scripts/emulator_identity.sh"
INSTANCE_CONFIG="$ROOT/../INSTANCE_CONFIG"
START_EMULATOR=1
KEEP_EMULATOR="${KEEP_EMULATOR:-}"
EMULATOR_HEADLESS="${EMULATOR_HEADLESS:-}"
EXTRA_ARGS=()

usage() {
  cat <<'EOF'
usage: run_android_chrome_livefeed_e2e.sh [--serial SERIAL] [--web-url URL] [--headless|--with-vnc] [--keep-emulator] [--no-start-emulator] [--json]

Boots the repo Android emulator stack, launches Chrome on Android, and runs the
web live-feed reconnect E2E against a scripted local feed server.

Local runs default to keeping the emulator around and showing VNC. CI defaults
to headless mode and stops the emulator after the test.
EOF
}

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --serial)
      ANDROID_SERIAL="${2:-}"
      EXTRA_ARGS+=(--serial "$ANDROID_SERIAL")
      shift
      ;;
    --web-url)
      EXTRA_ARGS+=(--web-url "${2:-}")
      shift
      ;;
    --web-port)
      EXTRA_ARGS+=(--web-port "${2:-}")
      shift
      ;;
    --live-feed-port)
      EXTRA_ARGS+=(--live-feed-port "${2:-}")
      shift
      ;;
    --cdp-port)
      EXTRA_ARGS+=(--cdp-port "${2:-}")
      shift
      ;;
    --headless)
      EMULATOR_HEADLESS=1
      ;;
    --with-vnc)
      EMULATOR_HEADLESS=0
      ;;
    --keep-emulator)
      KEEP_EMULATOR=1
      ;;
    --no-start-emulator)
      START_EMULATOR=0
      ;;
    --json)
      EXTRA_ARGS+=(--json)
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

if [[ -z "$KEEP_EMULATOR" ]]; then
  if [[ -n "${CI:-}" ]]; then
    KEEP_EMULATOR=0
  else
    KEEP_EMULATOR=1
  fi
fi
if [[ -z "$EMULATOR_HEADLESS" ]]; then
  if [[ -n "${CI:-}" ]]; then
    EMULATOR_HEADLESS=1
  else
    EMULATOR_HEADLESS=0
  fi
fi

aerobag_source_instance_config "$INSTANCE_CONFIG"

TARGET_ROOT_FILE="$ROOT/ui/target-root.txt"
DEFAULT_UI_TARGET_ROOT="$(python3 - <<'PY' "$ROOT" "$TARGET_ROOT_FILE"
from pathlib import Path
import sys
repo_root = Path(sys.argv[1])
target_root_file = Path(sys.argv[2])
print((repo_root / target_root_file.read_text().strip()).resolve())
PY
)"
AEROBAG_UI_TARGET_ROOT="${AEROBAG_UI_TARGET_ROOT:-$DEFAULT_UI_TARGET_ROOT}"
aerobag_configure_emulator_identity

cleanup() {
  if [[ "$START_EMULATOR" -eq 1 && "$KEEP_EMULATOR" -eq 0 ]]; then
    env VNC_PORT="$VNC_PORT" ANDROID_SERIAL="$ANDROID_SERIAL" \
      "$ANDROID_APP_DIR/scripts/stop_emulator_stack.sh" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ "$START_EMULATOR" -eq 1 ]]; then
  env VNC_PORT="$VNC_PORT" ANDROID_SERIAL="$ANDROID_SERIAL" \
    "$ANDROID_APP_DIR/scripts/stop_emulator_stack.sh" >/dev/null 2>&1 || true
  env VNC_PORT="$VNC_PORT" ANDROID_SERIAL="$ANDROID_SERIAL" \
    EMULATOR_HEADLESS="$EMULATOR_HEADLESS" "$ANDROID_APP_DIR/scripts/start_emulator_stack.sh"
fi

"$ROOT/ui/web-app/scripts/run-target-workspace.sh" \
  inner:e2e:android-chrome-livefeed \
  --serial "$ANDROID_SERIAL" \
  "${EXTRA_ARGS[@]}"
