#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LEGACY_DIR="${ROOT_DIR}/legacy-capture"
IMAGE_TAG="${IMAGE_TAG:-aerobag/legacy-capture:local}"
RUNTIME="${RUNTIME:-docker}"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
OUTPUT_ROOT="${OUTPUT_ROOT:-${ROOT_DIR}/runs/${RUN_ID}}"
CACHE_ROOT="${CACHE_ROOT:-${ROOT_DIR}/cache}"

mkdir -p "${OUTPUT_ROOT}" "${CACHE_ROOT}"

if ! command -v "${RUNTIME}" >/dev/null 2>&1; then
    echo "runtime not found: ${RUNTIME}" >&2
    echo "set RUNTIME=docker or RUNTIME=podman" >&2
    exit 1
fi

"${RUNTIME}" build \
    --tag "${IMAGE_TAG}" \
    "${LEGACY_DIR}"

"${RUNTIME}" run --rm \
    --name "legacy-capture-${RUN_ID}" \
    -e RUN_ID="${RUN_ID}" \
    -e ROOT_DIR="/work" \
    -e OUTPUT_ROOT="/capture" \
    -e CACHE_ROOT="/cache" \
    -e CPU_JOBS="${CPU_JOBS:-16}" \
    -e FETCH_JOBS="${FETCH_JOBS:-4}" \
    -e ZIP_JOBS="${ZIP_JOBS:-2}" \
    -v "${ROOT_DIR}:/work" \
    -v "${CACHE_ROOT}:/cache" \
    -v "${OUTPUT_ROOT}:/capture" \
    "${IMAGE_TAG}" \
    /bin/bash /work/legacy-capture/capture_inside_container.sh

echo "capture complete: ${OUTPUT_ROOT}"
