#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${RUN_ID:-$(date -u +%Y%m%dT%H%M%SZ)}"
VALIDATION_ROOT="${VALIDATION_ROOT:-${ROOT_DIR}/runs/${RUN_ID}-validation}"
AVARE_SOURCE_ROOT="${AVARE_SOURCE_ROOT:-${ROOT_DIR}/avare-source}"
CACHE_ROOT="${CACHE_ROOT:-${ROOT_DIR}/cache}"
FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT:-${CACHE_ROOT}/fetch}"
FETCH_CACHE_MODE="${FETCH_CACHE_MODE:-fill}"
FETCH_JOBS="${FETCH_JOBS:-4}"
ZIP_JOBS="${ZIP_JOBS:-2}"
CPU_JOBS="${CPU_JOBS:-$(command -v nproc >/dev/null 2>&1 && nproc || echo 8)}"
NATIVE_CHART_CPU_JOBS="${NATIVE_CHART_CPU_JOBS:-$(( CPU_JOBS > 8 ? 8 : (CPU_JOBS > 0 ? CPU_JOBS : 1) ))}"
IMAGE_SAMPLE_PERCENT="${IMAGE_SAMPLE_PERCENT:-100}"
IMAGE_RMSE_THRESHOLD="${IMAGE_RMSE_THRESHOLD:-0.0}"

LEGACY_RUN_ROOT="${VALIDATION_ROOT}/legacy"
NATIVE_ROOT="${VALIDATION_ROOT}/native"
PREP_ROOT="${VALIDATION_ROOT}/prep"
COMPARE_ROOT="${VALIDATION_ROOT}/compare"
LOG_ROOT="${VALIDATION_ROOT}/orchestrator-logs"

mkdir -p "${VALIDATION_ROOT}" "${PREP_ROOT}" "${COMPARE_ROOT}" "${LOG_ROOT}" "${CACHE_ROOT}" "${FETCH_CACHE_ROOT}"

for cmd in git python3 cargo unzip zip sha256sum; do
    if ! command -v "${cmd}" >/dev/null 2>&1; then
        echo "required command not found: ${cmd}" >&2
        exit 1
    fi
done

/bin/bash "${ROOT_DIR}/legacy-capture/hydrate_legacy_sources.sh"

FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT}" \
FETCH_CACHE_MODE="${FETCH_CACHE_MODE}" \
python3 "${ROOT_DIR}/legacy-capture/emit_source_urls.py" \
    --avare-source-root "${AVARE_SOURCE_ROOT}" \
    --output-dir "${PREP_ROOT}/source-urls" \
    > "${LOG_ROOT}/emit-source-urls.stdout.log" \
    2> "${LOG_ROOT}/emit-source-urls.stderr.log"

cargo build -p preprocessor-cli --manifest-path "${ROOT_DIR}/rust-preprocessor/Cargo.toml" \
    > "${LOG_ROOT}/cargo-build.stdout.log" \
    2> "${LOG_ROOT}/cargo-build.stderr.log"

CLI="${ROOT_DIR}/rust-preprocessor/target/debug/preprocessor-cli"

declare -a JOB_NAMES=()
declare -a JOB_PIDS=()
declare -A JOB_STATUS=()

run_bg() {
    local name="$1"
    shift
    JOB_NAMES+=("${name}")
    (
        "$@"
    ) > "${LOG_ROOT}/${name}.stdout.log" 2> "${LOG_ROOT}/${name}.stderr.log" &
    JOB_PIDS+=("$!")
}

wait_jobs() {
    local index
    for index in "${!JOB_NAMES[@]}"; do
        local name="${JOB_NAMES[$index]}"
        local pid="${JOB_PIDS[$index]}"
        if wait "${pid}"; then
            JOB_STATUS["${name}"]=0
        else
            JOB_STATUS["${name}"]=$?
        fi
    done
}

