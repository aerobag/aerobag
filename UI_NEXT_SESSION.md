# UI Next Session Handoff

Snapshot date: 2026-04-09

## Current Contract Notes

- Canonical map family ids are now:
  - `sec`
  - `tac`
  - `enr-l`
  - `enr-h`
- Web and Android runtime paths now use those canonical ids directly.
- Android package seeding was updated to include all four canonical map families again.
- app-core now emits canonical family ids at the catalog boundary and accepts legacy aliases (`sectional`, `ifr_low`, `ifr_high`, `ifr_area`) during the transition.

## Current UI State

The prototype is now a real 3-page shell on both web and Android:
- `Map`
- `Flight Plan`
- `Charts`

The current implementation is still driven by generated fixture/index data, but the shells are no longer a single map demo.

Important current split:
- web plate/CSUP viewing uses the richer validation `resource-index` and serves chart PNGs directly from real build-output paths via a manifest-backed Vite middleware
- Android plate/CSUP viewing is intentionally limited to `NW_TPP` and `NW_CSUP`, seeded as zip packages into app-local storage after install
- preprocessing artifacts now live outside this repo under:
  - `<source-root>/../aerobag-artifacts`
  - UI bridge/staging code must rebase stale absolute `artifact_path` values onto that root

## What Works Now

### Shared fixture pipeline

Primary generator:
- [ui/scripts/generate_content_fixture.py](/root/aerobag/ui/scripts/generate_content_fixture.py)

Generated outputs:
- [ui/shared-fixtures/content-prototype/content_fixture.json](/root/aerobag/ui/shared-fixtures/content-prototype/content_fixture.json)
- [ui/web-app/src/domain/generated/contentFixture.json](/root/aerobag/ui/web-app/src/domain/generated/contentFixture.json)
- [ui/android-app/app/src/main/assets/fixtures/contentFixture.json](/root/aerobag/ui/android-app/app/src/main/assets/fixtures/contentFixture.json)

The fixture now includes:
- real tiled chart metadata across the indexed families
- no `chart_page` bootstrap block anymore
- chart-page resources are derived from `resource-index.airport_resources + plates + csups`
- sample flight plan is still generated, but current plate page airport/chart selection is app-owned local state now

Chart asset staging:
- web manifest:
  - [ui/web-app/generated-static/chart-assets-manifest.json](/root/aerobag/ui/web-app/generated-static/chart-assets-manifest.json)
- Android seeded chart packages:
  - `NW_TPP.zip`
  - `NW_CSUP.zip`

### Web app

Location:
- [ui/web-app](/root/aerobag/ui/web-app)

Important files:
- [ui/web-app/src/App.tsx](/root/aerobag/ui/web-app/src/App.tsx)
- [ui/web-app/src/styles.css](/root/aerobag/ui/web-app/src/styles.css)
- [ui/web-app/src/domain/mapViewport.ts](/root/aerobag/ui/web-app/src/domain/mapViewport.ts)
- [ui/web-app/src/domain/imageViewport.ts](/root/aerobag/ui/web-app/src/domain/imageViewport.ts)
- [ui/web-app/src/domain/sampleData.ts](/root/aerobag/ui/web-app/src/domain/sampleData.ts)
- [ui/web-app/src/domain/types.ts](/root/aerobag/ui/web-app/src/domain/types.ts)
- [ui/web-app/vite.config.ts](/root/aerobag/ui/web-app/vite.config.ts)

What it does now:
- full-page tiled `Map` page
- bottom-centered `Nav Element` opens `Flight Plan`
- `Flight Plan` page shows a waypoint table and waypoint action modal
- `Charts` page shows flat PNG chart/CSup images with drag/zoom
- page navigation is now a real view stack, not just a current-page enum
  - page changes push snapshots onto the stack
  - on the plate page, changing selected airport/chart also pushes a snapshot
  - browser back / Android system back pop that same stack
  - because pages stay mounted, map/plate viewport state survives page changes
- chart-family tray is modal with scrim
- chart-family switching preserves lat/lon/continuous zoom
- leaving `Map` and coming back preserves the map viewport
- web map supports:
  - drag pan
  - wheel zoom
  - double-click zoom
  - pinch zoom
