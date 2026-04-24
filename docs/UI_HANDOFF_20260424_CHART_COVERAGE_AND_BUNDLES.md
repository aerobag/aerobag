UI handoff, 2026-04-24

What changed:

- `chart/catalog` entries in `nav_db` now again include chart coverage in the runtime payload.
- Current shape is polygon-set based:
  - `coverage: { "kind": "polygon_set_ref", "value": { polygon_set_id } }`
- Referenced coverage geometry lives in HAD under:
  - `geometry/polygon-set/<polygon_set_id>`
- Each polygon-set payload contains the cutline polygons for that package-region chart family.

Why:

- TAC-over-sectional compositing had regressed after coverage disappeared from the published contract.
- Transparent TAC edge tiles were then allowed to sit on top of sectional fallback outside true chart coverage, producing visible seams.
- We first tried bbox coverage, but it was not precise enough to fix the compositor bug. The producer now publishes the real cutline-derived geometry instead.

What to use on the UI side:

- Read `coverage` from `chart/catalog`.
- If present and `kind == "polygon_set_ref"`, load the corresponding `geometry/polygon-set/<polygon_set_id>` record and test the point against any polygon in the set before choosing that chart for compositing / selection.
- This is package-level coverage, not per-city TAC-sheet records. That matches the current package-level `chart/catalog` shape.

Bundle-contract note:

- `build-product` no longer rebuilds fast products.
- `current_artifacts` still points at both the active cycle bundle and the active fast bundle.
- So a normal production cycle build now avoids NOAA/MRMS fast-product flakiness but does not lose fast-product discovery.

Files of interest:

- `product/preprocessor/preprocessor-cli/src/product_build.rs`
- `product/preprocessor/preprocessor-cli/src/main.rs`
