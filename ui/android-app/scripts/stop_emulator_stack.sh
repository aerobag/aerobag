#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=emulator_identity.sh
source "$ROOT/ui/android-app/scripts/emulator_identity.sh"
INSTANCE_CONFIG="$ROOT/../INSTANCE_CONFIG"
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
AEROBAG_REQUIRE_EMULATOR_IDENTITY=1
aerobag_configure_emulator_identity
STATE_DIR="${AEROBAG_UI_TARGET_ROOT}/android/emulator-stack-${VNC_PORT}"
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

stop_matching_processes() {
  local name="$1"
  local pattern="$2"
  local pid
  while read -r pid; do
    if [[ -z "$pid" ]]; then
      continue
    fi
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
    echo "stopped stale $name (pid $pid)"
  done < <(pgrep -f "$pattern" || true)
}

if adb -s "$ANDROID_SERIAL" get-state >/dev/null 2>&1; then
  adb -s "$ANDROID_SERIAL" emu kill >/dev/null 2>&1 || true
  sleep 2
fi

stop_pid_file "emulator" "$EMULATOR_PID_FILE"
stop_pid_file "x11vnc" "$X11VNC_PID_FILE"
stop_pid_file "Xvfb" "$XVFB_PID_FILE"

stop_matching_processes "emulator" "qemu-system.*@$AVD_INSTANCE_NAME|qemu-system.*-ports ${EMULATOR_CONSOLE_PORT},${EMULATOR_ADB_PORT}"
stop_matching_processes "x11vnc" "x11vnc .* -rfbport ${VNC_PORT}( |$)"
stop_matching_processes "Xvfb" "Xvfb ${DISPLAY_NUM}( |$)"
