#!/usr/bin/env bash
set -euo pipefail

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

(
  cd "$CORE_DIR"
  CARGO_TARGET_DIR="$RUST_TARGET_DIR" cargo build -p app-wasm --target wasm32-unknown-unknown
)

"$WASM_BINDGEN_BIN" \
  "$RUST_TARGET_DIR/wasm32-unknown-unknown/debug/app_wasm.wasm" \
  --target web \
  --out-dir "$OUT_DIR" \
  --out-name app_wasm
