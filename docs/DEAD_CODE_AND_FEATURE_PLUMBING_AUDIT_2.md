# Dead Code And Feature Plumbing Audit 2

Date: 2026-06-18

Scope:
- `ui-core`: `ui/core-rust/crates/app-core`, plus exposed FFI/WASM surface.
- `ui-web`: `ui/web-app/src`.
- `ui-android`: `ui/android-app/app/src/main/java`.
- `preprocessor`: `product/preprocessor`.

Method:
- Re-audited each item from `docs/DEAD_CODE_AND_FEATURE_PLUMBING_AUDIT.md` against the current tree.
- Used targeted `rg` scans for the original evidence strings and nearby replacement code.
- This is a source audit, not a compiler-proven dead-code report.

## Remaining Live Items

### Gate or prune persistent web debug logging

Evidence:
- `ui/web-app/src/App.tsx` still has more than 100 `debugLog(...)` call sites.
- `ui/web-app/src/domain` still has dozens more, including worker, nav-KV, terrain, and adapter timing logs.
- `ui/web-app/src/domain/debugLog.ts` always queues and posts logs to `/__debug_log` when running under HTTP(S).
- There is no central debug-log enablement gate beyond a few feature-specific flags such as `debugTiles`.

Why this still smells:
- Some logs are useful high-level timing/error signals.
- Many are frame-, tile-, drag-, or query-level diagnostics from specific bug hunts and can distort later perf investigations.

Proposed action:
- Keep startup, fatal error, and coarse timing logs enabled.
- Put high-volume map/raster/overlay/drag/playback logs behind a single debug flag or local build switch.
- Prefer a central tag filter in `debugLog.ts` so call sites do not each re-check URL/local-storage state.

### Delete or debug-gate Android bug-hunt logs

Evidence:
- `ui/android-app/app/src/main/java/org/aerobag/app/FlightPlanPage.kt` still logs `AerobagReorder` row-resolution dumps.
- `MapExplorerPage.kt` and `MainActivity.kt` still emit always-on `TileBudgetLogTag`, `MapViewportLogTag`, and `MapLayerLogTag` timing/cache/viewport logs.
- These logs include high-volume tile/load/frame/cache records.

Why this still smells:
- Error logs and a few coarse timings are useful.
- Row dumps and per-frame/per-generation tile telemetry were added for specific bug hunts and should not be always-on.

Proposed action:
- Delete the `AerobagReorder` dump.
- Gate high-volume tile, viewport, and overlay logs behind a DBG flag shared with web/core logging policy.
- Keep warnings/errors and coarse operation timings always visible only if they are actionable.

### Finish shrinking `MainActivity.kt` after the first split

Evidence:
- `ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt` is down to about 3.5k lines, but still contains app-shell state, runtime bootstrap, tile loader machinery, page navigation, and live-feed startup.
- Major page surfaces have been split into files such as `MapExplorerPage.kt`, `HomePage.kt`, `FlightPlanPage.kt`, `ChartsPage.kt`, and `OfflinePackagesPage.kt`.

Why this still smells:
- This is no longer the original 10k-line emergency, but cross-cutting runtime/tile/navigation code remains interleaved.
- Dead-code cleanup and behavioral refactors are still riskier when app shell and resource loading live in one large file.

Proposed action:
- Continue the split narrowly: move tile loading/cache helpers, retained runtime/session setup, and app navigation shell into focused files.
- Do not move product logic out of core; this is Kotlin UI organization only.

### Remove legacy unpacked package reuse path from preprocessor publication

Evidence:
- `product/preprocessor/preprocessor-cli/src/product_build.rs` still constructs a `legacy_filename` from `{family_id}_{region}_{cycle}.zip`.
- If a matching old unpacked directory and marker exist, it syncs the new hashed unpacked package from that legacy directory.

Why this still smells:
- The current publication contract uses hashed package filenames and explicit package rows.
- Reusing old unpacked directories by legacy naming is migration glue and can hide stale publication state.

Proposed action:
- Delete the legacy-name reuse path once the current publication tree is clean.
- Require the unpacked publication to be derived from the current package filename/manifest only.

## Verified Resolved Or Accepted From The Previous Audit

- Home-page placeholder buttons: no `placeholderLabels` home-grid scaffold remains.
- Map-selection tray placeholders: still present, but now confined to a local render-only slot type in `MapSelectionTray`; this matches the accepted fixed-size tray layout decision.
- Web startup vector manifest synthesis: no `baseManifest`/`fetchVectorManifestJson` synthesis path remains; vector metadata is loaded through core/HAD and fast products use explicit ingestion.
- Android `MockAppCoreAdapter`/old `AppCoreAdapter`: no mock/interface scaffold found.
- Android legacy flight-plan JNI removal plumbing: no Android `removeFlightPlanLegJson` path found.
- Android installed-package filename fallback: no `fallbackArtifact` path found.
- Android unused `SampleData` fixture fields: `SampleData.kt` is gone; the remaining issue is the narrower dev-bootstrap schema listed above.
- Android terrain altitude bucket stub: no `terrainAltitudeBucketForOwnship(...)=null` stub remains; terrain altitude bucket is core-derived from ownship.
- Preprocessor `from_legacy_url`: no current use or definition found; callers use structured `PrefetchRequest` construction.
- Preprocessor legacy mutable output node: no `legacy_mutable_output_node` / `.mutable-output-root` path found.
- Preprocessor bundle-manifest compatibility validator: no `validate_bundle_manifest_compat` path found.
- Preprocessor procedure-geometry debug provenance: still used as scratch/diagnostic state, but core serialization skips these fields and the previous product-payload concern appears addressed.
- Legacy comparison commands: the old `--legacy-work-dir` shape was not found. Remaining `compare-provenance` / `compare-sampled-images` commands look like generic developer tools, not product fallback plumbing.
- Legacy flight-plan mutation stubs: `remove_flight_plan_leg` and `move_flight_plan_waypoint` have been deleted from `app-core`.
- `ResolvedLegSource::LegacyPlanLeg`: deleted from core and Android wire/model mirrors; `legacy_plan_leg` payloads now fail as unsupported.
- PNG terrain rendering exports: `render_terrain_warning_png` and `render_terrain_warning_png_from_tiles` were deleted; terrain tests now exercise the raw RGBA path.
- Bad AP source contract: retained as a dev feature, but renamed from debug-ownship-driver terminology to `BadAutopilot` / `bad_autopilot` across core, WASM, web, and Android.
- Playback panel visibility: moved from `UiDebugState.playback_visible` to core-owned `UiPlaybackPanelState.visible` / `playback_panel_state`; web and Android now render playback controls from the product UI field.
- Android dev-bootstrap stale fields: `WireDevBootstrap` now only accepts the optional `package_management_now_utc` planner-clock override, and the shared bootstrap asset no longer carries ignored content/chart/recent-airport fixture state.
