# Android End-To-End UI Tests

Android E2E tests drive a real installed app with `adb`. Every journey uses a
persistent instrumentation process to read and act on the rendered
accessibility tree; there is no slower `uiautomator` fallback path.
They exercise Android UI plus core through the same controls a pilot uses.

Run the CI-oriented entrypoint:

```sh
./ui/android-app/scripts/run_e2e_ci.sh
```

That script provisions the Android emulator path: it ensures the configured
system image is installed, serves `/packages/current_artifacts.json` if needed,
starts the repo emulator stack, installs the app, syncs the NW offline package
set through the UI on a clean emulator, and runs the suite.

`CI=1` makes the emulator headless. Local runs keep VNC enabled by default; use
`--headless` to match CI locally, or `--with-vnc` to force an inspectable
emulator. The VNC port follows the existing `VNC_PORT` mapping, for example
`VNC_PORT=5902 ./ui/android-app/scripts/run_e2e_ci.sh --with-vnc` exposes
`localhost:5902`.

Run the suite:

```sh
./ui/android-app/scripts/run_e2e.sh
```

Run against an already-installed app:

```sh
./ui/android-app/scripts/run_e2e.sh --skip-install
```

Select a specific device or emulator:

```sh
./ui/android-app/scripts/run_e2e.sh --serial emulator-5560
```

The runner preserves installed package data and clears only volatile UI state
before launch. If the app starts on Offline Packages, the runner syncs the NW
package set through the app UI by default. Use `--no-sync-offline-packages` for
device-local debugging when missing packages should fail immediately.

CI still needs a current publication root. The package server must expose:

```text
/packages/current_artifacts.json
```

`run_e2e_ci.sh` starts `tools/run_dev_stack.py` as a package server when that
URL is not already available.

## Current Tests

- `android.flight-plan-route-smoke`: launch the app, enter flight plan
  `KRNT KPWT`, switch to the chart page, and assert that Android exposes a
  flight-plan route overlay with at least one visible segment.
- `android.map-follow-ctr-gesture-smoke`: launch the app, enter the same
  route, enable/select Bad Autopilot as a deterministic ownship source, engage
  CTR, then assert that drag and zoom gestures keep CTR engaged while preserving
  an off-center ownship offset.

## Testability Contract

The harness uses stable Android Compose test tags exported through
`testTagsAsResourceId`. Tags intended for cross-platform parity or E2E coverage
use the `parity:` prefix.

The release driver never calls a core action directly. It finds a visible,
enabled node in the rendered hierarchy and invokes that node's ordinary Android
accessibility action. This preserves user-facing reachability while avoiding
coordinate guesses and multi-second `uiautomator` process startup for every
probe.

Native journey code is statically forbidden from bypassing this contract with
raw `adb shell input`. Map gestures and key events must also declare exact
readiness and a visible completion condition.

Painted canvas content is not directly visible to the accessibility tree, so canvas
render paths that need E2E coverage should expose a small semantic probe. The
route smoke test uses:

```text
parity:flight-plan-route-overlay:segments:<count>:visible:<count>
```

The CTR regression test uses:

```text
parity:map-follow-state:following:<0|1>:ownship-x:<px>:ownship-y:<px>:center-x:<px>:center-y:<px>:zoom-centi:<zoom*100>
```

These probes keep assertions tied to the platform UI render path while avoiding
business-logic duplication in the test runner.
