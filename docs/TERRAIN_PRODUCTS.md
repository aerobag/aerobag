# Terrain Products

Aerobag terrain products are derived from USGS 3DEP DEM source tiles. The same
DEM inputs should produce both:

- `terrain`: numeric terrain height tiles.
- `shaded-relief`: visual hillshade/relief tiles.

## Public Numeric Contract

The numeric terrain product is published as region-scoped ZIPs:

```text
terrain_<region>_<sha256>.zip
  manifest.json
  tiles/<z>/<x>/<y>.terrain
```

Each `.terrain` tile is a 512x512 signed 16-bit little-endian raster with a
small binary header. The current magic is `ABT1`; samples are integer feet and
`-32768` is nodata. The region set matches the existing Aerobag raster product
regions. Tile coordinates follow the existing GDAL/gdal2tiles TMS convention
used by chart tiles, not XYZ north-origin y.

Nodata is a first-class value, not an alias for zero elevation. If a source DEM
cell cannot be fetched and no alternate TNMAccess candidate exists, the builder
omits that source cell, records it in `manifest.json` as `missing_dem_cells`,
and emits `-32768` for uncovered samples. Clients must treat `-32768` as
"unknown terrain", not "sea level" or "safe/no granite here".

The terrain source/max zoom is z10. With 512px tiles this is roughly a 2x
horizontal downsample from USGS 3DEP 1 arc-second source spacing at
mid-latitudes. The package also contains parent tiles down to z0 so zoomed-out
views do not have to fetch an unbounded set of z10 children.

Parent terrain tiles are safety-conservative: each parent sample is the maximum
valid elevation from the corresponding child sample footprint. If every
contributing child sample is nodata, the parent sample remains nodata. This
means zoomed-out terrain can overstate terrain height, but should not hide a
peak by averaging it away.

The numeric terrain product is terrain height in feet above the WGS84 ellipsoid:

```text
output_height_ft_wgs84_ellipsoid =
    source_dem_height_m_orthometric * 3.280839895
    + geoid_or_vertical_datum_offset_ft
```

This is intentionally aligned with Android/GPS altitude semantics, where raw
location altitude is height above the WGS84 reference ellipsoid. Clients should
be able to compare:

```text
gps_altitude_ft_wgs84_ellipsoid - terrain_height_ft_wgs84_ellipsoid
```

without carrying a geoid model or doing vertical-datum conversion.

The intended sample encoding is signed 16-bit integer feet with explicit
metadata:

- `output_units: feet`
- `output_vertical_datum: WGS84 ellipsoid`
- `source_vertical_datum`: copied from the source DEM metadata
- `nodata: -32768`
- `missing_dem_cells`: one-degree DEM cells that were intentionally omitted
  because every discovered source candidate failed

## Publication

`current_artifacts_YYYYMMDD.json` carries one `static_products[]` entry per
published terrain region. The product ids are shaped as:

```text
terrain-ak
terrain-nw
terrain-sw
...
```

The packaged artifact is content-addressed, so unchanged source DEM coverage and
unchanged product code should keep the same published filename.

The shaded-relief visual product is published the same way, with ids shaped as:

```text
shaded-relief-ak
shaded-relief-nw
shaded-relief-sw
...
```

Its first-cut package layout is:

```text
shaded-relief-<region>_<sha256>.zip
  manifest.json
  tiles/<z>/<x>/<y>.png
```

The first-cut shaded-relief renderer derives directly from the same USGS 3DEP
DEM inputs as numeric terrain. It does not derive from the published `.terrain`
tiles, because visual hillshade needs neighboring DEM samples at tile edges and
should not be coupled to a client lookup format. The current renderer applies
coarse sectional-style elevation color buckets and multiplies in a simple
northwest hillshade. Nodata is transparent. Water and glacier masks are not
included yet.

The shaded-relief source/max zoom is z10, with alpha-preserving RGBA parent
tiles generated down to z0 for scalable zoomed-out rendering.

Terrain refresh state lives in `current_artifacts`, not inside the terrain ZIP.
That keeps product identity stable: if a later poll checks TNMAccess/DEM inputs
and the content is unchanged, the terrain ZIP filename can remain identical
while `current_artifacts.static_products[]` records a fresh
`source_fetched_at_utc`.

Refresh cadence is producer policy, not artifact metadata. A scheduler can
compare `source_fetched_at_utc` with its configured refresh interval, then run a
refresh build when the source validation is due. Clients should use the
content-addressed filename to decide whether they already have the current
terrain artifact.

## Source Data

The first source target is USGS 3DEP 1 arc-second DEM GeoTIFF tiles. Those tiles
are roughly 30 meters north/south, are distributed as 1x1 degree GeoTIFFs, and
are small enough to support demand-fetching by the published Aerobag raster tile
coverage.

The source discovery path should use TNMAccess/3DEP product metadata to find
intersecting DEM tiles for a requested bbox. Source DEM files should be cached
content-addressed before generating public products.

Normal terrain builds use the fetch cache in `cache-first` mode: existing local
inputs are reused, cached blobs are restored without HTTP validation, and only
true cache misses fetch from USGS. Intentional source refreshes should override
that policy with `TERRAIN_FETCH_CACHE_MODE=fill`, which performs the HTTP
validation/download behavior and updates cache provenance.

The production build discovers all existing Aerobag regions. The validation
profile limits terrain to `NW` so compiler/build smoke tests do not
accidentally fetch and tile the entire country.

## Validation

The audit command:

```text
preprocessor-cli audit-terrain-airports \
  --nav-db <main.db> \
  --dem-vrt <dem.tif-or.vrt> \
  --geo-csv <geo.csv> \
  --output-dir <dir> \
  [--bbox <west,south,east,north>] \
  [--limit <count>]
```

probes a DEM at FAA airport reference points and runway endpoints, applies the
terrain height transform, and writes:

- `terrain_airport_audit.csv`
- `terrain_airport_scatter.svg`

The scatter plot compares transformed DEM height against charted airport/runway
elevation plus the same geoid offset.

Current limitation: the implemented audit transform uses the existing Avare
`geo.csv` one-degree geoid-height grid as an approximate offset model. That is
good enough to catch sign errors, unit errors, and obvious source mistakes. The
production terrain product should fail closed unless a real source vertical CRS
to WGS84 ellipsoid transform is available for the DEM tile being processed.
