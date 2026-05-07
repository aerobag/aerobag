# Dead Code And Feature Plumbing Audit

Date: 2026-05-07

Scope:
- `ui-core`: `ui/core-rust/crates/app-core`, plus exported FFI/WASM surface when it exposes core plumbing.
- `ui-web`: `ui/web-app/src`.
- `ui-android`: `ui/android-app/app/src/main/java`.
- `preprocessor`: `product/preprocessor`.

Checks run:
- `cargo check -p app-core` from `ui/core-rust`: clean.
- `cargo check` from `product/preprocessor`: clean.
- `npm install` from `ui/web-app`: installed local web dependencies.
- `./node_modules/.bin/tsc --noEmit` from `ui/web-app`: clean.
- Text scan for `legacy`, `compat`, `placeholder`, `debug`, `stub`, `TODO`, and obvious feature names.

This means most findings below are not compiler-confirmed dead code. They are feature-plumbing candidates: compatibility shims, stale UI scaffolding, debug leftovers, or duplicated code paths that should be removed if we agree the feature contract has moved on.

## ui-core

### Removed legacy flight-plan mutation API

Evidence:
- `ui/core-rust/crates/app-core/src/lib.rs` has `remove_flight_plan_leg` and `move_flight_plan_waypoint`, both implemented only as `UnsupportedOperation` errors.
- `ui/core-rust/crates/app-ffi/src/lib.rs` still exposes `remove_flight_plan_leg_json` and JNI `NativeBindings_removeFlightPlanLegJson`.
- `ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/NativeAppCoreAdapter.kt` still has `removeFlightPlanLeg`, but app usage appears to have moved to session/row actions.

Why this smells:
- The app now uses structured route-component mutations and session-owned state.
- Keeping public functions whose only behavior is "this old API is dead" keeps dead UI paths alive through FFI.

Proposed action:
- Remove the core functions, FFI wrapper, JNI declaration, and Android adapter method in one pass.
- If any call sites fail, route them to `perform_flight_plan_row_action_in_session` or the structured component APIs rather than keeping the old shim.

Status:
- Removed on 2026-05-07.

### Removed `ResolvedLegSource::LegacyPlanLeg`

Evidence:
- `ui/core-rust/crates/app-core/src/planning.rs` still defines `ResolvedLegSource::LegacyPlanLeg`.
- `ui/core-rust/crates/app-core/src/session.rs` still converts it to pointer keys like `legacy:{leg_index}:from`.
- `ui/web-app/src/domain/types.ts` and `ui/android-app/.../WireModels.kt` mirror the variant.
- Search shows no obvious producer outside compatibility/deserialization paths.

Why this smells:
- Route components and synthetic bridges are now the real model.
- A legacy source variant crossing every UI boundary is exactly the kind of enum mirror that causes drift.

Proposed action:
- Confirm there is no current producer in normal route projection.
- If only old serialized snapshots need it, either delete it or deserialize it into a current structured source at the boundary.
- Then remove the web/Android mirrors.

Status:
- Removed on 2026-05-07.

### Quarantined `debug_element_sources` / `debug_element_roles` out of the UI contract

Evidence:
- `ui/core-rust/crates/app-core/src/planning.rs` includes `LegDisplayPath.debug_element_sources` and `debug_element_roles`.
- The fields are mirrored in `ui/web-app/src/domain/types.ts`.
- Current app code appears to consume rendered geometry, not these debug provenance strings.
- The preprocessor procedure geometry crate heavily populates these fields.

Why this smells:
- If these are only diagnostics, they should not be part of the normal UI contract.
- They add serialization weight to procedure geometry and create UI-side fields no renderer should rely on.

Proposed action:
- Decide whether procedure geometry diagnostics are a product artifact or a debug artifact.
- If debug only, move them behind a debug operation or build flag, not the primary `LegDisplayPath`.
- If product-visible, rename them away from `debug_` and document the consumer.

Status:
- Quarantined on 2026-05-07 by skipping serialization/deserialization in `app-core::LegDisplayPath` and removing the web type mirror.
- The fields are still used as preprocessor scratch state while building procedure-geometry diagnostics. The final `procedure-geometry-types::ProcedureGeometryPath` product payload does not carry them.

### Consolidated terrain rendering exports

