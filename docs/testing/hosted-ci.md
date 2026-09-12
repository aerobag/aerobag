# Hosted CI Invariants

Read this before changing or diagnosing GitHub Actions. Aerobag's hosted tests
must prove the same contracts as local tests without inheriting local machine
state. A green run is meaningful only when its inputs, selected tests, and
readiness conditions are explicit.

## Test Topology

| Workflow | Scope | When it runs |
| --- | --- | --- |
| `.github/workflows/ci.yml` | Fixture-free Rust, web, Android JVM/static, formatting, and workflow checks | Every push and pull request |
| `.github/workflows/fixture-ci.yml` | Compact tests against the pinned `aerobag/test-artifacts` repository | Every push and pull request |
| `.github/workflows/heavy-fixture-ci.yml` | Full NMS NOTAM and NEXRAD fixture replays | Relevant pushes and pull requests; weekly schedule; manual dispatch |
| `.github/workflows/e2e-ci.yml` | Native Android, Chrome-on-Android, and headless web journeys | Every push and pull request |
| `.github/workflows/reuse.yml` | REUSE licensing compliance | Every push and pull request |

Keep independently actionable tests in independently named jobs or report their
JUnit cases through `tools/ci/junit_summary.py`. Do not collapse unrelated
failures into one suite-wide boolean.

The green `CI` workflow is not the status of `E2E main`. Android shared journeys
already run all priorities on ordinary main/PR E2E runs; web runs p0 there and
adds p1/p2 for release tags, schedules, and explicit candidate runs. Inspect
existing E2E failures before staging, including still-relevant ancestor failures
when a newer push canceled its predecessor. Do not call a repeated application
timeout an infrastructure flake without examining its retained action/rendering
evidence. This does not add a second full qualification wait before staging.

## Cheap working-tree preflight

Before committing, run `/usr/bin/python3 tools/ci/cheap_preflight.py`. It works
with uncommitted changes and always runs the complete inexpensive suites:
fixture-free Rust tests/doctests and the hermetic service workload, Python tool
tests, web unit/type checks, Android JVM/static tests, all Node harness-contract
tests, actionlint, REUSE, Rust formatting, generated UI sources, fixture-contract
metadata, and diff checks. `--list` prints the exact commands. Rust core includes
the entire `ui_core_boundary` binary. Suite membership is shared with release
preflight, so adding a core UI action cannot bypass journey-coverage checks.

Preprocessor Cargo integration tests include `promotion_merge`: it invokes the
real Python activation controller and freshly built Rust merger on tiny generated
publication manifests. System-service/HTTP checks and app-byte validation are
stubbed; manifest selection, merging, channel symlinks, release endpoints and GC
roots are real. It covers promotion with a same-contract sunset plus a distinct
legacy contract, and confirms the merger still rejects unselected duplicates.
No downloaded fixtures, browser, emulator, or production access is needed.

Use warm target workspaces and tool caches. This command avoids web/WASM/native
app packaging, browsers/emulators, and external fixture replays. It does not
create release-qualification receipts. Every run retains its logs and has a
180-second deadline per suite, including compilation; a timeout is a failure,
not skipped coverage. `--timeout-seconds` is an explicit override for cold setup.
Generated schemas, wires, conformance data and symbols are compared in temporary
paths first, before Android generation can overwrite stale checked-in files.
The run also fails if source files or HEAD change while checks are in progress.

The Node harness contracts also have a standalone ordinary-CI job. They run
without waiting for the web build or launching a browser. The full release
preflight retains all ordinary-CI checks, including app builds and startup smoke.

Cheap preflight, fast/full release preflight, and the hosted harness job all use
`ui/web-app/scripts/run-target-workspace.sh inner:test:harness`. This prepares
lockfile-controlled dependencies and exports their workspace before selecting
every `tools/e2e/*.test.mjs`. The harness owns a separate workspace from the
parallel web checks; it must not depend on their setup or source-tree
`node_modules`. A cold, offline bootstrap regression uses a tiny local package
and runs the real entrypoint with the workspace environment unset.

