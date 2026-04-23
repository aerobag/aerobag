# Package Manifest Cleanup Plan

## Goals

- Make package download/install planning self-contained in static publication metadata.
- Keep semantic app lookup data out of package-management metadata.
- Let Android install from read-only published artifacts without filename inference or a dynamic resolver.
- Keep web able to fetch unpacked artifact pieces dynamically.
- Separate app-intended packages from transitional/debug artifacts.

## Target Split

`current_artifacts_YYYYMMDD.json` is the discovery root. It points clients at the current bundle manifests and diagnostics, but it is not itself the package inventory.

`bundle_cycle_YYCC_VV_<sha256>.json` is the cycle package manifest. It is the authority for app package planning and download resolution for one FAA cycle and producer correction version.

HAD/nav DB is the semantic lookup surface. It answers app-domain questions like "which plates exist for KSEA?" and "which package owns this plate?" Package IDs are the join key into `bundle_cycle_YYCC_VV_<sha256>.json`.

`ancillary[]` entries are published debug/transitional artifacts. They are verified by publication tooling but are not part of Android's app-installable package set.

## Rename Cycle Bundle

Rename:

```text
bundle_2604.json
```

to:

```text
bundle_cycle_2604_01_<sha256>.json
```

Expected top-level shape:

```json
{
  "schema_version": 2,
  "bundle_id": "cycle_2604_01",
  "bundle_type": "cycle",
  "cycle": "2604",
  "cycle_version": "01",
  "effective_date": "2026-04-16",
  "expiration_date": "2026-05-14",
  "generated_at_utc": "...",
  "packages": [],
  "ancillary": []
}
```

The `cycle_version` starts at `01` for the first publication of a cycle. Bumping it is a producer signal that the previous publication had an error worth correcting and that clients should consider re-downloading affected packages. It is separate from `checksum_sha256`: the hash proves exact bytes, while `cycle_version` communicates producer intent.

## Package Rows

Every app-installable package belongs in `packages[]`.

Common fields:

```text
id
family_id
region_id
cycle
cycle_version
relative_path
size_bytes
checksum_sha256
effective_date
expiration_date
```

`region_id` may be `null` for global packages.

Android should fetch:

```text
staticBaseUrl + relative_path
```

Then verify `size_bytes` and `checksum_sha256` before atomically installing.

Clients must not infer filenames.

## Cycle Packages

Move all app-intended cycle installables into `packages[]`, including artifacts that are currently top-level convenience fields.

Examples:

```text
tpp_*_2604_01_<sha256>.zip
csup_*_2603_01_<sha256>.zip, while still valid for cycle 2604
sec_*_2603_01_<sha256>.zip, while still valid for cycle 2604
tac_*_2603_01_<sha256>.zip, while still valid for cycle 2604
enr_*_2603_01_<sha256>.zip, while still valid for cycle 2604
vectors_data_2604_01_<sha256>.zip
nav_db_2604_01_<sha256>.zip, once nav_kv is packaged
```

`vectors_data_2604_01_<sha256>.zip` is package-like enough. Its current problem is schema placement, not content. It should become a normal package row:

```json
{
  "id": "VECTORS_DATA_2604_01",
  "family_id": "vectors",
  "region_id": null,
  "cycle": "2604",
  "cycle_version": "01",
  "relative_path": "vectors_data_2604_01_<sha256>.zip",
  "size_bytes": 85229465,
  "checksum_sha256": "...",
  "effective_date": "2026-04-16",
  "expiration_date": "2026-05-14"
}
```

## Content-Hash Filenames

All package and bundle filenames should include the SHA-256 hash of the exact published file bytes.

Cycle package examples:

```text
tpp_ak_2604_01_<sha256>.zip
csup_ak_2603_01_<sha256>.zip
sec_nw_2603_01_<sha256>.zip
vectors_data_2604_01_<sha256>.zip
nav_db_2604_01_<sha256>.zip
```

Stable product examples:

```text
geo_<sha256>.zip
terrain-nw_<sha256>.zip
shaded-relief-nw_<sha256>.zip
```

Bundle examples:

```text
bundle_cycle_2604_01_<sha256>.json
bundle_fast_YYYYMMDDTHHMMZ_<sha256>.json
```

`current_artifacts_YYYYMMDD.json` should keep a stable discovery name. It points at hashed bundle filenames and records their `checksum_sha256` values.

