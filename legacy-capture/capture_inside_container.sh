#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="${ROOT_DIR:-/work}"
AVARE_SOURCE_ROOT="${AVARE_SOURCE_ROOT:-${ROOT_DIR}/avare-source}"
OUTPUT_ROOT="${OUTPUT_ROOT:-/capture}"
CACHE_ROOT="${CACHE_ROOT:-/cache}"
FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT:-${CACHE_ROOT}/fetch}"
FETCH_CACHE_MODE="${FETCH_CACHE_MODE:-fill}"
RUN_ID="${RUN_ID:?RUN_ID is required}"
RUN_DIR="${OUTPUT_ROOT}"
LOG_DIR="${RUN_DIR}/logs"
ARTIFACT_DIR="${RUN_DIR}/artifacts"
META_DIR="${RUN_DIR}/meta"

mkdir -p "${LOG_DIR}" "${ARTIFACT_DIR}" "${META_DIR}" "${CACHE_ROOT}" "${FETCH_CACHE_ROOT}"

hash_file() {
    local file="$1"
    sha256sum "${file}" | awk '{print $1}'
}

record_tree_hashes() {
    local dir="$1"
    local out="$2"
    if [ -d "${dir}" ]; then
        find "${dir}" -type f -print0 \
            | sort -z \
            | xargs -0 sha256sum > "${out}"
    else
        : > "${out}"
    fi
}

record_zip_members() {
    local zip_file="$1"
    local out="$2"
    unzip -Z1 "${zip_file}" | sort > "${out}"
}

