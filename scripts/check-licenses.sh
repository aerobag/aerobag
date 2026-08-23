#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
required_version="$(<"${repo_root}/.reuse-tool-version")"
export PATH="${PIPX_BIN_DIR:-${HOME}/.local/bin}:${PATH}"

if ! command -v reuse >/dev/null 2>&1; then
    echo "reuse ${required_version} is required; install it with:" >&2
    echo "  pipx install 'reuse==${required_version}'" >&2
    exit 127
fi

version_line="$(reuse --version | head -n 1)"
installed_version="${version_line#reuse, version }"
if [[ "${installed_version}" != "${required_version}" ]]; then
    echo "reuse ${required_version} is required; found ${installed_version}" >&2
    echo "upgrade it with:" >&2
    echo "  pipx install --force 'reuse==${required_version}'" >&2
    exit 2
fi

exec reuse lint
