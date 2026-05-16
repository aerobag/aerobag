# Publication Contract

## Goal

Define stable, flat, published artifact surfaces for:

- snapshotting
- UI discovery
- device download
- multi-cycle coexistence

The key rule is:

- published metadata must reference only published filenames
- internal build paths such as `cache/nodes/...`, `private-work/...`, and `work/...` are not part of the contract


## Roots

Published roots live in:

- `published_packaged/`
- `published_unpacked/`

The packaged and unpacked surfaces are both flat at the top level.


## Canonical Filenames

Top-level discovery:

- `current_artifacts_YYYYMMDD.json`

Per-cycle manifests:

- `bundle_cycle_YYCC_VV_<sha256>.json`

Per-cycle metadata:

Per-cycle data packages:

- `data_YYCC_VV_<sha256>.zip`
- `vectors_data_YYCC_VV_<sha256>.zip`

Per-cycle regional packages:

- `sec_<region>_YYCC.zip`
- `tac_<region>_YYCC.zip`
- `enr_l_<region>_YYCC.zip`
- `enr_h_<region>_YYCC.zip`
- `csup_<region>_YYCC.zip`
- `tpp_<region>_YYCC.zip`

Standalone artifacts:

- `obstacles_<sha256>.zip`
- `tfrs_<sha256>.zip`
- `metars_<sha256>.zip`
- `nexrad_<sha256>.zip`

Operational files:

- `orchestrator-logs/master.log`

Region codes stay lowercase:

- `ak`
- `pac`
- `nw`
- `sw`
- `nc`
- `ec`
- `sc`
- `ne`
- `se`


## Discovery Hierarchy

```text
current_artifacts_YYYYMMDD.json
├── bundles[]
│   ├── bundle_cycle_2603_01_<sha256>.json
│   ├── bundle_cycle_2604_01_<sha256>.json
│   └── bundle_fast_<sha256>.json

bundle_cycle_YYCC_VV_<sha256>.json
└── packages[]
    ├── sec_*.zip
    ├── tac_*.zip
    ├── enr_l_*.zip
    ├── enr_h_*.zip
    ├── csup_*.zip
    ├── tpp_*.zip
    ├── vectors_data_*.zip
    ├── nav_db_*.zip
    ├── terrain-*.zip
    └── shaded-relief-*.zip

bundle_fast_<sha256>.json
└── packages[]
    ├── obstacles_*.zip
    ├── tfrs_*.zip
    ├── metars_*.zip
    └── nexrad_*.zip
```

Consumer rule:

1. discover from `current_artifacts_YYYYMMDD.json`
2. choose a cycle via `bundle_cycle_YYCC_VV_<sha256>.json`
3. fetch leaf artifacts named by that bundle


## Unpacked Contract

`published_unpacked/` mirrors `published_packaged/` with one transformation:

- every non-zip published file remains a sibling file with the same filename
- every published `foo.zip` becomes a sibling directory `foo/`
- every zip member `X/Y/Z.ext` inside `foo.zip` appears at `foo/X/Y/Z.ext`

Examples:

- `published_packaged/tpp_ne_2604_01_<sha256>.zip`
  becomes
  `published_unpacked/tpp_ne_2604_01_<sha256>/`
- `published_packaged/obstacles_<sha256>.zip`
  becomes
  `published_unpacked/obstacles_<sha256>/`

Examples of top-level unpacked files that remain files:

- `current_artifacts_YYYYMMDD.json`
- `bundle_cycle_YYCC_VV_<sha256>.json`

The unpacked contract allows a consumer to browse the exact published content shape
without re-extracting zip files locally.


## Manifest Rules

Published manifests must reference sibling filenames only.

Good:

- `bundle_cycle_2604_01_<sha256>.json`
- `tpp_ne_2604_01_<sha256>.zip`

Bad:

- `cache/nodes/data/.../output/data_2604.zip`
- `private-work/tpp-ne-2604/...`
- `published_packaged/work/resource-index/...`

