Both Android and web clients start from the public static discovery root:

```text
https://aerobag.org/packages/current_artifacts.json
```

The public product tree is rooted at `/packages`, one path component below `/`
so hosting can serve or redirect the whole product tree independently from the
web app.

Use `current_artifacts.json` with underscores. This matches the producer's
existing naming convention and the timestamped historical manifests:

```text
current_artifacts_YYYYMMDDTHHMMSSZ.json
```

## Public Layout

```text
/packages/current_artifacts.json
/packages/published_packaged/...
/packages/published_unpacked/...
```

`current_artifacts.json` and its timestamped historical siblings live only at
the `/packages` root. They are not mirrored into `published_packaged/` or
`published_unpacked/`.

`published_packaged/` contains the package-sync form:

- immutable bundle manifests
- immutable package ZIPs
- diagnostics
- other top-level public files referenced by discovery or bundles

`published_unpacked/` contains the web/direct-fetch form:

- every non-ZIP file from `published_packaged/` as the same sibling filename
- every `foo.zip` package from `published_packaged/` as an unpacked `foo/`
  directory
- every ZIP member available under that same-stem directory

Do not union packaged and unpacked outputs into one directory. Keeping them
separate preserves the simple invariant that unpacked output is a mirror
transform of packaged output.

## Discovery Manifest

`current_artifacts.json` is the moving alias for "what is current now." It
lists immutable bundle manifests and tells clients where packaged and unpacked
artifact trees live.

Required root fields include:

```json
{
  "artifact_roots": {
    "packaged": "published_packaged/",
    "unpacked": "published_unpacked/"
  },
  "bundles": []
}
```

`artifact_roots.*` values are relative URL prefixes from the directory
containing `current_artifacts.json`. They must end with `/`.

Historical timestamped `current_artifacts_*.json` files may also exist for
tests and cycle-boundary simulation. Production clients normally start at
`current_artifacts.json`.

## Android Resolution

Android uses `artifact_roots.packaged`.

1. Fetch:
   `https://aerobag.org/packages/current_artifacts.json`
2. Fetch each bundle manifest:
   `<staticBaseUrl>/<artifact_roots.packaged>/<bundle.relative_path>`
3. Read `packages[]` in each bundle. Each package has `id`, `family_id`,
   `region_id`, `relative_path`, `size_bytes`, `checksum_sha256`,
   `effective_date`, and optional `expiration_date`.
4. Download package ZIPs:
   `<staticBaseUrl>/<artifact_roots.packaged>/<package.relative_path>`
5. Verify size/checksum and install atomically.

Android must not infer filenames or ask a dynamic server to map package IDs to
files. It follows `relative_path` from the packaged root.

## Web Resolution

Web uses `artifact_roots.unpacked`.

1. Fetch:
   `https://aerobag.org/packages/current_artifacts.json`
2. Fetch each bundle manifest:
   `<staticBaseUrl>/<artifact_roots.unpacked>/<bundle.relative_path>`
3. Read the same `packages[]` contract.
4. For a package whose packaged `relative_path` is `foo_<hash>.zip`, fetch
   unpacked contents under:
   `<staticBaseUrl>/<artifact_roots.unpacked>/foo_<hash>/...`

Examples:

- nav HAD package: `nav_db_...zip` becomes `nav_db_.../root` and
  `nav_db_.../values/<page>`
- chart package: `sec_nw_...zip` becomes `sec_nw_.../tiles/...`
- fast package: `metars_...zip` becomes `metars_.../manifest.json`,
  `metars_.../metars.json`, and tile files

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

The web app may stage or alias these unpacked package paths into friendlier
runtime routes such as `/nav-kv/`, `/sectional-packages/`, or `/fast-products/`,
but those aliases are client/server presentation details. The publication
contract is the `/packages` discovery tree above.

## Rule Of Thumb

Clients do not infer public filenames. They discover immutable artifacts from
`current_artifacts.json`, choose either the packaged or unpacked root, follow
bundle and package `relative_path` values, and use package metadata/HAD catalogs
for internal package contents and tile planning.
