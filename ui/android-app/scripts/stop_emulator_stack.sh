#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
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
STATE_DIR="${AEROBAG_UI_TARGET_ROOT}/android/emulator-stack"
XVFB_PID_FILE="${STATE_DIR}/xvfb.pid"
X11VNC_PID_FILE="${STATE_DIR}/x11vnc.pid"
EMULATOR_PID_FILE="${STATE_DIR}/emulator.pid"

stop_pid_file() {
  local name="$1"
  local pid_file="$2"
  if [[ ! -f "$pid_file" ]]; then
    return
  fi
  local pid
  pid="$(cat "$pid_file")"
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    for _ in $(seq 1 20); do
      if ! kill -0 "$pid" 2>/dev/null; then
        break
      fi
      sleep 0.25
    done
    if kill -0 "$pid" 2>/dev/null; then
      kill -9 "$pid" 2>/dev/null || true
    fi
    echo "stopped $name (pid $pid)"
  fi
  rm -f "$pid_file"
}

adb_devices="$(adb devices 2>/dev/null || true)"
if grep -q '^emulator-' <<<"$adb_devices"; then
  adb emu kill >/dev/null 2>&1 || true
  sleep 2
fi

stop_pid_file "emulator" "$EMULATOR_PID_FILE"
stop_pid_file "x11vnc" "$X11VNC_PID_FILE"
stop_pid_file "Xvfb" "$XVFB_PID_FILE"
