# Next Session Handoff

## Session Notes

- 2026-04-07 07:05:00Z:
  - Replaced the old Bash full-validation harness logic with a Rust subprocess orchestrator under:
    - [`baseline/avare_equivalent/preprocessor-cli/src/full_validation.rs`](/root/aerobag/baseline/avare_equivalent/preprocessor-cli/src/full_validation.rs)
  - New CLI entrypoint:
    - `cd /root/aerobag/baseline/avare_equivalent && cargo run -q -p preprocessor-cli -- run-full-validation`
  - The old launcher:
    - [`legacy-capture/run_preprocessor_validation.sh`](/root/aerobag/legacy-capture/run_preprocessor_validation.sh)
    - is now just a compatibility shim that executes the Rust entrypoint in `baseline/avare_equivalent`
  - The Rust entrypoint now re-execs itself under a transient systemd cgroup unless already inside one:
    - default memory cap: `35G`
    - swap cap: `0`
    - env override:
      - `FULL_VALIDATION_MEMORY_MAX=<value>`
    - guard env used internally:
      - `FULL_VALIDATION_CGROUP_ACTIVE=1`
  - Practical effect:
    - if Banana runs away, it should die with a memory-related failure inside its own cgroup instead of pushing the whole VM into swap death
  - The Rust orchestrator still shells out to the legacy helpers and native CLI subcommands, but the orchestration/reporting logic is now in Rust.
  - It currently covers:
    - legacy capture
    - native `sec`, `tac`, `enr-l`, `enr-h`
    - native `csup`
    - native `tpp-ne`
    - native database rebuild from legacy extracted FAA inputs
  - Compare phase now includes:
    - chart tile-path/package/provenance/full-image parity
    - `csup` package/provenance/full-image parity
    - `tpp-ne` package/provenance/full-image parity
    - database parity via:
      - [`compare-data-db`](/root/aerobag/baseline/avare_equivalent/preprocessor-cli/src/main.rs)
  - Important correction:
    - the old Bash harness had gone stale after the source-tree refactor because it still targeted `rust-preprocessor/`
    - the new shim routes to `baseline/avare_equivalent/`, which is now the right baseline-equivalence workspace
  - Verification completed:
    - `cargo check -p preprocessor-cli --manifest-path /root/aerobag/baseline/avare_equivalent/Cargo.toml`
    - parser smoke test:
      - `cargo run -q -p preprocessor-cli --manifest-path /root/aerobag/baseline/avare_equivalent/Cargo.toml -- run-full-validation --bogus`
      - expected failure confirms the new Rust entrypoint is wired
    - cgroup tooling availability on host:
      - `systemd-run` present
      - cgroup v2 memory controller present in `/sys/fs/cgroup/cgroup.controllers`
  - Full end-to-end Banana rerun with the Rust orchestrator has not been launched yet in this session.

- 2026-04-07 06:40:00Z:
  - Added a new Rust crate for the legacy aviation database pipeline:
    - [`rust-preprocessor/preprocessor-data`](/root/aerobag/rust-preprocessor/preprocessor-data)
  - Wired new CLI commands in [`preprocessor-cli/src/main.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs):
    - `build-data --input-dir <path> --output-dir <path> --manifest-version <cycle>`
    - `compare-data-db --left-db <path> --right-db <path>`
  - The Rust builder now produces the primary legacy-style database package entirely in Rust:
    - `main.db`
    - `databases`
    - `databases.zip`
  - Implemented Rust parsers for the legacy source set:
    - `APT.txt`
    - `TWR.txt`
    - `NAV.txt`
    - `FIX.txt`
    - `DOF.DAT`
    - `AWOS.txt`
    - `AWY.txt`
    - `FAACIFP18`
    - `geo.csv`
    - SAA XML inputs
  - Important legacy-compatibility quirks now mirrored explicitly in Rust:
    - CIFP fixed-width fields preserve embedded spaces exactly like [`cifp.py`](/root/aerobag/avare-source/data/cifp.py)
    - `nav` / `fix` names preserve legacy trailing spaces where the Perl scripts leave them
    - AWOS missing coordinates stay empty rather than becoming `0.0`
    - runway coordinate text columns are compared by normalized numeric value, because legacy stored Perl float strings as text
    - SAA parsing is now SAX-style to match the old Perl handler behavior, including its lossy “last text chunk wins” handling for note fields
  - Built Rust output against the captured legacy FAA input dir:
    - input/reference dir:
      - [`runs/20260407T053200Z-data-build/work/data`](/root/aerobag/runs/20260407T053200Z-data-build/work/data)
    - Rust output dir:
      - [`rust-runs/data-native-check`](/root/aerobag/rust-runs/data-native-check)
  - The normalized database comparison is now fully green:
    - `cargo run -q -p preprocessor-cli -- compare-data-db --left-db /root/aerobag/runs/20260407T053200Z-data-build/work/data/main.db --right-db /root/aerobag/rust-runs/data-native-check/main.db`
    - result: `status match`
  - Added a dedicated integration harness in:
    - [`preprocessor-cli/tests/data_parity.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/tests/data_parity.rs)
  - Verification command:
    - `cd /root/aerobag/rust-preprocessor && cargo test -p preprocessor-cli --test data_parity`
    - result: passing
  - Current status:
    - Rust now covers the primary `databases.zip` / `main.db` build path
    - `databasesx` is still untranslated and still depends on optional vector-tile tooling (`tippecanoe`, `tile-join`)

