# UI Next Session Handoff

Snapshot date: 2026-04-07

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
- web chart page supports:
  - drag pan
  - wheel zoom
  - double-click zoom
  - pinch zoom
  - one-thumb overscroll margin around the image
- `?debugTiles=1` overlays tile `z/x/y`

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
- still mostly generated fixture/index data
- not yet driven by a full real unified client catalog

## Next Recommended Step

Stop expanding placeholder UI data and replace the fixture’s synthetic assembly with a real generated client index.

The next clean phase should be:
1. read nav data from:
   - `runs/20260407T053200Z-data-build/work/data/databases.zip`
   - specifically `main.db`
2. build one unified UI-facing catalog/index stream for:
   - all chart families:
     - sectional
     - tac
     - ifr low
     - ifr high
   - package metadata
   - coverage metadata
   - airport -> plate/csup index
   - artifact URLs, sizes, hashes
3. then make the current fixture generator consume that real index instead of constructing special-case records by hand

Important note:
- the user wants the metadata for all available content outside the individual packages, or duplicated at most, not only discoverable by opening installed zips
- that is the right direction because the UI needs to say things like:
  - “if you had downloaded `SEC_SE`, you could see this here”

## Resume Prompt

Resume from the 3-page shell checkpoint and replace the hand-assembled fixture data with a real generated UI catalog/index driven by `databases.zip`, chart package outputs, and TPP/CSup outputs. Keep the current map/plan/charts shells intact while swapping in the real data stream.

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
