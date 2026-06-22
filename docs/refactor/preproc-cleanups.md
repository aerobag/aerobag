# Preprocessor Cleanup Plan

## Goal

Reduce copy/paste in the preprocessor build pipeline without changing published artifact contracts. The highest-risk duplication is in orchestration code: cache-node execution, regional package loops, live-feed staging, and package/zip utilities.

## Cleanup Candidates

1. Done: Factor cached node execution in `preprocessor-cli/src/product_build.rs`.
   - Repeated pattern: build inputs, prepare node, claim/wait, start timer, run body, assemble outputs, write `NodeRecord`.
   - Desired helper: a local `run_cached_node` style wrapper that centralizes timing, cache-hit return, output recording, and claim/wait behavior.
   - Scope guard: keep node-specific inputs/outputs explicit at call sites.

2. Done: Factor regional package-node loops.
   - Charts and CSUP both loop `Region::ALL`, derive package paths, handle cached package records, synthesize fallback `PackageOutputRecord`s, and write aggregate `package_outputs.jsonl`.
   - Desired helper: generic regional package orchestration with product-specific naming/build closures.
   - Risk: package manifests and cache roots are contract-sensitive, so refactor only after step 1 is stable.

3. Done: Factor live-feed product builder skeleton.
   - TFR, METAR, and NEXRAD repeat timestamped scratch setup, provenance writing, fetch-cache wiring, node-cache wrapping, and output tuple creation.
   - Desired helper: common live-feed workspace/provenance/cache wrapper while keeping product-specific URL selection and parsing separate.

4. Done: Move small shared utilities out of product crates.
   - Moved the duplicated `sanitize_label` helper into `preprocessor-tools`.
   - Left recursive copy, tree hashing, and zip member helpers in place because the current call sites are not identical utilities.

5. Done: Centralize deterministic zip writing.
   - Added `preprocessor-zip::write_deterministic_zip`.
   - Migrated live-feed, vector, data, and static tile ZIP writers to the shared helper.
   - Kept ZIP utilities out of `preprocessor-tools` so ZIP-only changes do not spoil TPP/CSUP render caches.
   - Left raw ZIP writer usage in tests where it creates small fixture archives.

## Execution Notes

- Preserve current artifact filenames, manifests, HAD key contracts, and node fingerprints unless deliberately changing behavior.
- Add or keep tests around any deleted branchy code.
- Prefer small internal helpers before moving code across crate boundaries.
- Re-run targeted tests plus `cargo build -p preprocessor-cli` after each cleanup step.