run_capture() {
    local label="$1"
    local repo_rel="$2"
    shift 2

    local repo_dir
    case "${repo_rel}" in
        avare-source/*)
            repo_dir="${AVARE_SOURCE_ROOT}/${repo_rel#avare-source/}"
            ;;
        *)
            repo_dir="${ROOT_DIR}/${repo_rel}"
            ;;
    esac
    local work_dir="${RUN_DIR}/work/${label}"
    local stdout_log="${LOG_DIR}/${label}.stdout.log"
    local stderr_log="${LOG_DIR}/${label}.stderr.log"
    local before_hashes="${META_DIR}/${label}.before.sha256"
    local after_hashes="${META_DIR}/${label}.after.sha256"
    local output_hashes="${META_DIR}/${label}.outputs.sha256"
    local capture_meta_dir="${META_DIR}/provenance/${label}"

    mkdir -p "${work_dir}" "${capture_meta_dir}"
    cp -a "${repo_dir}/." "${work_dir}/"
    record_tree_hashes "${work_dir}" "${before_hashes}"

    (
        cd "${work_dir}"
        CAPTURE_LABEL="${label}" \
        CAPTURE_META_DIR="${capture_meta_dir}" \
        FETCH_CACHE_ROOT="${FETCH_CACHE_ROOT}" \
        FETCH_CACHE_MODE="${FETCH_CACHE_MODE}" \
        "$@"
    ) >"${stdout_log}" 2>"${stderr_log}"

    record_tree_hashes "${work_dir}" "${after_hashes}"

    mkdir -p "${ARTIFACT_DIR}/${label}"

    find "${work_dir}" -maxdepth 1 -type f \( -name '*.zip' -o -name '*.db' -o -name '*.txt' \) -print0 \
        | while IFS= read -r -d '' file; do
            cp "${file}" "${ARTIFACT_DIR}/${label}/"
        done

    if [ -d "${work_dir}/tiles" ]; then
        mkdir -p "${ARTIFACT_DIR}/${label}/tiles"
        find "${work_dir}/tiles" -type f -print0 \
            | while IFS= read -r -d '' file; do
                local_path="${file#${work_dir}/}"
                mkdir -p "${ARTIFACT_DIR}/${label}/$(dirname "${local_path}")"
                cp "${file}" "${ARTIFACT_DIR}/${label}/${local_path}"
            done
        find "${work_dir}/tiles" -type f | sed "s#${work_dir}/##" | sort > "${META_DIR}/${label}.tile-paths.txt"
    else
        : > "${META_DIR}/${label}.tile-paths.txt"
    fi

    if [ -d "${capture_meta_dir}" ]; then
        find "${capture_meta_dir}" -type f -print0 \
            | while IFS= read -r -d '' file; do
                local_path="${file#${META_DIR}/}"
                mkdir -p "${ARTIFACT_DIR}/${label}/$(dirname "${local_path}")"
                cp "${file}" "${ARTIFACT_DIR}/${label}/${local_path}"
            done
    fi

    record_tree_hashes "${ARTIFACT_DIR}/${label}" "${output_hashes}"

    find "${ARTIFACT_DIR}/${label}" -maxdepth 1 -type f -name '*.zip' -print0 \
        | while IFS= read -r -d '' zip_file; do
            record_zip_members "${zip_file}" "${META_DIR}/$(basename "${zip_file}").members.txt"
        done
}

run_capture "charts-sec" "avare-source/charts" python3 sec.py
run_capture "charts-tac" "avare-source/charts" python3 tac.py
run_capture "charts-enr-l" "avare-source/charts" python3 enr_l.py
run_capture "charts-enr-h" "avare-source/charts" python3 enr_h.py
run_capture "tpp-ne" "avare-source/tpp" python3 tpp.py NE
run_capture "tpp-nw" "avare-source/tpp" python3 tpp.py NW
run_capture "csup" "avare-source/csup" python3 csup.py
run_capture "data" "avare-source/data" python3 "${ROOT_DIR}/legacy-capture/run_legacy_data_primary.py"

cat > "${META_DIR}/manifest.json" <<EOF
{
  "schema_version": 1,
  "run_id": "${RUN_ID}",
  "created_at_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "image": {
    "tag": "${IMAGE_TAG:-aerobag/legacy-capture:local}"
  },
  "captures": [
    {
      "label": "charts-sec",
      "repo": "avare-source/charts",
      "command": ["python3", "sec.py"],
      "stdout_log": "logs/charts-sec.stdout.log",
      "stderr_log": "logs/charts-sec.stderr.log",
      "tile_paths": "meta/charts-sec.tile-paths.txt",
      "outputs_hashes": "meta/charts-sec.outputs.sha256",
      "source_urls": "meta/provenance/charts-sec/source_urls.jsonl",
      "downloads": "meta/provenance/charts-sec/downloads.jsonl",
      "package_outputs": "meta/provenance/charts-sec/package_outputs.jsonl"
    },
    {
      "label": "charts-tac",
      "repo": "avare-source/charts",
      "command": ["python3", "tac.py"],
      "stdout_log": "logs/charts-tac.stdout.log",
      "stderr_log": "logs/charts-tac.stderr.log",
      "tile_paths": "meta/charts-tac.tile-paths.txt",
      "outputs_hashes": "meta/charts-tac.outputs.sha256",
      "source_urls": "meta/provenance/charts-tac/source_urls.jsonl",
      "downloads": "meta/provenance/charts-tac/downloads.jsonl",
      "package_outputs": "meta/provenance/charts-tac/package_outputs.jsonl"
    },
    {
      "label": "charts-enr-l",
      "repo": "avare-source/charts",
      "command": ["python3", "enr_l.py"],
      "stdout_log": "logs/charts-enr-l.stdout.log",
      "stderr_log": "logs/charts-enr-l.stderr.log",
      "tile_paths": "meta/charts-enr-l.tile-paths.txt",
      "outputs_hashes": "meta/charts-enr-l.outputs.sha256",
      "source_urls": "meta/provenance/charts-enr-l/source_urls.jsonl",
      "downloads": "meta/provenance/charts-enr-l/downloads.jsonl",
      "package_outputs": "meta/provenance/charts-enr-l/package_outputs.jsonl"
    },
    {
      "label": "charts-enr-h",
      "repo": "avare-source/charts",
      "command": ["python3", "enr_h.py"],
      "stdout_log": "logs/charts-enr-h.stdout.log",
      "stderr_log": "logs/charts-enr-h.stderr.log",
      "tile_paths": "meta/charts-enr-h.tile-paths.txt",
      "outputs_hashes": "meta/charts-enr-h.outputs.sha256",
      "source_urls": "meta/provenance/charts-enr-h/source_urls.jsonl",
      "downloads": "meta/provenance/charts-enr-h/downloads.jsonl",
      "package_outputs": "meta/provenance/charts-enr-h/package_outputs.jsonl"
    },
    {
      "label": "tpp-ne",
      "repo": "avare-source/tpp",
      "command": ["python3", "tpp.py", "NE"],
      "stdout_log": "logs/tpp-ne.stdout.log",
      "stderr_log": "logs/tpp-ne.stderr.log",
      "outputs_hashes": "meta/tpp-ne.outputs.sha256",
      "source_urls": "meta/provenance/tpp-ne/source_urls.jsonl",
      "downloads": "meta/provenance/tpp-ne/downloads.jsonl",
      "package_outputs": "meta/provenance/tpp-ne/package_outputs.jsonl"
    },
    {
      "label": "tpp-nw",
      "repo": "avare-source/tpp",
      "command": ["python3", "tpp.py", "NW"],
      "stdout_log": "logs/tpp-nw.stdout.log",
      "stderr_log": "logs/tpp-nw.stderr.log",
      "outputs_hashes": "meta/tpp-nw.outputs.sha256",
      "source_urls": "meta/provenance/tpp-nw/source_urls.jsonl",
      "downloads": "meta/provenance/tpp-nw/downloads.jsonl",
      "package_outputs": "meta/provenance/tpp-nw/package_outputs.jsonl"
    },
    {
      "label": "csup",
      "repo": "avare-source/csup",
      "command": ["python3", "csup.py"],
      "stdout_log": "logs/csup.stdout.log",
      "stderr_log": "logs/csup.stderr.log",
      "outputs_hashes": "meta/csup.outputs.sha256",
      "source_urls": "meta/provenance/csup/source_urls.jsonl",
      "downloads": "meta/provenance/csup/downloads.jsonl",
      "package_outputs": "meta/provenance/csup/package_outputs.jsonl"
    },
    {
      "label": "data",
      "repo": "avare-source/data",
      "command": ["python3", "legacy-capture/run_legacy_data_primary.py"],
      "stdout_log": "logs/data.stdout.log",
      "stderr_log": "logs/data.stderr.log",
      "outputs_hashes": "meta/data.outputs.sha256"
    }
  ]
}
EOF

python3 "${ROOT_DIR}/legacy-capture/summarize_provenance.py" "${RUN_DIR}"
