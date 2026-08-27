#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
version="$(python3 - <<'PY' "$ROOT/ui/core-rust/Cargo.lock"
from pathlib import Path
import sys
import tomllib

lock = tomllib.loads(Path(sys.argv[1]).read_text())
print(next(package["version"] for package in lock["package"] if package["name"] == "wasm-bindgen"))
PY
)"
current="$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)"
if [[ "$current" != "$version" ]]; then
  cargo install wasm-bindgen-cli --version "$version" --locked --force
fi