Evidence:
- `ui/core-rust/crates/app-core/src/terrain.rs` exports PNG, RGBA, packed-tile, and raw-RGBA render entry points.
- `ui-web` and `ui-android` both appear to use raw/packed RGBA paths, not PNG paths.
- `render_terrain_warning_png` and `render_terrain_warning_png_from_tiles` appear used only by tests and public re-export.

Why this smells:
- Multiple equivalent render API shapes invite platform divergence.
- The PNG path may be a leftover from early prototyping.

Proposed action:
- Keep one platform contract: likely `render_terrain_warning_raw_rgba_from_tiles`.
- Move PNG encoding helpers to tests or remove them if no production caller remains.

Status:
- Consolidated on 2026-05-07.
- Removed PNG terrain rendering, single-tile terrain overlay render APIs, and single-tile WASM raw-RGBA render API.
- Web and Android now always use the packed-tile raw-RGBA path, even for one source tile.

### Debug ownship driver is product-plumbing unless intentionally retained

Evidence:
- `ui/core-rust/crates/app-core/src/session.rs` registers `__debug_ownship_driver__` in every session.
- Web mirrors it in `OwnshipSourceKind`.
- Debug flags control visibility, but the source exists in core even outside explicit debug use.

Why this smells:
- It is useful for development, but it is non-product state crossing the same source-selection path as real GPS/replay.

Proposed action:
- Keep if we still use it for deterministic demos/tests.
- Otherwise move it behind a debug build/config input so production sessions do not register it.

## ui-web

### Delete home-page placeholder buttons

Evidence:
- `ui/web-app/src/App.tsx` defines `placeholderLabels = ["S4", "S5", "S6", "S7", "S8", "S9"]` and renders disabled buttons on Home.

Why this smells:
- These are visual scaffolding, not feature UI.
- We explicitly renamed Settings to Home and should not keep grid filler as fake features.

Proposed action:
- Remove the placeholder buttons and let layout own empty space.
- If fixed grid density is desired, use CSS layout cells, not disabled buttons with fake labels.

### Remove action placeholder slots from the map-selection tray if layout can own it

Evidence:
- `ui/web-app/src/App.tsx` pads `MapSelectionTray` actions with `placeholder-*` disabled action objects.

Why this smells:
- This is UI-only layout scaffolding disguised as model-ish action data.
- It risks future code treating placeholders as real actions.

Proposed action:
- Prefer CSS grid fixed rows/cells, rendering empty cells directly without manufacturing fake action records.
- If keeping placeholders, confine them to a local render-only helper type and do not let them escape.

### Replace synthesized startup vector manifest with HAD-owned vector manifest

Evidence:
- `ui/web-app/src/domain/appCoreAdapter.ts` still builds a `baseManifest` in `fetchVectorManifestJson`.
- It fetches optional `/fast-products/obstacles/obstacles` and `/fast-products/metars/manifest.json`.
- Core sessions later also have `ensure_vector_manifest_loaded` that reads `NavKvQuery::VectorManifest` from attached HAD.

Why this smells:
- We already moved vector/overlay metadata into nav-db/HAD.
- Web still has optional fast-product manifest composition at startup, which duplicates the data contract and can diverge from Android.

Proposed action:
- Make session creation use a minimal bootstrap manifest or no manifest, then require `ensure_vector_manifest_loaded` to load the real HAD manifest before overlay queries.
- If METAR/obstacle fast products are intentionally outside nav-db, define that as a core input contract rather than ad hoc web-only manifest synthesis.

### Trim persistent debug logging once current perf bugs settle

Evidence:
- `ui/web-app/src/App.tsx` has persistent logs for startup, overlay query, drag/pinch, page-to-map paint, terrain tiles, playback scrubbing, route segments, etc.
- `ui/web-app/src/domain/debugLog.ts` sends logs to `/__debug_log`.

Why this smells:
- Some logs are still useful, but many were added during specific bug hunts.
- Chatty runtime logs distort perf investigations and make signal harder to find.

Proposed action:
- Keep error and high-level timing logs.
- Move drag/pinch frame logs, route segment logs, and overlay query internals behind a core debug flag or a URL/local-storage debug switch.

### Review playback UI after browser GPS source landed

Evidence:
- Web now supports replay and browser GPS through core ownship sources.
- Playback UI state is still surfaced through `debug_state.playback_visible`.

Why this smells:
- Playback is no longer just debug; it is a selectable ownship source.
- Hiding product UI behind `debug_state` naming is misleading.

Proposed action:
- Rename `playback_visible` out of `UiDebugState` into a normal ownship/source UI state field if replay remains a product feature.

