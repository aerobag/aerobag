#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
OUTPUT_ROOT="${OUTPUT_ROOT:-${ROOT_DIR}/runs/${RUN_ID}}"
CACHE_ROOT="${CACHE_ROOT:-${ROOT_DIR}/cache}"

mkdir -p "${OUTPUT_ROOT}" "${CACHE_ROOT}"

for cmd in python3 unzip zip sha256sum; do
    if ! command -v "${cmd}" >/dev/null 2>&1; then
        echo "required command not found: ${cmd}" >&2
        exit 1
    fi
done

for cmd in gdalinfo gs exiftool convert sqlite3; do
    if ! command -v "${cmd}" >/dev/null 2>&1; then
        echo "missing native dependency: ${cmd}" >&2
        echo "install the legacy tooling packages or use legacy-capture/run_legacy_capture.sh" >&2
        exit 1
    fi
done

RUN_ID="${RUN_ID}" \
ROOT_DIR="${ROOT_DIR}" \
OUTPUT_ROOT="${OUTPUT_ROOT}" \
CACHE_ROOT="${CACHE_ROOT}" \
FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT:-${CACHE_ROOT}/fetch}" \
FETCH_CACHE_MODE="${FETCH_CACHE_MODE:-fill}" \
CPU_JOBS="${CPU_JOBS:-16}" \
FETCH_JOBS="${FETCH_JOBS:-4}" \
ZIP_JOBS="${ZIP_JOBS:-2}" \
/bin/bash "${ROOT_DIR}/legacy-capture/capture_inside_container.sh" \
    >/tmp/legacy-capture-direct.stdout.log \
    2>/tmp/legacy-capture-direct.stderr.log

echo "capture complete: ${OUTPUT_ROOT}"
