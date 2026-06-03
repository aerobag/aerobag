# NEXRAD Live-Feed Analysis

This directory preserves the one-off analysis used while revisiting NEXRAD live-feed
transport. The sampled upstream run lives outside the repo at:

```text
/root/aerobag-five/tmp-fast-product-analysis/upstream-nexrad/2026-05-11T170642Z_2026-05-12T202440Z
```

The `tmp-fast-product-analysis` directory name is historical; it predates the
live-feed terminology.

The source files are upstream MRMS frames, not Aerobag's Avare-style postprocessed PNG bundle:

```text
CONUS_L2_CREF_QCD_YYYYMMDD_HHMMSS.tif.gz
```

`analyze_nexrad_palette.py` scans the upstream frames with 12 workers, caches the whole-day
opaque RGB union, builds a fixed palette, and reports max RGB channel error. The 255-color palette
leaves index 0 available for transparency and uses indices 1..255 for opaque radar colors.

Key result:

```text
unique_opaque_rgb         1566
palette_size              255 opaque + 1 transparent slot
max_rgb_channel_error     5
p50_rgb_channel_error     2
p90_rgb_channel_error     3
p95_rgb_channel_error     4
p99_rgb_channel_error     4
p999_rgb_channel_error    5
per_frame_min_colors      489
per_frame_median_colors   586
per_frame_max_colors      698
```

The saved files are:

```text
analyze_nexrad_palette.py
whole-day-greedy-256-palette.json
whole-day-greedy-256-palette-report.json
whole-day-greedy-255-palette.json
whole-day-greedy-255-palette-report.json
analyze_nexrad_index_deltas.py
whole-day-index-delta-report.json
```

`analyze_nexrad_index_deltas.py` encodes frames into palette indices and measures adjacent-frame
deltas. It de-duplicates duplicate upstream filenames from repeated prod fetches, sorts by upstream
frame filename, and uses a 16 MiB RGB lookup table for fast source-RGB-to-palette-index mapping.

Delta result from the 809 unique upstream frames in the run:

```text
source_tif_gz_median_mb          3.0206
indexed_frame_zlib_median_mb     0.9014
xor_delta_zlib_median_mb         0.5373
mod_delta_zlib_median_mb         0.5428
changed_pixels_median_pct        1.7398
changed_pixels_p90_pct           3.6442
xor_delta_vs_indexed_frame       0.6000
mod_delta_vs_indexed_frame       0.6055
```

The adjacent-frame delta is lossless relative to the palette-indexed frames, not relative to the
original RGBA source.
