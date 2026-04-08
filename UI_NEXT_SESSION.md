# UI Next Session Handoff

Snapshot date: 2026-04-08

## Current UI State

The prototype is now a real 3-page shell on both web and Android:
- `Map`
- `Flight Plan`
- `Charts`

The current implementation is still driven by generated fixture/index data, but the shells are no longer a single map demo.

## What Works Now

### Shared fixture pipeline

Primary generator:
- [ui/scripts/generate_content_fixture.py](/root/aerobag/ui/scripts/generate_content_fixture.py)

Generated outputs:
- [ui/shared-fixtures/content-prototype/content_fixture.json](/root/aerobag/ui/shared-fixtures/content-prototype/content_fixture.json)
- [ui/web-app/src/domain/generated/contentFixture.json](/root/aerobag/ui/web-app/src/domain/generated/contentFixture.json)
- [ui/android-app/app/src/main/assets/fixtures/contentFixture.json](/root/aerobag/ui/android-app/app/src/main/assets/fixtures/contentFixture.json)

The fixture now includes:
- real `NW_SEC` and `SW_SEC` sectional package metadata
- real `NW_TAC` package metadata
- chart-page seed data for BOS:
  - one real plate PNG
  - one real CSup PNG
- chart asset metadata outside the individual zip packages

Chart asset staging:
- web static root:
  - [ui/web-app/generated-static/chart-assets](/root/aerobag/ui/web-app/generated-static/chart-assets)
- Android assets:
  - [ui/android-app/app/src/main/assets/chart-assets](/root/aerobag/ui/android-app/app/src/main/assets/chart-assets)

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
  - shows family, lat/lon/zoom, rendered tile count, source zooms, package ids, active map ids
  - text is selectable for copy/paste
- web chart page supports:
  - drag pan
  - wheel zoom
  - double-click zoom
  - pinch zoom
  - one-thumb overscroll margin around the image
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
  - [ui/web-app/generated-static/chart-assets](/root/aerobag/ui/web-app/generated-static/chart-assets)
- [ui/web-app/vite.config.ts](/root/aerobag/ui/web-app/vite.config.ts) mounts both via explicit middleware

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

What it does now:
- full-page tiled `Map` page in Compose
- bottom-centered `Nav Element` opens `Flight Plan`
- `Flight Plan` page with simple waypoint table and modal
- `Charts` page showing flat chart/CSup PNGs from Android assets
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
  - family
  - lat/lon/zoom
  - rendered tile count
  - source zooms
  - rendered packages
  - active map ids
  - family status (`Local`, `Partial`, `Package missing`)
  - tile-label debug toggle
- the old lower-right family-status badge has been removed
- map control hit-testing was tightened:
  - root map drag ignores pointer changes already consumed by child controls
  - drags that start on `SEC`, `DBG`, or the nav element should not pan the map
  - the old outer tray card around `SEC` was removed, so the extra gray border is gone

Important current Android dev note:
- do not bundle the full chart zip universe into the APK
- the APK must stay small enough to install on the emulator
- for dev, chart zips need to be seeded separately
- `SectionalPackages` now prefers already-seeded local package files and no longer crashes if a bundled asset is missing

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
2. clears `logcat`
3. force-stops the app
4. launches the app
5. waits briefly
6. prints:
   - resumed activity
   - crash lines
   - pass/fail result

Important discipline:
- if the user reports a crash, read `logcat` before reinstalling or relaunching
- reinstalling the APK will kill the running app with `installPackageLI`, which is not evidence of a runtime crash

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
