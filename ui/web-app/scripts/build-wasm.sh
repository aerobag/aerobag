#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
CORE_DIR="$ROOT_DIR/ui/core-rust"
WEB_DIR="$ROOT_DIR/ui/web-app"
OUT_DIR="$WEB_DIR/public/generated"

if command -v wasm-bindgen >/dev/null 2>&1; then
  WASM_BINDGEN_BIN="$(command -v wasm-bindgen)"
elif [ -x "$HOME/.cargo/bin/wasm-bindgen" ]; then
  WASM_BINDGEN_BIN="$HOME/.cargo/bin/wasm-bindgen"
else
  echo "wasm-bindgen not found in PATH or \$HOME/.cargo/bin" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

(
  cd "$CORE_DIR"
  cargo build -p app-wasm --target wasm32-unknown-unknown
)

"$WASM_BINDGEN_BIN" \
  "$CORE_DIR/target/wasm32-unknown-unknown/debug/app_wasm.wasm" \
  --target web \
  --out-dir "$OUT_DIR" \
  --out-name app_wasm
