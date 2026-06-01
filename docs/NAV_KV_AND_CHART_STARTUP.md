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
exact data core/UI need to offer and render raster charts, including static
visual raster products such as shaded relief:

- chart family ids and display labels
- region ids and display labels
- chart package ids
- tile URL/package roots or package references
- chart index
- tile path templates
- tile size
- zoom levels and bounds
- default viewport
- min/max zoom

The UI should not derive this from `resource_index`.

Plate-page data must not be one large `{ "airports": [...] }` value. It is
looked up by airport, by plate id, and by CIFP procedure id, so those query
shapes are separate HAD keyspaces. See `docs/HAD_QUERY_KEYSPACES.md` for the
current consumer-derived key inventory.

Future `nav_kv` keys should cover map-shaped and spatial lookup needs:

```text
waypoint/ident/KRDD
waypoint/prefix/KR
plate/airport/KRDD
plate/by-id/plate%3AKRDD%3AIAP-CA-ILS%20OR%20LOC%20RWY%2034.png
plate/cifp/KRDD/I34
procedure/list/KRDD/APPROACH
procedure/distinct-rows/KRDD/I34
procedure/materialization-rows/KRDD/I34
airway/V23
```

The values may contain internal structure. For example, a waypoint suggestion
value can be an array of candidate summaries with identifiers, names, types, and
coordinates. Core can sort or filter those candidates according to current flight
plan state.

### Wire Format

`nav_kv` v1 uses one root file plus fixed-size value pages:

```text
nav_kv_YYCC.root
nav_kv_YYCC.values_0000
nav_kv_YYCC.values_0001
...
```

The root file contains everything needed for exact lookup and prefix range
lookup. Value pages contain the concatenated value byte stream.

Root file layout:

```text
header
entries[real_entry_count + 1]
key_bytes
```

All integer fields are little-endian.

Header:

```text
magic: 16 bytes
version: u32
real_entry_count: u32
page_size: u32
entry_table_offset: u32
key_bytes_offset: u32
key_bytes_len: u32
value_bytes_len: u32
reserved: u32
```

Entry:

```text
key_offset: u32
value_offset: u32
```

`entries[real_entry_count]` is a sentinel, not a real key:

```text
sentinel.key_offset == key_bytes_len
sentinel.value_offset == value_bytes_len
```

For real entry `i`:

```text
key_start = entries[i].key_offset
key_end = entries[i + 1].key_offset
value_start = entries[i].value_offset
value_end = entries[i + 1].value_offset
```

Keys are byte strings in `key_bytes[key_start..key_end]` and are strictly sorted
lexicographically. Values are byte strings in the logical concatenated value
stream `values[value_start..value_end]`.

Value pages are fixed-size chunks of that logical value stream:

```text
start_page = value_start / page_size
end_page = (value_end - 1) / page_size
```

Values may cross page boundaries. The reader fetches and caches each required
page, then reassembles the requested value bytes. Zero-length values are rejected
in v1 so `value_end - 1` cannot underflow.

This intentionally keeps the first version boring:

- Fetch one compact root file.
- Parse fixed-width header and entry table.
- Binary-search keys by slicing `key_bytes`.
- Fetch value pages on demand.
- Decode and parse one value at a time.

The entry table is 8 bytes per real key plus one sentinel. Both offsets are
`u32`; if a future artifact needs more than 4 GiB of key or value bytes, we will
roll the format version instead of paying the v1 startup-size penalty.

Required reader/builder tests:

- reject malformed magic/version
- reject duplicate keys
- reject unsorted keys unless the builder explicitly sorts its input
- reject zero-length values
- reject key offsets outside `key_bytes`
- reject value offsets beyond `value_bytes_len`
- require the sentinel key offset to equal `key_bytes_len`
- require the sentinel value offset to equal `value_bytes_len`
- ensure lookup never returns the sentinel
- compute the final real key length from the sentinel
- compute the final real value length from the sentinel
- exact lookup returns the correct value
- missing lookup returns none
- prefix lookup returns only the sorted matching real keys
- value extraction works when a value crosses one or more page boundaries
- repeated extraction reuses cached pages

If root-file size becomes material, the next version can add sharding or a
denser key index without changing the `key -> value bytes` abstraction.

## Preprocessor Organization

`nav_kv` is a collection of key/value tables contributed by multiple preprocessor
phases. The final publication artifact should be assembled by one dedicated node,
not hand-written independently by each producer.

Each contributing phase should emit an intermediate KV contribution artifact:

```text
chart-products.nav_kv_contrib
tpp.nav_kv_contrib
csup.nav_kv_contrib
data.nav_kv_contrib
```

The final `nav_kv` assembly node gathers all contribution artifacts, validates
that keys are unique, sorts all keys into final lexicographic order, orders the
value byte stream in that same key order, and writes the root file plus value
pages.

Ordering values by sorted key gives locality: adjacent keys usually touch nearby
value pages. For example, a burst of `waypoint/id/...` or
`procedure/airport/KRDD/...` lookups should tend to reuse cached pages.

### Intermediate Contribution Format

The intermediate format can be boring and build-time-friendly. It does not need
to be optimized for runtime startup.

