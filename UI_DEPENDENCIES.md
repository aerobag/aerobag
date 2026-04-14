# UI Build Dependencies

## Purpose

Track every external tool or environment dependency discovered while building the UI prototype so later developers do not need to rediscover them by trial and error.

## Shared Rust Core

Required:
- Rust toolchain
- Cargo

Observed on this machine:
- initial distro toolchain was `rustc 1.75.0`
- that toolchain could not build Android targets because the Android stdlibs were not installed and it had no target-management workflow

Practical conclusion:
- for Android-targeted Rust work here, use `rustup`, not the distro `rustc`/`cargo`

## Artifact Discovery Contract

UI-side discovery should no longer assume a fixed `product-build.json` path.

Current contract:
- cycle bundle manifests live at:
  - `<source-root>/../../aerobag-artifacts/published-packaged/production/bundle_<cycle>.json`
- obstacle snapshots live at:
  - `<source-root>/../../aerobag-artifacts/published-packaged/obstacles/<YYYY.MM.DD>/output/obstacles_<YYYY.MM.DD>.zip`
- for "current" online discovery, select the lexicographically latest:
  - `bundle_*.json`
  - obstacle date directory / `obstacles_*.zip`

The current UI-side scripts in this repo now follow that rule.

Canonical chart family ids from preprocessing are:
- `sec`
- `tac`
- `enr-l`
- `enr-h`
- `enr-a`

UI and app-core now use only those canonical ids at the catalog boundary.

Install used:

```bash
apt-get install -y rustup
rustup default stable
rustup target add x86_64-linux-android
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
```

For native tests:
- `cargo test`

For web WASM output:
- a Rust toolchain with `wasm32-unknown-unknown` stdlib installed
- JS binding generation tooling:
  - `wasm-bindgen` CLI or
  - `wasm-pack`

Validated web WASM generation path:

```bash
cd /root/aerobag/ui/core-rust
cargo build -p app-wasm --target wasm32-unknown-unknown
/root/.cargo/bin/wasm-bindgen target/wasm32-unknown-unknown/debug/app_wasm.wasm --target web --out-dir /root/aerobag/ui/web-app/public/generated --out-name app_wasm
```

Practical conclusion:
- web builds should regenerate the WASM bindings, not assume they already exist
- [ui/web-app/scripts/build-wasm.sh](/root/aerobag/ui/web-app/scripts/build-wasm.sh) now handles that
- [ui/web-app/package.json](/root/aerobag/ui/web-app/package.json) runs it from both `npm run dev` and `npm run build`

## Web Prototype

Required:
- Node.js
- npm

Observed on this machine:
- `node v18.19.0`
- `npm 9.2.0`

Verified:
- `npm test`
- `npm run build`

Current web build behavior:
- `npm run build` now regenerates `public/generated/app_wasm.js` and `public/generated/app_wasm_bg.wasm`
- `npm run dev` and `npm run build` now also rerun:
  - [ui/scripts/generate_content_fixture.py](/root/aerobag/ui/scripts/generate_content_fixture.py)
- build artifacts are no longer expected under this repo's own `product-builds/`
  - current artifact root:
    - `<source-root>/../../aerobag-artifacts`
  - override supported by the UI bridge:
    - `AEROBAG_ARTIFACT_ROOT`
- that generator also extracts real `NW_SEC` / `SW_SEC` web tiles into:
  - [ui/web-app/generated-static/sectional-packages](/root/aerobag/ui/web-app/generated-static/sectional-packages)
- and stages chart PNG assets into:
  - real build-output files referenced by:
    - [ui/web-app/generated-static/chart-assets-manifest.json](/root/aerobag/ui/web-app/generated-static/chart-assets-manifest.json)
