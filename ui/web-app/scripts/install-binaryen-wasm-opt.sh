#!/usr/bin/env bash
set -euo pipefail

BINARYEN_VERSION="${BINARYEN_VERSION:-version_129}"
BINARYEN_NODE_SHA256_VERSION_129="06e2dd4a29505f11bd174b9783ce022e6429eff213962331f823d99f628ecf59"

ROOT_DIR="${AEROBAG_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
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
INSTALL_ROOT="${BINARYEN_INSTALL_ROOT:-$UI_TARGET_ROOT/tools}"
INSTALL_DIR="$INSTALL_ROOT/binaryen-$BINARYEN_VERSION"
ARCHIVE="$INSTALL_ROOT/binaryen-$BINARYEN_VERSION-node.tar.gz"
URL="https://github.com/WebAssembly/binaryen/releases/download/$BINARYEN_VERSION/binaryen-$BINARYEN_VERSION-node.tar.gz"

case "$BINARYEN_VERSION" in
  version_129)
    EXPECTED_SHA256="$BINARYEN_NODE_SHA256_VERSION_129"
    ;;
  *)
    echo "No pinned checksum for $BINARYEN_VERSION; update this script before using a new Binaryen release." >&2
    exit 1
    ;;
esac

mkdir -p "$INSTALL_ROOT"

if [ ! -f "$ARCHIVE" ]; then
  if command -v wget >/dev/null 2>&1; then
    wget -O "$ARCHIVE" "$URL"
  elif command -v curl >/dev/null 2>&1; then
    curl -L -o "$ARCHIVE" "$URL"
  else
    echo "wget or curl is required to download Binaryen" >&2
    exit 1
  fi
fi

ACTUAL_SHA256="$(sha256sum "$ARCHIVE" | awk '{print $1}')"
if [ "$ACTUAL_SHA256" != "$EXPECTED_SHA256" ]; then
  echo "Binaryen archive checksum mismatch for $ARCHIVE" >&2
  echo "expected $EXPECTED_SHA256" >&2
  echo "actual   $ACTUAL_SHA256" >&2
  exit 1
fi

rm -rf "$INSTALL_DIR"
tar --no-same-owner -xf "$ARCHIVE" -C "$INSTALL_ROOT"

node "$INSTALL_DIR/wasm-opt.js" --version
printf 'Installed Binaryen wasm-opt at %s\n' "$INSTALL_DIR/wasm-opt.js"
