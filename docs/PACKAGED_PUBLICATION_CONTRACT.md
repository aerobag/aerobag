# Packaged Publication Notes

The canonical publication layout is defined in
[`PUBLICATION_CONTRACT.md`](PUBLICATION_CONTRACT.md). In particular, the retired
flat `published_packaged/` and `published_unpacked/` roots are no longer part of
the contract.

This file records product-specific package notes that still apply inside the
canonical layout:

```text
published/<publish-label>/<publish-timestamp>/packaged/
published/<publish-label>/<publish-timestamp>/unpacked/
```

## Nav Db

`nav_db_<contract>_<cycle>_<cycle-version>_<sha256>.zip` is the per-cycle
app-native key/value runtime index package.

The zip contains `root` plus `page_NNNN` page files. Android installs the zip
atomically. Web reads the unpacked mirror under the selected manifest's
`artifact_roots.unpacked`.

The initial required keys include:

```text
chart/catalog
resource/families
resource/regions
resource/temporal-summary
package/index
package/by-id/{package_id}
```

`chart/catalog` is JSON for the app-ready raster chart catalog. Consumers
should use this instead of parsing per-cycle publication metadata to discover
selectable raster chart layers. The catalog includes tiled chart packages and
app-visible static visual raster products, such as shaded relief.

Plate and procedure data are not published as one bulk chart-page catalog. They
are published under consumer-shaped HAD keyspaces such as
`plate/airport/{airport_id}`, `plate/by-id/{plate_id}`,
`plate/cifp/{airport_id}/{cifp_id}`, and `procedure/geometry/...`. The current
keyspace inventory lives in `docs/HAD_QUERY_KEYSPACES.md`.

## Obstacles

Obstacles are no longer part of the packaged cycle publication contract. They
are published through the live-feed contract.

## Terrain

`terrain-<region>_<contract>_<sha256>.zip` is a standalone content-addressed
terrain artifact. It is listed in each current cycle bundle's `packages[]`.
Consumers fetch it only if they explicitly need terrain.

Magnetic variation is not published as a standalone package. It is generated
into the nav-db HAD `magvar/` keyspace from NOAA/NCEI WMM coefficients fetched
by the preprocessor source pipeline. The nav-db also carries `magvar/source`,
which records upstream source metadata.

The terrain package contains `manifest.json` plus
`tiles/<z>/<x>/<y>.terrain` members. TER2 numeric terrain source/max zoom is
z9, and parent tiles are generated down to z0. Source tiles are generated from
the DEM with GDAL max resampling and overviews disabled. Terrain heights are
quantized upward into 64-foot bins. Parent terrain samples are the maximum
valid quantized elevation over the covered child sample footprint; all-nodata
footprints remain nodata. Terrain tile members are gzip-compressed `ABT2`
payloads stored directly in the outer zip. The outer zip must not deflate
`.terrain` members again.

When serving unpacked `terrain-*/tiles/**/*.terrain` over HTTP, the server
should treat the file bytes as precompressed content:

- `Content-Type: application/vnd.aerobag.terrain`
- `Content-Encoding: gzip`
- no additional dynamic gzip/deflate recompression

With those headers, browser fetch consumers receive decompressed `ABT2` bytes.
Offline zip consumers that read directly from packaged ZIPs must gzip-decode the
member payload after reading the zip entry.

## Shaded Relief

`shaded-relief-<region>_<contract>_<sha256>.zip` is a standalone
content-addressed shaded-relief raster artifact. It is listed in each current
cycle bundle's `packages[]`. Consumers fetch it only if they explicitly need a
terrain-background visual layer.

The package contains `manifest.json` plus `tiles/<z>/<x>/<y>.webp` members. The
source/max zoom uses the same z10, 512x512 grid as the shaded-relief source,
with alpha-preserving RGBA parent tiles generated down to z0. WebP tile members
are already image-compressed and are stored in the outer zip without another
deflate pass.