- [ui/web-app/.gitignore](/root/aerobag/ui/web-app/.gitignore) ignores `public/generated`
- [ui/web-app/.gitignore](/root/aerobag/ui/web-app/.gitignore) also ignores `generated-static`
- the web prototype fixture is generated and checked in at:
  - [ui/shared-fixtures/content-prototype/content_fixture.json](/root/aerobag/ui/shared-fixtures/content-prototype/content_fixture.json)
  - copied into [ui/web-app/src/domain/generated/contentFixture.json](/root/aerobag/ui/web-app/src/domain/generated/contentFixture.json)
- keep one Vite dev server alive and rely on HMR; restarting Vite unnecessarily caused confusion with stale ports earlier

Important Vite serving note:
- generated chart/tile assets proved unreliable when served from mutable `public/` subtrees under a long-lived dev server
- better approach now in:
  - [ui/web-app/vite.config.ts](/root/aerobag/ui/web-app/vite.config.ts)
- explicit middleware mounts:
  - `/sectional-packages` from `generated-static/sectional-packages`
  - `/chart-assets` from a manifest-backed resolver
- production builds also copy both trees into `dist/`
- important detail:
  - Vite mounts `/chart-assets` under a stripped path like `/KSEA/APD-...png`
  - the manifest is keyed as `/chart-assets/KSEA/APD-...png`
  - the middleware must handle both forms or it falls back to `index.html`
- if web tiles disappear, first verify the dev server is returning real asset content types before debugging map math
- if web plates disappear, verify a real PNG URL returns `image/png`
- after a machine reboot, Vite will not be running; restart with:

```bash
cd /root/aerobag/ui/web-app
npm run dev -- --host 0.0.0.0 --port 8080
```

Current important web rendering assumptions:
- packaged tiled chart assets are `512x512` WebP tiles
- the UI adapters must preserve that `512` tile size; forcing `256` makes TACs look blocky/overzoomed
- overlapping family mosaics can contain coarse and fine packages at once, so rendering should paint lower source zooms first and higher source zooms last

## Android Prototype

Required for source/build:
- Java 17
- modern Gradle wrapper
- Android SDK
- Android build-tools
- Android platform packages matching the chosen `compileSdk`
- Android NDK for Rust native builds

Required for local device/emulator workflow:
- `adb`
- Android emulator or physical device

Observed gaps at the start of this session:
- `java` missing
- `gradle` missing
- `adb` missing
- `emulator` missing

Actions taken:
- started installing:
  - `openjdk-17-jdk`
  - `gradle`
- confirmed installed:
  - `openjdk 17.0.18`
  - distro `gradle 4.4.1`

Important note:
- distro `gradle` here is `4.4.1`, which is too old for a modern Jetpack Compose Android app
- use it only once to bootstrap a modern Gradle wrapper if necessary
- do not rely on system Gradle as the long-term build path

Wrapper decision:
- bootstrap a modern Gradle wrapper for `ui/android-app`
- target wrapper version: `8.7`
- after wrapper creation, stop using system Gradle for project builds

Actual wrapper bootstrap sequence used:

```bash
mkdir -p /tmp/android-wrapper-bootstrap
cat >/tmp/android-wrapper-bootstrap/build.gradle <<'EOF'
task wrapper(type: Wrapper) {
    gradleVersion = '8.7'
    distributionType = Wrapper.DistributionType.BIN
}
EOF
gradle -p /tmp/android-wrapper-bootstrap wrapper
```

Then copy into the project:

```bash
mkdir -p /root/aerobag/ui/android-app/gradle/wrapper
cp /tmp/android-wrapper-bootstrap/gradlew /root/aerobag/ui/android-app/gradlew
cp /tmp/android-wrapper-bootstrap/gradlew.bat /root/aerobag/ui/android-app/gradlew.bat
cp /tmp/android-wrapper-bootstrap/gradle/wrapper/gradle-wrapper.jar /root/aerobag/ui/android-app/gradle/wrapper/gradle-wrapper.jar
cp /tmp/android-wrapper-bootstrap/gradle/wrapper/gradle-wrapper.properties /root/aerobag/ui/android-app/gradle/wrapper/gradle-wrapper.properties
chmod +x /root/aerobag/ui/android-app/gradlew
```

