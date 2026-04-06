# UI Next Session Handoff

Snapshot date: 2026-04-06

## What Was Built

### Shared Rust core

Location:
- [ui/core-rust](/root/aerobag/ui/core-rust)

Important files:
- [ui/core-rust/crates/app-core/src/lib.rs](/root/aerobag/ui/core-rust/crates/app-core/src/lib.rs)
- [ui/core-rust/crates/app-core/src/state.rs](/root/aerobag/ui/core-rust/crates/app-core/src/state.rs)
- [ui/core-rust/crates/app-core/src/ids.rs](/root/aerobag/ui/core-rust/crates/app-core/src/ids.rs)
- [ui/core-rust/crates/app-core/src/catalog.rs](/root/aerobag/ui/core-rust/crates/app-core/src/catalog.rs)
- [ui/core-rust/crates/app-core/src/content.rs](/root/aerobag/ui/core-rust/crates/app-core/src/content.rs)
- [ui/core-rust/crates/app-core/src/planning.rs](/root/aerobag/ui/core-rust/crates/app-core/src/planning.rs)
- [ui/core-rust/crates/app-wasm/src/lib.rs](/root/aerobag/ui/core-rust/crates/app-wasm/src/lib.rs)
- [ui/core-rust/crates/app-fixtures/src/lib.rs](/root/aerobag/ui/core-rust/crates/app-fixtures/src/lib.rs)

What it does now:
- parses `catalog.json`-shaped metadata
- parses geometry sidecar data
- finds charts by point-in-polygon lookup
- validates non-empty flight plans
- computes a first-pass content requirement set from airport-linked plate coverage
- resolves content status under `OfflineRequired`, `PreferLocal`, and `StreamAllowed`
- exposes an app-domain reducer:
  - `ReplaceFlightPlan`
  - `SetContentPolicy`
  - `RefreshContent`
  - `ClearFlightPlan`
- exposes JSON-oriented WASM wrapper functions:
  - `load_catalog`
  - `build_flight_plan`
  - `replace_flight_plan_state`
  - `set_content_policy_state`
  - `refresh_content_state`

### UI architecture docs

Files:
- [UI_ARCHITECTURE.md](/root/aerobag/UI_ARCHITECTURE.md)
- [UI_CATALOG_SCHEMA.md](/root/aerobag/UI_CATALOG_SCHEMA.md)

These define:
- why UI should be platform-native but behavior/domain shared
- the Rust/shared-core boundary
- the initial `catalog.json` and `chart_geometry.json` contract

### Web prototype

Location:
- [ui/web-app](/root/aerobag/ui/web-app)

Important files:
- [ui/web-app/src/App.tsx](/root/aerobag/ui/web-app/src/App.tsx)
- [ui/web-app/src/domain/appCoreAdapter.ts](/root/aerobag/ui/web-app/src/domain/appCoreAdapter.ts)
- [ui/web-app/src/domain/contentViewModel.ts](/root/aerobag/ui/web-app/src/domain/contentViewModel.ts)
- [ui/web-app/src/domain/sampleData.ts](/root/aerobag/ui/web-app/src/domain/sampleData.ts)
- [ui/web-app/src/domain/contentViewModel.test.ts](/root/aerobag/ui/web-app/src/domain/contentViewModel.test.ts)
- [ui/web-app/src/domain/appCoreAdapter.test.ts](/root/aerobag/ui/web-app/src/domain/appCoreAdapter.test.ts)

What it does now:
- renders a browser prototype for the `Content` screen
- loads a sample plan and sample catalog
- lets the user switch content policy:
  - `StreamAllowed`
  - `PreferLocal`
  - `OfflineRequired`
- lets the user switch between:
  - remote-only inventory
  - installed-package inventory
- shows whether the current plan is satisfied under the selected policy
- shows which backend is active:
  - `WASM` when generated bindings exist
  - `MOCK` only as fallback if the generated module is missing or broken

Operational note:
- [ui/web-app/vite.config.ts](/root/aerobag/ui/web-app/vite.config.ts) allows host `aerobag-dev.iac.jonh.net` so the Vite dev server can be opened through the remote browser path already in use
- [ui/web-app/scripts/build-wasm.sh](/root/aerobag/ui/web-app/scripts/build-wasm.sh) now builds `app-wasm` and generates browser bindings into `public/generated`
- [ui/web-app/package.json](/root/aerobag/ui/web-app/package.json) runs that generation step from both `npm run dev` and `npm run build`

