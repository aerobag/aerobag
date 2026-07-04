#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
APP_DIR="$ROOT/ui/android-app"
PACKAGE_SERVER_PID=""

usage() {
  cat <<'EOF'
usage: run_e2e_ci.sh [--route "KRNT KPWT"] [--headless|--with-vnc] [--keep-emulator] [--no-package-server] [--skip-system-image-install]

Starts a CI-suitable Android E2E environment:
  1. ensures the configured Android emulator system image is installed
  2. ensures /packages/current_artifacts.json is served locally
  3. boots the repo's emulator stack
  4. builds/installs the APK and runs the Android E2E suite

The offline package sync is driven through the app UI on a clean emulator.
CI defaults to headless emulator mode. Local runs default to VNC so the emulator
can be inspected.
EOF
}

ROUTE="KRNT KPWT"
START_PACKAGE_SERVER="${START_PACKAGE_SERVER:-auto}"
INSTALL_ANDROID_SYSTEM_IMAGE="${INSTALL_ANDROID_SYSTEM_IMAGE:-1}"
if [[ -z "${KEEP_EMULATOR+x}" ]]; then
  if [[ -n "${CI:-}" ]]; then
    KEEP_EMULATOR=0
  else
    KEEP_EMULATOR=1
  fi
fi
if [[ -z "${EMULATOR_HEADLESS+x}" ]]; then
  if [[ -n "${CI:-}" ]]; then
    EMULATOR_HEADLESS=1
  else
    EMULATOR_HEADLESS=0
  fi
fi

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --route)
      ROUTE="${2:-}"
      shift
      ;;
    --keep-emulator)
      KEEP_EMULATOR=1
      ;;
    --headless)
      EMULATOR_HEADLESS=1
      ;;
    --with-vnc)
      EMULATOR_HEADLESS=0
      ;;
    --no-package-server)
      START_PACKAGE_SERVER=0
      ;;
    --skip-system-image-install)
      INSTALL_ANDROID_SYSTEM_IMAGE=0
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
  shift
done

PACKAGE_SOURCE_PORT="${PACKAGE_SOURCE_PORT:-8083}"
PACKAGE_SERVER_LISTEN="${PACKAGE_SERVER_LISTEN:-127.0.0.1:${PACKAGE_SOURCE_PORT}}"
PACKAGE_CURRENT_URL="http://127.0.0.1:${PACKAGE_SOURCE_PORT}/packages/current_artifacts.json"
AVD_PACKAGE_PATH="${AVD_PACKAGE_PATH:-system-images;android-34;google_apis;x86_64}"
SDKMANAGER_BIN="${SDKMANAGER_BIN:-sdkmanager}"

cleanup() {
  if [[ -n "$PACKAGE_SERVER_PID" ]]; then
    kill -TERM "-$PACKAGE_SERVER_PID" >/dev/null 2>&1 || true
    wait "$PACKAGE_SERVER_PID" >/dev/null 2>&1 || true
  fi
  if [[ "$KEEP_EMULATOR" -eq 0 ]]; then
    "$APP_DIR/scripts/stop_emulator_stack.sh" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

ensure_android_system_image() {
  if [[ "$INSTALL_ANDROID_SYSTEM_IMAGE" -eq 0 ]]; then
    echo "[0/4] skip Android system image install"
    return
  fi
  if ! command -v "$SDKMANAGER_BIN" >/dev/null 2>&1; then
    echo "sdkmanager not found; install Android command line tools or pass --skip-system-image-install" >&2
    exit 1
  fi
  echo "[0/4] ensure Android emulator system image"
  yes | "$SDKMANAGER_BIN" --licenses >/dev/null || true
  "$SDKMANAGER_BIN" \
    "platform-tools" \
    "emulator" \
    "platforms;android-34" \
    "$AVD_PACKAGE_PATH" >/dev/null
}

package_server_ready() {
  curl -fsS --max-time 3 "$PACKAGE_CURRENT_URL" >/dev/null
}

ensure_package_server() {
  if package_server_ready; then
    echo "[1/4] package server already ready: $PACKAGE_CURRENT_URL"
    return
  fi
  if [[ "$START_PACKAGE_SERVER" == "0" ]]; then
    echo "package server is not ready: $PACKAGE_CURRENT_URL" >&2
    exit 1
  fi
  echo "[1/4] start package server on $PACKAGE_SERVER_LISTEN"
  setsid python3 "$ROOT/tools/run_dev_stack.py" \
    --listen "$PACKAGE_SERVER_LISTEN" \
    --skip-binary-build \
    --disable-live-feeds \
    --disable-build-watch \
    --disable-pipeline-health &
  PACKAGE_SERVER_PID="$!"
  for _ in $(seq 1 60); do
    if package_server_ready; then
      return
    fi
    sleep 1
  done
  echo "package server did not become ready: $PACKAGE_CURRENT_URL" >&2
  exit 1
}

ensure_android_system_image
ensure_package_server

echo "[2/4] clean and start emulator stack"
"$APP_DIR/scripts/stop_emulator_stack.sh" >/dev/null 2>&1 || true
EMULATOR_HEADLESS="$EMULATOR_HEADLESS" "$APP_DIR/scripts/start_emulator_stack.sh"

echo "[3/4] run Android E2E"
PACKAGE_SOURCE_PORT="$PACKAGE_SOURCE_PORT" \
  "$APP_DIR/scripts/run_e2e.sh" --route "$ROUTE"

echo "[4/4] Android E2E CI run passed"
