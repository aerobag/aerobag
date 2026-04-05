#!/usr/bin/env python3

import json
import os
import sys
from collections import Counter


def read_jsonl(path):
    rows = []
    if not os.path.isfile(path):
        return rows
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def summarize_capture(run_dir, capture):
    label = capture["label"]
    meta_dir = os.path.join(run_dir, "meta")

    source_rows = read_jsonl(os.path.join(run_dir, capture.get("source_urls", "")))
    download_rows = read_jsonl(os.path.join(run_dir, capture.get("downloads", "")))
    package_rows = read_jsonl(os.path.join(run_dir, capture.get("package_outputs", "")))

    crawl_urls = []
    for row in source_rows:
        crawl_urls.extend(row.get("results", []))

    download_events = [row for row in download_rows if row.get("event") == "download"]
    extract_events = [row for row in download_rows if row.get("event") == "extract_zip"]

    tile_paths_path = os.path.join(run_dir, capture.get("tile_paths", "")) if capture.get("tile_paths") else None
    tile_count = 0
    if tile_paths_path and os.path.isfile(tile_paths_path):
        with open(tile_paths_path, encoding="utf-8") as handle:
            tile_count = sum(1 for _ in handle)

    summary = {
        "label": label,
        "repo": capture["repo"],
        "command": capture["command"],
        "crawl_request_count": len(source_rows),
        "source_url_count": len(crawl_urls),
        "source_url_hostnames": sorted({url.split("/")[2] for url in crawl_urls if "://" in url}),
        "download_count": len(download_events),
        "downloaded_now_count": sum(1 for row in download_events if row.get("downloaded")),
        "download_reused_count": sum(1 for row in download_events if not row.get("downloaded")),
        "download_bytes_total": sum(int(row.get("size", 0)) for row in download_events),
        "download_files": [
            {
                "file": row["file"],
                "sha256": row["sha256"],
                "size": row["size"],
                "downloaded": row["downloaded"],
                "url": row["url"],
            }
            for row in download_events
        ],
        "extracted_archive_count": len(extract_events),
        "extracted_member_count_total": sum(len(row.get("members", [])) for row in extract_events),
        "package_count": len(package_rows),
        "package_files": package_rows,
        "tile_count": tile_count,
    }

    ext_counter = Counter()
    for row in package_rows:
        zip_name = row.get("zip")
        if zip_name:
            ext_counter[os.path.splitext(zip_name)[1] or "<none>"] += 1
    summary["package_extensions"] = dict(sorted(ext_counter.items()))

    out_path = os.path.join(meta_dir, f"{label}.summary.json")
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2, sort_keys=True)
        handle.write("\n")


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: summarize_provenance.py <run_dir>")

    run_dir = os.path.abspath(sys.argv[1])
    manifest_path = os.path.join(run_dir, "meta", "manifest.json")
    with open(manifest_path, encoding="utf-8") as handle:
        manifest = json.load(handle)

    for capture in manifest["captures"]:
        summarize_capture(run_dir, capture)


if __name__ == "__main__":
    main()
