#!/usr/bin/env bash
set -euo pipefail

PORT="${1:-8080}"
HOST="${HOST:-0.0.0.0}"
DEV_SCRIPT="${AEROBAG_DEV_SCRIPT:-inner:dev}"

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

list_dev_roots() {
  ps -eo pid=,args= | awk -v port="$PORT" '
    index($0, "run-target-workspace.sh") && index($0, "--port " port) && index($0, "vite") { print $1 }
  '
}

list_workspace_pids() {
  ps -eo pid=,args= | awk -v workspace="$WORKSPACE_DIR" '
    index($0, workspace "/node_modules/.bin/vite") || index($0, workspace "/node_modules/@esbuild/") { print $1 }
  '
}

kill_tree() {
  local pid="$1"
  local child
  while read -r child; do
    [ -n "$child" ] || continue
    kill_tree "$child"
  done < <(pgrep -P "$pid" || true)
  kill "$pid" 2>/dev/null || true
}

wait_for_shutdown() {
  local attempts=100
  local delay=0.1
  local remaining_roots remaining_workspace listeners
  while [ "$attempts" -gt 0 ]; do
    remaining_roots="$(list_dev_roots || true)"
    remaining_workspace="$(list_workspace_pids || true)"
    listeners="$(lsof -tiTCP:"$PORT" -sTCP:LISTEN 2>/dev/null || true)"
    if [ -z "$remaining_roots" ] && [ -z "$remaining_workspace" ] && [ -z "$listeners" ]; then
      return 0
    fi
    sleep "$delay"
    attempts=$((attempts - 1))
  done

  echo "timed out waiting for prior Vite processes on port $PORT to exit" >&2
  echo "remaining root pids:" >&2
  list_dev_roots >&2 || true
  echo "remaining workspace pids:" >&2
  list_workspace_pids >&2 || true
  echo "remaining listeners:" >&2
  lsof -iTCP:"$PORT" -sTCP:LISTEN -n -P >&2 || true
  return 1
}

while read -r pid; do
  [ -n "$pid" ] || continue
  kill_tree "$pid"
done < <(list_dev_roots || true)

while read -r pid; do
  [ -n "$pid" ] || continue
  kill "$pid" 2>/dev/null || true
done < <(list_workspace_pids || true)

wait_for_shutdown

exec env \
  AEROBAG_REPO_ROOT="$REPO_ROOT" \
  AEROBAG_UI_TARGET_ROOT="$UI_TARGET_ROOT" \
  "$WEB_SOURCE_DIR/scripts/run-target-workspace.sh" \
  "$DEV_SCRIPT" \
  --host "$HOST" \
  --port "$PORT" \
  --strictPort
