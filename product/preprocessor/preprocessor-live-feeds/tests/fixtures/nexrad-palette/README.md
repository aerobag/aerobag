# Historical NEXRAD Palette Regressions

Historical test data only. Not current weather or suitable for navigation.

Six complete source-grid tiles from NOAA/NWS MRMS CREF_QCD frames that triggered
production color-match warnings on September 8-11, 2026. Source PNGs are lossless
crops of the original four-band NOAA TIFF pixels, without recoloring or resampling.
Tiles are 512 x 512 except at the source grid's east edge (344 x 512).
The `-before.png` files are the corresponding deployed encoder's actual output,
reconstructed and checked against the captured production warnings.

`manifest.json` records source URLs, full source and fixture SHA-256 hashes,
observation times, source-grid coordinates, deployed encoder/palette hashes,
and the original error counts. Cases include the worst recovered error (54),
multiple exception colors, high palette usage, and an error just over the limit (9).
Synthetic tests cover a completely full palette separately.

Provenance and public-domain treatment follow `aerobag/test-artifacts`' NOAA
fixtures. See `LICENSES/LicenseRef-US-Government-Public-Domain.txt` in the repo root.
No private application logs or user information are included.

Regenerate into a fresh directory from the retained September 11 capture:

```sh
/usr/bin/python3 docs/nexrad/analysis/extract_palette_fixtures.py \
  /tmp/aerobag-nexrad-review-20260911 /tmp/nexrad-palette-fixtures
```

Ordinary Python CI discovers `product/preprocessor/scripts/test_nexrad_source_grid_tiles.py`.
It runs these six real cases independently and checks decoded PNG pixels as well
as quality metadata. The full continental sources are not needed to run it.
