#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

if ! command -v reuse >/dev/null 2>&1; then
    echo "reuse is required; install it with: pipx install reuse" >&2
    exit 127
fi

exec reuse lint