If a manifest keeps a path field, it must still be flat:

- `relative_path = "nav_db_2604_01_<sha256>.zip"`

not:

- `relative_path = "published_packaged/work/resource-index/abcd/resource-index.json"`


## Intended Semantics

### `current_artifacts_YYYYMMDD.json`

Top-level freshness and discovery document.

It answers:

- what date this publication set represents
- which bundle manifests are current
- which obstacle zip is current
- which optional standalone static products are current
- which standalone fast products are current

It does not replace `bundle_cycle_YYCC_VV_<sha256>.json`.


### `bundle_cycle_YYCC_VV_<sha256>.json`

Per-cycle package manifest.

It answers:

- which data/vector packages belong to this cycle
- which nav_db package belongs to this cycle
- which regional chart/CSUP/TPP packages belong to this cycle
- which ancillary debug/transitional artifacts belong to this cycle


### `nav_db_YYCC_VV_<sha256>.zip`

Per-cycle app-native key/value runtime index package.

The zip contains `nav_kv_YYCC.root` plus `nav_kv_YYCC.values_NNNN` page files.
Android installs the zip atomically. Web may read the unpacked mirror.

The initial required keys are:

```text
chart/catalog
resource/families
resource/regions
resource/temporal-summary
package/index
package/by-id/{package_id}
```

`chart/catalog` is JSON for the app-ready raster chart catalog. Consumers
should use this instead of parsing per-cycle publication metadata to discover
selectable raster chart layers.
The catalog includes tiled chart packages and app-visible static visual raster
products, such as shaded relief.

Plate and procedure data are not published as one bulk chart-page catalog. They
are published under consumer-shaped HAD keyspaces such as
`plate/airport/{airport_id}`, `plate/by-id/{plate_id}`,
`plate/cifp/{airport_id}/{cifp_id}`, and
`procedure/materialization-rows/{airport_id}/{procedure_id}`. The current
keyspace inventory lives in `docs/HAD_QUERY_KEYSPACES.md`.


### `vectors_data_YYCC_VV_<sha256>.zip`

Per-cycle vector-data package.


### `obstacles_<sha256>.zip`

Standalone content-addressed obstacle artifact.

It is published as a package row in `bundle_fast_<sha256>.json`, not in any cycle
bundle.


### `terrain-<region>_<sha256>.zip`

Standalone content-addressed terrain artifact.

It is listed in the current cycle bundle `packages[]`.

Consumers fetch it only if they explicitly need terrain.

Magnetic variation is not published as a standalone package. It is generated
into the nav-db HAD `magvar/` keyspace from NOAA/NCEI WMM2025 coefficients.
The nav-db also carries `magvar/source`, which records the WMM model, epoch,
coefficient release date, computed decimal year, and citation.

The package contains `manifest.json` plus `tiles/<z>/<x>/<y>.terrain` members.
The source/max zoom is z10, and parent tiles are generated down to z0. Parent
terrain samples are the maximum valid child elevation over the covered child
sample footprint; all-nodata footprints remain nodata. Terrain tile members are
gzip-compressed `ABT1` payloads stored directly in the outer zip. The outer zip
must not deflate `.terrain` members again.

When serving `published_unpacked/terrain-*/tiles/**/*.terrain` over HTTP, the
server should treat the file bytes as precompressed content:

- `Content-Type: application/vnd.aerobag.terrain`
- `Content-Encoding: gzip`
- no additional dynamic gzip/deflate recompression

With those headers, browser fetch consumers receive decompressed `ABT1` bytes.
Offline zip consumers that read directly from `published_packaged/*.zip` must
gzip-decode the member payload after reading the zip entry.

### `shaded-relief-<region>_<sha256>.zip`

Standalone content-addressed shaded-relief raster artifact.

It is listed in the current cycle bundle `packages[]`.

Consumers fetch it only if they explicitly need a terrain-background visual
layer.

