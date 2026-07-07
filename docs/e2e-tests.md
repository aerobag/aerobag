# End-to-End Test Status

Status as of 2026-07-04: Android E2E has an initial working framework in the
tree, but it is not yet a required CI gate.

## What Exists

- `tools/e2e/android-harness.mjs` provides the shared adb/uiautomator helpers.
- `tools/e2e/run-android-e2e-suite.mjs` runs the Android suite.
- `ui/android-app/scripts/run_e2e.sh` builds, installs, and runs the suite
  against a selected device or emulator.
- `ui/android-app/scripts/run_e2e_ci.sh` is the CI-oriented wrapper. It can
  install the emulator system image, start a local package server, boot the
  repo emulator stack, install the APK, and run the suite.
- `docs/testing/android-e2e.md` has operator-facing usage notes.

## Current Coverage

- `android.flight-plan-route-smoke`
  - Launches the app.
  - Accepts the disclaimer if needed.
  - Syncs the NW offline package set if the runtime starts on Offline Packages.
  - Enters the route `KRNT KPWT`.
  - Switches to the chart page.
  - Centers the chart on the destination.
  - Verifies that Android exposes a rendered flight-plan route overlay with at
    least one visible segment.
- `android.map-follow-ctr-gesture-smoke`
  - Performs the same route setup.
  - Enables the Bad Autopilot debug ownship source.
  - Selects the synthetic ownship source and waits for an ownship render probe.
  - Ensures CTR follow is engaged.
  - Drags the map and verifies CTR remains engaged with the aircraft offset
    from center.
  - Zooms the map and verifies CTR remains engaged with that offset preserved.

## App Test Hooks

The Android UI now exposes stable `parity:` tags for the smoke test:

- disclaimer accept button
- chart search field and suggestions
- debug Bad Autopilot flag and ownship source controls
- flight-plan route overlay semantic probe
- map-follow semantic probe

Painted canvas content is not directly visible to `uiautomator`, so the E2E
checks use semantic probe tags:

```text
parity:flight-plan-route-overlay:segments:<count>:visible:<count>
parity:map-follow-state:following:<0|1>:ownship-x:<px>:ownship-y:<px>:center-x:<px>:center-y:<px>:zoom-centi:<zoom*100>
```

## Emulator Support

The emulator stack scripts now support:

- deterministic emulator serial selection from `VNC_PORT`
- VNC-backed local runs
- `EMULATOR_HEADLESS=1` for CI-style runs
- cleanup of stale emulator, Xvfb, and x11vnc processes for repeatable reruns

## Known Gaps

- The full `run_e2e_ci.sh` path still needs to be re-run from a clean emulator
  and treated as the acceptance check for this foundation.
- The framework is Android-only. Web parity through headless Chrome is still a
  later step.
- The test depends on a current package publication being available to the
  local package server.
- Offline package sync is automated enough for the first smoke test, but it may
  need hardening once more clean-emulator runs are collected.
- The suite is not wired into GitHub Actions or any required merge gate yet.

## Next Step

Run:

```sh
./ui/android-app/scripts/run_e2e_ci.sh --with-vnc
```

Fix any clean-emulator failures, then make the same command run headless under
CI before expanding the suite.