`relative_path` remains the authority. Clients must still read `relative_path`; they must not infer filenames from package IDs.

`checksum_sha256` remains explicit even when duplicated in the filename. Publication validators should cheaply check that any embedded hash suffix agrees with `checksum_sha256`.

The hash is of the exact published file bytes. For zip files, deterministic zip output is useful for cache efficiency but is not required for correctness. If a zip writer is nondeterministic, the package gets a new hash/name and clients may redownload it, but static fetch integrity remains correct.

## Stable Products

Do not create a separate stable bundle yet.

Stable products are durable packages that may be useful across many FAA cycles, but the current cycle bundle may still list the stable packages available to the app.

Examples:

```text
geo_*.zip
terrain-*.zip
shaded-relief-*.zip
```

Stable package rows should have `effective_date`, meaning the day the product was assembled or published into the package set. They should not have `expiration_date`.

Example:

```json
{
  "id": "terrain-nw",
  "family_id": "terrain",
  "region_id": "nw",
  "relative_path": "terrain-nw_962138046aaadc0ecc94dcf5e6549a29bfd82f0e98bf8e74e90f0596be649bb5.zip",
  "size_bytes": 1091522850,
  "checksum_sha256": "962138046aaadc0ecc94dcf5e6549a29bfd82f0e98bf8e74e90f0596be649bb5",
  "effective_date": "2026-04-17",
  "source_version": "8834c5514d963b93",
  "source_fetched_at_utc": "2026-04-17T03:00:56Z"
}
```

Android can decide, or let the user decide, whether a newer stable product is worth downloading when a later `effective_date` becomes available. The older package remains valid.

This avoids implying that every cycle update strongly recommends redownloading huge stable products for marginal benefit.

## Fast Products

Fast products should not live in the cycle bundle because they roll every few minutes.

Introduce a separate timestamped, content-hash-named fast bundle later:

```text
bundle_fast_YYYYMMDDTHHMMZ_<sha256>.json
```

Examples:

```text
tfrs_*.zip
metars_*.zip
nexrad_*.zip
```

Fast rows should include:

```text
id
family_id
region_id
relative_path
size_bytes
checksum_sha256
source_generated_at_utc
published_at_utc
```

## Nav DB Package

The current nav_kv artifact is multi-file:

```text
nav_kv_2604_01.root
nav_kv_2604_01.values_*
```

Make it one Android-installable package:

```text
nav_db_2604_01_<sha256>.zip
```

Android downloads and atomically installs the zip. Web can still use the unpacked public layout to fetch the root and value pages dynamically.

Package row:

```json
{
  "id": "NAV_DB_2604_01",
  "family_id": "nav-db",
  "region_id": null,
  "cycle": "2604",
  "cycle_version": "01",
  "relative_path": "nav_db_2604_01_<sha256>.zip",
  "size_bytes": 123,
  "checksum_sha256": "...",
  "effective_date": "2026-04-16",
  "expiration_date": "2026-05-14"
}
```

## Ancillary Artifacts

Move transitional/debug artifacts into `ancillary[]`.

Initial entries:

```text
data_2604_01_<sha256>.zip
catalog_2604.json
```

`data_2604_01_<sha256>.zip` contains the old SQLite `main.db`. The app no longer uses it directly; it is subsumed by HAD/nav DB. Keep it ancillary until debug/audit flows no longer need it.

`catalog_2604.json` is the old app catalog. The app-ready raster chart catalog now exists in HAD/nav DB as `chart/catalog`. Keep it ancillary until UI/core only consume HAD/nav DB.

`resource_index_2604.json` was semantic lookup metadata in the wrong publication layer. Its contents have been moved into HAD/nav DB and the standalone public artifact has been removed from the live bundle contract.

Ancillary rows should still have:

```text
id
relative_path
size_bytes
checksum_sha256
```

They may also have explanatory fields:

```text
purpose
removal_condition
```

Android package management should ignore `ancillary[]`.

## Resource Index Migration

`resource_index_2604.json` was eliminated as a standalone public artifact. Its semantic tables now belong in HAD/nav DB.

Examples of semantic lookup questions that belong in HAD/nav DB:

```text
For airport KSEA, which plates exist?
Which package owns this plate?
Which chart resources are associated with this airport?
```

`bundle_cycle_YYCC_VV_<sha256>.json` should not answer these questions. It should only answer package planning and download resolution questions.

