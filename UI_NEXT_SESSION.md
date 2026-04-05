# UI Next Session Handoff

Snapshot date: 2026-04-05

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
  - `MOCK` fallback now
  - `WASM` later if generated bindings exist

## Verified Commands

### Shared Rust core

Run:

```bash
cd /root/aerobag/ui/core-rust
cargo test
```

Last known result:
- passed
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

Build:

```bash
cd /root/aerobag/ui/web-app
npm run build
```

Last known result:
- passed

## Current Blocker

The browser app is prepared to use real Rust WASM bindings, but this environment cannot produce them yet.

Attempted command:

```bash
cd /root/aerobag/ui/core-rust
cargo build -p app-wasm --target wasm32-unknown-unknown
```

Observed failure:
- Rust compiler target exists in the target list
- but the `wasm32-unknown-unknown` standard library is not installed
- `rustup` is not available in this environment
- `wasm-pack` is not installed
- `wasm-bindgen` CLI is not installed

Net result:
- [ui/web-app/src/domain/appCoreAdapter.ts](/root/aerobag/ui/web-app/src/domain/appCoreAdapter.ts) currently uses a real `WasmAppCoreAdapter` shape plus a `MockAppCoreAdapter`
- the loader `loadBestAvailableAdapter()` tries to import `/generated/app_wasm.js`
- because that generated module does not exist yet, the app falls back to mock

This is an environment/toolchain blocker, not a UI architecture blocker.

## What To Do Next

### Best next step if WASM toolchain becomes available

1. Install a Rust toolchain with `wasm32-unknown-unknown`.
2. Add whatever tool is needed to generate JS bindings:
   - `wasm-bindgen` CLI or
   - `wasm-pack`
3. Build [app-wasm](/root/aerobag/ui/core-rust/crates/app-wasm) into generated browser artifacts.
4. Emit:
   - `/root/aerobag/ui/web-app/public/generated/app_wasm.js`
   - matching `.wasm` binary
5. Confirm the web prototype switches from `MOCK` to `WASM` automatically.

### Best next step if staying in the current environment

Keep moving on prototype features that do not require actual WASM output:

1. Replace inline TS sample data with checked-in fixture JSON files.
2. Add a small `Map` shell:
   - chart family selector
   - a fake lat/lon selector or click target
   - current selected chart display
3. Expand content requirements beyond airport-linked plates:
   - route/region logic
   - chart-family requirements
4. Start the Android-side shell around the same reducer/domain model.

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

Earlier commit from this session:
- `b8304d9` `Add shared app state and wasm adapter tests`

This handoff file was added after that commit and should be committed with the latest web prototype changes.

## Unrelated Workspace State

There are many unrelated untracked directories in the workspace:
- `avare-source/`
- `runs/`
- `rust-runs/`
- `legacy-capture/`
- and others

Do not blindly add them.

## If You Need A Very Short Resume Prompt

Resume the UI prototype from `/root/aerobag/ui/web-app` and `/root/aerobag/ui/core-rust`.
The content screen works and is tested.
The web app currently falls back to a mock adapter because the environment lacks a usable `wasm32-unknown-unknown` Rust stdlib and JS binding toolchain.
Next either:
- enable real WASM generation and wire `/generated/app_wasm.js`, or
- continue with fixture files and the next `Map` prototype slice.
