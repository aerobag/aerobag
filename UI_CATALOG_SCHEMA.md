# Shared Catalog Schema

## Purpose

`catalog.json` is the generated source of truth consumed by:
- Android UI
- web UI
- Rust shared app core

It replaces hardcoded client metadata such as:
- chart family lists
- region names
- package naming conventions
- chart coverage metadata
- plate and supplement indexes

## Design Rules

- Stable ids first, display strings second
- Explicit schema version
- Additive evolution preferred
- Client-safe to cache by `cycle` and `catalog_revision`
- No platform-specific fields in the base schema

## Top-Level Shape

```json
{
  "schema_version": 1,
  "cycle": "2026-04-16",
  "catalog_revision": "2026-04-05T22:00:00Z",
  "families": [],
  "regions": [],
  "packages": [],
  "charts": [],
  "plates": [],
  "supplements": []
}
```

## Families

```json
{
  "id": "sec",
  "display_name": "VFR Sectional Charts",
  "kind": "tiled_raster",
  "max_zoom": 10,
  "tile_size": 512
}
```

## Regions

```json
{
  "id": "ne",
  "display_name": "Northeast",
  "sort_order": 0
}
```

## Packages

```json
{
  "id": {
    "region": "ne",
    "family": "sec",
    "cycle": "2026-04-16"
  },
  "package_name": "NE_SEC",
  "family_id": "sec",
  "region_id": "ne",
  "cycle": "2026-04-16",
  "artifact_kind": "zip",
  "relative_url": "/2026-04-16/NE_SEC.zip",
  "manifest_name": "NE_SEC",
  "size_bytes": 123456789,
  "checksum_sha256": "..."
}
```

## Charts

```json
{
  "id": {
    "family": "sec",
    "name": "Chicago",
    "cycle": "2026-04-16"
  },
  "family_id": "sec",
  "name": "Chicago",
  "display_name": "Chicago",
  "cycle": "2026-04-16",
  "region_ids": ["nc", "ec"],
  "max_zoom": 10,
  "tile_path_template": "tiles/{chart_index}/{z}/{x}/{y}",
  "coverage": {
    "kind": "polygon_ref",
    "value": {
      "polygon_id": "sec:chicago"
    }
  }
}
```

## Plates

```json
{
  "id": {
    "airport_id": "KBOS",
    "procedure_code": "IAP-ILS-RWY-04R",
    "page": 1,
    "cycle": "2026-04-16"
  },
  "airport_id": "KBOS",
  "region_id": "ne",
  "cycle": "2026-04-16",
  "procedure_code": "IAP-ILS-RWY-04R",
  "display_name": "ILS OR LOC RWY 04R",
  "kind": "approach",
  "georeferenced": true,
  "page_count": 1,
  "asset_base_path": "plates/KBOS/IAP-ILS-RWY-04R"
}
```

## Supplements

```json
{
  "airport_id": "KBOS",
  "region_id": "ne",
  "cycle": "2026-04-16",
  "page_count": 5,
  "asset_base_path": "afd/KBOS"
}
```

## Geometry Sidecar

Keep chart geometry in a separate file:

- `chart_geometry.json`

```json
{
  "schema_version": 1,
  "polygons": [
    {
      "id": "sec:chicago",
      "points": [
        [-93.0001, 44.2002],
        [-93.0001, 40.0],
        [-85.0002, 40.0],
        [-85.0002, 44.2002]
      ]
    }
  ]
}
```

## Minimum Viable Version

Version 1 only needs:
- top-level versioning fields
- families
- regions
- packages
- charts
- enough geometry to answer `chart_for_position`

Plates and supplements can follow immediately after the first content/planning slice.
