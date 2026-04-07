#!/usr/bin/env python3

import json
import os
import sys
from pathlib import Path


CAPTURE_ORDER = [
    "charts-sec",
    "charts-tac",
    "charts-enr-l",
    "charts-enr-h",
    "tpp-ne",
    "csup",
]


def read_text_tail(path: Path, limit: int = 1) -> list[str]:
    if not path.is_file():
        return []
    with path.open(encoding="utf-8", errors="replace") as handle:
        lines = handle.readlines()
    return [line.rstrip("\n") for line in lines[-limit:]]


def count_lines(path: Path) -> int | None:
    if not path.is_file():
        return None
    with path.open(encoding="utf-8", errors="replace") as handle:
        return sum(1 for _ in handle)


def capture_status(run_dir: Path, label: str) -> dict:
    meta_dir = run_dir / "meta"
    log_dir = run_dir / "logs"
    provenance_dir = meta_dir / "provenance" / label

    before = (meta_dir / f"{label}.before.sha256").is_file()
    after = (meta_dir / f"{label}.after.sha256").is_file()
    outputs = (meta_dir / f"{label}.outputs.sha256").is_file()
    summary = (meta_dir / f"{label}.summary.json").is_file()
    tile_paths = meta_dir / f"{label}.tile-paths.txt"

    if summary:
        state = "completed"
    elif outputs or after:
        state = "packaging"
    elif before:
        state = "running"
    else:
        state = "pending"

    source_urls = provenance_dir / "source_urls.jsonl"
    downloads = provenance_dir / "downloads.jsonl"
    package_outputs = provenance_dir / "package_outputs.jsonl"

    return {
        "label": label,
        "state": state,
        "tile_count": count_lines(tile_paths),
        "source_url_events": count_lines(source_urls),
        "download_events": count_lines(downloads),
        "package_output_events": count_lines(package_outputs),
        "stdout_tail": read_text_tail(log_dir / f"{label}.stdout.log"),
        "stderr_tail": read_text_tail(log_dir / f"{label}.stderr.log"),
    }


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: run_status.py <run_dir>")

    run_dir = Path(sys.argv[1]).resolve()
    if not run_dir.is_dir():
        raise SystemExit(f"run directory not found: {run_dir}")

    report = {
        "run_dir": str(run_dir),
        "captures": [capture_status(run_dir, label) for label in CAPTURE_ORDER],
    }
    json.dump(report, sys.stdout, indent=2)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
