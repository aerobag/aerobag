# Next Session Handoff

## Session Notes

- 2026-04-05:
  - Confirmed legacy `csup` completed successfully in [runs/20260405T154700Z](/root/aerobag/runs/20260405T154700Z).
  - Confirmed legacy `tpp-ne` failed because of network/DNS during the 6th source fetch, not because of an obvious transform bug.
  - Started targeted retry run in [runs/20260405T154700Z-tpp-retry](/root/aerobag/runs/20260405T154700Z-tpp-retry) by copying the partially populated `tpp-ne` work tree and rerunning `python3 tpp.py NE`.
  - The retry reused the already downloaded FAA ZIPs and progressed past the earlier download failure into state/city processing.
  - The retry produced:
    - [NE_TPP](/root/aerobag/runs/20260405T154700Z-tpp-retry/work/tpp-ne/NE_TPP)
    - [NE_TPP.zip](/root/aerobag/runs/20260405T154700Z-tpp-retry/work/tpp-ne/NE_TPP.zip)
    - provenance JSONL files in [runs/20260405T154700Z-tpp-retry/meta/provenance/tpp-ne](/root/aerobag/runs/20260405T154700Z-tpp-retry/meta/provenance/tpp-ne)
  - This should now be treated as the recovered legacy `tpp-ne` baseline unless later validation shows a packaging problem.
  - Added `compare-chart-tile-paths` to the Rust CLI in [preprocessor-cli/src/main.rs](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs).
  - New result:
    - `tac` tile paths match exactly.
    - `sec` and `enr-l` tile paths do not match the legacy raw tile tree.
  - Important implication:
    - Earlier package/member parity for `sec` and `enr-l` was not sufficient evidence of full chart parity.
    - The likely issue is stale or reused package artifacts in the Rust work dirs, or a native tiling/output gap that package-only checks did not expose.
  - Follow-up investigation showed the `sec-native-fixed` and `enr-l-native-fixed` directories are internally inconsistent:
    - their manifests and ZIPs are older than their current raw tile trees,
    - so they mix artifacts from multiple passes and should not be trusted as clean parity evidence.
  - Valid current chart-family baselines are now:
    - [rust-runs/sec-clean-check/work/charts-sec](/root/aerobag/rust-runs/sec-clean-check/work/charts-sec)
      - exact tile-path parity: match
      - package/member parity: match
    - [rust-runs/tac-native-fixed/work/charts-tac](/root/aerobag/rust-runs/tac-native-fixed/work/charts-tac)
      - exact tile-path parity: match
    - [rust-runs/enr-l-port/work/charts-enr-l](/root/aerobag/rust-runs/enr-l-port/work/charts-enr-l)
      - exact tile-path parity: match
      - package/member parity: match
  - Added executable chart parity integration tests in [preprocessor-cli/tests/chart_parity.rs](/root/aerobag/rust-preprocessor/preprocessor-cli/tests/chart_parity.rs).
  - Current regression command:
    - `cd rust-preprocessor && cargo test -p preprocessor-cli`
  - Current result:
    - 5 parity tests passing
  - Consolidated Rust region definitions into one source of truth in [preprocessor-core/src/lib.rs](/root/aerobag/rust-preprocessor/preprocessor-core/src/lib.rs).
  - Region consumers updated to reference the shared symbolic region set instead of duplicating region codes locally:
    - [preprocessor-cli/src/main.rs](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs)
    - [preprocessor-charts/src/lib.rs](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs)
  - Started a clean sectional verification run in [rust-runs/sec-clean-check](/root/aerobag/rust-runs/sec-clean-check) by copying the legacy populated sectional work dir, removing generated region manifests/ZIPs and `tiles/`, then rerunning Rust `build-vrts` and `build-tiles`.
  - That clean sectional run later completed overview generation and matched legacy exactly, so the earlier concern was only that it was still mid-run when checked.

Read these first:
- [notes](/home/jonh/aerobag/notes)
- [RUST_PREPROCESSOR_DESIGN.md](/home/jonh/aerobag/RUST_PREPROCESSOR_DESIGN.md)
- [AVARE_NOTES.md](/home/jonh/aerobag/avare-source/AVARE_NOTES.md)

