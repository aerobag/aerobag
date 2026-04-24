UI handoff, 2026-04-24

What changed:

- `chart/catalog` entries in `nav_db` now again include chart coverage in the runtime payload.
- Current shape is bbox only:
  - `coverage: { "kind": "bbox", "value": { south, north, west, east } }`
- This is emitted for chart package records so runtime can answer "does this chart cover this point?" without using raw tile bounds alone.

Why:

- TAC-over-sectional compositing had regressed after coverage disappeared from the published contract.
- Transparent TAC edge tiles were then allowed to sit on top of sectional fallback outside true chart coverage, producing visible seams.

What to use on the UI side:

- Read `coverage` from `chart/catalog`.
- If present and `kind == "bbox"`, treat that as the chart footprint gate before choosing that chart for compositing / selection.
- There is no polygon coverage payload yet. This is the first-step fix.

Bundle-contract note:

- `build-product` no longer rebuilds fast products.
- `current_artifacts` still points at both the active cycle bundle and the active fast bundle.
- So a normal production cycle build now avoids NOAA/MRMS fast-product flakiness but does not lose fast-product discovery.

Files of interest:

- `product/preprocessor/preprocessor-cli/src/product_build.rs`
- `product/preprocessor/preprocessor-cli/src/main.rs`

