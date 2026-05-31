#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

exec env \
  AEROBAG_DEV_SCRIPT="${AEROBAG_DEV_SCRIPT:-inner:preview:optimized-wasm}" \
  "$SCRIPT_DIR/restart-vite-dev.sh" \
  "$@"