Then verify:

```bash
cd /root/aerobag/ui/android-app
./gradlew --version
```

Likely additional missing pieces after Java/Gradle:
- Android SDK command-line tools
- Android platform package for the selected SDK level
- Android build-tools package
- `adb`
- emulator images if running locally on this box

Android build config note:
- with Kotlin `2.0.x`, Compose requires the `org.jetbrains.kotlin.plugin.compose` Gradle plugin
- a modern wrapper alone is not enough; the Android app build must apply that plugin explicitly
- Compose Material 3 UI dependencies do not by themselves provide XML theme resources for the manifest theme
- for a Compose app using an XML theme such as `Theme.Material3.*`, also add:
  - `com.google.android.material:material`
- Android package staging now resolves package filenames from the active bundle manifest
  - [ui/android-app/app/build.gradle.kts](/root/aerobag/ui/android-app/app/build.gradle.kts)
    resolves package files from:
    - the active `bundle_*.json` under the configured artifact root
  - override supported by Gradle staging too:
    - `AEROBAG_ARTIFACT_ROOT`

Confirmed Android blocker after wrapper setup:
- `./gradlew test` currently fails with:
  - `SDK location not found`
- required next step:
  - install Android SDK tooling
  - set `sdk.dir` in `ui/android-app/local.properties` or provide `ANDROID_HOME`

Current SDK state after tooling install:
- SDK root discovered at:
  - `/usr/lib/android-sdk`
- installed:
  - `build-tools;34.0.0`
  - `cmdline-tools;13.0`
  - `platform-tools`
  - `platforms;android-34`
- selected emulator system image target:
  - `system-images;android-34;google_apis;x86_64`

NDK path for Rust bridge work:
- install via SDK manager instead of relying on distro-packaged historical NDK revisions

Command used:

```bash
yes | /usr/lib/android-sdk/cmdline-tools/13.0/bin/sdkmanager --install "ndk;26.3.11579264"
```

Practical consequence:
- `compileSdk 34` is the coherent choice here because build-tools `34.0.0` and platform `android-34` are installed
- if a future developer changes `compileSdk`, they must also install the matching platform package
- the Android Rust bridge currently assumes:
  - NDK `26.3.11579264`
  - linker `/usr/lib/android-sdk/ndk/26.3.11579264/toolchains/llvm/prebuilt/linux-x86_64/bin/x86_64-linux-android21-clang`
  - Rust toolchain binaries under `/root/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin`
- in this Codex sandbox, use:

```bash
cd /root/aerobag/ui/android-app
env GRADLE_USER_HOME=/root/aerobag/.gradle-user-home ./gradlew test installDebug
```

because `/root/.gradle` is effectively read-only for wrapper lockfiles here

Keyboard note:
- the AVD file [config.ini](/root/.android/avd/aerobag34.avd/config.ini) needed:

```ini
hw.keyboard = yes
```

- without that, keyboard input would not reach the guest reliably through the emulator/VNC path

Shared fixture loading on Android:
- the Android app loads its prototype fixture from assets:
  - [ui/android-app/app/src/main/assets/fixtures/contentFixture.json](/root/aerobag/ui/android-app/app/src/main/assets/fixtures/contentFixture.json)
- the Android Gradle build now reruns the generator automatically before `preBuild`
- current Android plate/CSUP delivery is zip-backed, not loose assets:
  - seed `NW_TPP.zip` and `NW_CSUP.zip` into app-local storage after install
  - do not try to bundle the full loose chart PNG tree into the APK; it explodes install size
- the helper now handles the correct dev workflow:
  - [ui/android-app/scripts/install_launch_check.sh](/root/aerobag/ui/android-app/scripts/install_launch_check.sh)
  - installs APK
  - seeds sectional zips
  - seeds chart packages
  - launches and checks logs

