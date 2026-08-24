# End-to-End Test Status

Status as of 2026-08-24: native Android, Chrome-on-Android, and headless web
journeys run in GitHub Actions as independently reported E2E jobs. The
core-driven release-journey registry adds shared P0, P1, and P2 feature
coverage across web and Android; release tags run the complete registry and
publish one aggregate qualification result.

Hosted-runner invariants and failure diagnostics are documented in
[Hosted CI Invariants](testing/hosted-ci.md).

## What Exists

- `tools/e2e/release-journey-registry.mjs` is the canonical inventory of
  pilot-facing release journeys and their priorities.
- `tools/e2e/release-journey-implementations.mjs` implements shared web/Android
  journeys through platform semantic drivers.
- `tools/e2e/release_journey_lab.sh` is the stable local entry point for
  fixture, build, web, Android, and Cloud journey operations.
- `tools/e2e/android-harness.mjs` provides the shared adb/uiautomator helpers.
- `tools/e2e/run-android-e2e-suite.mjs` runs the Android suite.
- `tools/e2e/run-android-chrome-livefeed-e2e.mjs` runs the web live-feed
  reconnect suite inside Chrome on Android through adb/CDP.
- `ui/android-app/scripts/run_e2e.sh` builds, installs, and runs the suite
  against a selected device or emulator.
- `ui/android-app/scripts/run_e2e_ci.sh` is the CI-oriented wrapper. It can
  install the emulator system image, start a local package server, boot the
  repo emulator stack, install the APK, and run the suite.
- `ui/web-app/scripts/run_android_chrome_livefeed_e2e.sh` boots the same
  emulator stack, starts a scripted live-feed server and Vite, launches Android
  Chrome, and runs the web live-feed reconnect suite without using a physical
  tablet.
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
- `android.plate-first-render-smoke`
  - Launches the app and ensures offline packages are ready.
  - Searches the chart for `KPLU`.
  - Opens the airport inspector's `Plates` action.
  - Selects the first plate in the folder.
  - Captures the screen and verifies the plate canvas is visibly painted on the
    first open.
- `android.raw-map-inspector-terrain-smoke`
  - Centers the chart on KPLU, dismisses the search-opened airport inspector,
    and physically taps an unoccupied point southeast of the airport.
  - Verifies that the raw-click inspector selects `SPOT` and displays a terrain
    elevation, exercising Android session-resource paging. This test requires
    the ordinary NW terrain package and runs in the local full-publication
    suite rather than the compact fixture CI matrix.
- `android.map-follow-ctr-gesture-smoke`
  - Performs the same route setup.
  - Enables the Bad Autopilot debug ownship source.
  - Selects the synthetic ownship source and waits for an ownship render probe.
  - Ensures CTR follow is engaged.
  - Drags the map and verifies CTR remains engaged with the aircraft offset
    from center.
  - Zooms the map and verifies CTR remains engaged with that offset preserved.
- `android.layer-toggle-navdb-regression`
  - Opens the chart Layers tray against the installed NAVDB.
  - Disables terrain and enables NEXRAD.
  - Verifies both commands are accepted and the projected layer state changes.
- `android.rotation-session-retention-regression`
  - Creates `KRNT KPWT KPLU`, activates its destination leg, and records the
    rendered row order, labels, count, and core-projected active-leg identity.
  - Alternates real portrait and landscape bounds six times while requiring a
    stable process, responsive navigation, the same active plan, and a painted
    chart route after every recreation.
  - Seeds a generated persisted NOTAM cache and uses a debug-only private-file
    gate to rotate before one-shot promotion, then verifies the product reaches
    Data Status and rotates again after promotion.
  - Fails on an Aerobag ANR, fatal exception, process death, or consumed
    prepared-projection error and retains its transcript, signatures, logcat,
    screenshots, and window diagnostics.