Android JVM tests include small Compose/Robolectric component tests, with a
pinned Android SDK level and no production data or native-core initialization.
Use physical `performTouchInput` for input routing and layering: semantic
`performClick` bypasses hit testing and can make an untappable control look
functional. `MapSurfaceLayersTest` includes a reproduction of a non-consuming
full-screen editor intercepting HOME, plus portrait/landscape tests of the
production layer container and uncovered map input. This belongs in ordinary
CI and cheap preflight, not in another release-only gate.

For navigation effects, exercise real removal from and reentry into composition,
not just rerendering an always-mounted helper. `AirwayRoutingNavigationEffectTest`
distinguishes a newly activated editor from mounting an existing draft; persistent
state must not replay a one-shot navigation request. These component tests use
Compose's controlled synchronization, without sleeps, emulator startup, or FAA
fixtures. Their pinned test-library/SDK dependencies still require cold setup.

For overlapping async UI actions, control completion order instead of repeating
a journey until a race appears. `cloudActionFeedback.test.tsx` mounts the actual
React Cloud page in a per-file jsdom environment, clicks its DOM buttons, and
uses manually resolved/rejected promises at its action boundary. Resolve the
newer action, flush React with `act`, assert its rendered feedback, then finish
the older action and assert that feedback remains. Cover stale successes and
errors, normal ordering, and legitimate latest-action errors. No sleep, timer
advance, polling, cloud server, native core, or browser startup is involved.
These tests belong to the existing web unit suite and therefore cheap preflight
and ordinary CI. They test feedback ownership, not actual clipboard permissions
or browser hit testing; keep representative real-platform journeys for those.

Raster request tests cover both slow success and stalled-request recovery.
`RasterTileImage.test.tsx` controls DOM load/error events and the recovery clock:
elapsed time must not cancel pending images, either request can win, and a tile
fails only after both attempts report errors. The p0 `web.raster-slow-loads`
journey uses a fresh browser context and delays real fixture tile responses by
4.5 seconds, then requires every planned tile to render. This reproduced the
L41 blank-chart bug before the fix; fast local tile loads and the older
`web.raster-load-recovery` stall test did not cover slow successful transfers.

Harness model tests establish that a journey rejects modeled defects; they do
not establish that platform navigation or rendering works. New or changed
journeys require a focused real run on every claimed platform, with matching
application bytes. Preserve that evidence and explicitly report unrun checks.

Python preflight also verifies producer telemetry contracts. Hosted Python CI
fetches history/tags and compares immutable definitions and monitoring coverage
against the push/PR base SHA, not just the current tree. Both paths compare
against production and the checked-in coverage baseline. See
[telemetry contracts](../../contracts/telemetry/README.md) before adding a
measurement or intentionally removing coverage.

## Hermetic Inputs

Fixture-free jobs must make the absence of production data explicit. Core tests
set `AEROBAG_ARTIFACT_READ_PATH` to an empty runner-owned directory. A local
`/root/aerobag-artifacts` tree once allowed an overbroad ignored-test selection
to pass, so an unset artifact path is not evidence that a test is fixture-free.

Web jobs must also be independent of source-tree `node_modules`. Test and build
dependencies belong in `ui/web-app/package.json` and are installed by CI. The
target-workspace entrypoint is:

```sh
./ui/web-app/scripts/run-target-workspace.sh
```

Some scripts are symlinked into that workspace. Module lookup must start from
`process.cwd()` rather than `import.meta.url`, because the latter resolves the
symlink back into the source tree and can silently consume a developer's
`ui/web-app/node_modules`.

System packages are dependencies too. Declare them in the narrowest job that
uses them. For example, only the plate screenshot journey installs
`python3-pil` in ordinary E2E; the heavy NEXRAD replay also declares it because
the source-grid tiler imports Pillow.

## Fixture Ownership

`test-artifacts.lock.json` is the authority for the artifact repository commit,
fixture path, and fixture contract version. Fetch fixtures with:

```sh
python3 tools/ci/fetch_test_artifacts.py \
  --fixture <fixture-name> \
  --destination "$AEROBAG_TEST_ARTIFACTS_ROOT"
```

