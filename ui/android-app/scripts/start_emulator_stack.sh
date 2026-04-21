#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
INSTANCE_CONFIG="$ROOT/../INSTANCE_CONFIG"
if [[ -f "$INSTANCE_CONFIG" ]]; then
  # shellcheck source=/dev/null
  source "$INSTANCE_CONFIG"
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
DEFAULT_DISPLAY_NUM="$(python3 - <<'PY' "$VNC_PORT"
import sys
port = int(sys.argv[1])
print(f":{max(port - 5900, 1)}")
PY
)"
DEFAULT_EMULATOR_CONSOLE_PORT="$(python3 - <<'PY' "$VNC_PORT"
import sys
port = int(sys.argv[1])
index = max(port - 5900, 0)
print(5554 + index * 2)
PY
)"
STATE_DIR="${AEROBAG_UI_TARGET_ROOT}/android/emulator-stack-${VNC_PORT}"
DISPLAY_NUM="${DISPLAY_NUM:-$DEFAULT_DISPLAY_NUM}"
EMULATOR_CONSOLE_PORT="${EMULATOR_CONSOLE_PORT:-$DEFAULT_EMULATOR_CONSOLE_PORT}"
EMULATOR_ADB_PORT="${EMULATOR_ADB_PORT:-$((EMULATOR_CONSOLE_PORT + 1))}"
ANDROID_SERIAL="${ANDROID_SERIAL:-emulator-${EMULATOR_CONSOLE_PORT}}"
XVFB_SCREEN="${XVFB_SCREEN:-1440x3040x24}"
VNC_CLIP="${VNC_CLIP:-1080x2400+0+0}"
AVD_NAME="${AVD_NAME:-aerobag34}"
if [[ -z "${AVD_INSTANCE_NAME:-}" ]]; then
  if [[ "$VNC_PORT" == "5900" ]]; then
    AVD_INSTANCE_NAME="$AVD_NAME"
  else
    AVD_INSTANCE_NAME="${AVD_NAME}-${VNC_PORT}"
  fi
fi
if [[ -z "${EMULATOR_READ_ONLY:-}" ]]; then
  if [[ "$AVD_INSTANCE_NAME" == "$AVD_NAME" ]]; then
    EMULATOR_READ_ONLY=1
  else
    EMULATOR_READ_ONLY=0
  fi
fi
EMULATOR_BIN="${EMULATOR_BIN:-/usr/lib/android-sdk/emulator/emulator}"
AVDMANAGER_BIN="${AVDMANAGER_BIN:-avdmanager}"

XVFB_PID_FILE="${STATE_DIR}/xvfb.pid"
X11VNC_PID_FILE="${STATE_DIR}/x11vnc.pid"
EMULATOR_PID_FILE="${STATE_DIR}/emulator.pid"
XVFB_LOG="${STATE_DIR}/xvfb.log"
X11VNC_LOG="${STATE_DIR}/x11vnc.log"
EMULATOR_LOG="${STATE_DIR}/emulator.log"
DISPLAY_READY_TIMEOUT="${DISPLAY_READY_TIMEOUT:-15}"

mkdir -p "$STATE_DIR"

ensure_avd_instance() {
  if "$EMULATOR_BIN" -list-avds | grep -Fxq "$AVD_INSTANCE_NAME"; then
    return
  fi

  local package_path="${AVD_PACKAGE_PATH:-system-images;android-34;google_apis;x86_64}"
  local device_name="${AVD_DEVICE_NAME:-pixel_6}"
  echo "creating AVD instance $AVD_INSTANCE_NAME from $package_path"
  printf 'no\n' | "$AVDMANAGER_BIN" create avd \
    --name "$AVD_INSTANCE_NAME" \
    --package "$package_path" \
    --device "$device_name" \
    --force >/dev/null
}

ensure_avd_hardware_keyboard() {
  local avd_home="${ANDROID_AVD_HOME:-$HOME/.android/avd}"
  local config_file="${avd_home}/${AVD_INSTANCE_NAME}.avd/config.ini"
  if [[ ! -f "$config_file" ]]; then
    echo "AVD config not found: $config_file" >&2
    return 1
  fi

  # VNC/X11 key events only show up in the guest when the emulator exposes
  # the qwerty2 hardware-keyboard device. Fresh per-port AVDs defaulted this
  # off, which left +/- zoom dead even though adb-injected keyevents worked.
  if grep -q '^hw\.keyboard[[:space:]]*=' "$config_file"; then
    sed -i 's/^hw\.keyboard[[:space:]]*=.*/hw.keyboard = yes/' "$config_file"
  else
    printf '\nhw.keyboard = yes\n' >>"$config_file"
  fi
  if grep -q '^hw\.keyboard\.charmap[[:space:]]*=' "$config_file"; then
    sed -i 's/^hw\.keyboard\.charmap[[:space:]]*=.*/hw.keyboard.charmap = qwerty2/' "$config_file"
  else
    printf 'hw.keyboard.charmap = qwerty2\n' >>"$config_file"
  fi
}

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
  nohup setsid "$@" >"$log_file" 2>&1 &
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
  ensure_avd_instance
  ensure_avd_hardware_keyboard
  rm -f "$EMULATOR_PID_FILE"
  emulator_args=(
    "@$AVD_INSTANCE_NAME"
    -ports "$EMULATOR_CONSOLE_PORT,$EMULATOR_ADB_PORT"
    -gpu software
    -no-audio
    -no-snapshot-save
  )
  if [[ "$EMULATOR_READ_ONLY" == "1" ]]; then
    emulator_args+=(-read-only)
  fi
  DISPLAY="$DISPLAY_NUM" nohup setsid "$EMULATOR_BIN" "${emulator_args[@]}" \
    >"$EMULATOR_LOG" 2>&1 &
  echo "$!" >"$EMULATOR_PID_FILE"
  echo "started emulator (pid $!)"
fi

echo "waiting for adb device"
adb -s "$ANDROID_SERIAL" wait-for-device >/dev/null

for _ in $(seq 1 180); do
  boot_completed="$(adb -s "$ANDROID_SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')"
  if [[ "$boot_completed" == "1" ]]; then
    echo "emulator boot completed"
    echo "DISPLAY=$DISPLAY_NUM"
    echo "VNC=localhost:$VNC_PORT"
    echo "ANDROID_SERIAL=$ANDROID_SERIAL"
    exit 0
  fi
  sleep 1
done

echo "emulator device appeared but boot did not complete within timeout" >&2
exit 1
