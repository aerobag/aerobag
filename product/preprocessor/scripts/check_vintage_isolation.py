#!/usr/bin/env python3
import hashlib
import json
import os
import re
import shutil
import time
import subprocess
import urllib.request
from pathlib import Path


REGIONS = ["ak", "pac", "nw", "sw", "nc", "ec", "sc", "ne", "se"]


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def snapshot_tree(root: Path) -> dict[str, dict[str, object]]:
    snapshot: dict[str, dict[str, object]] = {}
    if not root.exists():
        return snapshot
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        rel = path.relative_to(root).as_posix()
        snapshot[rel] = {
            "size": path.stat().st_size,
            "sha256": sha256_file(path),
        }
    return snapshot


def assert_superset(before: dict, after: dict, label: str) -> None:
    missing = []
    changed = []
    for path, meta in before.items():
        other = after.get(path)
        if other is None:
            missing.append(path)
        elif other["sha256"] != meta["sha256"]:
            changed.append((path, meta["sha256"], other["sha256"]))
    if missing or changed:
        print(f"{label} superset check FAILED")
        if missing:
            print("missing paths:")
            for path in missing[:20]:
                print(f"  {path}")
        if changed:
            print("changed paths:")
            for path, left, right in changed[:20]:
                print(f"  {path}")
                print(f"    before {left}")
                print(f"    after  {right}")
        raise SystemExit(1)


