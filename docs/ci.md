# Continuous Integration

<!--
SPDX-FileCopyrightText: 2026 Aerobag contributors

SPDX-License-Identifier: AGPL-3.0-or-later
-->

GitHub Actions runs fixture-independent and compact fixture-backed tests on
pushes to `main` and on pull requests. The workflows are split into stable
checks so a failure names the affected surface:

- Rust formatting
- GitHub Actions workflows
- Rust shared crates
- Rust application core
- Rust preprocessors
- Web build and unit tests
- Android JVM and static tests
- Python tool tests
- NAVDB fixture tests
- METAR fixture tests
- Android native E2E journeys
- Chrome-on-Android live-feed recovery
- Web NAVDB rollover

Rust, web, Android, and Python test runners emit JUnit XML. The workflow adds
every test case to the GitHub step summary, annotates failures, and retains the
raw reports as workflow artifacts for 14 days.

## Fixture Boundary

The default tier must run from a fresh clone without a cycle-data publication
or the external `aerobag-test-artifacts` repository. Tests that query a
generated NAVDB are marked ignored and require a compatible publication through
`AEROBAG_ARTIFACT_READ_PATH` (or an explicit `AEROBAG_FIXTURE_*` override).

Tests that exercise the captured NAVDB transition, METAR, NEXRAD, or NOTAM data
are also marked ignored and require:

```sh
export AEROBAG_TEST_ARTIFACTS_ROOT=/path/to/aerobag-test-artifacts
```

Those tests fail if explicitly selected without the required fixture.
`test-artifacts.lock.json` pins the fixture repository to one full commit and
declares the expected contract version for every fixture family. The
`Fixture CI` workflow uses `tools/ci/fetch_test_artifacts.py` to perform a
blob-filtered sparse checkout of only the fixture needed by each job, then
validates the checked-out commit and manifest contract before running tests.

NAVDB, METAR, and the compact Android package publication are small enough to
run on every pull request. The full NEXRAD transform and NMS NOTAM recovery
matrix run each Monday and on manual `Fixture CI` dispatches. These large jobs
remain independent so their download, runtime, and failures are visible
separately.

The NMS NOTAM trace is intentionally run optimized because it parses roughly
250 MB of raw XML before exercising the incremental recovery matrix:

```sh
(cd product/preprocessor && \
  AEROBAG_TEST_ARTIFACTS_ROOT=/path/to/aerobag-test-artifacts \
  cargo test --release -p nms-notams-fetch \
    captured_nms_trace_converges_across_checkpoint_and_catchup_schedules \
    -- --ignored)
```

`End-to-end CI` runs each native Android scenario as a separate matrix job,
plus the Chrome-on-Android live-feed recovery journey and the headless web
NAVDB rollover journey. See [End-to-End Tests](e2e-tests.md).

## Local Commands

Install `cargo-nextest`, then run individual Rust workspaces from their roots:

```sh
(cd crates && cargo nextest run --workspace --profile ci)
(cd ui/core-rust && \
  AEROBAG_ARTIFACT_READ_PATH=/tmp/aerobag-no-artifacts \
  cargo nextest run --workspace --profile ci)
(cd product/preprocessor && cargo nextest run --workspace --profile ci)
```

Run the platform and tool lanes with:

```sh
AEROBAG_ARTIFACT_READ_PATH=/tmp/aerobag-no-artifacts \
  npm --prefix ui/web-app run ci

ANDROID_BUILD_NATIVE_LIBRARIES=false \
  AEROBAG_ARTIFACT_READ_PATH=/tmp/aerobag-no-artifacts \
  ./ui/android-app/scripts/test.sh testDebugUnitTest

python3 -m pytest \
  tools/test_admin_index.py \
  tools/test_chart_cutline_editor.py \
  tools/ci/test_build_e2e_package_fixture.py \
  tools/ci/test_fetch_test_artifacts.py \
  tools/ci/test_junit_summary.py \
  product/preprocessor/scripts/test_build_multi_version_publication.py \
  product/preprocessor/scripts/test_pipeline_health.py \
  product/preprocessor/scripts/test_watch_build_log.py
```

Fetch and run one pinned fixture lane locally with:

```sh
python3 tools/ci/fetch_test_artifacts.py \
  --fixture nav-db-advance \
  --destination /tmp/aerobag-test-artifacts

(cd ui/core-rust && \
  AEROBAG_TEST_ARTIFACTS_ROOT=/tmp/aerobag-test-artifacts \
  cargo nextest run --profile ci --locked --package app-core \
    --run-ignored ignored-only)
```
