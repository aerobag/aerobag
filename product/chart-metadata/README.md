# Chart Metadata

This is the authoritative Aerobag-owned metadata for laying out FAA raster
charts.

Family directories contain:

- `*.geojson`: chart neatlines used to crop source GeoTIFFs before tiling.
- `*.legend.json`: source-pixel rectangles rendered into chart legend sheets.
- `*.inset.json`: source-pixel rectangles rendered into chart inset sheets.
- `*.navigable-insets.json`: manually georeferenced source-chart insets added
  to their explicitly selected tile family: TAC/Flyway for VFR sources,
  IFR-L/IFR-H for the respective enroute sources.
- `SEC/navigable-inset-candidates.json`: reviewed work queue of useful map
  insets that are not already supplied as standalone TAC products.
- `visual-references/<region-id>.json`: explicitly approved chart thumbnails,
  control-point patches, and their cutline/georeference provenance. These are
  durable source assets, not regenerated cycle outputs.

The collection began with Apps4Av cutlines and is now maintained as an Aerobag
fork. See `UPSTREAM.md` and the repository's `THIRD_PARTY_NOTICES.md` for that
lineage.

Use `tools/chart_cutline_editor.py` to edit neatlines, legends, and inset
regions. Product preprocessing fingerprints this directory, so committed
changes invalidate the corresponding chart products.

Cutline edges are straight in their **explicitly recorded CRS**, not necessarily
in Web Mercator or source pixels. Existing GeoJSON files use EPSG:3857 metres or
CRS84 longitude/latitude. The editor simplifies dense outlines in the source
GeoTIFF's native CRS when that reduces the number of handles. These edits save
the full native CRS (WKT in `crs.properties.name`) and coordinates, not pixel
positions. Already compact outlines retain their stored CRS and sparse handles;
the displayed curves are projected separately, not extra handles. A future
source-image placement change therefore does not move
the saved geographic boundary. Missing or invalid CRS declarations are errors;
readers do not guess from coordinate magnitudes.

`preprocessor-charts/chart_cutlines.py` owns reading, projection, simplification,
and derived geometry for the editor, review tool, chart builder and NAVDB offline
region footprints. Native simplification produces sparse edit handles: turns of at
least 30 degrees are pinned, and the remaining spans are simplified to a 0.20
source-pixel error budget. Curved reprojection is adaptively sampled to 0.025
source pixels. Opening does not rewrite metadata or approve anything; saving
stores the outline in the selected editing CRS and invalidates its old review normally.
Browser drags send coalesced preview requests to this same implementation. Only
the latest result is drawn, and Save waits for a valid current preview. Both the
overview and loupe render the projected curves, not straight joins between handles.
The editor rejects unsupported holes/multiple polygons rather than dropping them;
the tile builder retains both.

During preprocessing, `.cutlines/` holds disposable, adaptively densified Web
Mercator geometry. Both VFR and IFR warps use it for clipping **and crop extents**.
Transforming only the sparse corners is not sufficient for either: straight native
edges can bow in Mercator, and their extrema need not occur at corners. Never
copy this derived vertex cloud back into the editable metadata. The same explicit
projection supplies reference coverage and Offline Packages region outlines.
Changes to this helper invalidate both chart and NAVDB build caches.

Run the hermetic edit/save/warp, curve-extrema, topology, and simplification tests:
`/usr/bin/python3 -m pytest tools/test_chart_cutlines.py tools/test_chart_cutline_editor.py`.
The disposable browser journey, `node tools/test-chart-cutline-browser.mjs`, uses
synthetic charts to verify four-handle editing, curved previews, save and reload.

New cycles are compared with the last **manually approved** visual reference,
not with the previous cycle. Missing references require review; strong changes
to manually placed insets block publication. Reports and unresolved alerts
appear in Pipeline Health. See [Chart Visual References](../../docs/chart-visual-reference-checks.md)
for the checker, approval commands, scoring policy, and calibration limitations.

