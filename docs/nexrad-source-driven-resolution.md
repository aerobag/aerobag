# NEXRAD Source-Driven Resolution

NEXRAD live feeds should be tiled in the source raster grid, not in Web Mercator.
The high-resolution product is a product-native coordinate system whose tile math
is based on source pixels. Core owns the manifest parsing, source-grid transforms,
tile-interest planning, and render georeferencing. Platform layers only fetch
opaque resources and draw the core-provided image geometry.

## State Layout

Each upstream NEXRAD frame publishes one immutable state:

```text
/live-feeds/v3/states/nexrad/<state-id>/manifest.json
/live-feeds/v3/states/nexrad/<state-id>/tiles/res0/<tile-x>/<tile-y>.png
/live-feeds/v3/states/nexrad/<state-id>/tiles/res1/<tile-x>/<tile-y>.png
/live-feeds/v3/states/nexrad/<state-id>/tiles/res2/<tile-x>/<tile-y>.png
/live-feeds/v3/states/nexrad/<state-id>/tiles/res3/<tile-x>/<tile-y>.png
/live-feeds/v3/versions/nexrad/<state-id>.json
```

The full SSE catalog and later product events name the current NEXRAD state and
point at that state's manifest. State paths are content/version addressed and
immutable.

## Resolution Levels

Resolution levels are source-grid relative:

```text
resN means output pixels sample the source grid at stride 2^N in each axis.

res0 = every original source pixel
res1 = 2x2 source pixels per output pixel
res2 = 4x4 source pixels per output pixel
res3 = 8x8 source pixels per output pixel
res4 = 16x16 source pixels per output pixel
```

We can publish only selected levels. The initial live-feed product publishes:

```json
"res-levels": [0, 1, 2, 3]
```

The Avare-style nominal 16:1 pixel-count reduction is `res2`.

## Tile Encoding

NEXRAD source-grid tiles are normal browser-renderable PNG files using
`png8-fixed-palette` encoding. The generator maps source RGBA colors to the
checked-in fixed palette from `docs/nexrad/analysis/whole-day-greedy-255-palette.json`,
with index 0 reserved for transparency. Each tile then remaps those fixed
indices into the shortest PNG-local palette that represents the colors actually
used in that tile. This keeps color choice stable across frames while avoiding a
full 256-entry `PLTE` chunk on empty or low-color tiles.

Future delta transport is deferred to TASK-121. That work should treat the
palette-index stream as the delta source and will require an explicit client
decoder/reconstruction path; it should not be hidden inside PNG.

## Tile Math

For a source image with width `W`, height `H`, tile size `T`, and resolution
level `resN`:

```text
stride = 1 << N

res_width  = ceil(W / stride)
res_height = ceil(H / stride)

tile_x = floor(res_pixel_x / T)
tile_y = floor(res_pixel_y / T)

source_pixel_x = res_pixel_x * stride
source_pixel_y = res_pixel_y * stride
```

Tile bounds in resolution-level pixels:

```text
x0 = tile_x * T
x1 = min((tile_x + 1) * T, res_width)
y0 = tile_y * T
y1 = min((tile_y + 1) * T, res_height)
```

To georeference a tile, core maps resolution-level tile corners back to source
pixel coordinates by multiplying by `stride`, then applies the source grid's
affine transform and projection from the manifest.

## Manifest Contract

The state manifest carries source-grid metadata and the available resolution
levels:

```json
{
  "schema_version": 1,
  "product": "nexrad",
  "state_id": "...",
  "observed_at_utc": "...",
  "source_file": "CONUS_L2_CREF_QCD_....tif.gz",
  "source_sha256": "...",
  "tile_encoding": "png8-fixed-palette",
  "palette": {
    "transparent_index": 0,
    "opaque_indices": [1, 255],
    "sha256": "..."
  },
  "tile_size": 512,
  "res-levels": [0, 1, 2, 3],
  "source_grid": {
    "width": 7000,
    "height": 3500,
    "projection_wkt": "...",
    "geo_transform": [origin_x, pixel_width, rot_x, origin_y, rot_y, pixel_height]
  },
  "tile_path_template": "tiles/res{res}/{x}/{y}.png",
  "levels": [
    {
      "res": 0,
      "width": 7000,
      "height": 3500,
      "tile_cols": 14,
      "tile_rows": 7
    }
  ]
}
```

The relationship between `resN` and source resolution is implied by the name; the
manifest does not need per-level scale metadata.

## Core Responsibilities

Core owns:

- NEXRAD state manifest parsing.
- Source pixel <-> source projection <-> lat/lon transforms.
- Selecting resolution levels based on display scale and operational interest.
- Planning tile requests around ownship, flight-plan corridors, and viewport.
- Emitting render-ready tile geometry.
- Invariant tests that known source pixels and known lat/lon points map to the
  expected places.

Platform renderers receive image resources plus core-provided geometry. They do
not reinterpret the NEXRAD projection.

## Fetch Policies

Web uses a just-in-time resource policy:

- Fetch a coarse CONUS overview from a reduced level such as `res3`.
- Fetch full source quality (`res0`) around ownship.
- Fetch full or intermediate quality around the flight-plan corridor.
- Keep old tiles visible while fetching new state tiles.

This gives local full-resolution quality where it matters without repeatedly
delivering the whole CONUS source frame.

Android intentionally does not use this policy. Connectivity may be brief in
flight, so Android eagerly downloads complete immutable packages for the recent
frame tail and renders package members after connectivity disappears. Future
deltas may reduce the bytes needed to assemble each complete state, but do not
change the complete-state cache contract.

Core owns both acquisition policies and feeds their results into one frame
catalog, animation selector, and geometry path. Platform code provides generic
HTTP, persistence, and package-member effects only. See
`docs/refactor/nexrad-acquisition-and-animation.md`.

## Large Fixture Tests

The source tree does not carry raw NEXRAD captures. Large real-world fixtures
live in the sibling `aerobag-test-artifacts` repository. To enable the
three-hour source-grid trace tests:

```sh
AEROBAG_TEST_ARTIFACTS=/root/aerobag-five/aerobag-test-artifacts \
  cargo test -p preprocessor-cli nexrad_three_hour_fixture -- --nocapture
```

Without `AEROBAG_TEST_ARTIFACTS`, these tests skip themselves so normal test
runs stay small.