The helper performs a sparse fetch of only the selected fixture subtree. Do not
replace the compact Android publication with the approximately 19 GiB
production publication.

Heavy fixture replays run for changes under `product/preprocessor/**`,
`crates/**`, or `test-artifacts.lock.json`, as well as changes to their workflow
and fixture-fetch/report helpers. They also run every Monday at 09:23 UTC and
through manual dispatch. Keep this path-filtered workflow non-required; GitHub
leaves a required workflow pending when path filters skip it.

NOTAM admission/identity/effects/expiry changes also need the full NMS replay and
a reviewed fixture-expectation update; a green cheap preflight cannot establish
that the external golden remains current. The fixture-free `projection_test`
cases cover real collector-to-client transitions for airport AIRSPACE, NAV,
OBST, and SVC, including expiry, duplicates, cancellation, and server-only
facilities. See the [September 11 audit](notam-projection-audit-2026-09-11.md)
for the missed expectation update that left Heavy Fixture CI red.

A narrowly scoped fixture job must select its exact test or test family.
`--run-ignored ignored-only` is not sufficient: it selects every ignored test
that survives the other filters. For example, the METAR fixture job adds:

```sh
-E 'test(/generic_metar_delta_fixture_reconstructs_three_hour_capture$/)'
```

When a fixture's structure changes, update its contract version and the lock
manifest. Do not weaken the consumer with field-level fallbacks.

NAVDB rollover no longer fetches two historical FAA cycles. The permanent
[logical source](../../crates/nav-db-fixture/README.md) lives in this repo and
generates initial, changed, and rejected NAVDBs through the production encoder.
Its Rust regression is an ordinary, non-ignored test in application-core CI;
there is no duplicate fixture job. Shared-crate CI tests the generator and
preprocessor CI tests its publication windows. The web lab uses the same generator. No FAA
calendar turnover or external fixture-repository publication is involved.

When the production NAVDB contract changes, review/migrate that source descriptor
and records. Regenerate the independent smoke and release-journey fixtures from
one available publication, and publish those together:

```sh
python3 tools/ci/build_e2e_package_fixture.py \
  --source-publication /path/to/published \
  --output /path/to/test-artifacts/e2e/android-smoke-publication \
  --cycle <available-cycle>
python3 tools/ci/build_release_journey_fixture.py \
  --source-publication /path/to/published \
  --output /path/to/test-artifacts/e2e/release-journey-publication \
  --primary-cycle <available-cycle> --had-query /path/to/had_query \
  --live-feed-source /path/to/pinned/live-feeds/fresh
python3 tools/ci/verify_fixture_contracts.py \
  --fixture-root /path/to/test-artifacts \
  --fixture android-smoke-publication \
  --fixture release-journey-publication
```

Commit and push the artifact repository first, then update its commit in
`test-artifacts.lock.json`. Fixture-backed jobs run the same contract check
immediately after sparse checkout, before expensive setup.

## Nextest Results

`CARGO_TARGET_DIR` controls compiled outputs, but nextest's configured JUnit
path remains relative to the Cargo workspace. The current reports are:

```text
crates/target/nextest/ci/junit.xml
ui/core-rust/target/nextest/ci/junit.xml
product/preprocessor/target/nextest/ci/junit.xml
```

Keep workflow summary and artifact-upload paths pointed there unless the
nextest configuration itself changes.

## Android Emulator

`avdmanager list avd` is authoritative for an AVD's path. Do not assume
`$HOME/.android/avd`; GitHub-hosted Android tooling may create it elsewhere.
After discovering the path, export its parent as `ANDROID_AVD_HOME` before
starting the emulator. Otherwise the emulator can reject an AVD that
`avdmanager` just created because it searches a different legacy directory.

`ui/android-app/scripts/start_emulator_stack.sh` owns this behavior. It also
enables the hardware keyboard in the discovered `config.ini`.

Every external readiness wait must:

- have a finite deadline;
- check whether the process being awaited has already exited;
- print the relevant log tail or state on failure.

