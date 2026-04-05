# Rust Preprocessor Design

Status: draft for first implementation pass

Related notes:
- [notes](/home/jonh/aerobag/notes)
- [AVARE_NOTES.md](/home/jonh/aerobag/avare-source/AVARE_NOTES.md)

## Purpose

Replace Avare's current preprocessing scripts with a maintainable Rust system that:
- reproduces the legacy output layout and package contract first,
- has deterministic fetching and local source caching,
- exposes parallelism cleanly for multi-core execution,
- makes regressions visible with automated tests,
- gives us a clean base for later format changes.

The first target is compatibility, not innovation.

## Corrected Model Of The Existing System

The current pipeline is not one thing. It is several artifact-specific pipelines that feed a shared client contract.

### Tiled raster artifacts

These are rendered into local Web Mercator tile pyramids and later zipped by Avare region.

Current evidence says the tiled families include:
- Sectionals
- TAC
- IFR Low
- IFR High
- IFR Area
- Heli / specialty charts
- Flyway charts
- WAC
- ONC
- TPC
- Terrain
- Shaded relief
- Canada topo

Important correction:
- The tile pyramid is Web Mercator, but Avare uses `512x512` tiles, not `256x256`.
- The app reads tiles from `tiles/<chartIndex>/<z>/<x>/<y>`.
- The current split-out `charts/` repo covers only part of the full legacy family set. WAC, ONC, TPC, terrain, topo, VFR area, and some other static products still show up only in legacy scripts and app resources.

### Standalone image artifacts

These are distributed as PNG files rather than tiled pyramids.

Families:
- FAA d-TPP plates
- Airport diagrams
- Chart Supplement page images
- Canada airport diagrams
- VFR area images by state
- Some minimums products

Important correction:
- Chart Supplement pages are image-based but not georeferenced in the current model.
- Plates and airport diagrams sometimes carry georeference metadata, but Avare does not resample them into a standard projected raster. The client keeps the original-ish image and maps aircraft position into image coordinates.

### Georeference sources

Plate-related georeference comes from two places:
- Embedded geospatial metadata already present in some FAA PDFs.
- External airport diagram tagging data from `https://www.outerworldapps.com/WairToNowWork/avare_aptdiags.php`.

That external feed is not from Apps4Av. It appears to be an Outer World Apps data source that Avare consumes.

## Compatibility Target

The first Rust implementation should intentionally preserve:
- package names,
- folder layout inside ZIP files,
- manifest file names and first-line cycle semantics,
- tile size,
- chart index numbering,
- EXIF `UserComment` metadata conventions where the app expects them,
- region split naming (`AK`, `PAC`, `NW`, `SW`, `NC`, `EC`, `SC`, `NE`, `SE`),
- output file naming for plates, CSUP, and tiles.

This lets us compare old and new output directly.

The compatibility target is:
- same package names,
- same package member paths,
- same tile path set,
- visually equivalent sampled imagery,
- no new coverage gaps.

## Non-Goals For V1

Do not do these in the first implementation:
- rewrite GDAL, Ghostscript, or ImageMagick behavior in pure Rust,
- change the client package contract,
- switch to vector maps,
- remove the regional packaging model,
- redesign the plate georeference format.

Those may become later phases once we have parity.

## High-Level Architecture

Use a Rust workspace with one binary crate and several library crates.

### Proposed workspace layout

- `preprocessor-cli`
  - top-level commands
  - config loading
  - progress reporting
- `preprocessor-core`
  - shared types
  - run IDs
  - source descriptors
  - artifact descriptors
  - manifest generation
  - error model
- `preprocessor-fetch`
  - HTTP client
  - local content-addressed cache
  - freshness policy
  - cycle-aware source resolution
  - offline mode
- `preprocessor-tools`
  - wrappers around GDAL, Ghostscript, ImageMagick, SQLite, Perl helpers where still needed
  - version checks
  - structured invocation logs
- `preprocessor-charts`
  - tiled raster families
  - chart mosaics
  - cutline handling
  - tiling
  - region splitting
- `preprocessor-plates`
  - d-TPP processing
  - airport diagram tags
  - EXIF metadata writing
  - minimums extraction
- `preprocessor-csup`
  - Chart Supplement processing
  - airport page extraction
- `preprocessor-db`
  - NASR / CIFP / DOF / AIXM ingestion
  - SQLite build
  - optional airspace MBTiles generation
- `preprocessor-legacy-test`
  - golden manifest format
  - tile-path comparisons
  - sampled image diff tooling

### Operating model

Rust should orchestrate the pipeline, not replace every mature external imaging tool immediately.