### Android prototype

Location:
- [ui/android-app](/root/aerobag/ui/android-app)

Important files:
- [ui/android-app/settings.gradle.kts](/root/aerobag/ui/android-app/settings.gradle.kts)
- [ui/android-app/build.gradle.kts](/root/aerobag/ui/android-app/build.gradle.kts)
- [ui/android-app/app/build.gradle.kts](/root/aerobag/ui/android-app/app/build.gradle.kts)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/MainActivity.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/MainActivity.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/AppCoreAdapter.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/AppCoreAdapter.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/NativeBindings.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/NativeBindings.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/NativeAppCoreAdapter.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/NativeAppCoreAdapter.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/WireModels.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/WireModels.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/Models.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/Models.kt)
- [ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/SampleData.kt](/root/aerobag/ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/SampleData.kt)
- [ui/android-app/app/src/test/java/net/jonh/aerobag/prototype/domain/ContentLogicTest.kt](/root/aerobag/ui/android-app/app/src/test/java/net/jonh/aerobag/prototype/domain/ContentLogicTest.kt)

What it does now:
- mirrors the web `Content` slice in a native Compose shell
- uses the same sample plan, policy choices, and inventory modes as the web prototype
- prefers a real Rust/JNI-backed adapter at runtime and only falls back to mock if native loading fails
- shows `Backend NATIVE` or `Backend MOCK` in the screen header
- includes unit tests for:
  - content-policy behavior
  - native-adapter parity against the mock contract
  - JSON/wire-format knowledge like the required `content_policy` field and Rust-style `NavRef` enum shape
- the whole screen now scrolls correctly past `Inventory mode`

## Verified Commands

### Shared Rust core

Run:

```bash
cd /root/aerobag/ui/core-rust
cargo test
```

Last known result:
- passed
- `npm run build` now also regenerates the WASM bindings before invoking Vite
- 18 tests green

Coverage currently includes:
- package-name contract tests
- chart lookup tests
- flight-plan validation tests
- content-policy behavior tests
- reducer behavior tests
- fixture round-trip tests
- WASM JSON boundary tests

### Web prototype

Install deps:

```bash
cd /root/aerobag/ui/web-app
npm install
```

Test:

```bash
cd /root/aerobag/ui/web-app
npm test
```

Last known result:
- passed

Install / run on emulator in this Codex sandbox:

```bash
cd /root/aerobag/ui/android-app
env GRADLE_USER_HOME=/root/aerobag/.gradle-user-home ./gradlew test installDebug
adb shell am start -W -n net.jonh.aerobag.prototype/.MainActivity
```

Last known result:
- passed
- direct activity launch returned `Status: ok`

### Android prototype

Test:

```bash
cd /root/aerobag/ui/android-app
./gradlew test
```

Last known result:
- passed

Install / run on emulator:

```bash
cd /root/aerobag/ui/android-app
./gradlew installDebug
adb shell am start -n net.jonh.aerobag.prototype/.MainActivity
```

Build:

```bash
cd /root/aerobag/ui/web-app
npm run build
```

Last known result:
- passed

## Current Blockers

### Android UI visibility note

The app now launches cleanly through JNI, but `adb shell dumpsys activity activities` has occasionally still claimed the launcher was top-resumed even when a direct `am start -W` launch succeeded. Treat `am start -W` plus what is visible in VNC as the authoritative check here, not the occasional oddity in `dumpsys`.

### Android emulator note

The original X11-forwarded emulator path was not usable in practice even after KVM became available:
- forwarded-X emulator windows stayed gray or badly behaved across multiple GPU modes
- `scrcpy` was not reliable enough in this environment either

The working path ended up being:
- `Xvfb` on `:1`
- emulator launched on `DISPLAY=:1`
- `x11vnc` exporting that display
- emulator renderer set to `-gpu software`

Working commands:

```bash
Xvfb :1 -screen 0 1440x2960x24 -ac
DISPLAY=:1 /usr/lib/android-sdk/emulator/emulator -avd aerobag34 -gpu software -no-audio
x11vnc -display :1 -forever -shared -nopw -rfbport 5900 -noxdamage -nowf -noscr -fixscreen 1 -ncache 0 -clip 1080x2400+0+0
```

Observed behavior:
- this VNC-backed software-rendered path was the first one that made the emulator actually usable
- launcher/system UI could still occasionally misbehave, but `adb` navigation was reliable
- the Android prototype app itself rendered correctly and was interactive enough to test

