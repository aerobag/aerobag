#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACE_ROOT="${ROOT_DIR}/baseline/avare_equivalent"

exec cargo run -q -p preprocessor-cli --manifest-path "${WORKSPACE_ROOT}/Cargo.toml" -- run-full-validation "$@"
