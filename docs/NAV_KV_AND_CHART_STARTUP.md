# Nav KV and Chart Startup Redesign

## Problem

The web app currently reaches a visually interactive shell quickly, but the app is
not actually ready. Raster charts can remain blank for roughly 20 seconds because
the startup path fetches and parses the full public `resource_index_YYCC.json`
before it can derive the raster chart catalog.

That resource index is producer-shaped. It contains raster chart metadata, plates,
CSup records, airport resource joins, package metadata, and other publication
data. The raster chart page needs only a small subset of that information. Parsing
the whole file, then pushing derived work through the JS/WASM boundary, couples
fast chart display to unrelated product data.

This is not a problem to hide with lazy loading. The app should become useful
quickly, not just hide its splash quickly.

## Goals

- Make time-to-first-chart fast and deterministic.
- Avoid startup parsing of large producer-shaped JSON.
- Keep UI logic dumb: UI asks core questions and renders core answers.
- Preserve one shared core behavior path for web and Android.
- Let preproc reshape data into app-native artifacts because the publication
  contract exists to serve the apps.
- Prefer generic runtime primitives over many one-off bespoke data interfaces.
- Allow the app to bulk-load app data without bulk-parsing every record.

## Naming

Use `chart` for raster chart products: sectional, TAC, IFR low, IFR high, and
their tiled chart packages.

Use `plate` for PLT page documents: TPP procedures, airport diagrams, minimums,
and Chart Supplement pages.

Avoid using `map` for raster chart products in new contract names.

## Proposed Runtime Primitives

The app should consume two generic app-native data shapes:

- `nav_kv`: immutable key/value lookup data for non-spatial records.
- `nav_tiles`: spatial tiled data for proximity and visible-area queries.

These are not replacements for the published source data model. They are runtime
indexes built by preproc for the app.

## `nav_kv`

`nav_kv` is an immutable key/value artifact. The app can load the table index
quickly, keep the value bytes unparsed, and parse each value only when core asks
for that key.

Conceptually:

```text
key -> byte range in values blob
```

Initial value encoding should be JSON because it keeps the schema easy to inspect
and lets core continue using typed serde structs. The important performance
property is not that JSON is fast; it is that the app parses only the selected
small value, not the entire nav product.

The first implementation should allow one bulk fetch of the entire `nav_kv`
artifact. That tests the best case: if downloading one app-native blob and parsing
only its compact index is fast enough, we do not need a staged "charts now,
everything else later" startup model.

### Initial Keys

Start with chart startup bundled into `nav_kv`:

```text
chart/catalog
```

`chart/catalog` is the app-ready raster chart catalog. It should contain the
exact data core/UI need to offer and render raster charts:

- chart family ids and display labels
- region ids and display labels
- chart package ids
- tile URL/package roots or package references
- chart index
- tile size
- zoom levels and bounds
- default viewport
- min/max zoom

The UI should not derive this from `resource_index`.

Future `nav_kv` keys should cover map-shaped lookup needs:

```text
waypoint/id/KRDD
waypoint/id/OLM
waypoint/suggest/KR
airport/charts/KRDD
plate/cifp/KRDD/I34
procedure/airport/KRDD/iap
procedure/id/KRDD/I34
airway/id/V23
```

The values may contain internal structure. For example, a waypoint suggestion
value can be an array of candidate summaries with identifiers, names, types, and
coordinates. Core can sort or filter those candidates according to current flight
plan state.

### Candidate File Shape

A simple first artifact can be a single binary file:

```text
nav_kv_YYCC.akv
```

Logical layout:

```text
magic: "AEROBAG_NAV_KV_1\n"
index_length: little-endian u32
index_json: UTF-8 JSON
values_blob: concatenated value bytes
```

Index JSON:

```json
{
  "schema": "aerobag-nav-kv-v1",
  "value_encoding": "json",
  "entries": [
    { "key": "chart/catalog", "offset": 0, "length": 12345 }
  ]
}
```