## What To Do Next

### Best next step now

1. Make the tile-backed `Map` slice more interactive:
   - zoom selection against real available tiles
   - movable probe within the viewport, not just canned points
   - explicit handling for missing neighbor tiles / edges
2. Expand content requirements beyond airport-linked plates:
   - route/region logic
   - chart-family requirements
3. Pull more real chart families/geometry into the generated fixture beyond the first Boston TAC example.

### Completed since the earlier Android/WASM checkpoint

1. Replaced inline TS/Kotlin sample data with checked-in shared fixture JSON files.
2. Added a fixture exporter at [ui/scripts/generate_content_fixture.py](/root/aerobag/ui/scripts/generate_content_fixture.py).
3. Added the canonical generated fixture at [ui/shared-fixtures/content-prototype/content_fixture.json](/root/aerobag/ui/shared-fixtures/content-prototype/content_fixture.json).
4. Wired both shells to consume the generated fixture:
   - web imports [contentFixture.json](/root/aerobag/ui/web-app/src/domain/generated/contentFixture.json)
   - Android loads [contentFixture.json](/root/aerobag/ui/android-app/app/src/main/assets/fixtures/contentFixture.json)
5. The fixture currently draws from preprocessor outputs for:
   - sectional package provenance
   - BOS plate metadata from the recovered `tpp-ne` run
6. Current app checkpoint is good:
   - `adb shell am start -W -n net.jonh.aerobag.prototype/.MainActivity` returns `Status: ok`
7. Added the first real `Map` lookup slice:
   - fixture now includes `geometry` and `initial_probe`
   - Rust `chart_for_position` is exposed through both WASM and JNI
   - web and Android both show family/probe controls and the current matching chart
   - the generated fixture currently uses a transformed Boston TAC cutline from the preprocessor outputs
8. The `Map` slice now renders real TAC tiles on both platforms:
   - the fixture includes `map_tile_view`
   - the generator copies a small Boston TAC tile subset into:
     - [ui/shared-fixtures/content-prototype/tiles](/root/aerobag/ui/shared-fixtures/content-prototype/tiles)
     - [ui/web-app/public/prototype-tiles](/root/aerobag/ui/web-app/public/prototype-tiles)
     - [ui/android-app/app/src/main/assets/tiles](/root/aerobag/ui/android-app/app/src/main/assets/tiles)
   - Android and web now both render the same 3x3 tile viewport with a probe marker
9. Android startup verification rule:
   - do not trust `am start` by itself
   - the correct checkpoint sequence is `adb logcat -c`, launch, then `adb logcat -d` and confirm no `FATAL EXCEPTION` / `AndroidRuntime`
10. Web WASM/Vite integration:
   - generated wasm-bindgen output now goes to [ui/web-app/src/generated](/root/aerobag/ui/web-app/src/generated), not `/public`
   - loader explicitly calls the module default init function before using exports
   - this avoids the earlier Vite `/public` import failure and the bundler-target WASM incompatibility

## Important Design Decisions Already Made

- Do not share UI component code between Android and web.
- Share domain logic, metadata, state transitions, and tests.
- Android is offline/package-first.
- Web is stream/cache-first.
- The shared abstraction is product semantics, not storage symmetry.
- The web shell should consume the same model but may fulfill content remotely.

## Git / Repo Notes

There is a git repo initialized at:
- `/root/aerobag/.git`

Earlier commits from this work:
- `b8304d9` `Add shared app state and wasm adapter tests`
- `850ea60` `Add web content prototype and UI handoff`
- `769c385` `Remove web build artifacts and dependencies`

This handoff file should now be committed with the Android scaffold and emulator checkpoint.

## Unrelated Workspace State

There are many unrelated untracked directories in the workspace:
- `avare-source/`
- `runs/`
- `rust-runs/`
- `legacy-capture/`
- and others

Do not blindly add them.

## If You Need A Very Short Resume Prompt

Resume the UI prototype from `/root/aerobag/ui/web-app`, `/root/aerobag/ui/android-app`, and `/root/aerobag/ui/core-rust`.
The `Content` slice is shared-core-backed on both platforms.
The `Map` slice now renders real Boston TAC tiles on both platforms.
The web dev server should be left running and allowed to hot-reload instead of being restarted repeatedly.
When checking Android, always verify with logcat after launch.
Next:
- make the tile-backed `Map` slice interactive, or
- connect map selection to content requirements / planning state.
