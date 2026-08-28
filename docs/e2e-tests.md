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
- `tools/e2e/android-harness.mjs` provides shared adb helpers and a persistent
  instrumentation-backed rendered-accessibility driver.
- `tools/e2e/transition-contract.mjs` defines the shared readiness, single-action,
  completion, and stability-sampling contracts plus named timing classes.
- `tools/e2e/journey-structure-audit.mjs` rejects fixed UI sleeps, mutations in
  observation loops, and unnamed journey deadlines before a journey can reach CI.
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

Release journeys reuse one immutable fixture process per lane for speed, but reset its
mutable fault/publication controls before every journey and repetition. Set
`AEROBAG_RELEASE_JOURNEY_REUSE_FIXTURE=0` to force process replacement while
debugging fixture lifecycle behavior.

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
  - Uses the immutable fixture's declared georeferenced-plate capability rather
    than assuming a particular airport or plate exists.
  - Opens the airport inspector's `Plates` action.
  - Selects the capability-addressed plate in the folder.
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
contract-valid TPP1 package containing only the declared journey fixtures. It exercises the ordinary
Offline Packages discovery, download, checksum, install, and runtime adoption
paths without depending on the 19 GiB production publication. Because that
publication contains only the packages needed by the suite, CI syncs every
available package and skips searching for absent regional toggles; tests
against a full publication retain the NW-only selection flow.

The rotation job additionally sparse-checks out
`e2e/android-rotation-live-feed`, a generated canonical empty NOTAM checkpoint
in Android's persisted cache layout. It contains no operational records and
does not contact a live-feed server.

## Isolation And Speed

Journeys follow one deterministic state-transition shape:

1. Observe that the intended control is rendered, reachable, and enabled.
2. Deliver the user action exactly once.
3. Observe the semantic or painted result without retrying the action.

Polling is allowed only for read-only observation. Scrolling a lazy list is an
explicit action, not a side effect hidden in a probe. Behavior that must remain
true after an action is sampled throughout a named stability interval rather
than checked after a sleep. Local user actions have a three-second response
budget; startup, resource loading, cloud convergence, and package sync use
separate named budgets so increasing an external-operation allowance cannot
hide an unresponsive button.

The transition helper enforces that three-second ceiling at runtime. Longer
waits are separate phases with names that expose what the test is actually
waiting for: local resource computation, replay progression, an animation
cycle, Android activity recreation, startup, remote consistency, or a bulk
operation. There is deliberately no generic "observation" timeout. A new
journey must either prove an ordinary UI postcondition promptly or declare the
specific asynchronous boundary it exercises.

The foundation suite statically audits both shared and Android-native journeys
for these rules. A failed action is a failed journey; the runner never repeats
typing, clicking, or navigation until it happens to pass.

Release fixtures declare the data capabilities each journey consumes. The
fixture builder validates those preconditions and injects stable synthetic data
where the assertion is about rendering rather than an upstream snapshot's
incidental contents. A missing prerequisite therefore fails fixture construction
instead of spending a journey deadline waiting for an impossible UI state.

Each hosted Android job still performs a real clean install, package sync, and
startup-navigation journey. It then saves an emulator snapshot of that prepared
state and restores it before each assigned behavior journey. The snapshot is
created inside the job rather than uploaded across machines, so it cannot hide
system-image or installation drift.

The persistent Android semantic driver reads the actual accessibility tree
rendered by Compose. Actions invoke the ordinary accessibility click or scroll
operation on a visible, enabled rendered node; they do not invoke core actions
directly or add random coordinate jitter. Keeping the driver process alive
removes the roughly two-second cost and process-race exposure of each standalone
`uiautomator dump`. The action endpoint also closes the probe/action race: if
Compose replaces a previously observed node, it waits on accessibility events
until Android accepts one action against the currently rendered replacement.
It never retries an action that Android accepted.

Web control actions verify that the rendered element is unobstructed and invoke
its ordinary DOM activation in the same browser task. This preserves the real
application click handler while eliminating the race where a render moves a
control between reading its coordinates and sending a later CDP pointer event.
Spatial behavior such as map selection, pan, zoom, and slider gestures still
uses browser pointer input because hit location is part of those contracts.

Set `AEROBAG_RELEASE_JOURNEY_REPETITIONS=N` to require every selected journey
to pass `N` times. Each repetition has separate diagnostics, and the suite
stops failed rather than retrying until green.

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

Before staging a release, run the local and hosted stability gate:

```sh
tools/prod_manage.py --prequalify
```

The local phase runs ordinary CI and all P0/P1/P2 journeys from the same commit
that will be pushed. Its receipt is invalidated if either workflow, the local
runner, or the journey lab changes. GitHub's four Android matrix shards receive
separate hosts. Local qualification preserves those four shards but defaults to
two concurrent emulators so host contention does not become a false product
latency failure; `--android-workers` can override that capacity setting.

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