run_bg legacy \
    env \
    RUN_ID="${RUN_ID}-legacy" \
    OUTPUT_ROOT="${LEGACY_RUN_ROOT}" \
    CACHE_ROOT="${CACHE_ROOT}" \
    FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT}" \
    FETCH_CACHE_MODE="${FETCH_CACHE_MODE}" \
    CPU_JOBS="${CPU_JOBS}" \
    FETCH_JOBS="${FETCH_JOBS}" \
    ZIP_JOBS="${ZIP_JOBS}" \
    /bin/bash "${ROOT_DIR}/legacy-capture/run_legacy_capture_direct.sh"

run_bg native-charts-sec \
    env \
    FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT}" \
    FETCH_CACHE_MODE="${FETCH_CACHE_MODE}" \
    "${CLI}" run-native-chart \
    --family sec \
    --source-repo "${AVARE_SOURCE_ROOT}/charts" \
    --run-root "${NATIVE_ROOT}/charts-sec" \
    --cpu-jobs "${NATIVE_CHART_CPU_JOBS}" \
    --prefetch-source-urls "${PREP_ROOT}/source-urls/charts-sec/source_urls.jsonl" \
    --fetch-jobs "${FETCH_JOBS}"

run_bg native-charts-tac \
    env \
    FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT}" \
    FETCH_CACHE_MODE="${FETCH_CACHE_MODE}" \
    "${CLI}" run-native-chart \
    --family tac \
    --source-repo "${AVARE_SOURCE_ROOT}/charts" \
    --run-root "${NATIVE_ROOT}/charts-tac" \
    --cpu-jobs "${NATIVE_CHART_CPU_JOBS}" \
    --prefetch-source-urls "${PREP_ROOT}/source-urls/charts-tac/source_urls.jsonl" \
    --fetch-jobs "${FETCH_JOBS}"

run_bg native-charts-enr-l \
    env \
    FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT}" \
    FETCH_CACHE_MODE="${FETCH_CACHE_MODE}" \
    "${CLI}" run-native-chart \
    --family enr-l \
    --source-repo "${AVARE_SOURCE_ROOT}/charts" \
    --run-root "${NATIVE_ROOT}/charts-enr-l" \
    --cpu-jobs "${NATIVE_CHART_CPU_JOBS}" \
    --prefetch-source-urls "${PREP_ROOT}/source-urls/charts-enr-l/source_urls.jsonl" \
    --fetch-jobs "${FETCH_JOBS}"

run_bg native-charts-enr-h \
    env \
    FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT}" \
    FETCH_CACHE_MODE="${FETCH_CACHE_MODE}" \
    "${CLI}" run-native-chart \
    --family enr-h \
    --source-repo "${AVARE_SOURCE_ROOT}/charts" \
    --run-root "${NATIVE_ROOT}/charts-enr-h" \
    --cpu-jobs "${NATIVE_CHART_CPU_JOBS}" \
    --prefetch-source-urls "${PREP_ROOT}/source-urls/charts-enr-h/source_urls.jsonl" \
    --fetch-jobs "${FETCH_JOBS}"

run_bg native-csup \
    env \
    FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT}" \
    FETCH_CACHE_MODE="${FETCH_CACHE_MODE}" \
    "${CLI}" run-native-csup \
    --source-repo "${AVARE_SOURCE_ROOT}/csup" \
    --run-root "${NATIVE_ROOT}/csup" \
    --prefetch-source-urls "${PREP_ROOT}/source-urls/csup/source_urls.jsonl" \
    --fetch-jobs "${FETCH_JOBS}"

run_bg native-tpp-ne \
    env \
    FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT}" \
    FETCH_CACHE_MODE="${FETCH_CACHE_MODE}" \
    "${CLI}" run-native-tpp \
    --region NE \
    --source-repo "${AVARE_SOURCE_ROOT}/tpp" \
    --run-root "${NATIVE_ROOT}/tpp-ne" \
    --prefetch-source-urls "${PREP_ROOT}/source-urls/tpp-ne/source_urls.jsonl" \
    --fetch-jobs "${FETCH_JOBS}"

wait_jobs