## Current State

- Official Avare sources are cloned under [avare-source](/root/aerobag/avare-source).
- I inspected the Android app and the split backend repos.
- I wrote a baseline architecture note in [AVARE_NOTES.md](/root/aerobag/avare-source/AVARE_NOTES.md).
- I wrote the compatibility-first design in [RUST_PREPROCESSOR_DESIGN.md](/root/aerobag/RUST_PREPROCESSOR_DESIGN.md).
- A Rust workspace now exists under [rust-preprocessor](/root/aerobag/rust-preprocessor).
- Legacy capture instrumentation was added to:
  - [avare-source/charts/common.py](/root/aerobag/avare-source/charts/common.py)
  - [avare-source/tpp/common.py](/root/aerobag/avare-source/tpp/common.py)
  - [avare-source/csup/common.py](/root/aerobag/avare-source/csup/common.py)
- Those local patches only record provenance and package hashes to JSONL when `CAPTURE_META_DIR` is set.
- They do not intentionally change preprocessing behavior.

## Investigated Run State

- The legacy reference run [runs/20260405T154700Z](/root/aerobag/runs/20260405T154700Z) is the main baseline.
- Legacy chart captures completed successfully:
  - `charts-sec`
  - `charts-tac`
  - `charts-enr-l`
- Legacy `csup` completed successfully and produced 9 region packages.
- Legacy `tpp-ne` did not complete.
  - It downloaded 5 FAA d-TPP ZIPs.
  - It then failed on the 6th fetch with DNS resolution failure against `aeronav.faa.gov`.
  - See [runs/20260405T154700Z/logs/tpp-ne.stderr.log](/root/aerobag/runs/20260405T154700Z/logs/tpp-ne.stderr.log).

## Verified Baselines

- The golden comparison contract in [legacy-capture/GOLDEN_COMPARISON_CONTRACT.md](/root/aerobag/legacy-capture/GOLDEN_COMPARISON_CONTRACT.md) is backed by the reference run.
- Rust CLI baseline counts match the captured legacy counts:
  - `charts-sec`: `35494`
  - `charts-tac`: `7174`
  - `charts-enr-l`: `27428`

## Rust Implementation Status

- The Rust CLI currently supports:
  - inspecting capture manifests,
  - comparing tile counts,
  - comparing chart package structure,
  - prefetching chart source archives,
  - running the legacy chart scripts under Rust orchestration,
  - native Rust orchestration for VRT build, tiling, and region packaging for `sec`, `tac`, and `enr-l`.
- Relevant code:
  - [preprocessor-cli/src/main.rs](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs)
  - [preprocessor-charts/src/lib.rs](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs)
  - [preprocessor-fetch/src/lib.rs](/root/aerobag/rust-preprocessor/preprocessor-fetch/src/lib.rs)
  - [preprocessor-tools/src/lib.rs](/root/aerobag/rust-preprocessor/preprocessor-tools/src/lib.rs)

## Rust Run Results

- Rust chart parity is already much farther along than the original handoff implied.
- These Rust work dirs structurally match the legacy chart outputs exactly by package manifest bytes, manifest entry sets, and ZIP member paths:
  - [rust-runs/sec-prefetch-20260405T1656Z/work/charts-sec](/root/aerobag/rust-runs/sec-prefetch-20260405T1656Z/work/charts-sec)
  - [rust-runs/sec-native-fixed/work/charts-sec](/root/aerobag/rust-runs/sec-native-fixed/work/charts-sec)
  - [rust-runs/tac-port/work/charts-tac](/root/aerobag/rust-runs/tac-port/work/charts-tac)
  - [rust-runs/tac-native-fixed/work/charts-tac](/root/aerobag/rust-runs/tac-native-fixed/work/charts-tac)
  - [rust-runs/enr-l-port/work/charts-enr-l](/root/aerobag/rust-runs/enr-l-port/work/charts-enr-l)
  - [rust-runs/enr-l-native-fixed/work/charts-enr-l](/root/aerobag/rust-runs/enr-l-native-fixed/work/charts-enr-l)
