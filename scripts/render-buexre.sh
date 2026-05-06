#!/usr/bin/env bash
set -euo pipefail

BUEXRE_TEST="${BUEXRE_TEST:-writes_current_buexre_overlay}"

rm -f /tmp/procedure-plots/*

cd "$(dirname "$0")/../product/preprocessor"

BUEXRE_AIRPORT="${BUEXRE_AIRPORT:-}" \
BUEXRE_PROCEDURE="${BUEXRE_PROCEDURE:-}" \
BUEXRE_TRANSITION="${BUEXRE_TRANSITION:-}" \
BUEXRE_OUTPUT="${BUEXRE_OUTPUT:-}" \
BUEXRE_OUTPUT_DIR="${BUEXRE_OUTPUT_DIR:-}" \
cargo test -p preprocessor-procedure-geometry "$BUEXRE_TEST" -- --ignored --nocapture