{
    echo "run_id ${RUN_ID}"
    echo "validation_root ${VALIDATION_ROOT}"
    for name in "${JOB_NAMES[@]}"; do
        echo "job ${name} exit_code=${JOB_STATUS[${name}]}"
    done
} > "${COMPARE_ROOT}/job-status.txt"

run_compare() {
    local name="$1"
    shift
    "${CLI}" "$@" > "${COMPARE_ROOT}/${name}.txt"
}

if [[ "${JOB_STATUS[legacy]}" == "0" && "${JOB_STATUS[native-charts-sec]}" == "0" ]]; then
    run_compare charts-sec-tile-paths compare-chart-tile-paths --family sec --legacy-work-dir "${LEGACY_RUN_ROOT}/work/charts-sec" --rust-work-dir "${NATIVE_ROOT}/charts-sec/work/charts-sec"
    run_compare charts-sec-packages compare-chart-packages --family sec --legacy-work-dir "${LEGACY_RUN_ROOT}/work/charts-sec" --rust-work-dir "${NATIVE_ROOT}/charts-sec/work/charts-sec"
    run_compare charts-sec-provenance compare-provenance --left-provenance-dir "${LEGACY_RUN_ROOT}/meta/provenance/charts-sec" --right-provenance-dir "${NATIVE_ROOT}/charts-sec/meta/provenance/charts-sec"
    run_compare charts-sec-images compare-sampled-images --left-root "${LEGACY_RUN_ROOT}/work/charts-sec/tiles/0" --right-root "${NATIVE_ROOT}/charts-sec/work/charts-sec/tiles/0" --sample-percent "${IMAGE_SAMPLE_PERCENT}" --rmse-threshold "${IMAGE_RMSE_THRESHOLD}"
fi

if [[ "${JOB_STATUS[legacy]}" == "0" && "${JOB_STATUS[native-charts-tac]}" == "0" ]]; then
    run_compare charts-tac-tile-paths compare-chart-tile-paths --family tac --legacy-work-dir "${LEGACY_RUN_ROOT}/work/charts-tac" --rust-work-dir "${NATIVE_ROOT}/charts-tac/work/charts-tac"
    run_compare charts-tac-packages compare-chart-packages --family tac --legacy-work-dir "${LEGACY_RUN_ROOT}/work/charts-tac" --rust-work-dir "${NATIVE_ROOT}/charts-tac/work/charts-tac"
    run_compare charts-tac-provenance compare-provenance --left-provenance-dir "${LEGACY_RUN_ROOT}/meta/provenance/charts-tac" --right-provenance-dir "${NATIVE_ROOT}/charts-tac/meta/provenance/charts-tac"
    run_compare charts-tac-images compare-sampled-images --left-root "${LEGACY_RUN_ROOT}/work/charts-tac/tiles/1" --right-root "${NATIVE_ROOT}/charts-tac/work/charts-tac/tiles/1" --sample-percent "${IMAGE_SAMPLE_PERCENT}" --rmse-threshold "${IMAGE_RMSE_THRESHOLD}"
fi

if [[ "${JOB_STATUS[legacy]}" == "0" && "${JOB_STATUS[native-charts-enr-l]}" == "0" ]]; then
    run_compare charts-enr-l-tile-paths compare-chart-tile-paths --family enr-l --legacy-work-dir "${LEGACY_RUN_ROOT}/work/charts-enr-l" --rust-work-dir "${NATIVE_ROOT}/charts-enr-l/work/charts-enr-l"
    run_compare charts-enr-l-packages compare-chart-packages --family enr-l --legacy-work-dir "${LEGACY_RUN_ROOT}/work/charts-enr-l" --rust-work-dir "${NATIVE_ROOT}/charts-enr-l/work/charts-enr-l"
    run_compare charts-enr-l-provenance compare-provenance --left-provenance-dir "${LEGACY_RUN_ROOT}/meta/provenance/charts-enr-l" --right-provenance-dir "${NATIVE_ROOT}/charts-enr-l/meta/provenance/charts-enr-l"
    run_compare charts-enr-l-images compare-sampled-images --left-root "${LEGACY_RUN_ROOT}/work/charts-enr-l/tiles/3" --right-root "${NATIVE_ROOT}/charts-enr-l/work/charts-enr-l/tiles/3" --sample-percent "${IMAGE_SAMPLE_PERCENT}" --rmse-threshold "${IMAGE_RMSE_THRESHOLD}"
