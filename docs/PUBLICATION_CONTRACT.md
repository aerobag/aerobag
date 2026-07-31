# Publication Contract

Both Android and web clients start from the public static package root. In the
dev artifact tree that root is:

```text
<artifact-root>/published/
```

In production this directory may be hosted as a public URL such as
`https://aerobag.org/packages/`. The public URL root must expose:

```text
current_artifacts.json
<publish-label>/<publish-timestamp>/packaged/...
<publish-label>/<publish-timestamp>/unpacked/...
```

The build cache, fetch cache, private work, and Rust target directories are not
part of the publication contract.

## Current Artifacts

`current_artifacts.json` is the moving alias for the set of product contract
versions currently being published. It is always a JSON list, even when only one
contract version is available.

Example shape:

```json
[
  {
    "schema_version": 1,
    "contracts": {
      "nav-db": "NAV10",
      "tpp": "TPP1"
    },
    "artifact_roots": {
      "packaged": "main-8829a91b7550/20260603T173005Z/packaged/",
      "unpacked": "main-8829a91b7550/20260603T173005Z/unpacked/"
    },
    "as_of_date": "2026-06-03",
    "as_of_utc": "2026-06-03T17:30:13Z",
    "bundles": [],
    "startup_prefetch": null
  }
]
```

Each list member is a complete publication candidate for one exact set of
product contract identifiers. Clients choose a member by exact contract match;
they must not order or compare contract identifiers numerically. `NAV10` is a
different exact contract from `NAV6`, not "greater than" it for selection
purposes.

`artifact_roots.*` values are relative URL prefixes from the directory
containing `current_artifacts.json`. They must be safe relative paths and must
end with `/`.

Production clients normally fetch only `current_artifacts.json`. Timestamped
top-level `current_artifacts_*.json`, top-level `version_artifacts_*.json`,
`published_packaged/`, and `published_unpacked/` are retired legacy artifacts
and must not be produced or consumed.

## Per-Build Product Artifacts

A single preprocessor build publishes into:

```text
published/<publish-label>/<publish-timestamp>/
```

with this layout:

```text
product_artifacts.json
packaged/
unpacked/
```

`product_artifacts.json` has the same manifest shape as one
`current_artifacts.json` list member. It is not the public moving alias.

`build-product` writes `product_artifacts.json` for its own
`<publish-label>/<publish-timestamp>` directory. The top-level
`published/current_artifacts.json` is written only by `merge-current-artifacts`,
which takes one or more `product_artifacts.json` files and writes their list.
For a single-version dev publication, run the merge step with one manifest.

## Packaged And Unpacked Roots

`packaged/` contains the package-sync form:

- immutable bundle manifests
- immutable package ZIPs
- diagnostics and other top-level public files referenced by manifests

`unpacked/` contains the web/direct-fetch form:

- every non-ZIP file from `packaged/` as the same sibling filename
- every `foo.zip` package from `packaged/` as an unpacked `foo/` directory
- every ZIP member available under that same-stem directory

Do not union packaged and unpacked outputs into one directory. Keeping them
separate preserves the invariant that unpacked output is a mirror transform of
packaged output.

## Bundle Manifests

Each `bundles[]` entry in a product/current manifest names an immutable bundle
manifest under that entry's artifact root. Bundle filenames are content
addressed:

```text
bundle_cycle_<cycle>_<cycle-version>_<sha256>.json
```

Bundle manifests contain `packages[]`. Each package record includes the package
identity, product family, contract id, region/cycle metadata when applicable,
`relative_path`, size/checksum, validity dates, optional `warning_text`, and
optional metadata. Clients follow `relative_path`; they do not infer public
filenames from package ids.

Package contract ids use a product-family prefix, such as `NAV10`, `TPP1`,
`CSUP1`, `SEC1`, and `TER2`. The same string appears in package filenames and
manifest content so the package hash self-describes the declared contract.

## Client Resolution

Android uses the selected manifest's `artifact_roots.packaged`.

1. Fetch `<publicRoot>/current_artifacts.json`.
2. Decode the list and select the member with exactly supported contracts.
3. Fetch each bundle manifest:
   `<publicRoot>/<artifact_roots.packaged>/<bundle.relative_path>`.