## ui-android

### Delete `MockAppCoreAdapter` and old `AppCoreAdapter` shape if unused

Evidence:
- `ui/android-app/.../domain/AppCoreAdapter.kt` defines an interface plus `MockAppCoreAdapter`.
- `MainActivity` directly uses `NativeAppCoreAdapter` rather than the interface.
- The interface exposes old state-style operations (`replaceFlightPlan`, `setContentPolicy`, `refreshContent`) while the app has largely moved to session-owned state and offline package controller handles.

Why this smells:
- This is leftover architecture from before session/core owned the model.
- The mock implementation encodes behavior that may no longer match core.

Proposed action:
- Delete the mock.
- Either delete the interface or shrink it to the currently useful abstraction. If no alternate implementation exists, use `NativeAppCoreAdapter` directly.

### Remove Android legacy flight-plan leg removal plumbing

Evidence:
- `NativeBindings.kt` declares `removeFlightPlanLegJson`.
- `NativeAppCoreAdapter.kt` wraps it.
- Core only returns `UnsupportedOperation`.

Why this smells:
- Same core issue, but Android is carrying stale JNI glue too.

Proposed action:
- Remove with the core FFI removal pass.

### Remove installed-package filename fallback once metadata is mandatory

Evidence:
- `ui/android-app/.../domain/InstalledPackages.kt` has `fallbackArtifact(zipFile)` that derives `artifactId` from `zipFile.name.removeSuffix(".zip")` when metadata is missing.

Why this smells:
- We explicitly moved package identity/filename mapping into manifest/metadata to avoid regex/name guessing.
- This fallback can resurrect bad old filenames and make planner behavior depend on stale local files.

Proposed action:
- Decide migration policy.
- If no migration needed, ignore zip files with missing metadata and surface them as GC candidates through core.
- If migration needed, make it a one-shot explicit migration with logging, not a permanent silent fallback.

### Remove unused runtime fixture fields

Evidence:
- `ui/android-app/.../domain/SampleData.kt` has `ContentFixture.vectorPackageId`, `mapView`, `chartPage`, `mapTileView`, `remoteOnlyInventory`, and `installedInventory`.
- Search shows several are constructed but not read by current UI.
- `FALLBACK_VECTOR_MANIFEST_JSON` is defined but appears unused.

Why this smells:
- These were useful when the Android prototype was fixture-driven.
- Runtime now opens nav-db/HAD and derives catalog state from core, so fixture fields are stale scaffolding.

Proposed action:
- Delete fields not read by `MainActivity`.
- Keep `bootstrap`, `vectorManifestJson`, `mapViews`, `samplePlan`, and `navKvStore` only if still needed.

### Implement or remove Android terrain altitude bucket stub

Evidence:
- `ui/android-app/.../MainActivity.kt` has `private fun terrainAltitudeBucketForOwnship(ownship: OwnshipRenderState): Double? = null`.
- Web computes a bucket from `ownship.altitude_msl_ft ?? ownship.pressure_altitude_ft`.

Why this smells:
- Android carries terrain-warning plumbing but this stub prevents altitude-sensitive rendering from being meaningful.
- This is either an unfinished feature or a deliberate disable with no visible contract.

Proposed action:
- If terrain warning should work on Android, port the web rule or move bucket computation into core and expose it.
- If not, disable/hide terrain warning on Android until supported.

### Remove stale debug logs

Evidence:
- `ui/android-app/.../MainActivity.kt` still logs `AerobagReorder` row-resolution details with full row dumps.
- There are persistent high-volume tile budget and overlay logs.

Why this smells:
- The reorder log looks like a specific bug-hunt leftover.
- High-volume logs should be debug-gated, not always on.

Proposed action:
- Delete the reorder log.
- Gate tile/overlay verbosity behind the DBG panel.

### Split `MainActivity.kt`

Evidence:
- `ui/android-app/.../MainActivity.kt` is now about 10k lines and contains app shell, map rendering, flight-plan UI, offline packages UI, vector symbol rendering, terrain rendering, playback, and helpers.

Why this smells:
- Not dead code by itself, but it makes dead-code cleanup unsafe because unrelated features are interleaved.

Proposed action:
- Before major deletion, split into files by surface: map page, home page, plan page, offline packages page, inspector tray, playback, terrain/nexrad render helpers.
- Do not move logic out of core; this split is only Kotlin UI organization.

## preprocessor

### Decide whether legacy comparison commands are still needed

