#!/usr/bin/env python3
from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
UI_DIR = ROOT / "ui"
CANONICAL_OUT = UI_DIR / "shared-fixtures" / "content-prototype" / "content_fixture.json"
WEB_OUT = UI_DIR / "web-app" / "src" / "domain" / "generated" / "contentFixture.json"
ANDROID_OUT = UI_DIR / "android-app" / "app" / "src" / "main" / "assets" / "fixtures" / "contentFixture.json"

SEC_PACKAGE_OUTPUTS = ROOT / "rust-runs" / "sec-20260405T1619Z" / "work" / "rust-runs" / "sec-20260405T1619Z" / "meta" / "provenance" / "charts-sec" / "package_outputs.jsonl"
TPP_PLATES_DIR = ROOT / "runs" / "20260405T154700Z-tpp-retry" / "work" / "tpp-ne" / "plates" / "BOS"

REGION_ORDER = ["ne", "nc", "nw", "se", "sc", "sw", "ec", "ak", "pac"]
REGION_DISPLAY_NAMES = {
    "ne": "Northeast",
    "nc": "North Central",
    "nw": "Northwest",
    "se": "Southeast",
    "sc": "South Central",
    "sw": "Southwest",
    "ec": "East Coast",
    "ak": "Alaska",
    "pac": "Pacific",
}


def load_jsonl(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def pick_bos_plate() -> tuple[str, str, str]:
    candidates = sorted(TPP_PLATES_DIR.glob("IAP-*.png"))
    if not candidates:
        raise RuntimeError(f"no IAP png plates found in {TPP_PLATES_DIR}")

    path = candidates[0]
    stem = path.stem
    parts = stem.split("-", 2)
    if len(parts) != 3:
        raise RuntimeError(f"unexpected plate filename format: {path.name}")

    plate_prefix, _state_code, procedure_name = parts
    procedure_code = f"{plate_prefix}-{procedure_name}"
    asset_base_path = f"plates/BOS/{stem}"
    return procedure_code, procedure_name, asset_base_path


def build_fixture() -> dict:
    sec_outputs = load_jsonl(SEC_PACKAGE_OUTPUTS)
    cycle = "2026-04-16"

    packages = []
    regions_seen = set()
    for entry in sec_outputs:
        region = entry["region"].lower()
        regions_seen.add(region)
        package_name = entry["manifest"]
        packages.append(
            {
                "id": {
                    "region": region,
                    "family": "sectional",
                    "cycle": cycle,
                },
                "package_name": package_name,
                "family_id": "sectional",
                "region_id": region,
                "cycle": cycle,
                "artifact_kind": "zip",
                "relative_url": f"/{cycle}/{entry['zip']}",
                "manifest_name": package_name,
                "size_bytes": None,
                "checksum_sha256": entry["zip_sha256"],
            }
        )

    packages.sort(key=lambda item: REGION_ORDER.index(item["region_id"]))

    regions = [
        {
            "id": region,
            "display_name": REGION_DISPLAY_NAMES[region],
            "sort_order": REGION_ORDER.index(region),
        }
        for region in REGION_ORDER
        if region in regions_seen
    ]

    procedure_code, procedure_name, asset_base_path = pick_bos_plate()

    return {
        "catalog": {
            "schema_version": 1,
            "cycle": cycle,
            "catalog_revision": "2026-04-06T00:00:00Z",
            "families": [
                {
                    "id": "sectional",
                    "display_name": "VFR Sectional Charts",
                    "kind": "tiled_raster",
                    "max_zoom": 10,
                    "tile_size": 512,
                }
            ],
            "regions": regions,
            "packages": packages,
            "charts": [],
            "plates": [
                {
                    "id": {
                        "airport_id": "BOS",
                        "procedure_code": procedure_code,
                        "page": 1,
                        "cycle": cycle,
                    },
                    "airport_id": "BOS",
                    "region_id": "ne",
                    "cycle": cycle,
                    "procedure_code": procedure_code,
                    "display_name": procedure_name,
                    "kind": "approach",
                    "georeferenced": True,
                    "page_count": 1,
                    "asset_base_path": asset_base_path,
                }
            ],
            "supplements": [],
        },
        "flight_plan": {
            "id": "plan-1",
            "name": "BOS local",
            "legs": [
                {
                    "from": {"Airport": "BOS"},
                    "to": {"Airport": "BOS"},
                    "airway": None,
                }
            ],
            "departure": "BOS",
            "destination": "BOS",
            "alternate": None,
            "cruise_altitude_ft": 3000,
            "notes": "Generated from preprocessor outputs",
            "updated_at_epoch_ms": 0,
            "version": 1,
        },
        "remote_only_inventory": {
            "installed_packages": [],
            "cached_tilesets": [],
            "cached_plates": [],
        },
        "installed_inventory": {
            "installed_packages": [
                {
                    "package_id": {
                        "region": "ne",
                        "family": "sectional",
                        "cycle": cycle,
                    },
                    "integrity_ok": True,
                }
            ],
            "cached_tilesets": [],
            "cached_plates": [],
        },
    }


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=False) + "\n")


def main() -> None:
    fixture = build_fixture()
    write_json(CANONICAL_OUT, fixture)
    write_json(WEB_OUT, fixture)
    write_json(ANDROID_OUT, fixture)


if __name__ == "__main__":
    main()