Each inset requires `target_family: "TAC"` or `target_family: "FLY"`; there is
no document-wide destination or decoder default. `enabled` independently
controls inclusion. The editor exposes **Output layer: TAC / Flyway** and
**Include in build**. One source sheet can contribute to both layers: Juneau
and Seward Glacier Area go to TAC, Juneau High Density Traffic Area to Flyway.
Both destinations use the same georeference/warp builder and filter insets into
their own mosaics. The process nodes depend on Sectional sources as well as
TAC archives, and fingerprint both sources and inset metadata. Flyway tiles
remain in the existing Flyway namespace inside the shared TAC download package;
sharing an archive does not mix the displayed layers.

Enabling an inset also removes that **same source-pixel polygon** from its parent
Sectional. This is independent of whether the inset goes to TAC or Flyway. The
builder adds a tiled, compressed validity mask to the parent's RGB VRT, before both
the normal neatline warp and any dateline supplement. The original FAA TIFF and
the edited neatline remain untouched; relocated insets read that original TIFF.
Polygon interiors, edge-touching notches, and overlapping insets use the same
masking path, not bounding-box erasure or a second set of hand-edited exclusions.
Disabled drafts neither publish an inset nor remove its pixels from the parent.
The parent output remains three-band RGB, like the other charts in its mosaic.
The mask is a derived build artifact; source, metadata, and builder fingerprints
invalidate the chart process cache when any of these change.

Navigable-inset layout schema 2 requires a `projection_wkt` string on **every**
inset, including drafts: a complete, valid projected CRS in WKT. Schema 1 and
missing, malformed, or geographic-only projections are rejected; there is no
builder inheritance or projection search. The editor copies the parent CRS once
when creating a draft, records that explicit choice, and displays it under
**Explicit calibration projection**. Saving/reloading preserves that string
verbatim. To choose another projection, edit the inset's `projection_wkt`, reload,
and validate against independent graticule marks before approving it.

Navigable inset controls pair source pixels with WGS84 coordinates. Each control
has an explicit `kind`: `intersection` requires both `latitude` and `longitude`;
`latitude` and `longitude` ticks supply only their named coordinate (the other
must be null). Disabled drafts may retain unfinished controls.
`preprocessor-charts/navigable_inset.py` supplies the **same fit** to
the editor and the tile builder: fit six affine pixel-to-projected coefficients
in the inset's explicit CRS (typically Lambert Conformal Conic). Inverse-project
each predicted location and minimize errors in only the observed coordinates.
No coordinate is invented for a one-dimensional tick. Neither projection nor
placement is inherited from the parent at build time. GDAL reprojects that fit
to Web Mercator. The polygon cutline
stays in the fitted plane, so it follows the same curved reprojection.

The editor reports leave-one-out errors: fit all but one control, predict the
withheld control, repeat. Pixel errors use the local inverse fitted transform;
for one-coordinate ticks these are cross-track errors, not errors along the
unconstrained line. Metre errors are approximate ground distances, not Web
Mercator distances. The build
writes the same diagnostics alongside each inset as `*.fit.json`. A typo remains
a large residual, rather than being hidden by a higher-order polynomial or a
rubber-sheet fit. Missing inset projections and degenerate controls fail explicitly.
Supply at least four latitude and four longitude observations with good spatial
spread, preferably more. Four intersections supply eight constraints; eight
single-coordinate ticks do too, provided both coordinates are well distributed.
Put latitude ticks on opposite left/right edges and longitude ticks on opposite
top/bottom edges. Rank-deficient arrangements, including leave-one-out subsets,
are rejected. Verify small residuals before enabling it. The initial parent-CRS
choice is a hypothesis to validate, not proof that every inset uses that CRS.

The September 2026 migration pins the existing parent projections for 14 insets,
with identical fit results. Miami TAC's Florida Keys inset instead pins the
Miami Sectional LCC CRS (standard parallels 30 2/3 and 25 1/3 degrees), not the
parent TAC's 45/33. Unchanged seven-point controls improve from 5.029 to 1.677px
worst leave-one-out error. Another 23 independently located graticule marks,
not used in either fit, give RMS 1.618 versus 0.822px. These are chart-graticule
checks, not certified absolute positional accuracy.

