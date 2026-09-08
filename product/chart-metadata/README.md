# Chart Metadata

This is the authoritative Aerobag-owned metadata for laying out FAA raster
charts.

Family directories contain:

- `*.geojson`: chart neatlines used to crop source GeoTIFFs before tiling.
- `*.legend.json`: source-pixel rectangles rendered into chart legend sheets.
- `*.inset.json`: source-pixel rectangles rendered into chart inset sheets.
- `*.navigable-insets.json`: manually georeferenced source-chart insets added
  to their explicitly selected TAC or Flyway tile family.
- `SEC/navigable-inset-candidates.json`: reviewed work queue of useful map
  insets that are not already supplied as standalone TAC products.

The collection began with Apps4Av cutlines and is now maintained as an Aerobag
fork. See `UPSTREAM.md` and the repository's `THIRD_PARTY_NOTICES.md` for that
lineage.

Use `tools/chart_cutline_editor.py` to edit neatlines, legends, and inset
regions. Product preprocessing fingerprints this directory, so committed
changes invalidate the corresponding chart products.

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

Navigable inset controls pair source pixels with WGS84 coordinates. Each control
has an explicit `kind`: `intersection` requires both `latitude` and `longitude`;
`latitude` and `longitude` ticks supply only their named coordinate (the other
must be null). Disabled drafts may retain unfinished controls.
`preprocessor-charts/navigable_inset.py` supplies the **same fit** to
the editor and the tile builder: fit six affine pixel-to-projected coefficients
in the source GeoTIFF's CRS (typically Lambert Conformal Conic). Inverse-project
each predicted location and minimize errors in only the observed coordinates.
No coordinate is invented for a one-dimensional tick. Only the parent's
projection is reused, never its
main-map placement. GDAL reprojects that fit to Web Mercator. The polygon cutline
stays in the fitted plane, so it follows the same curved reprojection.

The editor reports leave-one-out errors: fit all but one control, predict the
withheld control, repeat. Pixel errors use the local inverse fitted transform;
for one-coordinate ticks these are cross-track errors, not errors along the
unconstrained line. Metre errors are approximate ground distances, not Web
Mercator distances. The build
writes the same diagnostics alongside each inset as `*.fit.json`. A typo remains
a large residual, rather than being hidden by a higher-order polynomial or a
rubber-sheet fit. Missing projections and degenerate controls fail explicitly.
Supply at least four latitude and four longitude observations with good spatial
spread, preferably more. Four intersections supply eight constraints; eight
single-coordinate ticks do too, provided both coordinates are well distributed.
Put latitude ticks on opposite left/right edges and longitude ticks on opposite
top/bottom edges. Rank-deficient arrangements, including leave-one-out subsets,
are rejected. Verify small residuals before enabling it. The source projection is a hypothesis to validate,
not proof that every inset uses its parent's projection.

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