Evidence:
- `product/preprocessor/preprocessor-cli/src/main.rs` still exposes many `compare-* --legacy-work-dir` commands.
- README still references legacy-capture contracts.

Why this smells:
- If the Rust preprocessor has become authoritative, legacy comparison tooling is dead weight.
- If we still use it to validate refactors, it is developer tooling and should be clearly separated from product CLI.

Proposed action:
- If still useful, move these commands under an explicit `dev`/`legacy-compare` subcommand group or separate binary.
- If no longer used, delete command parsing and helper functions.

### Remove permanent legacy URL parsing if source manifests are now structured

Evidence:
- `product/preprocessor/preprocessor-fetch/src/lib.rs` uses `PrefetchRequest::from_legacy_url` in public fetch helpers.
- Structured `PrefetchRequest::new`, `with_logical_file_name`, `with_http1`, and `allow_html` also exist.

Why this smells:
- The legacy parser exists to recover logical filenames from old URL strings.
- Current pipeline should prefer explicit request structs over parsing overloaded strings.

Proposed action:
- Convert callers to structured `PrefetchRequest`.
- Keep `from_legacy_url` only in legacy comparison/import code, or remove it.

### Burn down legacy mutable output nodes

Evidence:
- `product/preprocessor/preprocessor-cli/src/product_build.rs` has `legacy_mutable_output_node`.
- It keeps `charts-*-render` and `csup-stage` writable because package outputs still land in render/stage dirs.

Why this smells:
- The node graph cannot guarantee output isolation while nodes mutate old stage directories.
- This is explicitly labeled legacy glue.

Proposed action:
- Move chart package outputs into package node dirs.
- Move CSUP markers/thumbnails/manifests into explicit output nodes.
- Delete `.mutable-output-root` behavior.

### Remove legacy unpacked production subtree cleanup when migration window closes

Evidence:
- `product/preprocessor/preprocessor-cli/src/product_build.rs` deletes `published-unpacked/production` via `remove_legacy_unpacked_subtree`.

Why this smells:
- Safe as migration hygiene, but it is not a product behavior forever.

Proposed action:
- Keep until all dev snapshots are known to have moved past the old layout.
- Then delete the cleanup path and fail loudly if unexpected legacy output appears.

### Revisit bundle-manifest compatibility validator

Evidence:
- `validate_bundle_manifest_compat` calls `validate_bundle_manifest_inner(..., false)`.
- Current-artifacts validation already enforces the current contract for active bundles.

Why this smells:
- Dual validators can mask stale bundle rows.
- We have a history of temporary fallbacks hiding real pipeline bugs.

Proposed action:
- Identify the caller that still needs compat mode.
- If it is only for historical snapshots, isolate it under a snapshot-migration test/helper.
- Otherwise delete compat mode and require current contract everywhere.

### Procedure-geometry debug provenance may be too heavy for product artifacts

Evidence:
- `product/preprocessor/preprocessor-procedure-geometry/src/procedure_geometry.rs` populates `debug_element_sources`.
- `product/preprocessor/preprocessor-procedure-geometry/src/lib.rs` inspects those debug fields for diagnostics.
- `ui-core` carries the same fields through `LegDisplayPath`.

Why this smells:
- This may be right for diagnostics, but it should not ride the product UI payload unless the UI uses it.

Proposed action:
- Same as ui-core: decide whether this is product or debug.
- If debug, emit it only in diagnostics artifacts, not in nav-db/HAD rows consumed by runtime UI.

## Suggested Burn-Down Order

1. Delete Android mock/old adapter interface if no test or alternate implementation uses it.
2. Remove web Home placeholder buttons and Android fixture fields that are clearly unused.
3. Decide the vector manifest source of truth: make HAD/core authoritative and remove web synthesis if possible.
4. Decide terrain warning status on Android: either wire altitude through core or hide/remove the toggle.
5. Gate or delete stale debug logs on both platforms.
6. Move preprocessor legacy comparison/migration code into explicit dev tooling or delete it once no longer needed.

## Notable Non-Findings

- `cargo check` did not report unused private Rust code in `app-core` or preprocessor.
- `ui-web` typechecks cleanly after installing dependencies.
- `ui-web` `tsconfig.json` does not enable `noUnusedLocals` or `noUnusedParameters`, so TypeScript did not perform a strict unused-symbol audit.
- Generated symbol files were not audited manually; they should be regenerated, not hand-edited.
