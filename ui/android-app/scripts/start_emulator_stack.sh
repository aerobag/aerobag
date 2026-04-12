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
DISPLAY_NUM="${DISPLAY_NUM:-:1}"
XVFB_SCREEN="${XVFB_SCREEN:-1440x3040x24}"
VNC_CLIP="${VNC_CLIP:-1080x2400+0+0}"
VNC_PORT="${VNC_PORT:-5900}"
AVD_NAME="${AVD_NAME:-aerobag34}"
EMULATOR_BIN="${EMULATOR_BIN:-/usr/lib/android-sdk/emulator/emulator}"

XVFB_PID_FILE="${STATE_DIR}/xvfb.pid"
X11VNC_PID_FILE="${STATE_DIR}/x11vnc.pid"
EMULATOR_PID_FILE="${STATE_DIR}/emulator.pid"
XVFB_LOG="${STATE_DIR}/xvfb.log"
X11VNC_LOG="${STATE_DIR}/x11vnc.log"
EMULATOR_LOG="${STATE_DIR}/emulator.log"
DISPLAY_READY_TIMEOUT="${DISPLAY_READY_TIMEOUT:-15}"

mkdir -p "$STATE_DIR"

is_running() {
  local pid_file="$1"
  if [[ ! -f "$pid_file" ]]; then
    return 1
  fi
  local pid
  pid="$(cat "$pid_file")"
  [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null
}

start_if_needed() {
  local name="$1"
  local pid_file="$2"
  local log_file="$3"
  shift 3
  if is_running "$pid_file"; then
    echo "$name already running (pid $(cat "$pid_file"))"
    return
  fi
  rm -f "$pid_file"
  nohup "$@" >"$log_file" 2>&1 &
  echo "$!" >"$pid_file"
  echo "started $name (pid $!)"
}

start_if_needed "Xvfb" "$XVFB_PID_FILE" "$XVFB_LOG" \
  Xvfb "$DISPLAY_NUM" -screen 0 "$XVFB_SCREEN" -ac

display_is_ready() {
  DISPLAY="$DISPLAY_NUM" xdpyinfo >/dev/null 2>&1
}

for _ in $(seq 1 "$DISPLAY_READY_TIMEOUT"); do
  if display_is_ready; then
    break
  fi
  sleep 1
done

if ! display_is_ready; then
  echo "X display $DISPLAY_NUM did not become ready" >&2
  exit 1
fi

start_if_needed "x11vnc" "$X11VNC_PID_FILE" "$X11VNC_LOG" \
  x11vnc -display "$DISPLAY_NUM" -forever -shared -nopw -rfbport "$VNC_PORT" \
  -noxdamage -nowf -noscr -fixscreen 1 -ncache 0 -clip "$VNC_CLIP"

sleep 1
if ! is_running "$X11VNC_PID_FILE"; then
  echo "x11vnc failed to stay running; see $X11VNC_LOG" >&2
  exit 1
fi

if is_running "$EMULATOR_PID_FILE"; then
  echo "emulator already running (pid $(cat "$EMULATOR_PID_FILE"))"
else
  rm -f "$EMULATOR_PID_FILE"
  DISPLAY="$DISPLAY_NUM" nohup "$EMULATOR_BIN" "@$AVD_NAME" \
    -gpu software \
    -no-audio \
    -no-snapshot-save \
    >"$EMULATOR_LOG" 2>&1 &
  echo "$!" >"$EMULATOR_PID_FILE"
  echo "started emulator (pid $!)"
fi

echo "waiting for adb device"
adb wait-for-device >/dev/null

for _ in $(seq 1 180); do
  boot_completed="$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')"
  if [[ "$boot_completed" == "1" ]]; then
    echo "emulator boot completed"
    echo "DISPLAY=$DISPLAY_NUM"
    echo "VNC=localhost:$VNC_PORT"
    exit 0
  fi
  sleep 1
done

echo "emulator device appeared but boot did not complete within timeout" >&2
exit 1