Generator concurrency note:
- do not run web and Android build commands in parallel if both invoke [ui/scripts/generate_content_fixture.py](/root/aerobag/ui/scripts/generate_content_fixture.py)
- it mutates shared generated trees and can race, especially around copied TAC tiles

Sectional package staging on Android:
- old approach of bundling full chart zips into APK assets does not scale:
  - APK install started failing with `Requested internal only, but not enough space`
  - and later plain sync failures like `write failed: No space left on device`
- current dev direction is:
  - keep APK small
  - seed chart zips separately for dev
- chart zips are staged for dev seeding by:
  - `stagePrototypeSectionalPackages`
- generated staging location:
  - `ui/android-app/app/build/generated/prototypeSeedPackages/sectional-packages`
- runtime lookup in:
  - [SectionalPackages.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/SectionalPackages.kt)
  now prefers existing locally seeded package files before falling back to APK assets
- tile rendering still reads directly from zip files, not unpacked trees

Android install / launch verification helper:

```bash
/root/aerobag/ui/android-app/scripts/install_launch_check.sh
```

This is now the preferred loop instead of ad hoc:
- install
- clear logcat
- force-stop
- launch
- wait
- verify resumed activity and crash lines

Important debugging note:
- if the user reports a crash, read `logcat` before reinstalling
- reinstalling via Gradle kills the running app with log lines like:
  - `installPackageLI`
  - `pkg removed`
  - `Force stopping ... due to installPackageLI`
- that is expected package replacement, not necessarily a runtime app bug

Package install path discovered:

- Installing `android-sdk` together with Google build-tools caused package conflicts.
- Better path was:
  - `adb`
  - `google-android-build-tools-34.0.0-installer`
  - `google-android-cmdline-tools-13.0-installer`

Observed Debian packaging quirk:
- `google-android-build-tools-34.0.0-installer` asks an interactive mirror-selection question.
- The debconf key is:
  - `google-android-installers/mirror`
- The config script is:
  - `/var/lib/dpkg/info/google-android-build-tools-34.0.0-installer.config`

Noninteractive recovery sequence used after the installer got stuck in `whiptail`:

1. Inspect the package config script to find the debconf key.
2. Kill the hung `apt` / `dpkg` / `whiptail` processes.
3. Preseed the mirror selection:

```bash
printf 'google-android-installers google-android-installers/mirror select https://dl.google.com\n' | debconf-set-selections
```

## Shared Fixture Generation

Required:
- Python 3
- GDAL Python bindings (`osgeo.osr`) for coordinate transformation
- access to the preprocessor output directories already present in this workspace

Generator:
- [ui/scripts/generate_content_fixture.py](/root/aerobag/ui/scripts/generate_content_fixture.py)

Current source inputs:
- sectional package provenance from:
  - `/root/aerobag/runs/20260406T032350Z-validation/native/charts-sec/meta/provenance/charts-sec/package_outputs.jsonl`
- sectional package zips from:
  - `/root/aerobag/runs/20260406T032350Z-validation/native/charts-sec/work/charts-sec/NW_SEC.zip`
  - `/root/aerobag/runs/20260406T032350Z-validation/native/charts-sec/work/charts-sec/SW_SEC.zip`
- BOS TPP data from:
  - `/root/aerobag/runs/20260405T154700Z-tpp-retry/work/tpp-ne/plates/BOS`

Command:

```bash
cd /root/aerobag
python3 ui/scripts/generate_content_fixture.py
```

What it does now:
- refreshes the shared fixture JSON consumed by both web and Android
- refreshes the TAC demo subset used for chart-lookup tests
- extracts the real `NW` and `SW` sectional package tile trees for the web prototype

Generator behavior:
- reads preprocessor package provenance and plate outputs
- now also reads chart cutline GeoJSON from the preprocessor workspace
- transforms chart geometry from `EPSG:3857` into WGS84 lat/lon points for the UI fixture
- now copies real TAC tile windows at zoom levels 9 and 10 for the full-page prototype map

