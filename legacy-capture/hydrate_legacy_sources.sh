#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
AVARE_SOURCE_ROOT="${AVARE_SOURCE_ROOT:-${ROOT_DIR}/avare-source}"

mkdir -p "${AVARE_SOURCE_ROOT}"

clone_if_missing() {
    local name="$1"
    local url="$2"
    local dest="${AVARE_SOURCE_ROOT}/${name}"
    if [ -d "${dest}/.git" ]; then
        return
    fi
    git clone "${url}" "${dest}"
}

apply_patch_if_needed() {
    local repo_dir="$1"
    local patch_path="$2"
    local marker_file="$3"
    local marker_text="$4"
    if grep -Fq "${marker_text}" "${repo_dir}/${marker_file}"; then
        return
    fi
    if git -C "${repo_dir}" apply --check "${patch_path}" >/dev/null 2>&1; then
        git -C "${repo_dir}" apply "${patch_path}"
        return
    fi
    echo "failed to apply patch ${patch_path} cleanly in ${repo_dir}" >&2
    exit 1
}

clone_if_missing "charts" "https://github.com/apps4av/charts.git"
clone_if_missing "tpp" "https://github.com/apps4av/tpp.git"
clone_if_missing "csup" "https://github.com/apps4av/csup.git"

apply_patch_if_needed "${AVARE_SOURCE_ROOT}/charts" "${ROOT_DIR}/legacy-capture/patches/charts-common.patch" "common.py" "CAPTURE_META_DIR = os.environ.get(\"CAPTURE_META_DIR\")"
apply_patch_if_needed "${AVARE_SOURCE_ROOT}/charts" "${ROOT_DIR}/legacy-capture/patches/charts-crawl-cache.patch" "common.py" "def _load_cached_bytes(url):"
apply_patch_if_needed "${AVARE_SOURCE_ROOT}/tpp" "${ROOT_DIR}/legacy-capture/patches/tpp-common.patch" "common.py" "CAPTURE_META_DIR = os.environ.get(\"CAPTURE_META_DIR\")"
apply_patch_if_needed "${AVARE_SOURCE_ROOT}/tpp" "${ROOT_DIR}/legacy-capture/patches/tpp-crawl-cache.patch" "common.py" "def _load_cached_bytes(url):"
apply_patch_if_needed "${AVARE_SOURCE_ROOT}/csup" "${ROOT_DIR}/legacy-capture/patches/csup-common.patch" "common.py" "CAPTURE_META_DIR = os.environ.get(\"CAPTURE_META_DIR\")"
apply_patch_if_needed "${AVARE_SOURCE_ROOT}/csup" "${ROOT_DIR}/legacy-capture/patches/csup-crawl-cache.patch" "common.py" "def _load_cached_bytes(url):"

echo "legacy sources ready under ${AVARE_SOURCE_ROOT}"
