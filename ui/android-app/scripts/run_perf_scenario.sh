#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
INSTANCE_CONFIG="$ROOT/../INSTANCE_CONFIG"

ENV_ANDROID_SERIAL_SET="${ANDROID_SERIAL+x}"
ENV_ANDROID_SERIAL_VALUE="${ANDROID_SERIAL-}"
if [[ -f "$INSTANCE_CONFIG" ]]; then
  # shellcheck source=/dev/null
  source "$INSTANCE_CONFIG"
fi
if [[ -n "$ENV_ANDROID_SERIAL_SET" ]]; then
  ANDROID_SERIAL="$ENV_ANDROID_SERIAL_VALUE"
fi

ANDROID_SERIAL="${ANDROID_SERIAL:-emulator-5560}"
APP_ID="org.aerobag.app"
ACTIVITY="$APP_ID/.MainActivity"
SCENARIO="${1:-map_selection_freeze}"
WAIT_SECONDS="${WAIT_SECONDS:-22}"

echo "target=$ANDROID_SERIAL"
echo "scenario=$SCENARIO"

adb -s "$ANDROID_SERIAL" logcat -c
adb -s "$ANDROID_SERIAL" shell am force-stop "$APP_ID"
adb -s "$ANDROID_SERIAL" shell am start -W -n "$ACTIVITY" --es aerobag_perf_scenario "$SCENARIO"
sleep "$WAIT_SECONDS"

LOG="$(adb -s "$ANDROID_SERIAL" logcat -d -v threadtime AndroidRuntime:E AerobagPerfScenario:V ActivityManager:I '*:S')"
printf '%s\n' "$LOG"

if ! grep -q "AerobagPerfScenario: done scenario=$SCENARIO" <<<"$LOG"; then
  echo "RESULT: scenario did not complete" >&2
  exit 1
fi

if grep -q "AerobagPerfScenario: threshold_violation" <<<"$LOG"; then
  echo "RESULT: threshold violation" >&2
  exit 2
fi

echo "RESULT: scenario passed"
