# Core-Owned Map Selection And Raster Tile Plans

## Problem

Map selection and raster tile planning are currently split across core, Web, and
Android.

- Core can derive `map_selector_state` from HAD, but the UI session does not own
  the selected map.
- Web stores `selectedMapId`, filters displayed map views, chooses raster source
  tiles, constructs URLs, and assigns z-order.
- Android stores selected map state, duplicates the same raster source tile
  policy, and additionally carries candidate package fallback logic that Web
  lacks.

This makes policy diverge across platforms. The current blurry-raster bug is one
example: Web selected an Alaska low-zoom representative for a Sacramento
viewport because low-zoom regional fallback policy depended on platform list
ordering.

## Target Boundary

Platform UI sends events and viewport facts to core. Core returns draw
instructions.

Platform UI may:

- Ask to select a chart family or concrete map id.
- Report the viewport and surface size.
- Fetch or read bytes for a core-specified package/path or URL.
- Paint returned draw records in returned z-order.

Platform UI must not:

- Decide which chart regions participate in a viewport.
- Decide source zoom fallback ladders.
- Collapse full-coverage regional packages.
- Decide TAC-over-SEC or other family stacking policy.
- Probe alternate chart packages for shared fallback tiles.

## Core Session State

Add session-owned map state:

- `selected_map_id`
- cached raster catalog records derived from HAD
- cached display geometry/polygon sets needed for coverage clipping

Core should initialize this state from the selected map id passed by the
platform, falling back to the existing TAC/NW preference. Subsequent platform
selection events mutate the session state.

## Core APIs

Add session APIs:

- `select_map_in_session(handle, selected_map_id) -> UiSessionSnapshot`
- `get_map_selector_state_in_session(handle) -> MapSelectorState`
- `get_raster_tile_plan_in_session(handle, viewport, width_px, height_px) -> RasterTilePlan`

`RasterTilePlan` returns platform-neutral draw records:

- `draw_key`
- `family`
- `source_zoom`
- `x`
- `y_tms`
- `left_px`
- `top_px`
- `size_px`
- `z_order`
- `primary` source
- `fallbacks` sources

Each source includes:

- `map_view_id`
- `package_name`
- `storage_kind`
- `relative_path`
- `url`

Web uses `url`. Android uses `package_name + relative_path` against installed
zips.

## Tile Policy To Move Into Core

Core owns:

- Visible tile enumeration from Web Mercator viewport and surface size.
- Source zoom selection from available levels.
- Full fallback ladder for chart packages.
- No fallback ladder for static products.
- TAC selected family renders both SEC and TAC.
- Deterministic z-order: higher source zoom above lower source zoom, TAC above
  SEC at the same source zoom, overlays above rasters.
- Full-coverage regional collapse without depending on list order.
- Android-style candidate source fallback for shared full-coverage tiles.

The representative for shared full-coverage levels must be viewport/tile aware,
not sorted-list aware. If multiple packages can serve the same fallback tile,
core returns all candidates in preferred order.

## Migration Steps

1. Move the map catalog record types and selector derivation out of HAD-only
   helpers into reusable core modules.
2. Add raster tile planning in core with regression tests for the Sacramento
   z7.32 failure and multi-region z10 boundary rendering.
3. Add WASM and FFI exports for map selection and raster tile plans.
4. Replace Web raster selection with core tile plans; Web only paints returned
   records.
5. Replace Android raster selection with core tile plans; Android only loads
   returned candidate sources and paints returned records.
6. Remove duplicated platform filter/render helpers after both platforms are
   migrated.

## Performance

This is feasible for pan/zoom animation.

The per-frame work is small: enumerate visible tile coordinates over tens to low
hundreds of draw records. The expensive work is parsing catalog and geometry
from HAD; that must be cached in the session and invalidated only when product
metadata changes.

JSON is acceptable initially for the raster plan crossing because the payload is
small compared with image fetch/decode/paint. If profiling later shows crossing
cost, the same plan can move to packed bytes without changing ownership.