Do not add an unbounded `adb wait-for-device`. It once hid an emulator startup
failure for roughly 20 minutes. The stack script now bounds adb discovery,
checks the emulator PID, and prints `emulator.log`.

Chrome's `chrome_devtools_remote` socket is a readiness condition, not an
application assertion. A cold hosted runner needed more than 15 seconds to
create it, so the deadline is 60 seconds. Increasing that readiness budget did
not weaken the journey's requirement that the socket appear or its live-feed
recovery assertions.

Desktop Chrome journeys use the DevTools pipe transport. Unlike the ephemeral
listener, it cannot lose a port race or fail while announcing a websocket URL
late in a long Android shard. Readiness is still proved by a bounded
`Browser.getVersion` CDP request. The pipe client also watches Chrome's process
exit, so inherited pipe descriptors cannot hide an early browser exit until
the readiness deadline.

Qualification installs the version/SHA256 in `tools/ci/test-browser.lock.json`
using `tools/ci/install_test_browser.py --destination DIR`. This sets `CHROME_BIN`
through `GITHUB_ENV` on hosted jobs; optional local qualification sets the same
binary explicitly. Never fall back to ambient Chrome on installation failure.
The cheap preflight does not install or launch a browser. Downloaded browser
files also avoid the captured cold runner-image I/O bottleneck; evidence and
limits of that diagnosis are in the [hardening work log](ci-hardening-plan.md).

`Chrome startup diagnostic` is a small non-qualifying workflow, separate from
release journeys. Its default tests the pinned browser; manual
`compare_system=true` compares cold/preloaded system Chrome as well. It retains
every first failure and executes real reset/reload boundary checks without an
app build. It cannot issue or satisfy release qualification receipts.

Release Android jobs install a same-signed instrumentation APK that serves the
actual rendered accessibility hierarchy over an adb-forwarded localhost port.
Do not replace rendered-node actions with direct app/core hooks. A visible,
enabled control is located in the hierarchy and activated through Android's
accessibility action. Text replacement uses the accessibility text action and
verifies the rendered value.

## Timing Rules

Start relative test clocks after expensive setup. The NAVDB rollover journey
once computed its transition timestamp before generating its publication.
Generation took longer than the 45-second delay on GitHub, so the browser never
saw the expected initial cycle. The publication generator now resolves a
relative delay from the current time only after package materialization.

Chrome-on-Android prepares generated web sources and WASM before starting Vite's
readiness deadline. Cold compilation has its own bounded build budget; the
server phase runs only `inner:serve:dev`, with no generation or compilation.
Do not put `inner:dev:fast` back inside the server-readiness wait. Supplying
`--web-url` uses an existing server and skips local preparation.

Keep this distinction explicit:

- readiness deadlines absorb legitimate runner variability;
- behavioral deadlines and assertions define the product contract.

Shared observations own deadlines even when a probe or event notification never
settles. Only `TransientObservationError` permits another read. A terminal
pre-action read forbids mutation; an action timeout aborts the journey, never
retries the action. The optional abort signal cannot undo already delivered
input. Failure diagnostics have a separate bound and retain the initiating error.
CDP load completion must match the requested frame/loader; old/foreign load
events and disconnected targets cannot masquerade as successful navigation.

Do not turn a product failure into a pass by adding a fallback or weakening an
assertion. First establish whether the failure is setup readiness, test timing,
or application behavior.

## Failure Signatures

| Symptom | First check |
| --- | --- |
| Fixture-free test passes only locally | Production artifact paths or other developer-owned data are still visible |
| Web E2E cannot import a package in CI | Dependency is undeclared, not installed in the target workspace, or resolved through the script's source symlink |
| More fixture tests run than the job owns | `--run-ignored` lacks an exact nextest expression |
| JUnit upload says the file is missing | Workflow points under `CARGO_TARGET_DIR` instead of the workspace-relative nextest path |
| Emulator says an AVD does not exist | Compare `avdmanager list avd` with `ANDROID_AVD_HOME` |
| Emulator job hangs without diagnostics | A readiness path still uses an unbounded wait |
| NAVDB rollover misses its initial cycle | A transition clock started before fixture generation completed |
| Plate journey fails before the app starts | Verify `python3-pil` is installed for that matrix row |
| Android Chrome fails before CDP connects | Inspect emulator diagnostics and the bounded DevTools-socket readiness wait |