Avoid a giant JSON object whose values are themselves escaped JSON strings. That
creates unnecessary escaping, hard-to-read diffs, and avoidable producer bugs.

A better first contribution format is newline-delimited JSON records:

```json
{"key":"chart/catalog","value":{"families":[],"regions":[],"charts":[]}}
{"key":"waypoint/id/KRDD","value":{"kind":"airport","id":"KRDD"}}
```

The assembler parses each line, validates the `key`, and serializes `value` once
to canonical compact JSON bytes for the runtime value stream. This keeps producer
output inspectable without introducing JSON-string escaping hell.

If contribution files become too large or slow, individual producers can switch
to a binary contribution format later. The final runtime `nav_kv` wire format
does not need to change.

Contribution rules:

- keys are UTF-8 and must not be empty
- keys use `/`-separated namespaces
- producers may emit records in any order
- the assembler owns final sorting
- duplicate keys across all contributions are fatal
- values must be valid JSON in v1
- values are serialized by the assembler into canonical compact JSON bytes
- zero-length serialized values are rejected

This keeps each preprocessor phase focused on the records it knows how to
produce, while the final assembler owns the runtime layout and performance
properties.

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
    "root": {
      "filename": "nav_kv_2604.root",
      "relative_path": "nav_kv_2604.root",
      "checksum_sha256": "...",
      "size_bytes": 1234
    },
    "value_pages": [
      {
        "filename": "nav_kv_2604.values_0000",
        "relative_path": "nav_kv_2604.values_0000",
        "checksum_sha256": "...",
        "size_bytes": 65536
      }
    ],
    "page_size": 65536,
    "value_bytes_len": 65536
  }
}
```

`resource_index_YYCC.json` may remain published for diagnostics and broad
inspection, but the web app should not parse it on startup.

The unpacked publication view should expose the same root and value-page files as
the packaged view unless and until we intentionally define a directory-based
variant.

## First Implementation Slice

1. Add the redesign doc.
2. Add a preproc writer for `nav_kv_YYCC.root` and value pages.
3. Populate the first key, `chart/catalog`, from existing published metadata.
4. Add `nav_kv` to `bundle_YYCC.json` and contract validation.
5. Add a small web/core loader for `nav_kv`.
6. Replace web startup raster chart derivation from `resource_index` with
   `nav_kv.get_json("chart/catalog")`.
7. Add the plate/procedure keyspaces in `docs/HAD_QUERY_KEYSPACES.md` and change
   the session boundary so core can ingest only the airport/plate records it
   needs.
8. Measure:
   - fetch time
   - index parse time
   - `chart/catalog` value parse time
   - splash-to-first-chart time

The one-blob `chart/page/catalog` experiment proved the wire format but kept the
wrong query shape; sharding by actual lookup key is now required.

## Measured Startup Notes

### 2026-06-01 NAV5 Unpacked Page Gzip

`NAV5` keeps the nav-db root raw and gzip-compresses unpacked `page_*` files at
publication time. The browser/platform layer explicitly decompresses those
pages before handing bytes to core. This keeps the browser cache-warming path
static-file-only while cutting WAN transfer for page faults and startup
prefetches.

Snapshot validation from `/root/aerobag-artifacts-snapshot/current_artifacts.json`
at `2026-06-01T04:25:12Z`:

```text
cycle  nav-db contract  root encoding  page encoding  page_0046 bytes
2605   NAV5             raw            gzip           7,824
2606   NAV5             raw            gzip           7,824
```

WASM size baseline after the NAV5 gzip work, before trying any in-core
decompressor bake-off:

```text
artifact             bytes
app_wasm_bg.wasm     3,217,955
app_wasm_bg.wasm.gz  1,099,848
app_wasm.js             87,136
```

Cold-cache network matrix against `http://127.0.0.1:8085/` with optimized WASM
under Vite dev. These full startup numbers are not production-preview numbers:
Vite dev module loading dominates the throttled `start` phase. The
`cache_warm_p50_ms` column is still the useful comparison for the NAV5 change,
because it measures the root plus startup-prefetch page fetches.

```text
profile     throttle              cache_warm_p50_ms  no_metar_p50_ms  full_p50_ms
none        local/no throttle                      41              274          349
wifi-okay   40ms / 12000 kbps                     307            2,653        4,276
wifi-bad    100ms / 3000 kbps                     976            9,609       15,702
cell-4g     150ms / 1500 kbps                   1,785           18,788       30,663
cell-bad    300ms / 600 kbps                    4,260           46,322       47,114
```

Earlier cold-cache production-lab cache-warm medians before this page-gzip
change were roughly:

```text
profile     old_cache_warm_p50_ms  NAV5_cache_warm_p50_ms
none                            24                       41
wifi-okay                      524                      307
wifi-bad                     1,963                      976
cell-4g                      3,800                    1,785
cell-bad                     9,559                    4,260
```

The measured effect is therefore about a 2x improvement in the nav-db startup
prefetch segment on bandwidth-constrained profiles. The no-METAR/full Vite-dev
startup rows should not be used to judge production user-visible startup until
the production startup lab can run again without the unrelated TypeScript
blocker.