The explicit projection participates in build fingerprints and the approved
region definition hash. Adding/changing it requires reapproval even with
unchanged pixels; migrations do not rewrite approvals. Changed source CRS
metadata also alarms independently of the pixel detector. Printed-geometry
changes are checked against the approved overview and native control patches;
the visual detector remains threshold-based, not a guarantee against all drift.

Controls need not be intersections of two full graticule lines. A labelled
meridian's minute ticks also supply latitude/longitude controls. For example,
Seward Glacier's south neatline is 60 N and the 140 W meridian has latitude
ticks above it; the Juneau inset has usable ticks on the 134 45 W and 134 30 W
meridians. Count from the labelled parallel and use multiple, spread-out controls.
If a chart genuinely has no graticule, matching precise features against an
already georeferenced source is another possible method, with inherited source
and feature-placement uncertainty; the editor does not yet have a paired-point UI.

Control points may be outside the cutline (e.g. on a labelled neatline), but
must remain inside the source raster. Calibration and clipping are independent:
the build uses every control to establish the transform, then clips to the
unchanged outline. The georeference overview includes the controls and a small
context margin. Click a control's numbered button to inspect its mark in the loupe.
In georeference mode, select the new point's type before placing it, or change
an existing point's type in its row. Coordinate fields accept decimal degrees
or space-separated degrees/minutes: `58 15` becomes `58.25`, `-134 30` becomes
`-134.5`. South/west use a minus sign applying to the whole angle. Minutes must
be nonnegative and less than 60. Tab/blur normalizes the entry; invalid input
stays visible and blocks saving rather than silently retaining an old value.

Inset overviews are downsampled on the server to a bounded image, including
regions larger than the full-resolution crop limit. Coordinates and loupe
requests remain in source pixels. Image-load failures are reported in the UI.

Run the focused projection, warp, and editor tests with:
`python3 -m unittest tools.test_chart_cutline_editor`.
The perimeter-only regression fits no full intersections and predicts an
independent interior grid through the production VRT, including projection curvature.
`cargo nextest run --locked --profile ci -p preprocessor-charts` (from
`product/preprocessor`) also checks actual pixels in the parent Sectional mosaic,
its dateline supplement, and the relocated TAC/Flyway VRTs. It verifies that
enabled polygons disappear only from their old placement while ordinary chart
content and disabled drafts survive.

The navigable-inset candidate inventory deliberately excludes inset maps that
duplicate standalone TACs (Los Angeles, St Louis, Tampa, and
Baltimore-Washington). Juneau's High Density Traffic Area graphic goes to
Flyway rather than TAC. It states a 1:150,000 scale and is calibrated using
its perimeter latitude-only and longitude-only ticks. Its
"NOT TO BE USED FOR NAVIGATION" label is not evidence that it is unscaled;
[ordinary FAA Flyway planning charts carry the same restriction](https://www.faa.gov/air_traffic/flight_info/aeronav/productcatalog/PlanningCharts/VFRFlyway/).

## FAA Detail Sources

An FAA-supplied detail GeoTIFF belongs in its publishing family (`ENR_L` or
`ENR_H`), with an ordinary cutline. It uses its own GeoTIFF georeference, not
manual inset calibration. The optional top-level `source_sheet` registration
places its cutline on the printed parent in the sheet reviewer and automatically
excludes that printed copy from the parent's raster. Its six-element
`pixel_transform` follows GDAL affine order and maps detail pixels to parent
pixels; the four stored dimensions reject source-layout changes.

The build splits native-CRS cutlines at the antimeridian before cropping, and
stacks registered details after ordinary charts. Editing a detail's cutline also
updates its parent's exclusion. Keep the single authoritative cutline in the
active family, not a second copy under the unused `ENR_A` directory.

For the current incorporation checklist and operator review steps, see
[`docs/chart-region-incorporation.md`](../../docs/chart-region-incorporation.md).