- The earlier run [rust-runs/sec-20260405T1619Z](/root/aerobag/rust-runs/sec-20260405T1619Z) is an older full-script run and appears superseded by the later `sec-prefetch` and `sec-native-fixed` outputs.

## Practical Resume Point

- Do not rerun legacy chart captures unless you want a fresh-cycle baseline or the old run artifacts are no longer trusted.
- Do not rerun Rust chart families just to regain parity evidence; the current `sec`, `tac`, and `enr-l` Rust outputs already match legacy structurally.
- The main unfinished legacy capture is `tpp-ne`.
- The main unfinished Rust product areas are `tpp` and `csup`; chart-family parity work is already in good shape.

## Important Corrections To Remember

- Avare tiles are Web Mercator, but `512x512`, not `256x256`.
- The client contract is strongly tied to package names and member paths from `arrays.xml`.
- The split `charts/`, `tpp`, `csup`, and `data` repos do not cover every legacy family.
- WAC / ONC / TPC / terrain / topo / VFR area / CAN_ADS still need explicit accounting.
- Airport diagram geotags are currently fetched from Outer World Apps:
  - `https://www.outerworldapps.com/WairToNowWork/avare_aptdiags.php`
- Chart Supplement images are standalone and not georeferenced in the present model.

## Suggested Immediate Next Steps On The 20-Core Box

1. Rerun `tpp-ne` legacy capture in a pinned environment with stable network.
   - Reuse the same instrumentation patches.
   - Confirm package outputs and provenance JSONL are produced.

2. Decide whether to treat current Rust chart parity as the chart baseline.
   - If yes, stop spending time rerunning `sec`, `tac`, and `enr-l`.
   - Move effort to visual diffs or to the non-chart families.

3. Extend Rust work beyond chart families.
   - `tpp` orchestration and capture parity.
   - `csup` orchestration and package/member parity.
   - fetch/cache hardening beyond simple archive prefetch.

4. Add stronger automated checks around the already-matching chart runs.
   - exact tile-path set assertions from run artifacts,
   - provenance parity where available,
   - sampled visual diffs once structural parity is stable.

## What Not To Do First

- Do not start by rewriting image transforms in pure Rust.
- Do not start by redesigning the Avare package format.
- Do not assume the split repos are the whole story.
- Do not optimize for GPU before profiling.

## Files And Code Paths Worth Revisiting

Android contract:
- [arrays.xml](/home/jonh/aerobag/avare-source/avare/app/src/main/res/values/arrays.xml)
- [Tile.java](/home/jonh/aerobag/avare-source/avare/app/src/main/java/com/ds/avare/shapes/Tile.java)
- [Boundaries.java](/home/jonh/aerobag/avare-source/avare/app/src/main/java/com/ds/avare/place/Boundaries.java)
- [Download.java](/home/jonh/aerobag/avare-source/avare/app/src/main/java/com/ds/avare/network/Download.java)
- [NetworkHelper.java](/home/jonh/aerobag/avare-source/avare/app/src/main/java/com/ds/avare/utils/NetworkHelper.java)

Split backend repos:
- [charts/common.py](/home/jonh/aerobag/avare-source/charts/common.py)
- [tpp/common.py](/home/jonh/aerobag/avare-source/tpp/common.py)
- [csup/common.py](/home/jonh/aerobag/avare-source/csup/common.py)
- [data/common.py](/home/jonh/aerobag/avare-source/data/common.py)

Legacy gap-filling scripts:
- [gtag.py](/home/jonh/aerobag/avare-source/avare/extra/charting/gtag.py)
- [gtag_tmerc.py](/home/jonh/aerobag/avare-source/avare/extra/charting/gtag_tmerc.py)
- [streets.sh](/home/jonh/aerobag/avare-source/avare/extra/area/streets.sh)
- [canada_ad.sh](/home/jonh/aerobag/avare-source/avare/extra/canads/canada_ad.sh)