Offsets are relative to the start of `values_blob`.

This intentionally keeps the first version boring:

- Fetch one blob.
- Parse one compact index.
- Binary-search or map keys in memory.
- Decode and parse one value at a time.

If index size becomes material, the next version can replace index JSON with
sorted length-prefixed keys and fixed-width offsets without changing the core
abstraction.

## `nav_tiles`

Spatial data should be tiled, not placed in `nav_kv`.

Use `nav_tiles` for data whose natural access pattern is "near this position" or
"visible in this chart viewport":

- nearby waypoint candidates
- nearby airway candidates
- vector point features
- obstacles
- weather overlays
- terrain-like gridded data

Web can fetch tile payloads dynamically and cache them. Android can read tile
payloads from packaged zips/assets. Core ingests tile payloads and answers the UI
questions; the UI should not implement spatial policy.

## Readiness Model

The target startup model is:

1. Fetch `startup_bootstrap` and `nav_kv`.
2. Initialize core with the `nav_kv` index and the initial flight plan.
3. Core exposes app-ready UI state, including `chart/catalog`, without waiting
   for sqlite-wasm or full producer JSON.
4. The splash hides when the core says the app is usable.

If `nav_kv` bulk load is fast enough, there is no separate chart-catalog staging
path. The chart catalog is simply one key in the app data store.

If later measurements show the full `nav_kv` blob is too large, the same
abstraction can support sharding without changing UI behavior:

```text
nav_kv_manifest.json
nav_kv_chart.akv
nav_kv_waypoints_*.akv
nav_kv_procedures_*.akv
```

But that should be a measured optimization, not the initial architecture.

## SQLite Role

SQLite remains useful as a source artifact or diagnostic format, and native
SQLite may remain acceptable on Android for some workflows. It should not be the
web hot path for normal flight-plan and chart startup behavior.

The app should not need to instantiate sqlite-wasm just to:

- draw raster charts
- resolve one waypoint id
- offer identifier completion
- list procedures for one airport
- find the primary plate for one CIFP procedure

Those are key/value or tiled lookup problems after preproc has done its job.

## Core Boundary

Core owns the runtime data model. UI code should not know whether a result came
from `nav_kv`, `nav_tiles`, SQLite, or a test fixture.

Desired core-facing operations:

```text
load_nav_kv(bytes)
load_nav_tile(layer, z, x, y, bytes)
get_chart_catalog()
resolve_waypoint(identifier, anchor)
suggest_waypoints(prefix, anchor)
list_procedure_options(airport_id, kind)
load_procedure(option_id)
find_plate_for_procedure(node_id)
```

The UI renders returned state and sends user intents back to core.

## Publication Contract Changes

`bundle_YYCC.json` should gain a `nav_kv` artifact:

```json
{
  "nav_kv": {
    "filename": "nav_kv_2604.akv",
    "relative_path": "nav_kv_2604.akv",
    "checksum_sha256": "...",
    "size_bytes": 1234567
  }
}
```

`resource_index_YYCC.json` may remain published for diagnostics and broad
inspection, but the web app should not parse it on startup.

The unpacked publication view should expose the same `nav_kv_YYCC.akv` bytes as
the packaged view unless and until we intentionally define an unpacked
directory-based variant.

## First Implementation Slice

1. Add the redesign doc.
2. Add a preproc writer for `nav_kv_YYCC.akv`.
3. Populate the first key, `chart/catalog`, from existing chart collection
   metadata.
4. Add `nav_kv` to `bundle_YYCC.json` and contract validation.
5. Add a small web/core loader for `nav_kv`.
6. Replace web startup raster chart derivation from `resource_index` with
   `nav_kv.get_json("chart/catalog")`.
7. Measure:
   - fetch time
   - index parse time
   - `chart/catalog` value parse time
   - splash-to-first-chart time

Only after those measurements should we decide whether to shard `nav_kv` or keep
the one-blob model.
