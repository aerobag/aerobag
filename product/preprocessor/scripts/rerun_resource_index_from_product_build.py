#!/usr/bin/env python3

import argparse
import json
import os
import subprocess
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--product-build-json", required=True)
    parser.add_argument("--output")
    return parser.parse_args()


def require_node(nodes: list[dict], name: str) -> dict:
    for node in nodes:
        if node["name"] == name:
            return node
    raise SystemExit(f"missing node {name}")


def first_node(nodes: list[dict], prefix: str) -> dict:
    for node in nodes:
        if node["name"].startswith(prefix):
            return node
    raise SystemExit(f"missing node with prefix {prefix}")


def chart_source_spec(nodes: list[dict], artifact_root: Path, source_urls_root: Path, family: str) -> str:
    try:
        node = require_node(nodes, f"charts-{family}-package")
    except SystemExit:
        node = first_node(nodes, f"charts-{family}-package-")
    outputs = node["outputs"]
    package_outputs = artifact_root / outputs["package_outputs"]
    asset_root = artifact_root / outputs["asset_root"]
    package_root = artifact_root / outputs["package_root"]
    unpack_source_root = artifact_root / outputs["unpack_source_root"]
    source_urls = source_urls_root / f"charts-{family}" / "source_urls.jsonl"
    return f"{family}:{package_outputs}:{asset_root}:{package_root}:{unpack_source_root}:{source_urls}"


def asset_source_spec(
    nodes: list[dict],
    artifact_root: Path,
    source_urls_root: Path,
    node_name: str,
    source_urls_name: str,
) -> str:
    node = require_node(nodes, node_name)
    outputs = node["outputs"]
    package_outputs = artifact_root / outputs["package_outputs"]
    asset_root = artifact_root / outputs["asset_root"]
    package_root = artifact_root / outputs["package_root"]
    unpack_source_root = artifact_root / outputs["unpack_source_root"]
    source_urls = source_urls_root / source_urls_name / "source_urls.jsonl"
    return f"{package_outputs}:{asset_root}:{package_root}:{unpack_source_root}:{source_urls}"


def main() -> int:
    args = parse_args()
    product_build_json = Path(args.product_build_json).resolve()
    product_build = json.loads(product_build_json.read_text())
    artifact_root = product_build_json.parent.parent.parent
    nodes = product_build["nodes"]

    source_urls_root = artifact_root / require_node(nodes, "source-urls")["outputs"]["output_dir"]
    nav_db_zip = artifact_root / require_node(nodes, "data")["outputs"]["zip"]
    output_path = (
        Path(args.output).resolve()
        if args.output
        else product_build_json.parent / "work" / "resource-index-debug" / "resource-index.json"
    )
    output_path.parent.mkdir(parents=True, exist_ok=True)

    preprocessor_cli = Path(
        os.environ.get(
            "AEROBAG_PREPROCESSOR_CLI",
            str(Path("../aerobag-artifacts/target/debug/preprocessor-cli").resolve()),
        )
    )
    cmd = [
        str(preprocessor_cli),
        "build-resource-index",
        "--nav-db-zip",
        str(nav_db_zip),
        "--output",
        str(output_path),
    ]

    for family in ["sec", "tac", "enr-l", "enr-h"]:
        cmd.extend(["--chart-source", chart_source_spec(nodes, artifact_root, source_urls_root, family)])

    for region in ["ne", "nw"]:
        cmd.extend(
            [
                "--tpp-source",
                asset_source_spec(
                    nodes,
                    artifact_root,
                    source_urls_root,
                    f"tpp-{region}-package",
                    f"tpp-{region}",
                ),
            ]
        )

    for region in ["ak", "pac", "nw", "sw", "nc", "ec", "sc", "ne", "se"]:
        cmd.extend(
            [
                "--csup-source",
                asset_source_spec(
                    nodes,
                    artifact_root,
                    source_urls_root,
                    f"csup-package-{region}",
                    "csup",
                ),
            ]
        )

    print(" ".join(cmd))
    result = subprocess.run(cmd, check=False)
    return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())
