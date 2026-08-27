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

A narrowly scoped fixture job must select its exact test or test family.
`--run-ignored ignored-only` is not sufficient: it selects every ignored test
that survives the other filters. One NAVDB job accidentally selected 27
unrelated ignored tests until it added:

```sh
-E 'test(/real_nav_db_2608_to_2609_advance_preserves_rich_session$/)'
```

When a fixture's structure changes, update its contract version and the lock
manifest. Do not weaken the consumer with field-level fallbacks.

When the production NAVDB contract changes, regenerate both compact NAVDB
fixtures from one publication and publish them together:

```sh
python3 tools/ci/build_e2e_package_fixture.py \
  --source-publication /path/to/published \
  --output /path/to/test-artifacts/e2e/android-smoke-publication \
  --cycle 2608
python3 tools/ci/build_nav_db_advance_fixture.py \
  --source-publication /path/to/published \
  --output /path/to/test-artifacts/nav-db/advance-2608-to-2609 \
  --cycle 2608 --cycle 2609
python3 tools/ci/verify_nav_db_fixture_contracts.py \
  --fixture-root /path/to/test-artifacts \
  --fixture android-smoke-publication \
  --fixture nav-db-advance
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
`Browser.getVersion` CDP request.

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

Keep this distinction explicit:

- readiness deadlines absorb legitimate runner variability;
- behavioral deadlines and assertions define the product contract.

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

## Release Stability Gate

`tools/prod_manage.py --prequalify` first runs the complete workload locally.
Ordinary CI lanes, three web priority lanes, four persistent Android shards, and
the native journeys are parallelized where they do not share generated source.
The local run builds one immutable app bundle, uses pinned fixtures, and requires
five successful repetitions per release journey.

After that succeeds, the command pushes synchronized `main` under a
`candidate-*` tag. The hosted run repeats the complete registry. `--stage`
rejects a commit without both the exact-commit local receipt and hosted result,
so expensive production reconciliation cannot precede either proof. The final
release tag still runs one complete exact-tag qualification.

Within each Android matrix job, clean installation and package sync happen
once. A job-local emulator snapshot then restores identical prepared state
before every journey and repetition. Snapshots are not shared between jobs or
persisted in repository artifacts.

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