- lower-left debug launcher:
  - closed: `1 thumb`
  - open: `4 thumbs`
  - present on `Map`, `Plan`, and `Charts`, not just `Map`
  - shows the current view stack
    - top of stack is leftmost
    - plate views are abbreviated like `PLT-16C`
  - on `Map`, also shows family, lat/lon/zoom, rendered tile count, source zooms, package ids, active map ids
  - text is selectable for copy/paste
- web chart page supports:
  - drag pan
  - wheel zoom
  - double-click zoom
  - pinch zoom
  - one-thumb overscroll margin around the image
- the selected chart `<img>` must stay mounted before viewport initialization
  - otherwise the page can deadlock into showing no plate at all
- `?debugTiles=1` overlays tile `z/x/y`

Important recent web rendering fixes:
- tiled-family adapters now use `tile_size = 512` instead of `256`
- family mosaic rendering sorts tiles so lower-zoom tiles paint first and higher-zoom tiles paint last
- this fixed the case where coarse `PAC_TAC` tiles could visually obscure sharper `SW_TAC` tiles in overlapping TAC coverage
- map overlay hit-testing was tightened:
  - dead space around `TAC` / `DBG` now falls through to the map
  - visible button/panel surfaces still own the pointer gesture

Important web serving note:
- sectional tiles no longer come from mutable `public/sectional-packages`
- they are served from:
  - [ui/web-app/generated-static/sectional-packages](/root/aerobag/ui/web-app/generated-static/sectional-packages)
- chart PNGs are served from:
  - real build-output files resolved through:
    - [ui/web-app/generated-static/chart-assets-manifest.json](/root/aerobag/ui/web-app/generated-static/chart-assets-manifest.json)
- [ui/web-app/vite.config.ts](/root/aerobag/ui/web-app/vite.config.ts) mounts both via explicit middleware
- important detail:
  - the `/chart-assets` middleware receives stripped paths like `/KSEA/APD-...png`
  - the manifest keys are `/chart-assets/KSEA/APD-...png`
  - the middleware must handle both forms or Vite falls back to `index.html`

This was necessary because long-lived Vite dev servers were intermittently returning `index.html` for generated tile asset URLs when generated trees changed under them.

### Android app

Location:
- [ui/android-app](/root/aerobag/ui/android-app)

Important files:
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/MainActivity.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/MainActivity.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/Models.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/Models.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/SampleData.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/SampleData.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/WireModels.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/WireModels.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/MapViewport.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/MapViewport.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/ImageViewport.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/ImageViewport.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/SectionalPackages.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/SectionalPackages.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/ChartPackages.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/ChartPackages.kt)

What it does now:
- full-page tiled `Map` page in Compose
- bottom-centered `Nav Element` opens `Flight Plan`
- `Flight Plan` page with simple waypoint table and modal
- `Charts` page showing flat chart/CSup PNGs from seeded zip packages, with asset fallback only if needed
- page navigation is now a real view stack
  - page changes push snapshots
  - plate airport/chart changes also push snapshots
  - Android `BackHandler` pops that stack
  - map/plate viewport state survives page switches because it is hoisted in `AerobagApp`, not because hidden pages stay mounted
- chart-family tray is modal with scrim
- chart-family switching preserves lat/lon/continuous zoom
- leaving `Map` and coming back preserves the map viewport
- runtime auto-installs selected package zips into app-local storage
- tiles are read directly from installed zip files
- zip handles and entry-name indices are cached in memory for faster repeated reads
- tile rendering is done as a single `Canvas` draw pass to avoid seams
- square top-level controls (`MAP`, `SEC`, etc.) are custom compact surfaces, not Material buttons, so their labels now fit
- map rendering is family-scoped, not single-package scoped
- Android startup now prefers the fixture's intended initial map family/package instead of the first arbitrary chart collection from `resource-index`
- Android map now uses a lower-left `DBG` launcher in the same compact square visual language as `SEC`
- tapping `DBG` opens a debug panel with:
  - current view stack
    - top of stack shown leftmost
    - plate entries abbreviated like `PLT-16C`
  - family
  - lat/lon/zoom
  - rendered tile count
  - source zooms
  - rendered packages
  - active map ids
  - family status (`Local`, `Partial`, `Package missing`)
  - tile-label debug toggle
