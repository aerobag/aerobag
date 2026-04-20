# Fast Product Handoff

## Snapshot products

The current packaged artifact manifest includes three fast products:

- `nexrad`: animated radar frames.
- `metars`: station observations with flight category and lat/lon.
- `tfrs`: TFR boundary polygons and schedule/altitude text.

Web stages these from the current artifact snapshot into `generated-static/fast-products/<product-id>` and serves them at `/fast-products/<product-id>`. Android should use the same current artifact manifest entries and product ids, but it does not need to mirror the web staging layout exactly.

## NEXRAD web prototype

The NEXRAD product is a zip containing `nexrad.json` plus PNG frames (`frame_0.png`, `frame_1.png`, ...). The manifest fields currently used by web are:

- `projection`: must be `EPSG:3857`.
- `frames[].filename`: PNG frame path relative to the NEXRAD product root.
- `frames[].observed_at_utc`: frame timestamp.
- `frames[].bounds`: Web Mercator meter bounds with `west`, `south`, `east`, `north`.

Because the radar bounds are already in Web Mercator, web renders each frame as an image overlay on the chart surface. The conversion is:

```ts
const halfWorldM = 20037508.342789244;
const worldSize = 256;

function mercatorMetersToWorld(xMeters: number, yMeters: number) {
  const spanM = halfWorldM * 2;
  return {
    x: ((xMeters + halfWorldM) / spanM) * worldSize,
    y: ((halfWorldM - yMeters) / spanM) * worldSize,
  };
}
```

Convert northwest (`west`, `north`) and southeast (`east`, `south`) to the map's normalized world coordinates, then use the existing map viewport projection to produce screen `left/top/width/height`.

The web prototype reverses the manifest frame order and loops oldest-to-newest. It has no controls yet; it is just validating that the product can be slapped over the existing Web Mercator chart tiles.

## Android parity notes

Android can use the same model: load `nexrad.json`, decode the PNG frames, convert EPSG:3857 meter bounds into the map's Web Mercator world coordinates, and draw the current frame as a bitmap overlay under route/ownship symbology. Keep the radar layer pointer-transparent and treat absence or unsupported projection as "no radar layer", not as a fatal map error.

METAR and TFR are not wired in web yet. METAR records have `station_id`, `raw_text`, `observation_time_utc`, `flight_category`, `longitude`, and `latitude`. TFR records are grouped as `areas`, with NOTAM metadata, schedule fragments, altitude limits, `avare_text`, and lat/lon polygon points.

## Terrain draft

Terrain is a static product, not a fast product, but it follows the same handoff principle: core owns product semantics and UI/platform owns byte transport plus final painting.

The draft web flow is:

- Core session call `get_terrain_overlay_in_session(viewport, width, height)` decides whether terrain is drawable from current ownship state and returns static product file requests plus screen placement.
- Web fetches `/terrain-products/<product-id>/<path>` for each request and does not parse the payload.
- Core session call `render_terrain_overlay_tile_in_session(bytes)` parses ABT1, applies terrain warning policy and nodata treatment, and returns a PNG.
- Web paints the returned PNG at core-provided `left/top/size`.

For Android parity, implement the same adapter boundary: satisfy core's static product file requests from the Android product/cache layer, pass bytes back to core, and draw the returned PNG/bitmap. Avoid duplicating ABT1 parsing, clearance thresholds, or nodata styling in Android UI code.

## Appendix: airspace overlay parity

Web now treats airspace as a core-owned vector overlay model. The UI fetches missing vector resources from `/vectors`, ingests them into the core session, and paints the returned screen-space display model:

- Reference tiles: `/vectors/airspace/refs/{z}/{x}/{y}.json`.
- Label tiles: `/vectors/airspace/labels/8/{x}/{y}.json`.
- Full HAD features: `/vectors/had/...`, using the path returned by core in `needed_airspace_features`.

Core owns the reusable pieces: visible reference tile selection, feature dedupe, HAD feature cache, lon/lat to viewport pixel projection, simple projected-point simplification, label projection, draw-order by returned array order, and style tokens. Web only converts returned point arrays to SVG paths and applies core-provided stroke/fill/dash values.

Android should mirror that seam rather than parse SVG. Add the new wire fields from `MapOverlayQueryResult`: `needed_airspace_ref_tiles`, `needed_airspace_features`, `needed_airspace_label_tiles`, `airspace_paths`, and `airspace_labels`. Add native bridge calls equivalent to web's `ingestAirspaceRefTiles`, `ingestAirspaceFeatures`, and `ingestAirspaceLabelTiles`. Fetch the requested files from Android's vector product/cache layer, ingest them, re-query core, then draw `airspace_paths` into Compose `Path` objects using the returned screen-pixel coordinates and style tokens. Draw `airspace_labels` with native text at `screen_x/screen_y`.

The coordinate system is the same one Android already uses for point overlays: viewport-local pixels, origin at the top left, x increasing right, y increasing down. Android should continue applying its density conversion at the final Canvas boundary, not in core.