- 2026-04-07 05:45:00Z:
  - Investigated the legacy aviation database build path in [`avare-source/data`](/root/aerobag/avare-source/data).
  - Confirmed the legacy builder exists and is the real producer of Avare's machine-readable database package:
    - entrypoint [`data.py`](/root/aerobag/avare-source/data/data.py)
    - orchestration/helpers [`common.py`](/root/aerobag/avare-source/data/common.py)
    - CIFP parser [`cifp.py`](/root/aerobag/avare-source/data/cifp.py)
    - schema import scripts:
      - [`legacy/importother.sql`](/root/aerobag/avare-source/data/legacy/importother.sql)
      - [`x/importother.sql`](/root/aerobag/avare-source/data/x/importother.sql)
  - Built a clean homegrown legacy database package in scratch run dir:
    - [`runs/20260407T053200Z-data-build/work/data/main.db`](/root/aerobag/runs/20260407T053200Z-data-build/work/data/main.db)
    - [`runs/20260407T053200Z-data-build/work/data/databases.zip`](/root/aerobag/runs/20260407T053200Z-data-build/work/data/databases.zip)
    - manifest [`runs/20260407T053200Z-data-build/work/data/databases`](/root/aerobag/runs/20260407T053200Z-data-build/work/data/databases)
  - The resulting `main.db` contains the expected tables:
    - `airports`
    - `airportfreq`
    - `airportrunways`
    - `nav`
    - `fix`
    - `obs`
    - `awos`
    - `saa`
    - `airways`
    - `cifp_sid_star_app`
    - `geo`
  - This confirmed that the Android app's current DB contract is still the plain `databases` package with `main.db`:
    - [`arrays.xml`](/root/aerobag/avare-source/avare/app/src/main/res/values/arrays.xml) only advertises `databases`
    - [`LocationDatabaseHelper.java`](/root/aerobag/avare-source/avare/app/src/main/java/com/ds/avare/content/LocationDatabaseHelper.java) opens `main.db`
  - Investigated `databasesx`:
    - it is built from the alternate `x/` parser set plus optional vector airspace MBTiles
    - the schema adds richer IDs such as `DLID` / `FaaID`
    - if present, the package can include `maps/nasr.mbtiles`
  - Important finding:
    - current Avare app code does not appear to reference:
      - `databasesx`
      - `maps/nasr.mbtiles`
      - `DLID`
      - `FaaID`
    - so `databasesx` looks like experimental / future-facing work rather than an active app contract
  - The optional `databasesx.zip` did not build in this environment because [`generate_airspace_tiles.sh`](/root/aerobag/avare-source/data/generate_airspace_tiles.sh) requires `tippecanoe` and `tile-join`, which are not installed here.
  - Current Rust status for `data/`:
    - no Rust replacement crate exists yet
    - current Rust workspace still covers charts / `csup` / `tpp`, not the aviation database pipeline

- 2026-04-07 05:50:00Z:
  - There were still uncommitted harness changes to integrate `charts-enr-h` into whole-banana:
    - [`legacy-capture/capture_inside_container.sh`](/root/aerobag/legacy-capture/capture_inside_container.sh)
    - [`legacy-capture/emit_source_urls.py`](/root/aerobag/legacy-capture/emit_source_urls.py)
    - [`legacy-capture/finalize_run.py`](/root/aerobag/legacy-capture/finalize_run.py)
    - [`legacy-capture/run_preprocessor_validation.sh`](/root/aerobag/legacy-capture/run_preprocessor_validation.sh)
    - [`legacy-capture/run_status.py`](/root/aerobag/legacy-capture/run_status.py)
  - Those changes add:
    - legacy `enr_h.py` execution
    - source URL emission for `charts-enr-h`
    - finalize/status metadata for `charts-enr-h`
    - native `enr-h` run plus tile-path/package/provenance/full-image compare in the harness
  - Verification had already passed for those edits:
    - shell syntax checks
    - Python compile checks
    - `cargo test -p preprocessor-cli`
  - The previously green whole-banana run [`20260406T051014Z-validation`](/root/aerobag/runs/20260406T051014Z-validation) still does not include `enr-h`; the next fresh banana after these harness edits will.