Practical consequence:
- the generated fixture now carries both catalog metadata and geometry for the map lookup slice
- the fixture now carries:
  - `map_view` metadata for the full-page continuous-zoom explorer
  - legacy `map_tile_view` metadata only for compatibility with older code paths

## Android Map Rendering Notes

Important rendering lessons from the full-page map work:
- Android and web must use the same effective viewport unit system
- using raw physical pixels on Android made the same numeric zoom appear about 2x farther out than web
- Android tile seams at high zoom were not fully fixed by snapping individual composables
- the reliable fix was to render tiles in a single `Canvas` draw pass, not as a stack of separate `Image` composables

Current reliable Android debug inputs in this environment:
- drag pan
- keyboard `+`
- keyboard `-`

Inputs that are still unreliable in this remote emulator path:
- emulator-generated wheel events
- modifier-assisted pointer gestures like `Shift`-drag
- emulator multitouch visualization over VNC
- if GDAL Python bindings are missing, fixture generation for the map slice will fail

Current copied tile destinations:
- [ui/shared-fixtures/content-prototype/tiles](/root/aerobag/ui/shared-fixtures/content-prototype/tiles)
- [ui/web-app/public/prototype-tiles](/root/aerobag/ui/web-app/public/prototype-tiles)
- [ui/android-app/app/src/main/assets/tiles](/root/aerobag/ui/android-app/app/src/main/assets/tiles)

## Web Vite / WASM

Important current setup:
- wasm-bindgen output is generated into [ui/web-app/src/generated](/root/aerobag/ui/web-app/src/generated)
- the web loader imports that generated module as source and calls its default init function
- do not generate into `/public` and import from source code; Vite rejects that pattern
- `wasm-bindgen --target bundler` was tried and rejected here because stock Vite needs extra wasm integration support for that path
- the working path is `wasm-bindgen --target web` plus explicit init in the loader

Operational note:
- once the Vite dev server is up, prefer letting hot reload handle code changes
- do not keep restarting Vite unless the process is actually dead

## Android Runtime Verification

Do not treat `adb shell am start -W -n ...` alone as proof that the app is healthy.

Working checkpoint verification sequence:

```bash
adb logcat -c
adb shell am start -W -n net.jonh.aerobag.prototype/.MainActivity
adb logcat -d
```

Then verify:
- no `FATAL EXCEPTION`
- no `AndroidRuntime` crash from `net.jonh.aerobag.prototype`

Current output targets:
- canonical shared fixture:
  - [ui/shared-fixtures/content-prototype/content_fixture.json](/root/aerobag/ui/shared-fixtures/content-prototype/content_fixture.json)
- web copy:
  - [ui/web-app/src/domain/generated/contentFixture.json](/root/aerobag/ui/web-app/src/domain/generated/contentFixture.json)
- Android asset copy:
  - [ui/android-app/app/src/main/assets/fixtures/contentFixture.json](/root/aerobag/ui/android-app/app/src/main/assets/fixtures/contentFixture.json)

Practical conclusion:
- the current `Content` slice no longer uses hand-authored sample catalog data
- if the preprocessor output locations change, update the generator rather than patching web/Android fixtures by hand

4. Resume package configuration noninteractively:

```bash
DEBIAN_FRONTEND=noninteractive dpkg --configure -a
```

This should be preferred over trying to drive the `whiptail` prompt remotely.

## X11 / Remote Launch Notes

For Android UI on this machine, likely options are:
- Android emulator over X11 forwarding
- emulator on the remote box with local forwarded display
- physical device over `adb`

Observed emulator runtime dependency:
- `/usr/lib/android-sdk/emulator/emulator -version` initially failed with:
  - `libpulse.so.0: cannot open shared object file`
- required host package:
  - `libpulse0`

Install used:

```bash
apt-get install -y libpulse0
```

After that, this verification succeeded:

```bash
DISPLAY=localhost:10.0 /usr/lib/android-sdk/emulator/emulator -version
```

