#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

serial="${1:-}"
if [[ ! "$serial" =~ ^emulator-[0-9]+$ ]]; then
  echo "configure_emulator_input requires an emulator serial; refusing to change physical-device preferences" >&2
  exit 1
fi

# WindowManager animates the whole app surface independently of Compose's
# published screen coordinates. A focused, drawn control can still slide out
# from under a physical tap. Keep app animations; remove only OS window motion.
for setting in window_animation_scale transition_animation_scale; do
  if ! adb -s "$serial" shell settings put global "$setting" 0; then
    echo "failed to configure emulator $setting" >&2
    exit 1
  fi
  if ! value="$(adb -s "$serial" shell settings get global "$setting")"; then
    echo "failed to verify emulator $setting" >&2
    exit 1
  fi
  value="${value//$'\r'/}"
  if [[ "$value" != 0 && "$value" != 0.0 ]]; then
    echo "emulator did not disable $setting: $value" >&2
    exit 1
  fi
done
echo "Android emulator window transitions disabled; app animations unchanged"