## Current Artifacts

`current_artifacts_YYYYMMDD.json` should point at bundle manifests and fast/current metadata.

Example:

```json
{
  "schema_version": 2,
  "as_of_date": "2026-04-22",
  "bundles": [
    {
      "id": "cycle_2604",
      "bundle_type": "cycle",
      "cycle": "2604",
      "cycle_version": "01",
      "relative_path": "bundle_cycle_2604_01_<sha256>.json",
      "checksum_sha256": "...",
      "size_bytes": 123
    },
    {
      "id": "fast_current",
      "bundle_type": "fast",
      "relative_path": "bundle_fast_YYYYMMDDTHHMMZ_<sha256>.json",
      "checksum_sha256": "...",
      "size_bytes": 123
    }
  ]
}
```

## Publication Invariants

Fast checks should run every build:

- Every `relative_path` in bundle `packages[]` and `ancillary[]` exists in `published-packaged`.
- Every listed `size_bytes` matches the file.
- Content-hash filenames agree with `checksum_sha256` when the filename embeds the hash.
- Bundle filenames agree with their own `checksum_sha256` entries in `current_artifacts`.
- Cycle package filenames and IDs include the cycle correction version (`YYCC_VV`) for cycle-scoped packages.
- Every app-intended installable appears in `packages[]`.
- Transitional artifacts such as `data_*.zip` and `catalog_*.json` appear only in `ancillary[]`.

Avoid rehashing huge files on every incremental build. Full SHA checks should run only when:

- the artifact was just produced,
- no trusted build record exists,
- a file has a non-content-hash filename and no trustworthy recorded checksum,
- or an explicit deep audit mode is requested.

Deep audit mode should verify all `checksum_sha256` fields by rereading artifact bytes.

## End-of-Build Regression Test

The production build must end with a contract regression test that asserts the published artifacts comply with this plan.

This should run as an explicit scheduler task after publication and before reporting build success. It should validate the packaged tree and the unpacked tree generated from it.

Required fast assertions:

- `current_artifacts_YYYYMMDD.json` exists and is the only stable discovery entry point clients need.
- Every bundle referenced by `current_artifacts` exists.
- Cycle bundle filenames match `bundle_cycle_YYCC_VV_<sha256>.json`.
- Fast bundle filenames, once implemented, match `bundle_fast_YYYYMMDDTHHMMZ_<sha256>.json`.
- Every app-installable package is listed in `packages[]`.
- No app-installable package appears only as a top-level convenience field.
- Transitional artifacts appear only in `ancillary[]`.
- `packages[]` entries have `id`, `family_id`, `region_id`, `relative_path`, `size_bytes`, `checksum_sha256`, and validity fields appropriate to their package type.
- Cycle package IDs and filenames include `YYCC_VV`.
- Stable package rows have `effective_date` and no `expiration_date`.
- Every `relative_path` exists in `published-packaged`.
- Every package `size_bytes` matches the filesystem.
- Embedded filename hashes match the recorded `checksum_sha256`.
- JSON metadata contains no internal `cache/`, `work/`, `private-work/`, or node-cache paths.

Required unpacked assertions:

- `published-unpacked` mirrors the packaged contract.
- Every zip package in `published-packaged` appears as a same-stem directory in `published-unpacked`.
- Every JSON/root/page metadata file that is not a zip appears as the same single filename in `published-unpacked`.
- There are no legacy-only compatibility directories or duplicate alternate layouts.

Deep audit mode should add byte-level SHA-256 verification for every referenced artifact. The default end-of-build test should avoid rereading huge packages unless they were produced in the current run or lack a trusted recorded checksum.

## Android Contract

Android should:

- Read `current_artifacts_YYYYMMDD.json`.
- Fetch `bundle_cycle_YYCC_VV_<sha256>.json` from the `relative_path` in `current_artifacts`.
- Optionally fetch fast bundle metadata on the fast-product polling cadence.
- Plan app installs only from `packages[]`.
- Ignore `ancillary[]` outside developer/debug tooling.
- Fetch `staticBaseUrl + relative_path`.
- Verify size and checksum.
- Atomically install packages.

Android should not:

- Infer filenames.
- Consult HAD/nav DB or the bundle manifest for package resolution, depending on whether the question is semantic lookup or package download planning.
- Ask a dynamic dev server to map package IDs to files.
