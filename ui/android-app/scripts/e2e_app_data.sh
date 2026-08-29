#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

AEROBAG_E2E_APP_PACKAGE="org.aerobag.app"

aerobag_e2e_stop_app() {
  local serial="$1"
  adb -s "$serial" shell am stop-app "$AEROBAG_E2E_APP_PACKAGE" >/dev/null
}

aerobag_e2e_clear_app_data() {
  local serial="$1"
  aerobag_e2e_stop_app "$serial"
  # `pm clear` removes the app task asynchronously; a following launch can be
  # killed by that stale removal. The E2E APK is debuggable, so clear its
  # private directory directly after the synchronous process stop instead.
  adb -s "$serial" shell run-as "$AEROBAG_E2E_APP_PACKAGE" \
    find . -mindepth 1 -delete >/dev/null
}
