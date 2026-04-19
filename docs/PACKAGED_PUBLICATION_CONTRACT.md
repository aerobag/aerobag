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

- `published-packaged/`
- `published-unpacked/`

The packaged and unpacked surfaces are both flat at the top level.


## Canonical Filenames

Top-level discovery:

- `current_artifacts_YYYYMMDD.json`

Per-cycle manifests:

- `bundle_YYCC.json`

Per-cycle metadata:

- `catalog_YYCC.json`
- `resource_index_YYCC.json`

Per-cycle data packages:

- `data_YYCC.zip`
- `vectors_data_YYCC.zip`

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
- `geo_<sha256>.zip`

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
│   ├── bundle_2603.json
│   └── bundle_2604.json
├── fast_products[]
│   ├── tfrs_<sha256>.zip
│   ├── metars_<sha256>.zip
│   └── nexrad_<sha256>.zip
├── static_products[]
│   └── geo_<sha256>.zip
└── obstacles
    └── obstacles_<sha256>.zip

bundle_YYCC.json
├── catalog_YYCC.json
├── resource_index_YYCC.json
├── data_YYCC.zip
├── vectors_data_YYCC.zip
└── packages[]
    ├── sec_*.zip
    ├── tac_*.zip
    ├── enr_l_*.zip
    ├── enr_h_*.zip
    ├── csup_*.zip
    └── tpp_*.zip
```

Consumer rule:

1. discover from `current_artifacts_YYYYMMDD.json`
2. choose a cycle via `bundle_YYCC.json`
3. fetch leaf artifacts named by that bundle


## Unpacked Contract

`published-unpacked/` mirrors `published-packaged/` with one transformation:

- every non-zip published file remains a sibling file with the same filename
- every published `foo.zip` becomes a sibling directory `foo/`
- every zip member `X/Y/Z.ext` inside `foo.zip` appears at `foo/X/Y/Z.ext`

Examples:

- `published-packaged/data_2604.zip`
  becomes
  `published-unpacked/data_2604/`
- `published-packaged/tpp_ne_2604.zip`
  becomes
  `published-unpacked/tpp_ne_2604/`
- `published-packaged/obstacles_<sha256>.zip`
  becomes
  `published-unpacked/obstacles_<sha256>/`

Examples of top-level unpacked files that remain files:

- `current_artifacts_YYYYMMDD.json`
- `bundle_YYCC.json`
- `catalog_YYCC.json`
- `resource_index_YYCC.json`

The unpacked contract allows a consumer to browse the exact published content shape
without re-extracting zip files locally.


## Manifest Rules

Published manifests must reference sibling filenames only.

Good:

- `bundle_2604.json`
- `catalog_2604.json`
- `resource_index_2604.json`
- `data_2604.zip`
- `tpp_ne_2604.zip`

Bad:

- `cache/nodes/data/.../output/data_2604.zip`
- `private-work/tpp-ne-2604/...`
- `published-packaged/work/resource-index/...`

If a manifest keeps a path field, it must still be flat:

- `relative_path = "resource_index_2604.json"`

not:

- `relative_path = "published-packaged/work/resource-index/abcd/resource-index.json"`


## Intended Semantics

### `current_artifacts_YYYYMMDD.json`

Top-level freshness and discovery document.

It answers:

- what date this publication set represents
- which bundle manifests are current
- which obstacle zip is current
- which optional standalone static products are current
- which standalone fast products are current

It does not replace `bundle_YYCC.json`.


### `bundle_YYCC.json`

Per-cycle package manifest.

It answers:

- which metadata files belong to this cycle
- which data/vector packages belong to this cycle
- which regional chart/CSUP/TPP packages belong to this cycle


### `catalog_YYCC.json`

Per-cycle leaf metadata artifact for catalog-style browsing.


### `resource_index_YYCC.json`

Per-cycle leaf metadata artifact for runtime lookup and asset indexing.


### `data_YYCC.zip`

Per-cycle nav-data package.


### `vectors_data_YYCC.zip`

Per-cycle vector-data package.


### `obstacles_<sha256>.zip`

Standalone content-addressed obstacle artifact.

It is not owned by one cycle in the same way as chart/data/TPP packages.


### `geo_<sha256>.zip`

Optional standalone static geo grid artifact.

It contains Avare-compatible `geo.csv` with one-degree grid rows:
`Latitude,Longitude,geoid_height,magnetic_declination`.

It is referenced from `current_artifacts_YYYYMMDD.json` under `static_products[]`,
not from any per-cycle bundle, so consumers fetch it only if they explicitly need it.

### `terrain-<region>_<sha256>.zip`

Standalone content-addressed terrain artifact.

It is referenced from `current_artifacts_YYYYMMDD.json` under `static_products[]`.
Consumers fetch it only if they explicitly need terrain.

The package contains `manifest.json` plus `tiles/<z>/<x>/<y>.terrain` members.
Terrain tile members are gzip-compressed `ABT1` payloads stored directly in the
outer zip. The outer zip must not deflate `.terrain` members again.

When serving `published-unpacked/terrain-*/tiles/**/*.terrain` over HTTP, the
server should treat the file bytes as precompressed content:

- `Content-Type: application/vnd.aerobag.terrain`
- `Content-Encoding: gzip`
- no additional dynamic gzip/deflate recompression

With those headers, browser fetch consumers receive decompressed `ABT1` bytes.
Offline zip consumers that read directly from `published-packaged/*.zip` must
gzip-decode the member payload after reading the zip entry.


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
     - hardlinked flat published files in `published-packaged/`
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
   - mirror non-zip published files as sibling files in `published-unpacked/`

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
- emit internal cache markers into `published-unpacked/`
- keep legacy directory trees like `published-unpacked/production/`


## Validation Rules

We should add a post-build validation step that checks:

- every filename referenced by `current_artifacts_*.json` exists
- every filename referenced by `bundle_YYCC.json` exists
- every referenced path is flat, with no `/`
- every published `*.zip` in `published-packaged/` has a sibling directory in `published-unpacked/`
- every top-level non-zip public file in `published-packaged/` has a sibling file in `published-unpacked/`
- no published manifest contains:
  - `cache/`
  - `private-work/`
  - `work/`
  - `published-packaged/`
- `published-unpacked/` contains no:
  - `.source-zip-sha256`
  - `production/`

That validator should fail the build.


## Consumer Guidance

Consumers should assume:

- filenames are stable and flat
- `bundle_YYCC.json` is the authoritative per-cycle manifest
- `current_artifacts_YYYYMMDD.json` is the authoritative top-level discovery document
- `published-unpacked/` is a direct unzip-shaped mirror of `published-packaged/`

Consumers should not:

- crawl internal directories
- infer publication structure from cache layout
- follow historical `work/...` paths
- rely on `build-manifest_*.json`, which is internal and not part of the public contract