- 2026-04-06 00:46:00Z:
  - Investigated adding `WAC` while the validation harness run was in flight.
  - Important finding: `WAC` is not present in the split backend repo [avare-source/charts](/root/aerobag/avare-source/charts).
  - The current Rust `ChartFamily` enum and CLI only model the split FAA chart families:
    - `sec`
    - `tac`
    - `enr-l`
  - WAC lives on the older `avare/extra` side of the legacy system, alongside `ONC` and `TPC`, not in the split `charts` crawler/tiler flow.
  - Confirmed app-side WAC contract exists in:
    - [arrays.xml](/root/aerobag/avare-source/avare/app/src/main/res/values/arrays.xml)
      - `resNameWAC`
      - `resFilesWAC`
    - [Boundaries.java](/root/aerobag/avare-source/avare/app/src/main/java/com/ds/avare/place/Boundaries.java)
      - polygon bounds for sheets such as `CC-8`, `CF-16`, `CJ-27`
  - Confirmed the old manual charting scripts exist in:
    - [gtag.py](/root/aerobag/avare-source/avare/extra/charting/gtag.py)
    - [gtag_tmerc.py](/root/aerobag/avare-source/avare/extra/charting/gtag_tmerc.py)
    - [zip.pl](/root/aerobag/avare-source/avare/extra/charting/zip.pl)
  - Those scripts are a different pipeline shape from sectional/TAC/ENR_L:
    - hand-authored georeference text files
    - local source imagery
    - manual chart mosaicing
    - then `gdal2tiles.py` and zip packaging
  - Critical blocker: no WAC-specific source imagery or georeference/input files were found in the workspace.
    - Searches for sheet IDs like `CC-8`, `CF-16`, and `CJ-27` only found the app contract and bounds, not build inputs.
    - The only WAC-named artifact found was app asset [chart_wac.png](/root/aerobag/avare-source/avare/app/src/main/assets/chart_wac.png).
  - Practical implication:
    - WAC is not “easy like sectional” in the current repo layout.
    - Adding it correctly requires first locating the real legacy WAC source-of-truth inputs or deciding on a new explicit input model for these manual-chart families.

- 2026-04-06 01:03:00Z:
  - Added a lightweight visual-equivalence command to the Rust CLI:
    - [`compare-sampled-images`](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs)
  - The command:
    - recursively finds shared image paths under two roots,
    - selects a deterministic hash-based sample,
    - runs ImageMagick `compare -metric RMSE` on each sampled pair,
    - reports left-only/right-only paths and sampled mismatches.
  - Supported image extensions currently include:
    - `png`
    - `jpg`
    - `jpeg`
    - `tif`
    - `tiff`
    - `webp`
  - Added a regression test in:
    - [`chart_parity.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/tests/chart_parity.rs)
  - `cargo test -p preprocessor-cli` now passes with 10 tests.
  - Initial calibration run on sectional tile trees:
    - left: [`runs/20260405T154700Z/work/charts-sec/tiles/0`](/root/aerobag/runs/20260405T154700Z/work/charts-sec/tiles/0)
    - right: [`rust-runs/sec-clean-check/work/charts-sec/tiles/0`](/root/aerobag/rust-runs/sec-clean-check/work/charts-sec/tiles/0)
    - sample: `1%`, capped to `20`
  - Result with `--rmse-threshold 0.0`:
    - one mismatch at `0/0/0.webp`
    - `rmse=0.01221550`
  - Result with `--rmse-threshold 0.02`:
    - sample passes
  - Practical implication:
    - the visual-diff machinery works,
    - threshold policy still needs an explicit decision before folding this into the whole-banana harness as a required gate.

- 2026-04-06 01:16:00Z:
  - Investigated the fresh whole-banana native chart summary lines that reported:
    - sectional `tile_count 35490`
    - TAC `tile_count 7170`
  - Actual fresh-run parity checks showed no chart regression:
    - sectional tile paths match exactly between fresh legacy and native outputs
    - TAC tile paths match exactly between fresh legacy and native outputs
    - sectional and TAC package parity both match exactly
  - Verified actual native on-disk counts are the expected full tile-tree counts:
    - sectional `35494`
    - TAC `7174`
  - Root cause:
    - [`build_tiles_from_spec`](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs) was reporting only `.webp` payload count via `count_tile_webps()`
    - parity tooling counts the full tile tree, including:
      - `googlemaps.html`
      - `leaflet.html`
      - `openlayers.html`
      - `tilemapresource.xml`
    - that explains the consistent `-4`
  - Fix:
    - native chart summary now uses full tile-tree count via [`count_tile_files()`](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs)
  - Verification:
    - `cargo test -p preprocessor-cli` passes with 10 tests
  - Practical implication:
    - the earlier fresh whole-banana chart-count discrepancy was only a reporting bug, not an output mismatch.

- 2026-04-06 01:45:00Z:
  - Wired `enr-h` through the shared Rust chart-family path.
  - Updated:
    - [`preprocessor-core/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-core/src/lib.rs)
    - [`preprocessor-charts/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs)
    - [`preprocessor-cli/src/main.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs)
  - `ChartFamily` now includes `EnrH` with:
    - capture label `charts-enr-h`
    - script `enr_h.py`
    - chart dir `ENR_H`
    - tile index `4`
    - max zoom `9`
    - IFR VRT path
  - The Android contract already contains the expected package names:
    - `AK_ENR_H`, `PAC_ENR_H`, `NW_ENR_H`, `SW_ENR_H`, `NC_ENR_H`, `EC_ENR_H`, `SC_ENR_H`, `NE_ENR_H`, `SE_ENR_H`
    - in [`arrays.xml`](/root/aerobag/avare-source/avare/app/src/main/res/values/arrays.xml)
  - Baseline tile count for `enr-h` is not recorded yet, so:
    - `print-baseline` prints `ENR_H unknown`
    - `compare-tile-counts` reports `expected=unknown` for `charts-enr-h`
  - Verification:
    - `cargo test -p preprocessor-cli` passes
    - `cargo run -q -p preprocessor-cli -- explain-chart --family enr-h --cpus 4` works
  - Next obvious `enr-h` step:
    - produce a legacy capture slice and run the same parity/provenance/visual workflow used for `enr-l`.

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