## Practical Notes

- The local shell sandbox here needed escalated execution even for read-only commands because of environment restrictions. On the new box this may not matter, but do not assume current friction is code-related.
- There is a top-level `notes` file containing the original user brief. Leave it alone unless you want to append dated session notes.
- There is a stray `.notes.swp` in the workspace root. Ignore it unless the user says otherwise.
- `git status` in the legacy repos shows the instrumentation patches in `common.py`, plus a stray `__pycache__/` under `avare-source/csup`.

## Default Direction

If the next session starts with no further instruction, do this:
- confirm whether `tpp-ne` should be restarted immediately,
- treat existing chart-family Rust outputs as the current parity baseline,
- then continue into Rust `tpp` / `csup` work and stronger parity tests.

## Session Notes

- 2026-04-05 22:45:27Z:
  - Added `compare-provenance` to [`rust-preprocessor/preprocessor-cli/src/main.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs).
  - The command currently compares three provenance sets:
    - `source_urls.jsonl` URL set
    - `downloads.jsonl` download triples (`url`, `file`, `sha256`)
    - `downloads.jsonl` `extract_zip` archive/member sets
  - Added one CLI integration test in [`rust-preprocessor/preprocessor-cli/tests/chart_parity.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/tests/chart_parity.rs) that self-compares legacy `charts-sec` provenance to lock the parser/CLI path.
  - `cargo test -p preprocessor-cli` now passes with 6 tests.
  - Important finding: Rust provenance is not absent. It exists for at least the sectional runs under nested paths such as:
    - [`rust-runs/sec-prefetch-20260405T1656Z/work/rust-runs/sec-prefetch-20260405T1656Z/meta/provenance/charts-sec`](/root/aerobag/rust-runs/sec-prefetch-20260405T1656Z/work/rust-runs/sec-prefetch-20260405T1656Z/meta/provenance/charts-sec)
  - `compare-provenance` shows full parity for sectional inputs between:
    - legacy [`runs/20260405T154700Z/meta/provenance/charts-sec`](/root/aerobag/runs/20260405T154700Z/meta/provenance/charts-sec)
    - Rust [`rust-runs/sec-prefetch-20260405T1656Z/work/rust-runs/sec-prefetch-20260405T1656Z/meta/provenance/charts-sec`](/root/aerobag/rust-runs/sec-prefetch-20260405T1656Z/work/rust-runs/sec-prefetch-20260405T1656Z/meta/provenance/charts-sec)
  - Output was:
    - `source_urls left=55 right=55 status=match`
    - `downloads left=55 right=55 status=match`
    - `extracts left=55 right=55 status=match`
  - Next likely follow-up:
    - compare `tac` and `enr-l` provenance the same way if Rust provenance artifacts exist,
    - decide whether to normalize/fix the nested Rust provenance output path,
    - then move on to `tpp` or `csup`.

