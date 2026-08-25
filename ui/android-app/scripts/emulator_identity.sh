#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

aerobag_source_instance_config() {
  local config_path="$1"
  if [[ ! -f "$config_path" ]]; then
    return
  fi

  local -A explicit_values=()
  local name
  for name in \
    VNC_PORT DISPLAY_NUM EMULATOR_CONSOLE_PORT EMULATOR_ADB_PORT \
    ANDROID_SERIAL AVD_NAME AVD_INSTANCE_NAME EMULATOR_READ_ONLY; do
    if [[ -v "$name" ]]; then
      explicit_values["$name"]="${!name}"
    fi
  done

  # shellcheck source=/dev/null
  source "$config_path"
  for name in "${!explicit_values[@]}"; do
    printf -v "$name" '%s' "${explicit_values[$name]}"
  done
}

aerobag_configure_emulator_identity() {
  VNC_PORT="${VNC_PORT:-5900}"
  if [[ ! "$VNC_PORT" =~ ^[0-9]+$ ]]; then
    echo "VNC_PORT must be numeric, got: $VNC_PORT" >&2
    return 2
  fi

  local instance_index=$((VNC_PORT - 5900))
  if ((instance_index < 0)); then
    instance_index=0
  fi
  local default_display_index="$instance_index"
  if ((default_display_index < 1)); then
    default_display_index=1
  fi

  DISPLAY_NUM="${DISPLAY_NUM:-:${default_display_index}}"
  EMULATOR_CONSOLE_PORT="${EMULATOR_CONSOLE_PORT:-$((5554 + instance_index * 2))}"
  EMULATOR_ADB_PORT="${EMULATOR_ADB_PORT:-$((EMULATOR_CONSOLE_PORT + 1))}"

  local expected_serial="emulator-${EMULATOR_CONSOLE_PORT}"
  ANDROID_SERIAL="${ANDROID_SERIAL:-$expected_serial}"
  if [[ "${AEROBAG_REQUIRE_EMULATOR_IDENTITY:-0}" == "1" && "$ANDROID_SERIAL" != "$expected_serial" ]]; then
    echo "ANDROID_SERIAL=$ANDROID_SERIAL does not match EMULATOR_CONSOLE_PORT=$EMULATOR_CONSOLE_PORT; expected $expected_serial" >&2
    return 2
  fi

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

  export VNC_PORT DISPLAY_NUM EMULATOR_CONSOLE_PORT EMULATOR_ADB_PORT
  export ANDROID_SERIAL AVD_NAME AVD_INSTANCE_NAME EMULATOR_READ_ONLY
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  set -euo pipefail
  aerobag_configure_emulator_identity
  printf 'VNC_PORT=%s\n' "$VNC_PORT"
  printf 'DISPLAY_NUM=%s\n' "$DISPLAY_NUM"
  printf 'EMULATOR_CONSOLE_PORT=%s\n' "$EMULATOR_CONSOLE_PORT"
  printf 'EMULATOR_ADB_PORT=%s\n' "$EMULATOR_ADB_PORT"
  printf 'ANDROID_SERIAL=%s\n' "$ANDROID_SERIAL"
  printf 'AVD_INSTANCE_NAME=%s\n' "$AVD_INSTANCE_NAME"
fi
