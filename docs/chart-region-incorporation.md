# Chart Region Incorporation

Status on 2026-09-12: curation and approval are complete: 196 regions and
175 source-sheet inventories, with no outstanding review flags. All 15 manual
insets have approved control patches (116 patches total). Remaining work is
the full cycle rebuild and app-level rendering tour in
[chart-inset-tour.md](chart-inset-tour.md).

The September 2026 sheet review identified 14 incomplete source sheets. A
disposition is not an approval: preserve the operator's decisions until every
region has a reviewed boundary and is available through a built map layer or
an explicitly designated reference image.

## Work Order

1. Agent: repair ENR_P01's omitted western-Pacific coverage using an editable
   native-projection boundary and dateline-split output; prove both sides render.
2. Agent: incorporate the eight FAA-georeferenced detail TIFFs below through the
   ordinary cutline/build path. Link each to its printed parent sheet so nobody
   wastes time manually calibrating an already georeferenced image.
3. Agent: expose Add region on each review sheet; support manual insets from all
   chart families and distinguish map insets from reference-only diagrams.
4. Operator: refine and approve rough boundaries; add/calibrate genuinely missing
   regions. Keep sheets incomplete until their entire inventories are accounted for.
5. Agent: run cutline, projection, build and editor regressions; verify each added
   source participates in source discovery, cache fingerprints and quality checks.
6. Operator: rebuild cycle products after committing the curated metadata.
7. Agent/operator: inspect actual rendered tiles for every new region and confirm
   displaced copies do not remain on their parent maps. Only then close this list.

## Source Checklist

| Source sheet | Disposition / remaining work |
| --- | --- |
| SEC/Samoan Islands Inset SEC | Main cutline corrected and approved; retains FAA georeference. |
| ENR_H/ENR_P01 | Native boundary and dateline split implemented, locally rendered, curated and approved. |
| ENR_H/ENR_P01 | FAA `ENR_P01_GUA.tif`: Guam; no manual calibration. |
| ENR_H/ENR_AKH01 | FAA `ENR_AKH01_SEA.tif`: Seattle; no manual calibration. |
| ENR_L/ENR_AKL01 | FAA `ENR_AKL01_JNU.tif` and `ENR_AKL01_VR.tif`: Juneau and Vancouver. |
| ENR_L/ENR_AKL03 | FAA `ENR_AKL03_FAI.tif` and `ENR_AKL03_OME.tif`: Fairbanks and Nome. |
| ENR_L/ENR_AKL04 | FAA `ENR_AKL04_ANC.tif`: Anchorage. |
| ENR_L/ENR_L23 | FAA `ENR_L23_WILM_INSET.tif`: Wilmington. |
| SEC/Jacksonville SEC | Jacksonville and Tampa manual TAC insets calibrated and approved. |
| SEC/Los Angeles SEC | Source-sheet inventory reviewed and approved; no additional manual calibration added. |
| SEC/New Orleans SEC | Special Air Traffic Rule diagram retained as a non-georeferenced Chart Reference. |
| SEC/St Louis SEC | Indianapolis manual TAC inset calibrated and approved. |
| SEC/Twin Cities SEC | Lake of the Woods / Northwest Angle manual TAC inset calibrated and approved. |
| TAC/Grand Canyon General Aviation | Marble Canyon calibrated, enabled in TAC and approved; redundant reference removed. |
| TAC/Miami TAC | Florida Keys enabled and approved with explicit Miami Sectional projection; redundant reference removed. |
| TAC/Puerto Rico-VI TAC | Main cutline and reference extraction curated; final inventory approval complete. |

The separate ENR_H/ENR_H04 region rejection was also resolved. Do not confuse
region approval with sheet completeness or with successful publication.

## Completion Evidence

- Explicit inset projection migration: all 15 definitions now pin their CRS.
  The other 14 retain identical fits; Miami Keys improves from 5.029 to 1.677px
  worst leave-one-out residual. Source control points, boundaries, destinations,
  enabled flags were not changed by the migration. After explicit operator
  authorization, all 15 candidates were refreshed and approved. The 14 unchanged
  insets have exactly equal affine transforms using the original and new fitters;
  all 15 retain identical review pixels and source provenance. Earlier approvals
  are retained under the local review state's `projection-approval-20260912T143727Z`.
  The actual Miami production VRT was rendered and sampled successfully.
- Eight FAA detail cutlines now live in their active ENR_L/ENR_H families.
  Five obsolete copies under ENR_A were removed. Detail images are built last
  in the mosaic; their registered source-pixel footprints are excluded from
  the printed parent without editing the parent's authored outline.
- The reviewer groups these details on their six parent sheets, explicitly
  labels them as FAA-georeferenced, and offers their ordinary cutline editors.
  The operator has reviewed and approved all eight detail cutlines and inventories.
- P01 has six native-projection handles instead of 7,522 vertices. Its two
  output crops were read successfully: western Pacific (positive longitude)
  and eastern Pacific (negative longitude). Guam uses its separate FAA TIFF.
- Real cycle-2610 build previews: `/tmp/chart-regions-build-preview/ENR_H`
  contains five VRT inputs (P01's two halves, AKH01, Guam, Seattle); ENR_L
  contains ten (four parents and six details). All inputs occur in the final
  mosaics and all individual rasters contain non-nodata pixels.
- Verification: 76 Python tests across cutline projection, editor, review and
  visual references; 21 preprocessor-charts Rust tests; editor and reviewer
  browser journeys. Includes actual warped-pixel checks on both sides of the
  dateline, detail exclusion without changing the source TIFF, non-Sectional
  inset publication/masking, and preservation of review annotations.
- Rust formatting and whitespace checks passed. Full cycle tile generation,
  package publication, and app-level map inspection have **not** been run.

## Operator Workflow

Reviewer: `http://aerobag-dev.iac.jonh.net:8094/`.
Editor: `http://aerobag-dev.iac.jonh.net:8093/`.

1. Reload the reviewer and select **Flagged sheets** when new changes require
   review. It is currently empty. FAA details need cutline and source-sheet
   placement review, but **no manual georeference**.
2. For a genuinely new region, expand **Add region** on its parent sheet.
   Select **Map inset (manual georeference)** or **Reference-only diagram**.
   This keeps the current family/chart selected, including TAC and enroute
   sheets. Use **New inset**, draw its boundary, finish the outline, and save.
   Map insets start disabled; add control points, check the fit, choose the
   destination family, and enable only after calibration.
   For an existing reference image, use **Edit this region**, then **Start
   georeferencing**. This copies the saved rectangle into a disabled map draft
   and opens its georeference editor without redrawing. The reference remains
   available until the operator removes it after validating/enabling the map.
   Marble Canyon and Miami have completed this conversion.
3. Back in the reviewer, **Reload regions and render new candidates** discovers
   saved regions without restarting or importing an entire cycle report.
   Approve each region separately, then mark **All inset regions accounted for**
   only when the whole source sheet is covered. A reference-only diagram is
   accessible as a reference image, not an overlay; do not use that designation
   to hide a map that still needs calibration.
4. Resolve any new region or inventory flags before recording approval.
5. Commit the curated metadata and approvals, rebuild the cycle, and inspect
   the actual app layers at every added region. Keep this checklist open until
   every outstanding row has a final disposition and published result.

The decisions live in `/root/aerobag-three/chart-review/review.json`; approved
reference images/provenance live in the repo's `visual-references` directories.
Before restarting the tools, a safety copy of the decisions was taken at
`/tmp/chart-review-before-detail-incorporation.json`. No incomplete decision was
silently changed to complete.
