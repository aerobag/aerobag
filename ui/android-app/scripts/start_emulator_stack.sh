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
PACKAGE_SOURCE_PORT="${PACKAGE_SOURCE_PORT:-8083}"
ANDROID_PACKAGE_SOURCE_DEVICE_PORT="${ANDROID_PACKAGE_SOURCE_DEVICE_PORT:-$PACKAGE_SOURCE_PORT}"
ANDROID_PACKAGE_SOURCE_REVERSE="${ANDROID_PACKAGE_SOURCE_REVERSE:-1}"
EMULATOR_HEADLESS="${EMULATOR_HEADLESS:-0}"
XVFB_SCREEN="${XVFB_SCREEN:-1440x3040x24}"
VNC_CLIP="${VNC_CLIP:-1080x2400+0+0}"
ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-/usr/lib/android-sdk}}"
EMULATOR_BIN="${EMULATOR_BIN:-$ANDROID_SDK_ROOT/emulator/emulator}"
AVDMANAGER_BIN="${AVDMANAGER_BIN:-avdmanager}"
EMULATOR_DATA_PARTITION_SIZE="${EMULATOR_DATA_PARTITION_SIZE:-17179869184}"

XVFB_PID_FILE="${STATE_DIR}/xvfb.pid"
X11VNC_PID_FILE="${STATE_DIR}/x11vnc.pid"
EMULATOR_PID_FILE="${STATE_DIR}/emulator.pid"
XVFB_LOG="${STATE_DIR}/xvfb.log"
X11VNC_LOG="${STATE_DIR}/x11vnc.log"
EMULATOR_LOG="${STATE_DIR}/emulator.log"
DISPLAY_READY_TIMEOUT="${DISPLAY_READY_TIMEOUT:-15}"
ADB_DEVICE_READY_TIMEOUT="${ADB_DEVICE_READY_TIMEOUT:-120}"
DISPLAY_CONFIGURATION_READY_TIMEOUT="${DISPLAY_CONFIGURATION_READY_TIMEOUT:-30}"

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
  local avd_path
  avd_path="$(
    "$AVDMANAGER_BIN" list avd | awk -v wanted="$AVD_INSTANCE_NAME" '
      /^[[:space:]]*Name:[[:space:]]*/ {
        name = $0
        sub(/^[[:space:]]*Name:[[:space:]]*/, "", name)
      }
      /^[[:space:]]*Path:[[:space:]]*/ && name == wanted {
        path = $0
        sub(/^[[:space:]]*Path:[[:space:]]*/, "", path)
        print path
        exit
      }
    '
  )"
  if [[ -z "$avd_path" ]]; then
    echo "AVD path not reported by avdmanager: $AVD_INSTANCE_NAME" >&2
    return 1
  fi

  ANDROID_AVD_HOME="$(dirname "$avd_path")"
  export ANDROID_AVD_HOME

  local config_file="${avd_path}/config.ini"
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
  if grep -q '^disk\.dataPartition\.size[[:space:]]*=' "$config_file"; then
    sed -i "s/^disk\.dataPartition\.size[[:space:]]*=.*/disk.dataPartition.size = ${EMULATOR_DATA_PARTITION_SIZE}/" "$config_file"
  else
    printf 'disk.dataPartition.size = %s\n' "$EMULATOR_DATA_PARTITION_SIZE" >>"$config_file"
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

if [[ "$EMULATOR_HEADLESS" != "1" ]]; then
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
else
  echo "headless emulator mode; skipping Xvfb and x11vnc"
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
  if [[ "$EMULATOR_HEADLESS" == "1" ]]; then
    emulator_args+=(-no-window)
  fi
  if [[ "$EMULATOR_READ_ONLY" == "1" ]]; then
    emulator_args+=(-read-only)
  fi
  if [[ "$EMULATOR_HEADLESS" == "1" ]]; then
    nohup setsid "$EMULATOR_BIN" "${emulator_args[@]}" >"$EMULATOR_LOG" 2>&1 &
  else
    DISPLAY="$DISPLAY_NUM" nohup setsid "$EMULATOR_BIN" "${emulator_args[@]}" \
      >"$EMULATOR_LOG" 2>&1 &
  fi
  echo "$!" >"$EMULATOR_PID_FILE"
  echo "started emulator (pid $!)"
fi

echo "waiting for adb device"
device_ready=0
for _ in $(seq 1 "$ADB_DEVICE_READY_TIMEOUT"); do
  if [[ "$(adb -s "$ANDROID_SERIAL" get-state 2>/dev/null || true)" == "device" ]]; then
    device_ready=1
    break
  fi
  if ! is_running "$EMULATOR_PID_FILE"; then
    echo "emulator exited before appearing in adb; see $EMULATOR_LOG" >&2
    tail -120 "$EMULATOR_LOG" >&2 || true
    exit 1
  fi
  sleep 1
done
if [[ "$device_ready" != "1" ]]; then
  echo "emulator did not appear in adb within ${ADB_DEVICE_READY_TIMEOUT}s; see $EMULATOR_LOG" >&2
  tail -120 "$EMULATOR_LOG" >&2 || true
  exit 1
fi

for _ in $(seq 1 180); do
  boot_completed="$(adb -s "$ANDROID_SERIAL" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')"
  if [[ "$boot_completed" == "1" ]]; then
    echo "emulator boot completed"
    package_manager_ready=0
    for _ in $(seq 1 120); do
      if adb -s "$ANDROID_SERIAL" shell service check package 2>/dev/null \
        | tr -d '\r' | grep -q '^Service package: found$'; then
        package_manager_ready=1
        break
      fi
      sleep 1
    done
    if [[ "$package_manager_ready" != "1" ]]; then
      echo "Android package-manager service did not become ready within 120s" >&2
      exit 1
    fi
    echo "waiting for final Android display configuration"
    display_configuration_deadline=$((SECONDS + DISPLAY_CONFIGURATION_READY_TIMEOUT))
    while ! adb -s "$ANDROID_SERIAL" shell dumpsys window displays 2>/dev/null \
      | grep -Eq 'mAppBounds=Rect\(0, [1-9][0-9]* -'; do
      if (( SECONDS >= display_configuration_deadline )); then
        echo "Android display cutout did not initialize within ${DISPLAY_CONFIGURATION_READY_TIMEOUT}s" >&2
        exit 1
      fi
      sleep 0.1
    done
    echo "Android display configuration ready"
    if [[ "$ANDROID_PACKAGE_SOURCE_REVERSE" == "1" ]]; then
      adb -s "$ANDROID_SERIAL" reverse \
        "tcp:${ANDROID_PACKAGE_SOURCE_DEVICE_PORT}" "tcp:${PACKAGE_SOURCE_PORT}" >/dev/null
      echo "PACKAGE_SOURCE_REVERSE=tcp:${ANDROID_PACKAGE_SOURCE_DEVICE_PORT}->tcp:${PACKAGE_SOURCE_PORT}"
      echo "ANDROID_PACKAGE_SOURCE_BASE_URL=http://127.0.0.1:${ANDROID_PACKAGE_SOURCE_DEVICE_PORT}/packages/"
    fi
    if [[ "$EMULATOR_HEADLESS" != "1" ]]; then
      echo "DISPLAY=$DISPLAY_NUM"
      echo "VNC=localhost:$VNC_PORT"
    else
      echo "EMULATOR_HEADLESS=1"
    fi
    echo "ANDROID_SERIAL=$ANDROID_SERIAL"
    exit 0
  fi
  sleep 1
done

echo "emulator device appeared but boot did not complete within timeout" >&2
exit 1
