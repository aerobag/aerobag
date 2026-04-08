#!/usr/bin/env python3

from __future__ import annotations

import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


CAPTURES = [
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
        "package_outputs": "meta/provenance/charts-sec/package_outputs.jsonl",
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
        "package_outputs": "meta/provenance/charts-tac/package_outputs.jsonl",
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
        "package_outputs": "meta/provenance/charts-enr-l/package_outputs.jsonl",
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
        "package_outputs": "meta/provenance/charts-enr-h/package_outputs.jsonl",
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
        "package_outputs": "meta/provenance/tpp-ne/package_outputs.jsonl",
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
        "package_outputs": "meta/provenance/tpp-nw/package_outputs.jsonl",
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
        "package_outputs": "meta/provenance/csup/package_outputs.jsonl",
    },
    {
        "label": "data",
        "repo": "avare-source/data",
        "command": ["python3", "legacy-capture/run_legacy_data_primary.py"],
        "stdout_log": "logs/data.stdout.log",
        "stderr_log": "logs/data.stderr.log",
        "outputs_hashes": "meta/data.outputs.sha256",
    },
]


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: finalize_run.py <run_dir>", file=sys.stderr)
        return 2

    run_dir = Path(sys.argv[1]).resolve()
    manifest_path = run_dir / "meta" / "manifest.json"
    manifest = {
        "schema_version": 1,
        "run_id": run_dir.name,
        "created_at_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "image": {"tag": "aerobag/legacy-capture:local"},
        "captures": CAPTURES,
    }
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")

    subprocess.run(
        ["python3", str(Path(__file__).with_name("summarize_provenance.py")), str(run_dir)],
        check=True,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
