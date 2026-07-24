#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

PROFILE="${1:-debug}"
case "$PROFILE" in
  debug|release|optimized)
    ;;
  *)
    echo "usage: $0 [debug|release|optimized]" >&2
    exit 1
    ;;
esac

ROOT_DIR="${AEROBAG_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
CORE_DIR="$ROOT_DIR/ui/core-rust"
TARGET_ROOT_FILE="$ROOT_DIR/ui/target-root.txt"
DEFAULT_UI_TARGET_ROOT="$(python3 - <<'PY' "$ROOT_DIR" "$TARGET_ROOT_FILE"
from pathlib import Path
import sys
repo_root = Path(sys.argv[1])
target_root_file = Path(sys.argv[2])
print((repo_root / target_root_file.read_text().strip()).resolve())
PY
)"
UI_TARGET_ROOT="${AEROBAG_UI_TARGET_ROOT:-$DEFAULT_UI_TARGET_ROOT}"
OUT_DIR="$UI_TARGET_ROOT/web/generated"
RUST_TARGET_DIR="$UI_TARGET_ROOT/shared/rust-target"

if command -v wasm-bindgen >/dev/null 2>&1; then
  WASM_BINDGEN_BIN="$(command -v wasm-bindgen)"
elif [ -x "$HOME/.cargo/bin/wasm-bindgen" ]; then
  WASM_BINDGEN_BIN="$HOME/.cargo/bin/wasm-bindgen"
else
  echo "wasm-bindgen not found in PATH or \$HOME/.cargo/bin" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
mkdir -p "$RUST_TARGET_DIR"
STAGE_DIR="$(mktemp -d)"
cleanup_stage_dir() {
  rm -rf "$STAGE_DIR"
}
trap cleanup_stage_dir EXIT

case "$PROFILE" in
  debug)
    CARGO_PROFILE_ARGS=()
    CARGO_OUTPUT_PROFILE="debug"
    RUN_WASM_OPT=0
    ;;
  release)
    CARGO_PROFILE_ARGS=(--release)
    CARGO_OUTPUT_PROFILE="release"
    RUN_WASM_OPT="${AEROBAG_WASM_OPT:-1}"
    ;;
  optimized)
    CARGO_PROFILE_ARGS=(--profile wasm-perf)
    CARGO_OUTPUT_PROFILE="wasm-perf"
    RUN_WASM_OPT="${AEROBAG_WASM_OPT:-1}"
    ;;
esac

(
  cd "$CORE_DIR"
  CARGO_TARGET_DIR="$RUST_TARGET_DIR" cargo build "${CARGO_PROFILE_ARGS[@]}" -p app-wasm --target wasm32-unknown-unknown
)

WASM_INPUT="$RUST_TARGET_DIR/wasm32-unknown-unknown/$CARGO_OUTPUT_PROFILE/app_wasm.wasm"

"$WASM_BINDGEN_BIN" \
  "$WASM_INPUT" \
  --target web \
  --out-dir "$STAGE_DIR" \
  --out-name app_wasm

if grep -q 'Date\.now' "$STAGE_DIR/app_wasm.js"; then
  echo "generated app_wasm.js imports Date.now; pass wall-clock time explicitly through appCoreAdapter instead" >&2
  exit 1
fi

if [ "$RUN_WASM_OPT" != "0" ]; then
  if [ -n "${AEROBAG_WASM_OPT_BIN:-}" ]; then
    read -r -a WASM_OPT_CMD <<< "$AEROBAG_WASM_OPT_BIN"
  elif [ -f "$UI_TARGET_ROOT/tools/binaryen-version_129/wasm-opt.js" ]; then
    WASM_OPT_CMD=(node "$UI_TARGET_ROOT/tools/binaryen-version_129/wasm-opt.js")
  elif command -v wasm-opt >/dev/null 2>&1; then
    WASM_OPT_CMD=(wasm-opt)
  else
    echo "wasm-opt not found; run ui/web-app/scripts/install-binaryen-wasm-opt.sh, install Binaryen version 129+, or set AEROBAG_WASM_OPT=0" >&2
    exit 1
  fi

  WASM_OPT_VERSION="$("${WASM_OPT_CMD[@]}" --version)"
  WASM_OPT_MAJOR="$(printf '%s\n' "$WASM_OPT_VERSION" | sed -n 's/.*version[_ ]\([0-9][0-9]*\).*/\1/p' | head -1)"
  if [ -z "$WASM_OPT_MAJOR" ] || [ "$WASM_OPT_MAJOR" -lt 129 ]; then
    echo "wasm-opt version 129+ is required; found: $WASM_OPT_VERSION" >&2
    exit 1
  fi

  "${WASM_OPT_CMD[@]}" -O2 "$STAGE_DIR/app_wasm_bg.wasm" -o "$STAGE_DIR/app_wasm_bg.wasm.opt"
  mv "$STAGE_DIR/app_wasm_bg.wasm.opt" "$STAGE_DIR/app_wasm_bg.wasm"
fi

case "$PROFILE" in
  release|optimized)
    AEROBAG_REPO_ROOT="$ROOT_DIR" AEROBAG_WASM_GENERATED_DIR="$STAGE_DIR" node "$ROOT_DIR/ui/web-app/scripts/wasm-startup-smoke.mjs"
    ;;
esac

cp "$STAGE_DIR/app_wasm.js" "$OUT_DIR/app_wasm.js"
cp "$STAGE_DIR/app_wasm_bg.wasm" "$OUT_DIR/app_wasm_bg.wasm"
cp "$STAGE_DIR/app_wasm.d.ts" "$OUT_DIR/app_wasm.d.ts"
cp "$STAGE_DIR/app_wasm_bg.wasm.d.ts" "$OUT_DIR/app_wasm_bg.wasm.d.ts"