V1 pipeline strategy:
- use Rust for control flow, caching, concurrency, logging, manifests, and tests,
- use pinned external tools for raster and PDF transformations,
- make every external tool invocation explicit and logged.

This gives us clean code quickly without taking on unnecessary image-processing risk.

## Source Fetching And Cache Design

This is a core feature, not just a convenience.

### Requirements

- Never re-download upstream data unnecessarily during development or tests.
- Keep a durable local cache keyed by content, not just by URL.
- Record enough metadata to reproduce a run.
- Support offline re-runs using cached sources only.
- Support force-refresh when upstream content is known to have changed.

### Cache model

Use a content-addressed cache with a metadata index.

Suggested structure:
- `cache/blobs/<sha256>`
- `cache/objects/<logical-name>.json`
- `cache/http/<url-hash>.json`
- `runs/<timestamp-or-uuid>/`

Each fetched object should record:
- logical source name,
- original URL,
- fetch timestamp,
- HTTP headers,
- content hash,
- file size,
- cycle/date inference,
- local blob path,
- extraction outputs if archive.

### Fetch policy

Support these modes:
- `online`
  - use cache when valid, fetch missing content
- `refresh`
  - revalidate and refresh content
- `offline`
  - fail if required source is not already cached

### Archive extraction

Treat extraction as a cached derivation:
- archive blob hash in,
- extracted directory tree hash out,
- extraction manifest recorded.

That lets tests reuse decompressed inputs too.

## Build Graph And Parallelism

The work naturally forms a DAG.

Parallelize at these levels:
- source downloads,
- per-chart warp/crop jobs,
- per-family VRT preparation,
- per-family tile generation,
- region ZIP assembly,
- per-state or per-airport standalone image generation,
- database parser stages where inputs are independent.

### Scheduler

Use a bounded work scheduler:
- async network fetches,
- bounded CPU job pool for imaging steps,
- bounded I/O queue for ZIP assembly and file copies.

Do not simply spawn one process per artifact with no limit.

### Initial concurrency assumptions

On the 20-core box:
- imaging jobs should use most of the CPU budget,
- a few jobs will be RAM-heavy,
- storage throughput may become the bottleneck before CPU does.

So concurrency should be configurable:
- `--fetch-jobs`
- `--cpu-jobs`
- `--zip-jobs`

### GPU

Do not design around GPU in V1.

This workload is likely dominated by:
- GDAL warps,
- resampling,
- PDF rasterization,
- compression,
- ZIP I/O.

GPU support should wait for profiling evidence that it helps enough to justify complexity.

## Artifact Families

### Family A: tiled chart packages

Examples:
- `NE_SEC`
- `NC_TAC`
- `AK_ENR_H`
- `PAC_HEL`
- `NE_FLY`

General flow:
1. resolve cycle and source URLs,
2. fetch and cache raw chart payloads,
3. normalize names,
4. project/crop charts using cutlines or legacy chart metadata,
5. build family VRT,
6. generate `512x512` WebP tiles,
7. partition tiles into Avare regions,
8. write ZIP and manifest.

### Family B: plate packages

Examples:
- `NE_TPP`
- `AK_TPP`

General flow:
1. fetch d-TPP archive,
2. fetch airport diagram tag feed,
3. parse FAA metadata XML,
4. rasterize relevant PDFs,
5. derive or inject georeference metadata,
6. emit PNGs in legacy naming format,
7. zip by region.

### Family C: Chart Supplement packages

Examples:
- `NE_CSUP`
- `PAC_CSUP`

General flow:
1. fetch DCS ZIP,
2. parse XML,
3. rasterize pages to trimmed PNGs,
4. emit `afd/<APTID>/CSUP-<REGION>_<page>.png`,
5. zip by region.

### Family D: database package

Example:
- `databases`

General flow:
1. fetch NASR, CIFP, DOF, AIXM and related inputs,
2. parse and normalize records,
3. build `main.db`,
4. write ZIP and manifest,
5. later decide whether `databasesx` compatibility matters to the current client.

### Family E: legacy static families still needing coverage

Examples:
- WAC
- ONC
- TPC
- Canada topo
- terrain / relief
- VFR area images
- Canada airport diagrams

These must not be forgotten just because the newer split repos do not cover them all.

## CLI Shape

Suggested command style:

```text
preprocessor fetch charts --family SEC --cycle 2605
preprocessor build charts --family SEC --cycle 2605
preprocessor build tpp --region NE --cycle 2605
preprocessor build csup --region NE --cycle 2605
preprocessor build db --cycle 2605
preprocessor package all --cycle 2605
preprocessor legacy-capture charts --family SEC --cycle 2605
preprocessor compare tile-paths legacy.json new.json
preprocessor compare sampled-images legacy.json new.json
```

