# Preprocessor Cleanup Plan

## Goal

Reduce copy/paste in the preprocessor build pipeline without changing published artifact contracts. The highest-risk duplication is in orchestration code: cache-node execution, regional package loops, fast-product staging, and package/zip utilities.

## Cleanup Candidates

1. Factor cached node execution in `preprocessor-cli/src/product_build.rs`.
   - Repeated pattern: build inputs, prepare node, claim/wait, start timer, run body, assemble outputs, write `NodeRecord`.
   - Desired helper: a local `run_cached_node` style wrapper that centralizes timing, cache-hit return, output recording, and claim/wait behavior.
   - Scope guard: keep node-specific inputs/outputs explicit at call sites.

2. Factor regional package-node loops.
   - Charts and CSUP both loop `Region::ALL`, derive package paths, handle cached package records, synthesize fallback `PackageOutputRecord`s, and write aggregate `package_outputs.jsonl`.
   - Desired helper: generic regional package orchestration with product-specific naming/build closures.
   - Risk: package manifests and cache roots are contract-sensitive, so refactor only after step 1 is stable.

3. Factor fast-product builder skeleton.
   - TFR, METAR, and NEXRAD repeat timestamped private-work setup, provenance writing, fetch-cache wiring, node-cache wrapping, and output tuple creation.
   - Desired helper: common fast-product workspace/provenance/cache wrapper while keeping product-specific URL selection and parsing separate.

4. Move small shared utilities out of product crates.
   - Duplicates include `sanitize_label`, recursive copy helpers, tree hashing, and zip member collection.
   - Likely home: `preprocessor-tools` or a small artifact utility crate.

5. Centralize deterministic zip writing.
   - Several crates write ZIPs independently with different timestamp/compression behavior.
   - Because package filenames are content-addressed, deterministic ZIP behavior should be the default utility, with explicit opt-outs.

## Execution Notes

- Preserve current artifact filenames, manifests, HAD key contracts, and node fingerprints unless deliberately changing behavior.
- Add or keep tests around any deleted branchy code.
- Prefer small internal helpers before moving code across crate boundaries.
- Re-run targeted tests plus `cargo build -p preprocessor-cli` after each cleanup step.