4. Read `packages[]` in each bundle.
5. Download package ZIPs:
   `<publicRoot>/<artifact_roots.packaged>/<package.relative_path>`.
6. Verify size/checksum and install atomically.

Web uses the selected manifest's `artifact_roots.unpacked`.

1. Fetch `<publicRoot>/current_artifacts.json`.
2. Decode the list and select the member with exactly supported contracts.
3. Fetch each bundle manifest:
   `<publicRoot>/<artifact_roots.unpacked>/<bundle.relative_path>`.
4. For a package whose packaged `relative_path` is `foo_<hash>.zip`, fetch
   unpacked contents under:
   `<publicRoot>/<artifact_roots.unpacked>/foo_<hash>/...`.

Examples:

- nav HAD package: `nav_db_...zip` becomes `nav_db_.../root` and
  `nav_db_.../page_NNNN`
- chart package: `sec_nw_...zip` becomes `sec_nw_.../tiles/...`
- live-feed payloads are not part of this static package tree. They use the
  separate `/live-feeds` contract.

## Startup Prefetch

`startup_prefetch` is an optional optimization in each selected current/product
manifest. It lists concrete public URLs for a small set of startup-critical
resources, currently nav-db `root` and selected page files.

This is not a complete inventory of nav-db pages. Clients may prefetch these
resources before normal core-driven package reads, but correctness must not
depend on prefetching them.

## Warnings

Package records may carry `warning_text`. Clients should surface warnings from
all selected package manifests through the shared warning UI and Data Status
page. This is the mechanism for sunset warnings such as "this contract version
is being sunsetted; update the app."

## Sparse Tiles

Raster tile packages may be sparse. This is expected for products such as TAC
wide-angle packages, where large areas inside a coarse tile bounding rectangle
have no product coverage.

Tile level metadata such as `levels[]` with `{x_min, x_max, y_tms_min,
y_tms_max}` is a coarse planning/culling bound, not a promise that every tile in
that rectangle exists. A missing unpacked tile file or a `404` response for an
otherwise contract-correct tile URL means "no tile here; draw nothing here."

Servers must not synthesize transparent placeholder tiles for these holes.
Clients should treat missing raster tiles as no-draw, not as a fatal product or
publication-contract error.

## Chart Package Tiers

Chart package manifests declare `metadata.chart_package_tier` as exactly one of
`wide`, `regional`, or `detail`.

- `wide` owns the all-region overview levels through z7.
- `regional` owns one region's normal-resolution levels from z8 through the
  family-specific base maximum.
- `detail` owns only the same region's next zoom level. It contains tiles and
  the package manifest, but does not duplicate chart reference assets.

The NAV chart catalog combines one regional package and its optional detail
package into one logical map view. A detail package must start exactly one zoom
after its regional package and must not overlap the regional levels.

Public-unpacked clients may use every advertised detail package. Clients using
installed packages may use a detail package only when that exact artifact is
installed. If detail is unavailable, core plans from the regional maximum and
intentionally stretches that tile; clients must not probe a missing detail
resource and fall back after an error.

## Package-Relative HAD Paths

HAD records describe logical package contents. Paths inside HAD records are
relative to the package that owns the record; they must not contain public server
routes such as `/packages/...`, `/sectional-packages/...`, `/nav-kv/...`, or
`/live-feeds/...`.

For raster map records, `package_name` identifies the package and
`tile_url_root` is the package-relative tile root, normally `tiles`. Clients
combine the package id with discovered bundle package metadata to find the
installed package or the public unpacked package root, then append
`tile_url_root` and `tile_path_template`.

Clients and server-side staging code should use the same package-member
resolution path: discover `current_artifacts.json`, choose the published
package, and append the package-relative member path. Do not introduce alternate
public content aliases such as `/nav-kv/`, `/sectional-packages/`, or
`/live-feeds/`; they create a second contract that can drift from the published
package tree.

## Rule Of Thumb

Clients do not infer public filenames. They discover immutable artifacts from
`current_artifacts.json`, select one supported contract manifest from the list,
choose either the packaged or unpacked root, follow bundle and package
`relative_path` values, and use package metadata/HAD catalogs for internal
package contents and tile planning.