Hosted failure artifacts are retained for 14 days. Inspect the uploaded journey
directory and `.ci/ui-target/android/emulator-stack-5900` before changing code.

## Repository Authentication

GitHub deploy keys are repository-specific. GitHub rejects attaching one key to
both `aerobag/aerobag` and `aerobag/test-artifacts` with "key already in use".
Use a dedicated key per repository, or use a GitHub App when one identity needs
access to multiple repositories.

The test-artifact lock uses a public HTTPS URL for reads. Write access for
publishing fixtures is a separate credential concern and must not be required
by CI test jobs.

## Release qualification and stability testing

The implementation/audit checklist and retained findings live in the
[CI hardening plan](ci-hardening-plan.md).

### Fast iteration without weakening qualification

Run `tools/ci/fast_release_preflight.py` on the integrated clean commit first.
Full local qualification reuses its complete, exact-commit ordinary-CI evidence
when the receipt and lane logs are available. It still runs every required
journey repetition and never carries journey passes across commits or retries.

Each attempt retains its own run directory, printed at startup. A new attempt
does not delete the previous failure, app bundle, fixture, or logs. Local full
qualification and focused diagnostics take a host-wide lock because their
emulator/fixture lanes use fixed ports. An overlapping invocation fails promptly.

For a failing journey, reuse the retained inputs through the diagnostic command:

```sh
python3 tools/ci/diagnose_release_journey.py \
  --from-run /tmp/aerobag-local-candidate-COMMIT \
  --platform android --journey shared.inspector-details --repetitions 20
python3 tools/ci/diagnose_release_journey.py \
  --from-run /tmp/aerobag-local-candidate-COMMIT \
  --platform web --journey shared.about-and-saved-state --repetitions 20 --net-log
```

This checks the bundle and fixture inputs, runs the existing lane setup and
cleanup, and records app/harness revision provenance in `diagnostic.json`.
The app is the **retained build**, not an automatic rebuild of the current tree.
Use it for harness iteration; rebuild the app after application changes.
Diagnostic results never create qualification receipts. A diagnostic pass after
a failure is not permission to retry the candidate into green.

Web journeys automatically retain `chrome-netlog.json` alongside failure
artifacts, including worker fetch evidence without attaching a worker debugger.
Failures before the journey starts also retain `runner-failure.json` with the
startup phase, Chrome process state before and after teardown, launch arguments,
and drained stderr. Inspect it when browser readiness fails before a
`result.json` can exist.
Default capture excludes sensitive payloads. Successful runs discard this log;
`--net-log` retains it during focused investigations. Use that evidence to
distinguish transport failures from application failures before changing waits.
Do not enable the DevTools Network domain merely for qualification diagnostics;
the independent netlog covers both page and worker requests. Startup navigation
is single-shot, including browser-canceled module fetches. Only read-only DOM
observations interrupted by navigation are transient, within their existing
deadline; clicks and application failures must never be retried into green.

Web test **reset** replaces the entire browser context, including its storage
and caches. Clearing origin data inside a reused context left intermittent
module-loading failures during immediate startup. Use the shared page lifecycle
helper, not a delay or a startup retry. A **reload** replaces only the page and
retains its browser context, so saved-state and offline-persistence assertions
still exercise the same storage. Permissions are scoped to that context.

### Complete workload

The normal low-latency release path is `tools/prod_manage.py --stage`: run the
cheap emulator-free ordinary-CI preflight, then start deployment and hosted
exact-release qualification concurrently. Do not insert a full local pass or a
hosted candidate round trip by default. Promotion still requires deployed
staging checks, ordinary CI, and the full exact-tag journey run to pass.

`--stage --watch` follows those checks after deployment instead of returning
immediately. Resume with `--qualification-status --watch`. It pins one release,
polls every 30 seconds, stops on failure/cancellation or full success, and has a
one-hour post-deployment budget (`--watch-timeout SECONDS` to override).
Interrupting or timing out the watch does not cancel jobs or promote anything.

