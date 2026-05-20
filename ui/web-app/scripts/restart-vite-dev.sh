#!/usr/bin/env bash
set -euo pipefail

HOST="${HOST:-0.0.0.0}"
DEV_SCRIPT="${AEROBAG_DEV_SCRIPT:-inner:dev}"
ONLY_TEAR_DOWN=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --only-tear-down|--teardown-only)
      ONLY_TEAR_DOWN=1
      ;;
    *)
      echo "usage: $0 [--only-tear-down]" >&2
      exit 2
      ;;
  esac
  shift
done

REPO_ROOT="${AEROBAG_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"
INSTANCE_CONFIG="$REPO_ROOT/../INSTANCE_CONFIG"
if [ ! -f "$INSTANCE_CONFIG" ]; then
  echo "missing instance config: $INSTANCE_CONFIG" >&2
  exit 1
fi

# shellcheck source=/dev/null
source "$INSTANCE_CONFIG"

if [ -z "${WEB_PORT:-}" ]; then
  echo "WEB_PORT must be defined in $INSTANCE_CONFIG" >&2
  exit 1
fi

PORT="$WEB_PORT"
LIVE_FEEDS_PORT="${AEROBAG_LIVE_FEEDS_PORT:-$((PORT + 1000))}"
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
ARTIFACT_READ_PATH_FILE="$REPO_ROOT/.aerobag-artifact-read-path"
DEFAULT_ARTIFACT_READ_ROOT="$(python3 - <<'PY' "$REPO_ROOT" "$ARTIFACT_READ_PATH_FILE"
from pathlib import Path
import sys
repo_root = Path(sys.argv[1])
path_file = Path(sys.argv[2])
configured = Path(path_file.read_text().strip())
print((configured if configured.is_absolute() else repo_root / configured).resolve())
PY
)"
ARTIFACT_READ_ROOT="${AEROBAG_ARTIFACT_READ_PATH:-$DEFAULT_ARTIFACT_READ_ROOT}"
LIVE_FEEDS_ROOT="${AEROBAG_LIVE_FEEDS_ROOT:-$REPO_ROOT/../live-feeds}"
LIVE_FEEDS_LOG="${AEROBAG_LIVE_FEEDS_LOG:-/tmp/aerobag-live-feedsd-$PORT.log}"
LIVE_FEEDS_MODE="${AEROBAG_LIVE_FEEDS_MODE:-production}"
mkdir -p "$LIVE_FEEDS_ROOT"

list_dev_roots() {
  ps -eo pid=,args= | awk -v port="$PORT" '
    index($0, "run-target-workspace.sh") && index($0, "--port " port) && index($0, "vite") { print $1 }
    index($0, "npm run inner:dev") && index($0, "--port " port) { print $1 }
    index($0, "sh -c npm run generate:symbols") && index($0, "--port " port) { print $1 }
  '
}

list_workspace_pids() {
  ps -eo pid=,args= | awk -v workspace="$WORKSPACE_DIR" '
    index($0, workspace "/node_modules/.bin/vite") || index($0, workspace "/node_modules/@esbuild/") { print $1 }
  '
}

list_port_listener_pids() {
  lsof -tiTCP:"$PORT" -sTCP:LISTEN 2>/dev/null || true
}

list_live_feed_port_listener_pids() {
  lsof -tiTCP:"$LIVE_FEEDS_PORT" -sTCP:LISTEN 2>/dev/null || true
}

list_live_feed_daemon_pids() {
  ps -eo pid=,args= | awk -v port="$LIVE_FEEDS_PORT" '
    index($0, "aerobag-live-feedsd") && index($0, "--listen 127.0.0.1:" port) { print $1 }
    index($0, "cargo run") && index($0, "aerobag-live-feedsd") && index($0, "--listen 127.0.0.1:" port) { print $1 }
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
    listeners="$(list_port_listener_pids)"
    live_feed_listeners="$(list_live_feed_port_listener_pids)"
    live_feed_daemons="$(list_live_feed_daemon_pids || true)"
    if [ -z "$remaining_roots" ] && [ -z "$remaining_workspace" ] && [ -z "$listeners" ] && [ -z "$live_feed_listeners" ] && [ -z "$live_feed_daemons" ]; then
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
  echo "remaining live-feed daemons:" >&2
  list_live_feed_daemon_pids >&2 || true
  echo "remaining live-feed listeners:" >&2
  lsof -iTCP:"$LIVE_FEEDS_PORT" -sTCP:LISTEN -n -P >&2 || true
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

while read -r pid; do
  [ -n "$pid" ] || continue
  kill_tree "$pid"
done < <(list_port_listener_pids)

while read -r pid; do
  [ -n "$pid" ] || continue
  kill_tree "$pid"
done < <(list_live_feed_daemon_pids || true)

while read -r pid; do
  [ -n "$pid" ] || continue
  kill_tree "$pid"
done < <(list_live_feed_port_listener_pids)

wait_for_shutdown

if [ "$ONLY_TEAR_DOWN" -eq 1 ]; then
  exit 0
fi

LIVE_FEEDS_ARGS=(
  --live-root "$LIVE_FEEDS_ROOT"
  --listen "127.0.0.1:$LIVE_FEEDS_PORT"
  --event-interval-ms "${AEROBAG_LIVE_FEED_EVENT_INTERVAL_MS:-5000}"
)
case "$LIVE_FEEDS_MODE" in
  disabled)
    ;;
  production)
    ;;
  simulation)
    if [ -z "${AEROBAG_LIVE_FEEDS_FIXTURE_ROOT:-}" ]; then
      echo "AEROBAG_LIVE_FEEDS_FIXTURE_ROOT must be set for simulation mode" >&2
      exit 1
    fi
    LIVE_FEEDS_ARGS+=(
      --simulation
      --fixture-root "$AEROBAG_LIVE_FEEDS_FIXTURE_ROOT"
      --speedup "${AEROBAG_LIVE_FEEDS_SPEEDUP:-720}"
    )
    ;;
  *)
    echo "AEROBAG_LIVE_FEEDS_MODE must be disabled, production, or simulation" >&2
    exit 1
    ;;
esac

if [ "$LIVE_FEEDS_MODE" != "disabled" ]; then
  cargo run \
    --manifest-path "$REPO_ROOT/product/preprocessor/Cargo.toml" \
    -p live-feeds-daemon \
    --bin aerobag-live-feedsd \
    -- \
    "${LIVE_FEEDS_ARGS[@]}" \
    >"$LIVE_FEEDS_LOG" 2>&1 &
fi

LIVE_FEEDS_ENV=()
if [ "$LIVE_FEEDS_MODE" != "disabled" ]; then
  LIVE_FEEDS_ENV+=(AEROBAG_LIVE_FEEDS_ORIGIN="http://127.0.0.1:$LIVE_FEEDS_PORT")
fi

exec env \
  AEROBAG_REPO_ROOT="$REPO_ROOT" \
  AEROBAG_UI_TARGET_ROOT="$UI_TARGET_ROOT" \
  AEROBAG_ARTIFACT_READ_PATH="$ARTIFACT_READ_ROOT" \
  "${LIVE_FEEDS_ENV[@]}" \
  "$WEB_SOURCE_DIR/scripts/run-target-workspace.sh" \
  "$DEV_SCRIPT" \
  --host "$HOST" \
  --port "$PORT" \
  --strictPort