Requirements:
- every command should be restartable,
- every command should have a dry-run mode,
- outputs should be deterministic given identical cached inputs,
- all tool invocations should be logged.

## Run Manifest Design

Every build should emit a machine-readable run manifest.

Suggested contents:
- run ID,
- git revision of the Rust code,
- config file hash,
- source object hashes,
- tool versions,
- output packages,
- output member paths,
- tile counts per package,
- warnings and fallback decisions.

This is useful for reproducibility and for regression tests.

## Test Strategy

The test strategy must focus on coverage and contract stability, not on exact byte identity.

### 1. Contract tests

Verify:
- package names,
- ZIP member paths,
- manifest file names,
- manifest first line,
- expected chart index / zoom layout,
- EXIF/UserComment presence and parseability for georeferenced outputs.

### 2. Tile-path equality tests

Run the legacy preprocessor once and record every output tile path for a chosen cycle and family.

The new pipeline should produce:
- the same set of tile paths,
- no missing paths,
- no unexpected extra paths.

This is the best direct guard against newly introduced gaps.

### 3. Stratified image comparison

Do not compare every tile initially.

Sample tiles from:
- chart edges,
- chart overlaps,
- high-zoom dense areas,
- low-zoom shared tiles,
- random tiles per region,
- historically tricky charts.

Use both:
- exact hash where feasible,
- perceptual diff thresholds where toolchain differences make exact bytes unstable.

### 4. Standalone image regression tests

Verify for plates and CSUP:
- expected output files exist,
- expected page counts are within tolerance,
- geotag metadata exists where expected,
- extracted minimums pages match legacy selection.

### 5. Small synthetic tests

Use tiny local fixtures for:
- manifest generation,
- region splitting,
- cache behavior,
- failed fetch recovery,
- retry and resume semantics,
- tile-path derivation.

These should run in normal CI.

### 6. Heavy parity runs

The full golden comparison should run on the 20-core box or its container, not in lightweight CI.

## Legacy Capture Plan

Before writing much Rust:
1. build a pinned container image for the legacy tools,
2. run selected legacy families,
3. collect:
   - output ZIP member lists,
   - tile path lists,
   - source URL lists,
   - tool stdout/stderr,
   - file hashes for selected outputs.

Store this as a reusable golden dataset.

Suggested first golden targets:
- one sectional family,
- one TAC family,
- one IFR low family,
- one TPP region,
- one CSUP region.

Then expand.

## Migration Phases

### Phase 0: discovery and golden capture

- inventory legacy families and contracts,
- pin legacy execution environment,
- capture golden outputs.

### Phase 1: Rust skeleton and fetch/cache

- create workspace,
- implement cache,
- implement config,
- implement tool wrapper layer,
- implement run manifest.

### Phase 2: tiled charts compatibility

- implement the modern split chart families first,
- confirm tile-path equality,
- sample visual equivalence.

### Phase 3: plates and CSUP compatibility

- implement TPP and CSUP,
- preserve metadata conventions,
- compare standalone outputs.

### Phase 4: database compatibility

- implement `databases`,
- verify app expectations and schema compatibility,
- decide whether `databasesx` matters.

### Phase 5: legacy static families

- port WAC / ONC / TPC / terrain / topo / VFR area / CAN_ADS,
- remove dependency on the old `extra/` scripts entirely.

### Phase 6: cleanup and future changes

- replace remaining Perl or shell helpers,
- rationalize metadata,
- consider format changes only after parity is proven.

## Operational Plan For The 20-Core Box

Use a pinned container for both legacy and new pipelines.

Container benefits:
- reproducible system tool versions,
- easier parity debugging,
- simpler handoff between machines,
- easier cache mounting.

Recommended mounted volumes:
- source cache,
- extracted cache,
- output directory,
- golden manifests,
- logs.

Keep caches outside the container image.

## Open Questions

- Which of the static legacy families are still actively published and still worth full compatibility?
- Is `databasesx` currently dead code or just underused?
- How stable is the Outer World Apps airport diagram feed, and should we mirror/cache it explicitly?
- For terrain and topo, is there any hidden preprocessing outside the checked-in legacy scripts?
- Do we want to preserve the exact WebP quality settings and resampling choices, or only visual equivalence?

## Initial Implementation Recommendation

The first Rust deliverable should be:
- a fetch/cache subsystem,
- a pinned tool runner,
- a legacy capture command,
- one fully working tiled family, probably Sectionals,
- tile-path and sampled-image comparison against legacy output.

That is the fastest route to proving the architecture is sound.
