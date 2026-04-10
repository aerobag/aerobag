#!/usr/bin/env bash
set -euo pipefail

SCRIPT_NAME="${1:?missing target script name}"
shift || true

REPO_ROOT="${AEROBAG_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
TARGET_ROOT_FILE="$REPO_ROOT/ui/target-root.txt"
DEFAULT_UI_TARGET_ROOT="$(python3 - <<'PY' "$REPO_ROOT" "$TARGET_ROOT_FILE"
from pathlib import Path
import sys
repo_root = Path(sys.argv[1])
target_root_file = Path(sys.argv[2])
print((repo_root / target_root_file.read_text().strip()).resolve())
PY
)"
UI_TARGET_ROOT="${AEROBAG_UI_TARGET_ROOT:-$DEFAULT_UI_TARGET_ROOT}"
WEB_SOURCE_DIR="$REPO_ROOT/ui/web-app"
WORKSPACE_DIR="$UI_TARGET_ROOT/web/workspace"

mkdir -p "$WORKSPACE_DIR"

cp "$WEB_SOURCE_DIR/package.json" "$WORKSPACE_DIR/package.json"
cp "$WEB_SOURCE_DIR/package-lock.json" "$WORKSPACE_DIR/package-lock.json"
cp "$WEB_SOURCE_DIR/tsconfig.json" "$WORKSPACE_DIR/tsconfig.json"
cp "$WEB_SOURCE_DIR/vite.config.ts" "$WORKSPACE_DIR/vite.config.ts"
cp "$WEB_SOURCE_DIR/index.html" "$WORKSPACE_DIR/index.html"

ln -sfn "$WEB_SOURCE_DIR/src" "$WORKSPACE_DIR/src"
ln -sfn "$WEB_SOURCE_DIR/scripts" "$WORKSPACE_DIR/scripts"

SOURCE_LOCK_HASH="$(sha256sum "$WEB_SOURCE_DIR/package-lock.json" | awk '{print $1}')"
STAMP_FILE="$WORKSPACE_DIR/.package-lock.sha256"
STAGED_LOCK_HASH="$(cat "$STAMP_FILE" 2>/dev/null || true)"

if [ ! -d "$WORKSPACE_DIR/node_modules" ] || [ "$SOURCE_LOCK_HASH" != "$STAGED_LOCK_HASH" ]; then
  (cd "$WORKSPACE_DIR" && npm install)
  printf '%s\n' "$SOURCE_LOCK_HASH" > "$STAMP_FILE"
fi

cd "$WORKSPACE_DIR"
exec env AEROBAG_REPO_ROOT="$REPO_ROOT" AEROBAG_UI_TARGET_ROOT="$UI_TARGET_ROOT" npm run "$SCRIPT_NAME" -- "$@"
