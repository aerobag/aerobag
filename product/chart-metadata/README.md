# Chart Metadata

This is the authoritative Aerobag-owned metadata for laying out FAA raster
charts.

Family directories contain:

- `*.geojson`: chart neatlines used to crop source GeoTIFFs before tiling.
- `*.legend.json`: source-pixel rectangles rendered into chart legend sheets.
- `*.inset.json`: source-pixel rectangles rendered into chart inset sheets.

The collection began with Apps4Av cutlines and is now maintained as an Aerobag
fork. See `UPSTREAM.md` and the repository's `THIRD_PARTY_NOTICES.md` for that
lineage.

Use `tools/chart_cutline_editor.py` to edit neatlines, legends, and inset
regions. Product preprocessing fingerprints this directory, so committed
changes invalidate the corresponding chart products.