- 2026-04-06T00:10:16Z:
  - Started the next major track: turning chart validation into a repeatable cold-start harness.
  - Decided against making an HTTPS caching web proxy the primary cache mechanism.
  - Reason:
    - a normal forward proxy does not reliably give reusable body caching for all `https://` downloads without TLS interception/MITM and local CA trust plumbing across Python, `curl`, and the legacy tooling.
    - the better primary mechanism here is a shared application-level fetch cache used by both legacy and Rust.
  - Implemented a shared fetch cache contract on both sides:
    - `FETCH_CACHE_ROOT`
    - `FETCH_CACHE_MODE=fill|offline`
  - Legacy side:
    - patched centralized `download()` helpers in:
      - [`avare-source/charts/common.py`](/root/aerobag/avare-source/charts/common.py)
      - [`avare-source/csup/common.py`](/root/aerobag/avare-source/csup/common.py)
      - [`avare-source/tpp/common.py`](/root/aerobag/avare-source/tpp/common.py)
    - behavior:
      - if file already exists in work dir, provenance records `source=local`
      - else if a shared cached blob exists, copy it in and record `source=cache`
      - else in `fill` mode download from network, store into cache, record `source=network`
      - else in `offline` mode fail fast on cache miss
    - cache layout used by legacy:
      - `${FETCH_CACHE_ROOT}/blobs/<sha256>`
      - `${FETCH_CACHE_ROOT}/http/<sha256(url)>.json`
  - Legacy wrapper:
    - [`legacy-capture/capture_inside_container.sh`](/root/aerobag/legacy-capture/capture_inside_container.sh) now exports:
      - `FETCH_CACHE_ROOT="${CACHE_ROOT}/fetch"` by default
      - `FETCH_CACHE_MODE=fill` by default
    - and passes both through to each capture job
  - Rust side:
    - [`rust-preprocessor/preprocessor-fetch/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-fetch/src/lib.rs) now uses the same cache contract before running `curl`
    - provenance download rows now also record `source=local|cache|network`
    - Rust `offline` mode also fails fast on cache miss
  - Verification:
    - `python3 -m py_compile` passed for the three patched legacy `common.py` files
    - `cargo test -p preprocessor-cli` passed with 9 tests after the Rust cache-layer changes
  - Audit of how invasive the legacy changes are:
    - top-level wrapper patch is tiny:
      - [`legacy-capture/capture_inside_container.sh`](/root/aerobag/legacy-capture/capture_inside_container.sh) changed by only 5 insertions and 1 replacement
    - repo-local legacy patches are now materially larger:
      - charts `common.py`: `131 insertions, 3 deletions`
      - csup `common.py`: `128 insertions, 4 deletions`
      - tpp `common.py`: `128 insertions, 4 deletions`
    - important nuance:
      - those totals include both the earlier provenance instrumentation and the new shared-cache logic
      - they are still concentrated in the centralized `download()` helper path, which is the least-bad place to touch legacy
    - interpretation:
      - legacy should now be treated as “legacy plus observability/cache shim”, not pristine upstream gold
  - Current status:
    - shared cache logic is implemented
    - single-command orchestrator is not implemented yet
    - no end-to-end proof run of `fill` then `offline` replay has been executed yet
  - Resume suggestion:
    1. build the orchestrator script/command for cold-start validation
    2. point both legacy and native at one `FETCH_CACHE_ROOT`
    3. run one `fill` pass
    4. rerun in `offline` mode and require cache misses to fail
  - Workspace note:
    - there is an untracked top-level `resumes` file at the moment; I did not touch it or include it in any commit

- 2026-04-06T00:30:42Z:
  - Began implementing the actual fresh-checkout validation harness.
  - Added tracked legacy patch files under [`legacy-capture/patches`](/root/aerobag/legacy-capture/patches) for the currently required shim set:
    - base provenance/cache patch per repo:
      - `charts-common.patch`
      - `csup-common.patch`
      - `tpp-common.patch`
    - follow-up crawl-page cache patch per repo:
      - `charts-crawl-cache.patch`
      - `csup-crawl-cache.patch`
      - `tpp-crawl-cache.patch`
  - Added an idempotent legacy hydrate script:
    - [`legacy-capture/hydrate_legacy_sources.sh`](/root/aerobag/legacy-capture/hydrate_legacy_sources.sh)
  - Current hydrate behavior:
    - clones `charts`, `tpp`, and `csup` into `avare-source/` if missing
    - applies the tracked shim patches if their marker text is absent
    - treats already-patched repos as success
  - Important correction:
    - initial `git apply --reverse --check` idempotence logic was too strict once a repo had later follow-up edits
    - hydrate now uses marker-based idempotence instead
    - verified by rerunning hydrate against the already-patched local repos; it now succeeds cleanly
  - Added a source-URL prep helper:
    - [`legacy-capture/emit_source_urls.py`](/root/aerobag/legacy-capture/emit_source_urls.py)
  - Current source-URL prep behavior:
    - emits `source_urls.jsonl` files for:
      - `charts-sec`
      - `charts-tac`
      - `charts-enr-l`
      - `csup`
      - `tpp-ne`
    - uses the same cycle logic as the legacy repos
    - now also honors `FETCH_CACHE_ROOT` / `FETCH_CACHE_MODE` for crawl-page HTML, so offline replay is not blocked on the FAA index pages
  - Live legacy repos were also updated so `list_crawl(...)` uses the shared fetch cache for the crawl-page HTML, not just archive downloads.
  - Added the first single-command orchestrator:
    - [`legacy-capture/run_preprocessor_validation.sh`](/root/aerobag/legacy-capture/run_preprocessor_validation.sh)
  - Current orchestrator behavior:
    - hydrates legacy sources
    - emits source-url files
    - builds `preprocessor-cli`
    - launches:
      - the legacy representative capture runner
      - native `sec`, `tac`, `enr-l`, `csup`, and `tpp-ne`
      - all against the same `FETCH_CACHE_ROOT`
    - waits for completion
    - runs comparison commands into `compare/*.txt`
    - writes a combined `compare/summary.txt`
  - Current limitations:
    - I have not yet executed a full end-to-end run of `run_preprocessor_validation.sh`
    - so the orchestrator is syntax-checked and component-checked, but not yet operationally proven
    - CPU budgeting is intentionally simple right now; it is not yet tuned
  - Additional detail:
    - native `tpp` now appends the Outer World Apps airport-diagram URL internally during prefetch, so native `tpp-ne` download provenance can match legacy while leaving `source_urls.jsonl` aligned with the legacy `list_crawl` contract
  - Verification completed:
    - `bash -n` passed for:
      - [`legacy-capture/hydrate_legacy_sources.sh`](/root/aerobag/legacy-capture/hydrate_legacy_sources.sh)
      - [`legacy-capture/run_legacy_capture_direct.sh`](/root/aerobag/legacy-capture/run_legacy_capture_direct.sh)
      - [`legacy-capture/run_preprocessor_validation.sh`](/root/aerobag/legacy-capture/run_preprocessor_validation.sh)
    - `python3 -m py_compile` passed for:
      - [`legacy-capture/emit_source_urls.py`](/root/aerobag/legacy-capture/emit_source_urls.py)
      - the three patched legacy `common.py` files
    - `cargo test -p preprocessor-cli` still passed with 9 tests after the native `tpp` prefetch tweak
    - rerunning [`legacy-capture/hydrate_legacy_sources.sh`](/root/aerobag/legacy-capture/hydrate_legacy_sources.sh) succeeded on the current machine
  - Workspace note:
    - unrelated user changes were present in `notes` and `UI_DEPENDENCIES.md`; do not include them in any commit for this harness work
    - there is also an untracked top-level `.codex` path; leave it alone
  - Next exact step:
    1. run `legacy-capture/run_preprocessor_validation.sh` once in `FETCH_CACHE_MODE=fill`
    2. inspect failures and fix harness bugs
    3. rerun in `FETCH_CACHE_MODE=offline`
    4. only after that decide whether to commit the harness as “working”
## 2026-04-06 gdal2tiles / TAC debug

- Copied the installed `gdal2tiles.py` implementation to `/tmp/gdal2tiles.py` and inspected the multiprocessing path.
- Important source finding: overview tiles are always generated from already-written child image files, and `--resume` skips any tile or metadata file that already exists. This makes reruns into a non-clean tree unsafe for parity/debugging.
- Native chart tiling was tightened in [`rust-preprocessor/preprocessor-charts/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs):
  - we already removed the per-family tile root before `gdal2tiles.py`
  - now we also no longer pass `--resume` from Rust
- Sectional conclusion stands: matching legacy `gdal2tiles.py --processes 8` eliminated the previously observed overlap/seam glitches.
- TAC initially still looked broken after a clean no-`--resume` retile, but that turned out to be a stale-input mistake:
  - I had rebuilt tiles only
  - the native [`TAC.vrt`](/root/aerobag/runs/20260406T003224Z-validation/native/charts-tac/work/charts-tac/TAC.vrt) was still the old pre-fix alphabetized artifact
- After rebuilding TAC VRTs and then retiling with `--processes 8`:
  - native [`TAC.vrt`](/root/aerobag/runs/20260406T003224Z-validation/native/charts-tac/work/charts-tac/TAC.vrt) is byte-identical to legacy
  - previously worst tiles now match exactly:
    - `11/592/1269.webp` RMSE `0`
    - `11/592/1270.webp` RMSE `0`
    - `11/592/1268.webp` RMSE `0`
  - full `100%` TAC visual compare at `RMSE=0.0` passed:
    - `visual status=match sampled=7170 mismatches=0 left_only=0 right_only=0`

## 2026-04-06 ENR_L / ENR_H visual parity cleanup

- The remaining chart-family cleanup was to apply the same clean-run discipline to `enr-l` and `enr-h`.
- Rust chart input enumeration in [`rust-preprocessor/preprocessor-charts/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs) now shells out to Python `glob.glob("*.geojson", root_dir=...)` so input discovery matches legacy behavior instead of relying on `fs::read_dir()`.
- Rust now also encodes the family-specific legacy ordering quirks:
  - `ENR_L`: sort ascending before main VRT assembly, matching [`avare-source/charts/enr_l.py`](/root/aerobag/avare-source/charts/enr_l.py)
  - `ENR_H`: sort descending before main VRT assembly, matching [`avare-source/charts/enr_h.py`](/root/aerobag/avare-source/charts/enr_h.py)
- After rebuilding:
  - native [`ENR_H.vrt`](/root/aerobag/rust-runs/enr-h-native-from-legacy/work/charts-enr-h/ENR_H.vrt) is byte-identical to legacy slice [`ENR_H.vrt`](/root/aerobag/runs/20260406T021449Z-enr-h-slice/work/charts-enr-h/ENR_H.vrt)
  - native [`ENR_L.vrt`](/root/aerobag/runs/20260406T003224Z-validation/native/charts-enr-l/work/charts-enr-l/ENR_L.vrt) is byte-identical to legacy
- Full `100%` image compares at `RMSE=0.0` now pass for both:
  - `ENR_H`: `visual status=match sampled=20619 mismatches=0 left_only=0 right_only=0`
  - `ENR_L`: `visual status=match sampled=27424 mismatches=0 left_only=0 right_only=0`
- Current chart-family certification state:
  - `SEC`: full visual parity clean after matching `--processes 8`
  - `TAC`: full visual parity clean after rebuilding corrected VRTs and retiling with `--processes 8`
  - `ENR_L`: full visual parity clean
  - `ENR_H`: full visual parity clean on slice baseline

## 2026-04-06 CSUP / TPP image comparator

- Added dedicated image-compare commands in [`rust-preprocessor/preprocessor-cli/src/main.rs`](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs):
  - `compare-csup-images`
  - `compare-tpp-images`
- These compare packaged PNG artifacts named by manifests, instead of walking arbitrary work-dir images:
  - `CSUP`: union of the 9 `*_CSUP` manifest entries
  - `TPP`: the region `*_TPP` manifest entries
- This avoids false positives from staging-layout differences and ignores un-packaged intermediates.
- Validation on the existing banana split [`20260406T003224Z-validation`](/root/aerobag/runs/20260406T003224Z-validation):
  - `TPP-NE` full `100%` compare at `RMSE=0.0` passed:
    - `visual status=match sampled=3277 mismatches=0 left_only=0 right_only=0`
  - `CSUP` full `100%` compare at `RMSE=0.0` found exactly 2 mismatches:
    - `afd/TVR/CSUP-SC_0.png` `rmse=0.25927900`
    - `afd/PUW/CSUP-NW_0.png` `rmse=0.25061900`
    - summary: `visual status=mismatch sampled=6022 mismatches=2 left_only=0 right_only=0`

## 2026-04-06 CSUP duplicate-airport overwrite bug

- The two remaining `CSUP` visual mismatches were traced to duplicate FAA XML `<airport>` records with the same `aptid` but different single-PDF refs:
  - `TVR`: `sc_137_19MAR2026.pdf` and `sc_186_19MAR2026.pdf`
  - `PUW`: `nw_80_19MAR2026.pdf` and `nw_260_19MAR2026.pdf`
- Legacy behavior is not “first one wins.” It renders both in XML order to the same `afd/<APT>/CSUP-<REGION>_0.png` path, so the later duplicate silently overwrites the earlier one.
- Native `csup` had been skipping preexisting `CSUP-..._<index>.png` outputs, which preserved the earlier duplicate instead of matching legacy.
- Fixed in [`rust-preprocessor/preprocessor-csup/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-csup/src/lib.rs):
  - later duplicate airports now overwrite earlier outputs for the same `CSUP-<REGION>_<index>` base
  - added explicit compatibility comments explaining why this surprising behavior is preserved
- Also added explicit compatibility comments in [`rust-preprocessor/preprocessor-charts/src/lib.rs`](/root/aerobag/rust-preprocessor/preprocessor-charts/src/lib.rs) for:
  - Python `glob`-based chart input discovery
  - `ENR_L` / `ENR_H` family-specific ordering quirks
  - clean tile-tree rebuilds and legacy-matching chart `--processes 8`
- Manual confirmation of the `CSUP` overwrite rule:
  - rendering `SC_137` then `SC_186` into the same output path reproduces legacy `afd/TVR/CSUP-SC_0.png` exactly (`RMSE 0`)
  - rendering `NW_80` then `NW_260` into the same output path reproduces legacy `afd/PUW/CSUP-NW_0.png` exactly (`RMSE 0`)
- The long in-place native `csup` rerun against [`20260406T003224Z-validation`](/root/aerobag/runs/20260406T003224Z-validation) was still awkward to verify live, but the compatibility rule itself is now established and encoded.

## 2026-04-06 Unified certification harness

- The top-level harness in [`legacy-capture/run_preprocessor_validation.sh`](/root/aerobag/legacy-capture/run_preprocessor_validation.sh) now bundles the full parity suite, not just structure/provenance:
  - chart tile-path parity
  - package / manifest parity
  - provenance parity
  - full `100%` image compare at `RMSE=0.0` for:
    - `sec`
    - `tac`
    - `enr-l`
    - `csup`
    - `tpp-ne`
- Harness defaults now pin native chart tiling to legacy-matching `--processes 8`.
- New CLI commands used by the harness:
  - [`compare-csup-images`](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs)
  - [`compare-tpp-images`](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs)
- These image compares operate on manifest-defined packaged PNGs, not arbitrary work-dir contents.

## 2026-04-06 Fresh whole-banana certification run

- A third fresh whole-banana run was launched after the unified harness update:
  - [`20260406T051014Z-validation`](/root/aerobag/runs/20260406T051014Z-validation)
- This is the first run that should certify the full current stack in one pass:
  - charts with the ordering/process-count fixes
  - native `csup` from the current source tree
  - native `tpp-ne`
  - bundled parity checks in the compare phase
- Earlier runs remain useful references:
  - [`20260406T003224Z-validation`](/root/aerobag/runs/20260406T003224Z-validation): original banana split used for diagnosis
  - [`20260406T032350Z-validation`](/root/aerobag/runs/20260406T032350Z-validation): fresh whole-banana before the `csup` duplicate-airport fix
- At handoff time, [`20260406T051014Z-validation`](/root/aerobag/runs/20260406T051014Z-validation) is still in flight.

## 2026-04-07 Resource-Index checkpoint

- A new Rust crate now exists at:
  - [rust-preprocessor/preprocessor-resource-index](/root/aerobag/rust-preprocessor/preprocessor-resource-index)
- A new CLI command exists in:
  - [rust-preprocessor/preprocessor-cli/src/main.rs](/root/aerobag/rust-preprocessor/preprocessor-cli/src/main.rs)
  - command:
    - `build-resource-index`

- The command now emits a real top-level resource index from:
  - `runs/20260407T053200Z-data-build/work/data/databases.zip`
  - chart package outputs for:
    - `sectional`
    - `tac`
    - `ifr_low`
    - `ifr_high`
  - TPP package outputs / assets
  - CSUP package outputs / assets

- Latest verified real output:
  - [rust-runs/resource-index/resource-index.json](/root/aerobag/rust-runs/resource-index/resource-index.json)
  - contains:
    - `cycle = 2604`
    - `packages = 46`
    - `chart_collections = 36`
    - `airports = 19445`
    - `plates = 4749`
    - `csups = 6022`

- The resource index currently includes:
  - package records
  - airport records from `main.db`
  - plate records
  - CSUP records
  - chart collection records with:
    - package name
    - chart index
    - tile path template
    - zoom-level bounds
    - derived coverage bounds
    - derived default view

- Tests / verification last known good:
  - `cd /root/aerobag/rust-preprocessor && cargo test -p preprocessor-resource-index`
  - `cd /root/aerobag/rust-preprocessor && cargo check -p preprocessor-cli`
  - both passed

- Architectural decision from the user:
  - preserve the current Rust preprocessor as the long-lived Avare-equivalent baseline
  - start a new evolving product preprocessor separately
  - keep the baseline source tree around, not just a git tag

- Next requested repo step:
  1. rename `rust-preprocessor` to `baseline/avare_equivalent`
  2. create `product/preprocessor` by copying from the baseline
  3. fold `resource-index` into the product pipeline as first-class output
  4. then make intentional product changes there, beginning with airport-id canonicalization

## 2026-04-07 Repo Refactor Completed

- The requested split has now been performed on disk:
  - baseline reference pipeline:
    - [baseline/avare_equivalent](/root/aerobag/baseline/avare_equivalent)
  - evolving product pipeline:
    - [product/preprocessor](/root/aerobag/product/preprocessor)
- Historical notes above still reference `rust-preprocessor` because that was the path at the time. Read those as pre-refactor history.
- Current policy:
  - keep [baseline/avare_equivalent](/root/aerobag/baseline/avare_equivalent) Avare-equivalent
  - land intentional product changes only in [product/preprocessor](/root/aerobag/product/preprocessor)
- `preprocessor-resource-index` has been copied into the product pipeline and should now be treated as a first-class preprocessing output there.
- The next product-only change requested by the user is airport-id canonicalization in:
  - [product/preprocessor/preprocessor-data/src/lib.rs](/root/aerobag/product/preprocessor/preprocessor-data/src/lib.rs)
  - use ICAO when present
  - otherwise keep the FAA/local id as-is
  - leave the baseline pipeline unchanged

## 2026-04-08 Full Banana Orchestrator + Data Integration

- Full Banana is now launched from the baseline Rust workspace, not Bash:
  - `cd /root/aerobag/baseline/avare_equivalent && cargo run -q -p preprocessor-cli -- run-full-validation`
- The old shell entrypoint is now just a thin compatibility wrapper:
  - [legacy-capture/run_preprocessor_validation.sh](/root/aerobag/legacy-capture/run_preprocessor_validation.sh)

- The Rust orchestrator lives in:
  - [baseline/avare_equivalent/preprocessor-cli/src/full_validation.rs](/root/aerobag/baseline/avare_equivalent/preprocessor-cli/src/full_validation.rs)
- Important current orchestrator behavior:
  - self-reexec under `systemd-run`
  - default memory cap `MemoryMax=80G`
  - `MemorySwapMax=0`
  - default heavy-job throttle `4`
  - override via `--max-heavy-jobs <n>`
  - master progress log:
    - `runs/<run-id>-validation/orchestrator-logs/master.log`
  - master log now ends with an explicit terminal line:
    - `complete PASS`
    - or `complete FAIL error=...`

- The baseline `tpp` helper path bug from the repo refactor is fixed in:
  - [baseline/avare_equivalent/preprocessor-tpp/src/lib.rs](/root/aerobag/baseline/avare_equivalent/preprocessor-tpp/src/lib.rs)
- Root cause:
  - `find_plate_pages.py` was still being looked up through the old `rust-preprocessor/...` path during Banana runs
- Fix:
  - helper lookup now prefers the crate-local script and falls back across known workspace layouts
  - this keeps parity reruns from becoming impure due to repo layout drift

- `tpp-nw` is now included in Banana end to end:
  - legacy capture
  - source URL emission
  - native run
  - package/provenance/image comparisons
- Touched files:
  - [legacy-capture/emit_source_urls.py](/root/aerobag/legacy-capture/emit_source_urls.py)
  - [legacy-capture/capture_inside_container.sh](/root/aerobag/legacy-capture/capture_inside_container.sh)
  - [legacy-capture/finalize_run.py](/root/aerobag/legacy-capture/finalize_run.py)
  - [legacy-capture/run_status.py](/root/aerobag/legacy-capture/run_status.py)
  - [baseline/avare_equivalent/preprocessor-cli/src/full_validation.rs](/root/aerobag/baseline/avare_equivalent/preprocessor-cli/src/full_validation.rs)

- Legacy `data` is now included in Banana as the app-relevant primary build only:
  - build `databases.zip` / `main.db`
  - do not attempt `databasesx`
  - do not require `tippecanoe`
- New helper:
  - [legacy-capture/run_legacy_data_primary.py](/root/aerobag/legacy-capture/run_legacy_data_primary.py)
- Legacy capture now runs:
  - `python3 legacy-capture/run_legacy_data_primary.py`
  - from the staged legacy `work/data` directory

- First Banana with legacy `data` failed, but for a harness bug, not a parity bug:
  - run:
    - [20260408T062148Z-validation](/root/aerobag/runs/20260408T062148Z-validation)
  - failure line in master log:
    - `complete FAIL error=validation job legacy failed with exit code 1`
  - exact failing log:
    - [data.stderr.log](/root/aerobag/runs/20260408T062148Z-validation/legacy/logs/data.stderr.log)
  - root cause:
    - `ModuleNotFoundError: No module named 'common'`
  - fix:
    - prepend `os.getcwd()` to `sys.path` in [run_legacy_data_primary.py](/root/aerobag/legacy-capture/run_legacy_data_primary.py)
    - this makes the helper behave like `python3 data.py` from inside staged `work/data`

- The legacy `data` import-path fix was incrementally verified before spending another 2-hour Banana:
  - cold scratch test:
    - got past the old import failure and only died on DNS/network
  - seeded offline-style test:
    - copied from [20260407T053200Z-data-build/work/data](/root/aerobag/runs/20260407T053200Z-data-build/work/data)
    - reran with `FETCH_CACHE_MODE=offline`
    - completed successfully through:
      - `Downloading/unzipping: 100%`
      - `Running PERL database files: 100%`
      - `Cycle to be put in manifest is 2604`

- Current open resume point:
  1. launch a fresh Banana after the legacy `data` import fix
  2. wait for `master.log` terminal `complete PASS` / `complete FAIL ...`
  3. if green, repoint the lightweight fixture tests away from old historical run dirs and start deleting superseded `runs/` directories

- Important nuance:
  - baseline `cargo test -p preprocessor-cli` is currently not a trustworthy certification signal because several fixture-style tests still point at stale refactor-broken paths like `/root/aerobag/baseline/runs/...`
  - Banana is the real certification path; fixture tests still need a cleanup pass later