fi

if [[ "${JOB_STATUS[legacy]}" == "0" && "${JOB_STATUS[native-charts-enr-h]}" == "0" ]]; then
    run_compare charts-enr-h-tile-paths compare-chart-tile-paths --family enr-h --legacy-work-dir "${LEGACY_RUN_ROOT}/work/charts-enr-h" --rust-work-dir "${NATIVE_ROOT}/charts-enr-h/work/charts-enr-h"
    run_compare charts-enr-h-packages compare-chart-packages --family enr-h --legacy-work-dir "${LEGACY_RUN_ROOT}/work/charts-enr-h" --rust-work-dir "${NATIVE_ROOT}/charts-enr-h/work/charts-enr-h"
    run_compare charts-enr-h-provenance compare-provenance --left-provenance-dir "${LEGACY_RUN_ROOT}/meta/provenance/charts-enr-h" --right-provenance-dir "${NATIVE_ROOT}/charts-enr-h/meta/provenance/charts-enr-h"
    run_compare charts-enr-h-images compare-sampled-images --left-root "${LEGACY_RUN_ROOT}/work/charts-enr-h/tiles/4" --right-root "${NATIVE_ROOT}/charts-enr-h/work/charts-enr-h/tiles/4" --sample-percent "${IMAGE_SAMPLE_PERCENT}" --rmse-threshold "${IMAGE_RMSE_THRESHOLD}"
fi

if [[ "${JOB_STATUS[legacy]}" == "0" && "${JOB_STATUS[native-csup]}" == "0" ]]; then
    run_compare csup-packages compare-csup-packages --legacy-work-dir "${LEGACY_RUN_ROOT}/work/csup" --rust-work-dir "${NATIVE_ROOT}/csup/work/csup"
    run_compare csup-provenance compare-provenance --left-provenance-dir "${LEGACY_RUN_ROOT}/meta/provenance/csup" --right-provenance-dir "${NATIVE_ROOT}/csup/meta/provenance/csup"
    run_compare csup-images compare-csup-images --legacy-work-dir "${LEGACY_RUN_ROOT}/work/csup" --rust-work-dir "${NATIVE_ROOT}/csup/work/csup" --sample-percent "${IMAGE_SAMPLE_PERCENT}" --rmse-threshold "${IMAGE_RMSE_THRESHOLD}"
fi

if [[ "${JOB_STATUS[legacy]}" == "0" && "${JOB_STATUS[native-tpp-ne]}" == "0" ]]; then
    run_compare tpp-ne-packages compare-tpp-packages --region NE --legacy-work-dir "${LEGACY_RUN_ROOT}/work/tpp-ne" --rust-work-dir "${NATIVE_ROOT}/tpp-ne/work/tpp-ne"
    run_compare tpp-ne-provenance compare-provenance --left-provenance-dir "${LEGACY_RUN_ROOT}/meta/provenance/tpp-ne" --right-provenance-dir "${NATIVE_ROOT}/tpp-ne/meta/provenance/tpp-ne"
    run_compare tpp-ne-images compare-tpp-images --region NE --legacy-work-dir "${LEGACY_RUN_ROOT}/work/tpp-ne" --rust-work-dir "${NATIVE_ROOT}/tpp-ne/work/tpp-ne" --sample-percent "${IMAGE_SAMPLE_PERCENT}" --rmse-threshold "${IMAGE_RMSE_THRESHOLD}"
fi

{
    cat "${COMPARE_ROOT}/job-status.txt"
    for file in "${COMPARE_ROOT}"/*.txt; do
        [[ "$(basename "${file}")" == "job-status.txt" ]] && continue
        echo "===== $(basename "${file}") ====="
        cat "${file}"
    done
} > "${COMPARE_ROOT}/summary.txt"

cat "${COMPARE_ROOT}/summary.txt"