- the old lower-right family-status badge has been removed
- `DBG` is now present on `Map`, `Plan`, and `Charts`, not only `Map`
- map control hit-testing was tightened:
  - root map drag ignores pointer changes already consumed by child controls
  - drags that start on `SEC`, `DBG`, or the nav element should not pan the map
  - the old outer tray card around `SEC` was removed, so the extra gray border is gone
- Android top-bar trays now use a shared anchored panel instead of `DropdownMenu`
  - reason:
    - `DropdownMenu` kept auto-repositioning over the launcher or off-screen
  - current behavior in `MenuDock`:
    - tray hangs below the launcher
    - tray height is capped by actual space below the launcher, not a fixed row count
    - if content exceeds that height, it scrolls inside the tray
- Android hardware `+/-` zoom is now handled at the `MainActivity` level
  - reason:
    - Compose focus on the active surface proved too fragile after tray/page changes
  - current behavior:
    - `MainActivity.dispatchKeyEvent()` intercepts `+/-`
    - visible map/plate page registers a zoom callback with the activity via `onHardwareZoomDelta`
    - this path does not depend on whichever composable currently owns focus
- important recent Android input/root-cause fix:
  - the app shell previously kept `Map`, `Plan`, and `Charts` all composed at once with hidden pages only faded out
  - hidden pages were still hit-testable and could steal gestures behind the visible page
  - current fix:
    - `AerobagApp()` now composes only the active page
    - back-stack/view state still works because page state is hoisted into app-level state and snapshots
  - symptom this explained:
    - chart drag mysteriously stopped working
    - plate drag could break again while logs showed chart-page handlers receiving the drag

Important current Android dev note:
- do not bundle the full chart zip universe into the APK
- do not bundle loose chart PNG trees into the APK either
- the APK must stay small enough to install on the emulator
- for dev, payloads are seeded separately after install:
  - tiled zips via `seedPrototypeSectionalPackages`
  - `NW_TPP.zip` and `NW_CSUP.zip` via `seedPrototypeChartPackages`
- `SectionalPackages` and `ChartPackages` both prefer already-seeded local package files
- current Android plate/CSUP universe is intentionally NW-only because only `NW_TPP` and `NW_CSUP` are seeded
- current repo-layout note:
  - Android Gradle staging and `generate_content_fixture.py` both know how to resolve package `artifact_path` entries against `<source-root>/../aerobag-artifacts/product-builds/...`
  - environment override:
    - `AEROBAG_ARTIFACT_ROOT`
  - additional fallback:
    - if an indexed `shared/...` artifact is absent, both helpers now try profile-specific fallbacks like `validation/...`
    - this currently matters for `NW_CSUP.zip`, which exists under validation artifacts, not shared
- current fixture-generator note:
  - `copy_tac_tile_subset()` now tolerates missing Boston TAC demo tiles at some zoom levels
  - if a zoom level has no available TAC subset tiles, it is skipped instead of crashing on `min()/max()` over an empty set
- important seeding gotcha that bit us:
  - Gradle was reusing a stale generated `NW_TPP.zip` under:
    - `ui/android-app/app/build/generated/prototypeSeedChartPackages/chart-packages/NW_TPP.zip`
  - even though the shared product zip at:
    - `product-builds/shared/work/tpp-nw/work/tpp-nw/NW_TPP.zip`
    had already been regenerated with stitched pages / transparent-edge fixes
  - cause:
    - the seed staging tasks declared outputs but effectively let Gradle reuse old generated copies
  - current fix:
    - [ui/android-app/app/build.gradle.kts](/root/aerobag/ui/android-app/app/build.gradle.kts)
      forces fresh restaging with `outputs.upToDateWhen { false }` on:
      - `stagePrototypeSectionalPackages`
      - `stagePrototypeChartPackages`
  - symptom to remember:
    - web shows fresh stitched plate
    - Android shows old first-page-only or pre-fix imagery
    - compare seeded device zip size against shared source zip before blaming rendering code

### Shared Rust core

Still in use for:
- catalog/state/content logic
- Android JNI adapter
- web WASM adapter