- 2026-04-05 22:50:34Z:
  - There were no existing Rust provenance dirs for `charts-tac` or `charts-enr-l`.
  - Cause: provenance is emitted by the script-driven `run-chart` path in [`rust-preprocessor/preprocessor-charts/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs), not by `run-native-chart`.
  - I reused the populated legacy work dirs as offline `source-repo` inputs and started:
    - [`rust-runs/tac-provenance-check`](/root/aerobag/rust-runs/tac-provenance-check)
    - [`rust-runs/enr-l-provenance-check`](/root/aerobag/rust-runs/enr-l-provenance-check)
  - Those runs write provenance to the clean top-level path:
    - `meta/provenance/charts-tac`
    - `meta/provenance/charts-enr-l`
    - not the old nested `work/rust-runs/.../meta/provenance/...` shape seen in earlier sectional runs.
  - Current input-provenance parity results:
    - legacy [`runs/20260405T154700Z/meta/provenance/charts-tac`](/root/aerobag/runs/20260405T154700Z/meta/provenance/charts-tac)
      vs Rust [`rust-runs/tac-provenance-check/meta/provenance/charts-tac`](/root/aerobag/rust-runs/tac-provenance-check/meta/provenance/charts-tac)
      - `source_urls left=30 right=30 status=match`
      - `downloads left=30 right=30 status=match`
      - `extracts left=30 right=30 status=match`
    - legacy [`runs/20260405T154700Z/meta/provenance/charts-enr-l`](/root/aerobag/runs/20260405T154700Z/meta/provenance/charts-enr-l)
      vs Rust [`rust-runs/enr-l-provenance-check/meta/provenance/charts-enr-l`](/root/aerobag/rust-runs/enr-l-provenance-check/meta/provenance/charts-enr-l)
      - `source_urls left=42 right=42 status=match`
      - `downloads left=42 right=42 status=match`
      - `extracts left=42 right=42 status=match`
  - At this timestamp both offline runs were still alive and had not yet written final top-level meta outputs such as a manifest, so treat them as:
    - input provenance validated
    - full capture completion still in flight / unconfirmed
  - Practical conclusion:
    - for `sec`, `tac`, and `enr-l`, the Rust path has now demonstrated provenance parity on the captured source inputs in addition to chart structural parity.

- 2026-04-05 22:53:28Z:
  - Cleaned up staging behavior in [`rust-preprocessor/preprocessor-charts/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs).
  - Root cause of the old nested provenance path: when staging from an already-populated source tree, `copy_dir_recursive(..., preserve_generated = true)` preserved generated chart artifacts but also copied prior run scaffolding if it was present under the source tree.
  - `should_skip_copy(...)` now always skips prior run scaffolding directories:
    - `logs`
    - `meta`
    - `work`
    - `rust-runs`
  - This is intentionally narrower than “delete everything generated”; downloaded charts, extracted rasters, zips, and other family artifacts are still preserved for offline/parity reuse.
  - Added a unit regression test:
    - [`populated_staging_skips_prior_run_scaffolding`](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs)
  - Verification:
    - `cargo test -p preprocessor-charts` passed
    - `cargo test -p preprocessor-cli` passed