`tools/prod_manage.py --prequalify` optionally runs the complete workload locally.
Ordinary CI lanes, three web priority lanes, four fresh Android shard lanes
(each spanning all priorities), and the native journeys run with the same
boundaries as the hosted matrix. The local qualifier isolates GUI-heavy phases
to avoid contention between emulators and browsers on this one host.
The local run builds one immutable app bundle, uses pinned fixtures, and runs
each release journey **once**, without trimming priorities or native checks.
It stops locally, without GitHub API credentials, a candidate-tag push, or a
hosted wait. It requires clean synchronized `main`; receipts are exact-commit,
not transferable to a different revision. The command runs fast preflight first
so those checks are cached for both the full run and a later `--stage` of the
same commit, even if a journey fails. A cached receipt is not a fresh test run.

#### Stability testing is not a release gate

Use `tools/ci/local_candidate_qualification.py --repetitions 5` for an explicit
full local stability check, or the focused diagnostic command above when
investigating one journey. Full stability receipts live separately under
`.git/aerobag-local-qualification/stability/`; they neither overwrite nor stand
in for the normal single-pass receipt. `--check --repetitions N` checks evidence
for that exact count. Every requested repetition must pass; this does not add
retry-to-green behavior.

Hosted manual runs can select `release_candidate=true` for all priorities and
`repetitions=5` (or another offered count) for repetition testing. Counts above
one use the `Journey stability` run title and cannot satisfy candidate or release
qualification. Normal release tags and optional `candidate-*` tags default to
one pass. `prod_manage.py --candidate-status` remains a read-only view of legacy
or manually requested hosted candidate runs, not local prequalification status.

#### Latency and local parallelism

September 8 measurements on the 20-core, 96-GiB dev host: fast preflight took
2m22s; the five-pass local workload took another 44m29s; its five-pass hosted
candidate took 58m04s. The subsequent
[one-pass hosted release](https://github.com/aerobag/aerobag/actions/runs/34264045527/attempts/1)
took 22m47s (excluding the later manual rerun of a failed browser startup).
Per-journey Android medians were about 2.4 times slower hosted, not
five times, and GitHub runs more lanes concurrently.

A one-pass local workload including fast checks is **estimated**, not yet
benchmarked end to end, at about 18 minutes with warm inputs/caches. With no
failures, adding it serially to a ~23-minute hosted run increases latency; use
it selectively, not as the normal release path. The measured staging deployment
itself took ~24 minutes and overlapped the hosted checks.

Ordinary CI already runs multiple lanes in parallel. Android defaults to two
concurrent emulators, each with fresh shard state. An explicit
`--android-workers 4` could theoretically save roughly three minutes on one
complete pass, but this is not a contention-tested result. No utilization trace
was retained for the earlier run, so spare hardware capacity is not proof that
more GUI concurrency is safe. Keep browser phases isolated from emulators;
benchmark a worker change before making it the default.

An Android baseline job prepares a commit-scoped app-data archive once. Each
Android matrix job clean-installs the immutable apps into a fresh AVD, then
restores that archive after `pm clear` before every journey and repetition.
The archive is shared only within that qualification run, not stored in the
fixture repository. Unlike a VM snapshot, this does not restore stale GPU
surfaces, clocks, sockets, or running app work.

The local qualifier mirrors the hosted matrix boundary: each shard gets a
fresh AVD and installation, and restores the run's prepared baseline. Reusing
one emulator across different shards is forbidden because GitHub does not do
so; it can hide startup contamination or invent order-dependent failures that
the hosted jobs cannot reproduce.

## Before Pushing

Check that:

- fixture-free jobs explicitly hide production artifacts;
- fixture jobs fetch only locked subtrees and select only owned tests;
- every dependency is declared in the workflow or package manifest;
- every external wait is bounded and diagnostic;
- relative clocks begin after expensive preparation;
- JUnit summary and upload paths match the workspace-relative nextest paths;
- a readiness adjustment does not weaken a behavioral assertion.
- every candidate journey repetition passes; no retry-to-green result is accepted.