Relevant locations:
- [ui/core-rust](/root/aerobag/ui/core-rust)
- [ui/core-rust/crates/app-ffi/src/lib.rs](/root/aerobag/ui/core-rust/crates/app-ffi/src/lib.rs)
- [ui/core-rust/crates/app-wasm/src/lib.rs](/root/aerobag/ui/core-rust/crates/app-wasm/src/lib.rs)

## Verified Commands

### Shared fixture generation

```bash
cd /root/aerobag
python3 ui/scripts/generate_content_fixture.py
```

Last known result:
- passed

### Web

```bash
cd /root/aerobag/ui/web-app
PATH=/root/local/node-v24.10.0/bin:$PATH npm test
PATH=/root/local/node-v24.10.0/bin:$PATH npm run build
```

Last known result:
- both passed

Live dev server convention:
- keep one Vite process alive and rely on HMR
- expected host URL:
  - `http://aerobag-dev.iac.jonh.net:8080/`
- after a machine reboot, Vite will of course be gone; restart it with:

```bash
cd /root/aerobag/ui/web-app
npm run dev -- --host 0.0.0.0 --port 8080
```

If the live dev server starts returning HTML for tile URLs again, verify with:

```bash
curl -I http://localhost:8080/sectional-packages/NW_SEC/tiles/0/9/93/324.webp
```

Expected:
- `Content-Type: image/webp`

If the live dev server is not showing plates, verify one real chart PNG directly:

```bash
curl -I 'http://localhost:8080/chart-assets/06N/IAP-NY-RNAV%20(GPS)%20RWY%2008.png'
```

Expected:
- `Content-Type: image/png`

### Android

Build and install:

```bash
cd /root/aerobag/ui/android-app
env GRADLE_USER_HOME=/root/aerobag/.gradle-user-home ./gradlew test installDebug
```

Runtime verification:

```bash
adb logcat -c
adb shell am start -W -n net.jonh.aerobag.prototype/.MainActivity
adb logcat -d AndroidRuntime:E '*:S'
```

Last known result:
- build passed
- launch returned `Status: ok`
- crash log clean

Preferred Android verification command now:

```bash
/root/aerobag/ui/android-app/scripts/install_launch_check.sh
```

What it does:
1. `installDebug`
2. seeds sectional zips and chart packages
3. clears `logcat`
4. force-stops the app
5. launches the app
6. waits briefly
7. prints:
   - resumed activity
   - crash lines
   - pass/fail result

Important discipline:
- if the user reports a crash, read `logcat` before reinstalling or relaunching
- reinstalling the APK will kill the running app with `installPackageLI`, which is not evidence of a runtime crash
- do not run `npm run build` and `./gradlew test` in parallel
  - both invoke [ui/scripts/generate_content_fixture.py](/root/aerobag/ui/scripts/generate_content_fixture.py)
  - that generator mutates shared generated trees and can race

Current reboot bring-up sequence that worked:

```bash
Xvfb :1 -screen 0 1440x3040x24
x11vnc -display :1 -forever -shared -rfbport 5900 -noxdamage -nowf -noscr -fixscreen 1 -clip 1080x2400+0+0
DISPLAY=:1 /usr/lib/android-sdk/emulator/emulator -avd aerobag34 -gpu software -no-audio -no-snapshot-save
env GRADLE_USER_HOME=/root/aerobag/.gradle-user-home /root/aerobag/ui/android-app/scripts/install_launch_check.sh
```

## Current Design State

The UI is now using the `thumb` sizing idea as the main layout token.

Current high-level page structure:
- `Map`
  - top-left chart-family launcher and modal tray
  - bottom-centered nav element
- `Flight Plan`
  - waypoint table
  - waypoint action modal
- `Charts`
  - top-left airport selector
  - top-left chart selector
  - flat image pan/zoom viewer

Current data reality:
- still uses the fixture bridge for staging/bootstrap
- but chart/package discovery now comes from the real `resource-index.json`
- nav DB is the real product `main.db`
- there is still too much UI-side fixture glue, but much less than before

## Next Recommended Step

The next clean phase is to keep shrinking the fixture bridge:
1. stop using `content_fixture.json` as the thing that decides app availability
2. bootstrap UI more directly from:
   - `resource-index.json`
   - `main.db`
