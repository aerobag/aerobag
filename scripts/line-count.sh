#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

if ! command -v cloc >/dev/null 2>&1; then
    echo "cloc is required; install it with: apt install cloc" >&2
    exit 127
fi

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

exec cloc --vcs=git --exclude-lang=JSON,Markdown "$@"
