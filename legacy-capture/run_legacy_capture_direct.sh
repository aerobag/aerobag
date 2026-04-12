#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARTIFACT_ROOT_CONFIG_FILE="${ROOT_DIR}/.aerobag-artifact-write-path"
if [[ -z "${AEROBAG_ARTIFACT_WRITE_PATH:-}" && -f "${ARTIFACT_ROOT_CONFIG_FILE}" ]]; then
    ARTIFACT_ROOT="$(<"${ARTIFACT_ROOT_CONFIG_FILE}")"
    if [[ "${ARTIFACT_ROOT}" != /* ]]; then
        ARTIFACT_ROOT="${ROOT_DIR}/${ARTIFACT_ROOT}"
    fi
else
    ARTIFACT_ROOT="${AEROBAG_ARTIFACT_WRITE_PATH:-}"
fi
if [[ -z "${ARTIFACT_ROOT}" ]]; then
    echo "artifact write path unset: set AEROBAG_ARTIFACT_WRITE_PATH or create ${ARTIFACT_ROOT_CONFIG_FILE}" >&2
    exit 1
fi
if [[ -z "${AVARE_SOURCE_ROOT:-}" ]]; then
    if [[ -d "${ROOT_DIR}/avare-source" ]]; then
        AVARE_SOURCE_ROOT="${ROOT_DIR}/avare-source"
    else
        AVARE_SOURCE_ROOT="$(dirname "${ROOT_DIR}")/../avare-reference/avare-source"
    fi
fi
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
OUTPUT_ROOT="${OUTPUT_ROOT:-${ARTIFACT_ROOT}/runs/${RUN_ID}}"
CACHE_ROOT="${CACHE_ROOT:-${ARTIFACT_ROOT}/cache}"

mkdir -p "${OUTPUT_ROOT}" "${CACHE_ROOT}"

for cmd in python3 unzip zip sha256sum; do
    if ! command -v "${cmd}" >/dev/null 2>&1; then
        echo "required command not found: ${cmd}" >&2
        exit 1
    fi
done

for cmd in gdalinfo gs exiftool convert sqlite3 perl; do
    if ! command -v "${cmd}" >/dev/null 2>&1; then
        echo "missing native dependency: ${cmd}" >&2
        echo "install the legacy tooling packages or use legacy-capture/run_legacy_capture.sh" >&2
        exit 1
    fi
done

RUN_ID="${RUN_ID}" \
ROOT_DIR="${ROOT_DIR}" \
AVARE_SOURCE_ROOT="${AVARE_SOURCE_ROOT}" \
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