3. move demo/session state out of fixture generation and into explicit local app state
4. make Android dev package seeding a first-class helper instead of ad hoc shell work

Important note:
- the user wants metadata for all available content outside the individual packages, or duplicated at most, not only discoverable by opening installed zips
- that remains the correct direction because the UI needs to say things like:
  - “if you had downloaded `SEC_SE`, you could see this here”

## Resume Prompt

Resume from the current 3-page shell and continue removing fixture glue. Keep the current map/plan/charts shells intact while moving bootstrap/state onto `resource-index.json` + `main.db`, and stabilize the Android dev package-seeding workflow.

## 2026-04-07 Resource-Index / Family Mosaic Checkpoint

- The UI bridge now consumes a generated `resource-index.json` alongside the legacy-ish fixture bundle.
- Web derives:
  - `mapViews` from `resource-index.chart_collections`
  - `chartPage` from `resource-index.plates` and `resource-index.csups`
  - via [ui/web-app/src/domain/resourceIndexAdapters.ts](/root/aerobag/ui/web-app/src/domain/resourceIndexAdapters.ts)
- Android mirrors that logic in:
  - [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/ResourceIndexAdapters.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/ResourceIndexAdapters.kt)

- Map rendering is no longer single-package-per-family.
  - Both web and Android render a family mosaic from all visible `MapView`s in the selected family.
  - That fixed the earlier `SECTIONAL` bug where panning south from `NW` hit gray instead of drawing `SW`.
  - Tests were added on both platforms to pin the “neighboring packages can appear in one viewport” behavior.

- Current Android packaging note:
  - Android still installs chart packages from APK assets under `sectional-packages`.
  - The build now stages `NW_SEC`, `SW_SEC`, `NW_TAC`, and `SW_TAC`.
  - Family selection auto-installs all packages in the selected family, not just one package.

- Current sample flight plan:
  - `KRNT SEA PAE KAWO`
  - stored in the generated fixture, not yet generated from nav-db logic

- Important remaining design issue:
  - airport ids in the app domain should become canonical airport ids
    - use ICAO when present
    - otherwise leave the FAA/local id as-is

## 2026-04-08 Android Dev-Content Checkpoint

- The recurring `FileNotFoundException: sectional-packages/NE_SEC.zip` crash was traced to a bad dev packaging path:
  - bundling all chart zips into the APK made the APK too large to install cleanly
  - the emulator kept running an older APK without the expected assets

- Current fix direction:
  - keep the APK small
  - seed chart zips separately for dev
  - `SectionalPackages` now checks existing local package files before trying APK assets

- Current local package lookup:
  - [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/SectionalPackages.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/SectionalPackages.kt)

- Current Android install/launch verification helper:
  - [ui/android-app/scripts/install_launch_check.sh](/root/aerobag/ui/android-app/scripts/install_launch_check.sh)

- Current known rough edge:
  - full-family package seeding on Android is still not a polished single-command path
  - for this session, sectional family zips were seeded directly into app-internal storage with `adb shell run-as ... dd of=...`
  - that needs to be formalized if Android family browsing is going to stay easy to reproduce

- Another important log interpretation note:
  - `Force stopping ... installPackageLI`
  - `Killing ... due to installPackageLI`
  means the app was killed because a reinstall replaced it
  - do not mistake that for a runtime crash

## 2026-04-07 Repo Layout Note

- The preprocessing source tree has been split:
  - baseline reference implementation:
    - [baseline/avare_equivalent](/root/aerobag/baseline/avare_equivalent)
  - evolving product pipeline:
    - [product/preprocessor](/root/aerobag/product/preprocessor)
- The UI should continue to treat generated preprocessing outputs as external artifacts.
- Product-facing metadata changes such as canonical airport ids should now come from [product/preprocessor](/root/aerobag/product/preprocessor), not from UI-side cleanup logic.
  - do this in preprocessing, not as a UI string-formatting convention

- Transitional technical debt:
  - [ui/scripts/generate_content_fixture.py](/root/aerobag/ui/scripts/generate_content_fixture.py) still exists and is still Python/GDAL-based
  - it now mostly stages assets and copies the generated `resource-index`, but it is still too involved
  - long-term this should be replaced by product-side preprocessing outputs, not live UI-side assembly