def write_jsonl(path: Path, records: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("".join(json.dumps(record) + "\n" for record in records))


def fetch_html(url: str) -> str:
    with urllib.request.urlopen(url) as response:
        return response.read().decode("utf-8", errors="replace")


def extract_hrefs(html: str) -> list[str]:
    return re.findall(r'href="([^"]+)"', html, flags=re.IGNORECASE)


def absolute_urls(listing_url: str) -> list[str]:
    html = fetch_html(listing_url)
    return [
        urllib.request.urljoin(listing_url, href)
        for href in extract_hrefs(html)
        if href.lower().endswith(".zip")
    ]


def matching_urls(listing_url: str, pattern: str) -> list[str]:
    regex = re.compile(pattern, flags=re.IGNORECASE)
    results = [url for url in absolute_urls(listing_url) if regex.search(url)]
    if not results:
        raise RuntimeError(f"no URLs matched {pattern} under {listing_url}")
    return sorted(set(results))


def source_url(url: str) -> dict:
    return {"event": "source_url", "label": "data", "url": url}


def list_crawl(label: str, url: str, match: str, results: list[str]) -> dict:
    return {
        "event": "list_crawl",
        "label": label,
        "match": match,
        "results": results,
        "url": url,
    }


def build_source_url_tree(
    root: Path,
    chart_date: str,
    csup_iso_date: str,
    data_iso_date: str,
    tpp_compact_date: str,
    cifp_compact_date: str,
) -> Path:
    root.mkdir(parents=True, exist_ok=True)
    sectional_files_url = f"https://aeronav.faa.gov/visual/{chart_date}/sectional-files/"
    caribbean_url = f"https://aeronav.faa.gov/visual/{chart_date}/Caribbean/"
    tac_files_url = f"https://aeronav.faa.gov/visual/{chart_date}/tac-files/"
    enroute_url = f"https://aeronav.faa.gov/enroute/{chart_date}/"

    sectional_urls = matching_urls(sectional_files_url, rf"/visual/{chart_date}/sectional-files/.*\.zip$")
    caribbean_urls = matching_urls(caribbean_url, rf"/visual/{chart_date}/Caribbean/.*\.zip$")
    tac_urls = matching_urls(tac_files_url, rf"/visual/{chart_date}/tac-files/.*_TAC\.zip$")
    enr_l_urls = sorted(
        set(
            matching_urls(enroute_url, rf"/enroute/{chart_date}/ENR_L\d+\.zip$")
            + matching_urls(enroute_url, rf"/enroute/{chart_date}/ENR_AKL\d+\.zip$")
            + matching_urls(enroute_url, rf"/enroute/{chart_date}/ENR_P\d+\.zip$")
        )
    )
    enr_h_urls = sorted(
        set(
            matching_urls(enroute_url, rf"/enroute/{chart_date}/ENR_H\d+\.zip$")
            + matching_urls(enroute_url, rf"/enroute/{chart_date}/ENR_AKH\d+\.zip$")
            + matching_urls(enroute_url, rf"/enroute/{chart_date}/ENR_P\d+\.zip$")
        )
    )

    write_jsonl(
        root / "charts-sec" / "source_urls.jsonl",
        [
            list_crawl(
                "charts-sec",
                "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/",
                rf"^http.*{chart_date}/sectional-files/.*.zip$",
                sectional_urls,
            ),
            list_crawl(
                "charts-sec",
                "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/",
                rf"^http.*{chart_date}/Caribbean/.*.zip$",
                caribbean_urls,
            ),
        ],
    )
    write_jsonl(
        root / "charts-tac" / "source_urls.jsonl",
        [
            list_crawl(
                "charts-tac",
                "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/vfr/",
                rf"^http.*{chart_date}.*_TAC.zip$",
                tac_urls,
            )
        ],
    )
    write_jsonl(
        root / "charts-enr-l" / "source_urls.jsonl",
        [
            list_crawl(
                "charts-enr-l",
                "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                rf"^http.*{chart_date}/enr_l.*.zip$",
                enr_l_urls,
            ),
        ],
    )
    write_jsonl(
        root / "charts-enr-h" / "source_urls.jsonl",
        [
            list_crawl(
                "charts-enr-h",
                "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/ifr/",
                rf"^http.*{chart_date}/enr_h.*.zip$",
                enr_h_urls,
            ),
        ],
    )
    write_jsonl(
        root / "csup" / "source_urls.jsonl",
        [
            list_crawl(
                "csup",
                "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dafd/",
                rf"^http.*DCS_{csup_iso_date.replace('-', '')}.zip$",
                [f"https://aeronav.faa.gov/Upload_313-d/supplements/DCS_{csup_iso_date.replace('-', '')}.zip"],
            )
        ],
    )
    tpp_urls = [
        f"https://aeronav.faa.gov/upload_313-d/terminal/DDTPP{suffix}_{tpp_compact_date}.zip"
        for suffix in ["A", "B", "C", "D", "E"]
    ]
    for region in REGIONS:
        write_jsonl(
            root / f"tpp-{region}" / "source_urls.jsonl",
            [
                list_crawl(
                    f"tpp-{region}",
                    "https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dtpp/",
                    rf"^http.*DDTPP[A-E]+_{tpp_compact_date}.zip$",
                    tpp_urls,
                )
            ],
        )
    write_jsonl(
        root / "data" / "source_urls.jsonl",
        [
            source_url(f"https://nfdc.faa.gov/webContent/28DaySub/28DaySubscription_Effective_{data_iso_date}.zip"),
            source_url(f"https://nfdc.faa.gov/webContent/28DaySub/{data_iso_date}/aixm5.0.zip"),
            source_url("https://aeronav.faa.gov/Obst_Data/DAILY_DOF_DAT.ZIP"),
            source_url(f"https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_{cifp_compact_date}.zip"),
        ],
    )
    return root


def run_build_once(
    repo_root: Path,
    artifact_root: Path,
    source_urls_root: Path,
    obstacle_snapshot: str,
) -> None:
    env = os.environ.copy()
    env["AEROBAG_ARTIFACT_WRITE_PATH"] = str(artifact_root)
    env["AEROBAG_SOURCE_URLS_ROOT"] = str(source_urls_root)
    env["AEROBAG_OBSTACLE_SNAPSHOT_DATE"] = obstacle_snapshot
    env["PRODUCT_BUILD_CGROUP_ACTIVE"] = "1"
    env["FETCH_CACHE_ROOT"] = str(artifact_root / "cache" / "fetch")
    env["FETCH_CACHE_MODE"] = "fill"
    cmd = [
        "cargo",
        "run",
        "-q",
        "-p",
        "preprocessor-cli",
        "--manifest-path",
        "product/preprocessor/Cargo.toml",
        "--",
        "build-cycle",
    ]
    subprocess.run(cmd, cwd=repo_root, env=env, check=True)


def print_cache_summary(artifact_root: Path, label: str) -> None:
    manifest_dir = artifact_root / "cache" / "build-manifests"
    manifests = sorted(manifest_dir.glob("**/build-manifest_*.json"))
    if not manifests:
        print(f"{label} cache summary unavailable: missing build-manifest_*.json under {manifest_dir}")
        return
    manifest_path = manifests[-1]
    manifest = json.loads(manifest_path.read_text())
    nodes = manifest.get("nodes", [])
    cache_hits = sum(1 for node in nodes if node.get("cache_hit"))
    rebuilt = len(nodes) - cache_hits
    elapsed_ms = sum(int(node.get("elapsed_ms", 0)) for node in nodes)
    rebuilt_names = [node.get("name", "?") for node in nodes if not node.get("cache_hit")]
    print(
        json.dumps(
            {
                "label": label,
                "node_count": len(nodes),
                "cache_hits": cache_hits,
                "rebuilt": rebuilt,
                "elapsed_ms_sum": elapsed_ms,
                "rebuilt_names": rebuilt_names,
            },
            indent=2,
            sort_keys=True,
        )
    )


def run_build(
    repo_root: Path,
    artifact_root: Path,
    source_urls_root: Path,
    obstacle_snapshot: str,
    label: str,
    attempts: int = 3,
) -> None:
    for attempt in range(1, attempts + 1):
        try:
            run_build_once(repo_root, artifact_root, source_urls_root, obstacle_snapshot)
            print_cache_summary(artifact_root, label)
            return
        except subprocess.CalledProcessError:
            if attempt == attempts:
                raise
            print(f"{label} build attempt {attempt} failed; retrying after FAA/network hiccup...")
            time.sleep(15)


def main() -> int:
    repo_root = Path(__file__).resolve().parents[3]
    artifact_root = repo_root.parent / "aerobag-artifacts-vintage-test"
    source_root = artifact_root / "manual-source-urls"
    resume = os.environ.get("AEROBAG_VINTAGE_TEST_RESUME") == "1"
    if artifact_root.exists() and not resume:
        shutil.rmtree(artifact_root)
    artifact_root.mkdir(parents=True, exist_ok=True)

    older_root = build_source_url_tree(
        source_root / "older",
        chart_date="01-22-2026",
        csup_iso_date="2026-01-22",
        data_iso_date="2026-01-22",
        tpp_compact_date="260122",
        cifp_compact_date="260122",
    )
    current_root = build_source_url_tree(
        source_root / "current",
        chart_date="03-19-2026",
        csup_iso_date="2026-03-19",
        data_iso_date="2026-04-16",
        tpp_compact_date="260416",
        cifp_compact_date="260416",
    )

    print("running older vintage build...")
    run_build(repo_root, artifact_root, older_root, "2026-01-23", "older vintage")
    fetch_before = snapshot_tree(artifact_root / "cache" / "fetch")
    nodes_before = snapshot_tree(artifact_root / "cache" / "nodes")

    print("running current vintage build...")
    run_build(repo_root, artifact_root, current_root, "2026-04-10", "current vintage")
    fetch_after = snapshot_tree(artifact_root / "cache" / "fetch")
    nodes_after = snapshot_tree(artifact_root / "cache" / "nodes")

    assert_superset(fetch_before, fetch_after, "fetch cache")
    assert_superset(nodes_before, nodes_after, "node cache")

    summary = {
        "artifact_root": str(artifact_root),
        "fetch_before": len(fetch_before),
        "fetch_after": len(fetch_after),
        "nodes_before": len(nodes_before),
        "nodes_after": len(nodes_after),
        "new_fetch_paths": len(fetch_after) - len(fetch_before),
        "new_node_paths": len(nodes_after) - len(nodes_before),
    }
    summary_path = artifact_root / "vintage_isolation_summary.json"
    summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(f"summary_path {summary_path}")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
