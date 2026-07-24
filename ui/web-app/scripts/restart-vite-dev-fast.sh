#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

exec env AEROBAG_DEV_SCRIPT="inner:dev:fast" "$SCRIPT_DIR/restart-vite-dev.sh" "$@"
