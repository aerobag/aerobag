#!/usr/bin/env python3

import argparse
import importlib.util
import hashlib
import json
import os
import re
import urllib.request

from bs4 import BeautifulSoup


def load_module(name: str, path: str):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def cache_metadata_path(fetch_cache_root: str, url: str) -> str:
    return os.path.join(fetch_cache_root, "http", hashlib.sha256(url.encode("utf-8")).hexdigest() + ".json")


def load_cached_bytes(fetch_cache_root: str | None, url: str) -> bytes | None:
    if not fetch_cache_root:
        return None
    metadata_path = cache_metadata_path(fetch_cache_root, url)
    if not os.path.isfile(metadata_path):
        return None
    with open(metadata_path, "r", encoding="utf-8") as handle:
        metadata = json.load(handle)
    blob_path = os.path.join(fetch_cache_root, "blobs", metadata["sha256"])
    if not os.path.isfile(blob_path):
        return None
    with open(blob_path, "rb") as handle:
        return handle.read()


def store_cached_bytes(fetch_cache_root: str | None, url: str, data: bytes) -> None:
    if not fetch_cache_root:
        return
    blobs = os.path.join(fetch_cache_root, "blobs")
    http = os.path.join(fetch_cache_root, "http")
    os.makedirs(blobs, exist_ok=True)
    os.makedirs(http, exist_ok=True)
    sha256 = hashlib.sha256(data).hexdigest()
    blob_path = os.path.join(blobs, sha256)
    if not os.path.isfile(blob_path):
        with open(blob_path, "wb") as handle:
            handle.write(data)
    with open(cache_metadata_path(fetch_cache_root, url), "w", encoding="utf-8") as handle:
        json.dump({"sha256": sha256, "size": len(data), "url": url}, handle, sort_keys=True)


def fetch_url_bytes(url: str) -> bytes:
    fetch_cache_root = os.environ.get("FETCH_CACHE_ROOT")
    fetch_cache_mode = os.environ.get("FETCH_CACHE_MODE", "fill").lower()
    cached = load_cached_bytes(fetch_cache_root, url)
    if cached is not None:
        return cached
    if fetch_cache_mode == "offline":
        raise RuntimeError(f"cache miss in offline mode for crawl {url}")
    data = urllib.request.urlopen(url).read()
    store_cached_bytes(fetch_cache_root, url, data)
    return data


def list_crawl(url: str, pattern: str) -> list[str]:
    soup = BeautifulSoup(fetch_url_bytes(url), "html.parser")
    matches = []
    for link in soup.find_all("a"):
        href = link.get("href")
        if href is None:
            continue
        if re.search(pattern, href):
            matches.append(href)
    return sorted(set(matches))


def write_source_urls(output_dir: str, label: str, records: list[dict]) -> str:
    target_dir = os.path.join(output_dir, label)
    os.makedirs(target_dir, exist_ok=True)
    path = os.path.join(target_dir, "source_urls.jsonl")
    with open(path, "w", encoding="utf-8") as handle:
        for record in records:
            handle.write(json.dumps(record, sort_keys=True) + "\n")
    return path


def build_records(avare_source_root: str) -> dict[str, list[dict]]:
    charts_cycle = load_module("charts_cycle", os.path.join(avare_source_root, "charts", "cycle.py"))
    csup_cycle = load_module("csup_cycle", os.path.join(avare_source_root, "csup", "cycle.py"))
    tpp_cycle = load_module("tpp_cycle", os.path.join(avare_source_root, "tpp", "cycle.py"))

    charts_start = charts_cycle.get_version_start(charts_cycle.get_cycle_download())
    csup_start = csup_cycle.get_version_start(csup_cycle.get_cycle_download())
    tpp_start = tpp_cycle.get_version_start(tpp_cycle.get_cycle_download())

    return {
        "charts-sec": [
            {
                "event": "list_crawl",
                "label": "charts-sec",
                "match": "^http.*" + charts_start + "/sectional-files/.*.zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/",
                    "^http.*" + charts_start + "/sectional-files/.*.zip$",
                ),
            },
            {
                "event": "list_crawl",
                "label": "charts-sec",
                "match": "^http.*" + charts_start + "/Caribbean/.*.zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/",
                    "^http.*" + charts_start + "/Caribbean/.*.zip$",
                ),
            },
        ],
        "charts-tac": [
            {
                "event": "list_crawl",
                "label": "charts-tac",
                "match": "^http.*" + charts_start + ".*_TAC.zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/",
                    "^http.*" + charts_start + ".*_TAC.zip$",
                ),
            }
        ],
        "charts-enr-l": [
            {
                "event": "list_crawl",
                "label": "charts-enr-l",
                "match": "^http.*" + charts_start + "/enr_l.*.zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                    "^http.*" + charts_start + "/enr_l.*.zip$",
                ),
            },
            {
                "event": "list_crawl",
                "label": "charts-enr-l",
                "match": "^http.*" + charts_start + "/enr_akl.*.zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                    "^http.*" + charts_start + "/enr_akl.*.zip$",
                ),
            },
            {
                "event": "list_crawl",
                "label": "charts-enr-l",
                "match": "^http.*" + charts_start + "/enr_p.*.zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                    "^http.*" + charts_start + "/enr_p.*.zip$",
                ),
            },
        ],
        "charts-enr-h": [
            {
                "event": "list_crawl",
                "label": "charts-enr-h",
                "match": "^http.*" + charts_start + "/enr_h.*.zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                    "^http.*" + charts_start + "/enr_h.*.zip$",
                ),
            },
            {
                "event": "list_crawl",
                "label": "charts-enr-h",
                "match": "^http.*" + charts_start + "/enr_akh.*.zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                    "^http.*" + charts_start + "/enr_akh.*.zip$",
                ),
            },
            {
                "event": "list_crawl",
                "label": "charts-enr-h",
                "match": "^http.*" + charts_start + "/enr_p.*.zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                    "^http.*" + charts_start + "/enr_p.*.zip$",
                ),
            },
        ],
        "csup": [
            {
                "event": "list_crawl",
                "label": "csup",
                "match": "^http.*DCS_" + csup_start.replace("-", "") + ".zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dafd/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dafd/",
                    "^http.*DCS_" + csup_start.replace("-", "") + ".zip$",
                ),
            }
        ],
        "tpp-ne": [
            {
                "event": "list_crawl",
                "label": "tpp-ne",
                "match": "^http.*DDTPP[A-E]+_" + tpp_start.replace("-", "")[2:] + ".zip$",
                "url": "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dtpp/",
                "results": list_crawl(
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dtpp/",
                    "^http.*DDTPP[A-E]+_" + tpp_start.replace("-", "")[2:] + ".zip$",
                ),
            }
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--avare-source-root", required=True)
    parser.add_argument("--output-dir", required=True)
    args = parser.parse_args()

    records_by_label = build_records(args.avare_source_root)
    for label, records in records_by_label.items():
        path = write_source_urls(args.output_dir, label, records)
        print(f"{label} {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