This still depends on installing:
- creating an AVD definition
- launching the emulator with `DISPLAY=localhost:10.0`

Recommended first emulator target on this machine:
- `system-images;android-34;google_apis;x86_64`

Commands used to get this far:

```bash
yes | /usr/lib/android-sdk/cmdline-tools/13.0/bin/sdkmanager --install "system-images;android-34;google_apis;x86_64"
echo no | /usr/lib/android-sdk/cmdline-tools/13.0/bin/avdmanager create avd -n aerobag34 -k "system-images;android-34;google_apis;x86_64" -d pixel_6
DISPLAY=localhost:10.0 /usr/lib/android-sdk/emulator/emulator -avd aerobag34 -gpu swiftshader_indirect -no-snapshot-save
```

Observed hard blocker in this container:
- the emulator reaches startup checks, finds the AVD and X11 display, then exits with:
  - `x86_64 emulation currently requires hardware acceleration`
  - `/dev/kvm is not found`
- this is because the current environment is an unprivileged container without nested virtualization
- fixing X11 alone is not sufficient

Practical consequence:
- if you want an Android emulator here, rebuild the container/host setup with nested virtualization and `/dev/kvm` available
- otherwise use a physical Android device over `adb`

Observed presentation issue after emulator boot:
- the Android guest renders correctly and `adb shell screencap` shows the app UI
- but the emulator's own X11 window can still appear as a gray rectangle over forwarded X
- this appears to be a host-side emulator/Qt presentation problem rather than an Android rendering problem

Practical workaround:
- install `scrcpy`
- keep the emulator running in the background
- interact with the emulator through `scrcpy` instead of the emulator's own window

Install used:

```bash
apt-get install -y scrcpy
```

Android 14 compatibility note:
- distro `scrcpy 1.25` crashes on startup if clipboard autosync is enabled
- the failing method is an Android 14 clipboard listener API mismatch
- launch it with clipboard sync disabled:

```bash
DISPLAY=localhost:10.0 scrcpy --serial emulator-5554 --no-clipboard-autosync
```

Alternative remote-display path:
- if forwarded X still produces a gray or unusably sluggish emulator window, use a virtual X server plus VNC instead
- packages used:

```bash
apt-get install -y xvfb x11vnc
```

Intended shape:
- run `Xvfb` on a private display such as `:1`
- launch the emulator with `DISPLAY=:1`
- export that same display through `x11vnc`

This avoids relying on the emulator's Qt window over forwarded X11.

Observed working emulator recipe in this environment:
- direct forwarded-X emulator windows remained gray or sluggish across:
  - default `auto`
  - `-gpu software`
  - `-gpu host`
- `scrcpy` was not a reliable fallback here
- the first actually usable path was:
  - `Xvfb`
  - emulator on `DISPLAY=:1`
  - `x11vnc` with conservative refresh settings
  - emulator renderer set to `-gpu software`

Commands used:

```bash
Xvfb :1 -screen 0 1440x2960x24 -ac
DISPLAY=:1 /usr/lib/android-sdk/emulator/emulator -avd aerobag34 -gpu software -no-audio
x11vnc -display :1 -forever -shared -nopw -rfbport 5900 -noxdamage -nowf -noscr -fixscreen 1 -ncache 0 -clip 1080x2400+0+0
```

Practical note:
- with this setup, the emulator became usable enough to interact with the Android prototype through a VNC client
- system UI and app rendering were materially better than the forwarded-X attempts
- if the launcher or assistant lands on a black screen, use `adb` to navigate rather than trusting the on-screen controls

Useful adb controls:

```bash
adb shell input keyevent KEYCODE_HOME
adb shell am start -n net.jonh.aerobag.prototype/.MainActivity
```

## Next Dependency Checks To Run

After Java/Gradle install completes:
- `java -version`
- `gradle -version`

Then likely:
- install Android SDK command-line tools
- install `adb`
- install emulator
- install platform/build-tools packages

Keep updating this file whenever a missing dependency is discovered.