- `android.chrome.live-feed-network-recovery`
  - Starts a strict local live-feed v3 server with a METAR product.
  - Starts Vite with `AEROBAG_LIVE_FEEDS_ORIGIN` pointed at that server.
  - Launches Chrome on Android, not the native Android app.
  - Waits for the live-feed connection and METAR version `v1`.
  - Publishes `v2` over SSE and verifies the Data Status projection advances.
  - Forces Chrome offline, drops the SSE connection, and verifies a reconnect
    backoff is pending.
  - Publishes `v3`, restores Chrome online, and verifies the online event opens
    a new EventSource before the old backoff could expire.
  - Verifies Data Status advances to METAR version `v3` and that at most one
    EventSource remains active.

## App Test Hooks

The Android UI now exposes stable `parity:` tags for the smoke test:

- disclaimer accept button
- chart search field and suggestions
- plate folder tiles and plate canvas bounds
- debug Bad Autopilot flag and ownship source controls
- flight-plan route overlay semantic probe
- map-follow semantic probe
- rendered flight-plan state and Data Status row semantic probes

Painted canvas content is not directly visible to `uiautomator`, so the E2E
checks use semantic probe tags for map overlays and screenshot analysis for
plate imagery:

```text
parity:flight-plan-route-overlay:segments:<count>:visible:<count>
parity:map-follow-state:following:<0|1>:ownship-x:<px>:ownship-y:<px>:center-x:<px>:center-y:<px>:zoom-centi:<zoom*100>
```

The web app also exposes a read-only browser hook for Chrome-on-Android tests:

```text
window.__aerobagE2e.liveFeeds()
```

That hook returns core-projected Data Status live-feed rows plus adapter
counters for EventSource opens, errors, reconnect timers, and online events.

## Emulator Support

The emulator stack scripts now support:

- deterministic emulator serial selection from `VNC_PORT`
- VNC-backed local runs
- `EMULATOR_HEADLESS=1` for CI-style runs
- cleanup of stale emulator, Xvfb, and x11vnc processes for repeatable reruns

## Package Fixture

CI sparse-checks out
`e2e/android-smoke-publication` from the commit pinned in
`test-artifacts.lock.json`. The compact frozen publication contains a full
production NAVDB package matching the current client contract and a
contract-valid TPP1 package restricted to KPLU. It exercises the ordinary
Offline Packages discovery, download, checksum, install, and runtime adoption
paths without depending on the 19 GiB production publication. Because that
publication contains only the packages needed by the suite, CI syncs every
available package and skips searching for absent regional toggles; tests
against a full publication retain the NW-only selection flow.

The rotation job additionally sparse-checks out
`e2e/android-rotation-live-feed`, a generated canonical empty NOTAM checkpoint
in Android's persisted cache layout. It contains no operational records and
does not contact a live-feed server.

## Known Gaps

- Emulator system image installation dominates cold-run setup time; Gradle and
  Rust outputs are cached, while the Android state itself starts clean.
- The release-journey fixture commit pinned by `test-artifacts.lock.json` must
  be present in the GitHub `aerobag/test-artifacts` mirror before hosted release
  qualification can run.

## Local Reproduction

Run:

```sh
./ui/android-app/scripts/run_e2e_ci.sh --with-vnc
```

For the registry-driven release lab, use:

```sh
./tools/e2e/release_journey_lab.sh foundation
./tools/e2e/release_journey_lab.sh web-dist-suite p0
./tools/e2e/release_journey_lab.sh android-suite p0
```

Use `--headless` to reproduce the hosted-runner display mode.

Run only the rotation/session-retention regression with:

```sh
./ui/android-app/scripts/run_e2e_ci.sh \
  --headless \
  --test android.rotation-session-retention-regression
```

For the Android Chrome live-feed recovery suite:

```sh
./ui/web-app/scripts/run_android_chrome_livefeed_e2e.sh --with-vnc
```

Use `--headless` in CI, or `--no-start-emulator --serial emulator-5554` when an
emulator is already running.