The package contains `manifest.json` plus `tiles/<z>/<x>/<y>.webp` members.
The source/max zoom uses the same `z10` / `512x512` grid as the numeric terrain
product, with alpha-preserving RGBA parent tiles generated down to z0. WebP tile
members are already image-compressed and are stored in the outer zip without
another deflate pass.

The initial renderer derives directly from the same USGS 3DEP DEM inputs as
numeric terrain, not from the published `.terrain` tiles. It applies coarse
sectional-style elevation color buckets and a DEM hillshade. Nodata pixels are
transparent. Water and glacier masks are not part of the first cut.


## Non-Goals

This contract does not require:

- build cache paths to be flat
- internal node outputs to be renamed

It only requires that the final published packaged and unpacked surfaces be flat and stable.


## Recommended Implementation

Yes: implement this as explicit publish nodes inside the content-addressed build graph.

That is the right model.

### Why

If publication remains an ad hoc side effect, we keep reintroducing:

- internal path leakage into manifests
- snapshot inconsistencies
- ambiguity about what is canonical

If publication is modeled as graph nodes:

- inputs are explicit
- filenames are deterministic
- manifest content is fingerprinted
- cache reuse is preserved
- contract breakage becomes testable


## Proposed Build-Graph Shape

Per cycle:

1. build internal artifacts as we do now
   - charts
   - CSUP
   - TPP
   - data
   - vectors
   - resource-index

2. add a `publish-cycle-YYCC` node
   - inputs:
     - bundle metadata inputs
     - `catalog.json`
     - `resource-index.json`
     - `data_YYCC.zip`
     - `vectors_data_YYCC.zip`
     - all regional package zips
   - outputs:
     - hardlinked flat published files in `published_packaged/`
     - `bundle_YYCC.json` written against those flat filenames

3. add a top-level `publish-current-artifacts` node
   - inputs:
     - all `publish-cycle-*` outputs
     - published obstacle zip
     - publication date
   - outputs:
     - `current_artifacts_YYYYMMDD.json`

4. add unpacked publish nodes
   - per published zip, materialize a sibling unpacked directory using hardlinks
     from the pre-zip source tree
   - mirror non-zip published files as sibling files in `published_unpacked/`

Obstacle publishing can remain its own content-addressed publish node, but the
published obstacle zip should also have an unpacked sibling directory.


## Publish-Node Behavior

The publish node should:

- compute canonical published filenames
- hardlink source artifacts into those filenames when possible
- fall back to copy if hardlinking fails
- write manifests that reference only canonical filenames

It should not:

- expose source cache paths
- expose `work/`
- expose `private-work/`
- expose node fingerprints in the published contract
- emit internal cache markers into `published_unpacked/`
- keep legacy directory trees like `published_unpacked/production/`


## Validation Rules

We should add a post-build validation step that checks:

- every filename referenced by `current_artifacts_*.json` exists
- every filename referenced by `bundle_YYCC.json` exists
- every referenced path is flat, with no `/`
- every published `*.zip` in `published_packaged/` has a sibling directory in `published_unpacked/`
- every top-level non-zip public file in `published_packaged/` has a sibling file in `published_unpacked/`
- no published manifest contains:
  - `cache/`
  - `private-work/`
  - `work/`
  - `published_packaged/`
- `published_unpacked/` contains no:
  - `.source-zip-sha256`
  - `production/`

That validator should fail the build.


## Consumer Guidance

Consumers should assume:

- filenames are stable and flat
- `bundle_YYCC.json` is the authoritative per-cycle manifest
- `current_artifacts_YYYYMMDD.json` is the authoritative top-level discovery document
- `published_unpacked/` is a direct unzip-shaped mirror of `published_packaged/`

Consumers should not:

- crawl internal directories
- infer publication structure from cache layout
- follow historical `work/...` paths
- rely on `build-manifest_*.json`, which is internal and not part of the public contract