- 2026-04-05 23:17:50Z:
  - Added a new lightweight Rust `csup` implementation in:
    - [`rust-preprocessor/preprocessor-csup/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-csup/src/lib.rs)
    - wired through [`rust-preprocessor/preprocessor-cli/src/main.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs)
  - Current model:
    - Rust handles staging, cycle/manifest writing, XML parsing, provenance capture, and region packaging.
    - It still delegates PDF-to-PNG rendering to ImageMagick `mogrify`.
    - It can reuse a populated legacy `csup` work dir offline and skip rerendering PNGs that already exist.
  - Added CLI commands:
    - `run-native-csup`
    - `compare-csup-packages`
  - Added tests in [`rust-preprocessor/preprocessor-cli/tests/chart_parity.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/tests/chart_parity.rs):
    - `csup_native_dedup_packages_match_legacy_entries`
    - `csup_native_dedup_provenance_matches_legacy`
  - `cargo test -p preprocessor-cli` now passes with 8 tests.
  - Provenance parity for `csup` is clean between:
    - legacy [`runs/20260405T154700Z/meta/provenance/csup`](/root/aerobag/runs/20260405T154700Z/meta/provenance/csup)
    - Rust [`rust-runs/csup-native-check-dedup/meta/provenance/csup`](/root/aerobag/rust-runs/csup-native-check-dedup/meta/provenance/csup)
  - Output was:
    - `source_urls left=1 right=1 status=match`
    - `downloads left=1 right=1 status=match`
    - `extracts left=1 right=1 status=match`
  - Package parity status for Rust [`rust-runs/csup-native-check-dedup/work/csup`](/root/aerobag/rust-runs/csup-native-check-dedup/work/csup) against legacy [`runs/20260405T154700Z/work/csup`](/root/aerobag/runs/20260405T154700Z/work/csup):
    - all 9 regions: `manifest_entries=match`
    - all 9 regions: `members=match`
    - all 9 regions: `manifest_bytes=mismatch`
  - Interpretation:
    - the remaining `csup` gap is manifest line ordering only, not missing or extra packaged content.
    - zip member sets and provenance inputs already match.
  - Important implementation note:
    - The remaining manifest-byte mismatch is likely due to legacy `glob`/filesystem ordering subtleties.
    - Rust now preserves XML airport order and deduplicates repeated XML airport entries, which was enough to eliminate true content mismatches.

- 2026-04-05T23:55:24Z:
  - Reconciled the still-dirty [`rust-preprocessor/preprocessor-csup/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-csup/src/lib.rs) state.
  - It was not unrelated drift; it was the real post-commit `csup` packaging-order fix that removed airport-list-driven packaging and switched to recursive directory traversal order.
  - Current `csup` parity baseline should use:
    - work dir [`rust-runs/csup-native-check-globorder/work/csup`](/root/aerobag/rust-runs/csup-native-check-globorder/work/csup)
    - provenance dir [`rust-runs/csup-native-check-globorder/meta/provenance/csup`](/root/aerobag/rust-runs/csup-native-check-globorder/meta/provenance/csup)
  - Regression coverage was updated accordingly in [`rust-preprocessor/preprocessor-cli/tests/chart_parity.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/tests/chart_parity.rs), and `csup` now requires `manifest_bytes=match`.
  - Added a new lightweight Rust `tpp` implementation in:
    - [`rust-preprocessor/preprocessor-tpp/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-tpp/src/lib.rs)
    - helper script [`rust-preprocessor/preprocessor-tpp/scripts/find_plate_pages.py`](/root/aerobag/rust-preprocessor/preprocessor-tpp/scripts/find_plate_pages.py)
  - CLI additions in [`rust-preprocessor/preprocessor-cli/src/main.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs):
    - `run-native-tpp --region ...`
    - `compare-tpp-packages --region ...`
  - Shared region modeling in [`rust-preprocessor/preprocessor-core/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-core/src/lib.rs) now also includes:
    - `Region::from_code(...)`
    - `Region::state_codes()`
  - Important implementation choices:
    - `pdftotext` was considered and rejected because it is not part of the legacy toolchain here.
    - MIN page selection uses a tiny Python helper with `pypdf`, which mirrors the legacy dependency more closely.
    - APD `UserComment` values are copied from the Outer World Apps six-value payload in `avare_aptdiags.php`, joined with `|`.
    - Georeferenced non-APD/non-MIN plate comments use the same legacy formula derived from `gdalinfo` corner DMS coordinates after `gdalwarp` to EPSG:3857.
  - First Rust `tpp-ne` offline parity run showed:
    - `manifest_entries=match`
    - `members=match`
    - `manifest_bytes=mismatch`
  - Root cause was package ordering: Rust directory walk order did not match legacy Python `glob.glob("plates/**/*-STATE-*.png", recursive=True)`.
  - Fixed `tpp` package ordering by enumerating regional PNGs with the same Python glob strategy used by legacy.
  - Clean rerun baseline:
    - Rust work dir [`rust-runs/tpp-ne-native-check/work/tpp-ne`](/root/aerobag/rust-runs/tpp-ne-native-check/work/tpp-ne)
  - Current `tpp-ne` package parity result against legacy retry baseline:
    - legacy [`runs/20260405T154700Z-tpp-retry/work/tpp-ne`](/root/aerobag/runs/20260405T154700Z-tpp-retry/work/tpp-ne)
      vs Rust [`rust-runs/tpp-ne-native-check/work/tpp-ne`](/root/aerobag/rust-runs/tpp-ne-native-check/work/tpp-ne)
      - `NE manifest_bytes=match manifest_entries=match legacy_members=3278 rust_members=3278 members=match`
  - Regression coverage now includes:
    - `tpp_ne_native_packages_match_legacy`
  - Verification:
    - `cargo test -p preprocessor-tpp` passed
    - `cargo test -p preprocessor-cli` passed with 9 tests
  - Remaining `tpp` gaps:
    - this is package parity for `NE` from an offline populated legacy work dir
    - native `tpp` provenance parity is not yet demonstrated
    - broader region coverage is not yet validated
