# NEXRAD Live-Feed Analysis

## Pixel Review and Encoding Experiments

The reusable pixel-review tool collects warning evidence, reconstructs frames
using the **deployed** encoder and palette, and serves original/compressed
comparisons with blinking, native-pixel crops, pan/zoom, and numeric RGBA values.
There is no error amplification or interpolation in the detailed comparisons.
Unrecovered frames are listed explicitly, never replaced with other frames.
The reconstruction tool currently targets the historical `png8-fixed-palette`
captures; the new bounded encoder prevents these particular warnings at publication.

The collector is read-only on production. Use a fresh local capture directory:

```sh
cd docs/nexrad/analysis
python3 collect_pixel_review.py \
  --capture-dir /tmp/nexrad-review \
  --start 2026-09-11T00:00:00Z --end 2026-09-12T00:00:00Z
/usr/bin/python3 build_pixel_review.py \
  --evidence /tmp/nexrad-review/evidence --output /tmp/nexrad-review/site
/usr/bin/python3 -m http.server 8092 --bind 0.0.0.0 \
  --directory /tmp/nexrad-review/site
```

Choose an unused port. Serve **only `site/`**; `evidence/` includes raw health
logs for other subsystems. Host, remote paths and the optional development
fetch cache are CLI options. Dependencies: SSH/SCP for collection; system
Python with NumPy, Pillow and GDAL for reconstruction. Use trusted deployed
tiler files: reconstruction executes their Python code locally. Never point
the evidence reader at an untrusted capture.

`evidence/frames.json` records each state's source filename, release commit,
source hash, publication time and warning samples. Original TIFF gzip files
live in `evidence/sources/`; deployed code and palette are named
`<commit>-tiler.py` and `<commit>-palette.json`. They are hash-checked before
reconstruction. Existing matching analyses are reused; `--rebuild` recomputes
them. The directory should be retained outside git if the incident matters:
fetch caches and short-lived published history are **not incident archives**.

Browser checks use Playwright and Chrome stable; `PLAYWRIGHT_MODULE` can point
at an existing Playwright installation instead of installing it in this repo:

```sh
PLAYWRIGHT_MODULE=/path/to/node_modules/playwright \
  node check_pixel_review.cjs http://127.0.0.1:8092/ /tmp/nexrad-review/screenshots
/usr/bin/python3 -m unittest test_pixel_review test_nexrad_encodings
```

To compare encoding alternatives without changing production:

```sh
/usr/bin/python3 compare_nexrad_encodings.py \
  --review /tmp/nexrad-review/site/review.json \
  --output /tmp/nexrad-review/site/encoding-comparison.json
```

The experiment compares lossless RGBA on flagged tiles against keeping existing
indices and adding exact outlier colors to the tile palette. The latter uses
lossless RGBA if adding the exceptions would exceed PNG's 256-entry limit.
Both paths verify decoded pixels; the baseline writer must reproduce the
current tile bytes exactly before comparing sizes. These functions are
analysis-only, not production encoding policy.

### September 2026 Results

The September 11 investigation identified 29 warned-about frames in September
8-11 health logs, and recovered 13 originals (4 from production, 9 from the
development cache). All 13 reconstructed counts and maximum errors matched
the production logs. The remaining 16 include the September 11 01:02:31Z
source frame responsible for the count-6 peak; its original was not recovered.

On the **13 recovered warning frames**, across all four resolution levels:

| Encoding policy | PNG payload bytes | Change |
| --- | ---: | ---: |
| Original fixed-palette tiles | 19,175,917 | baseline |
| Lossless RGBA only for flagged tiles | 20,162,581 | +986,664 (+5.145%) |
| Add exact exception colors to flagged tile palettes | 19,176,079 | +162 (+0.000845%) |

20 of 1,768 tiles needed repair. Their existing palettes had 40-215 entries;
each needed 1-6 extra colors, so none required RGBA. Each formerly flagged
pixel became exact; all resulting opaque pixels stayed within the existing
max-channel error bound of 8. Per frame, palette repair added 4-49 bytes.
Replacing whole affected tiles with RGBA added 15,418-197,312 bytes per frame.

These are losslessly compressed PNG bytes, **not uncompressed pixel sizes**.
They exclude HTTP, manifests and package-container overhead. They include all
resolutions, not a particular viewport. A viewport concentrated on affected
tiles would see a larger relative RGBA increase (the 20 affected tiles alone
grow 3.16x in aggregate). This warning-selected sample is not a normal-traffic
sample and does not estimate future anomaly frequency.

### Production Repair

The production encoder now applies the tile-local repair, with the threshold
unchanged at 8. It verifies decoded PNGs before publishing. See
[`nexrad-source-driven-resolution.md`](../../nexrad-source-driven-resolution.md)
for the new descriptor and immutable encoder/state identity.
Replaying all 13 recovered frames through the production implementation
confirmed the table above: 34 pixels repaired in 20 tiles, zero remaining
violations, 1,748 byte-identical unaffected tiles, and exactly 162 added bytes.

Six complete historical tiles, with original pixels, before-repair PNGs and
source provenance, are checked in under
`product/preprocessor/preprocessor-live-feeds/tests/fixtures/nexrad-palette/`.
They total approximately 902 KiB and require no external fixture download.
All six failed the decoded-error test before the encoder was changed.

Run the production regression suite from the repository root:

```sh
/usr/bin/python3 -m pytest product/preprocessor/scripts/test_nexrad_source_grid_tiles.py
```

It also covers palette capacity, exact threshold boundaries, transparency,
lossless overflow, all resolution levels, edge tiles, metadata, unchanged
healthy PNG bytes, and rejection of corrupt output. Ordinary Python CI runs it.

## Original Palette Study

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
