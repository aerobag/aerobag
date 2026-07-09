#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
ANDROID_APP_DIR="$ROOT/ui/android-app"
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
VNC_PORT="${VNC_PORT:-5900}"
DEFAULT_EMULATOR_CONSOLE_PORT="$(python3 - <<'PY' "$VNC_PORT"
import sys
port = int(sys.argv[1])
index = max(port - 5900, 0)
print(5554 + index * 2)
PY
)"
ANDROID_SERIAL="${ANDROID_SERIAL:-emulator-${EMULATOR_CONSOLE_PORT:-$DEFAULT_EMULATOR_CONSOLE_PORT}}"

cleanup() {
  if [[ "$START_EMULATOR" -eq 1 && "$KEEP_EMULATOR" -eq 0 ]]; then
    "$ANDROID_APP_DIR/scripts/stop_emulator_stack.sh" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ "$START_EMULATOR" -eq 1 ]]; then
  "$ANDROID_APP_DIR/scripts/stop_emulator_stack.sh" >/dev/null 2>&1 || true
  EMULATOR_HEADLESS="$EMULATOR_HEADLESS" "$ANDROID_APP_DIR/scripts/start_emulator_stack.sh"
fi

"$ROOT/ui/web-app/scripts/run-target-workspace.sh" \
  inner:e2e:android-chrome-livefeed \
  --serial "$ANDROID_SERIAL" \
  "${EXTRA_ARGS[@]}"
