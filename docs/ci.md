# Continuous Integration

<!--
SPDX-FileCopyrightText: 2026 Aerobag contributors

SPDX-License-Identifier: AGPL-3.0-or-later
-->

GitHub Actions runs the fixture-independent test tier on pushes to `main` and
on pull requests. The workflow is split into stable checks so a failure names
the affected surface:

- Rust formatting
- Rust shared crates
- Rust application core
- Rust preprocessors
- Web build and unit tests
- Android JVM and static tests
- Python tool tests

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

Those tests fail if explicitly selected without the required fixture. They will
become a separate CI tier after the fixture repository is published.

Android emulator journeys, Chrome-on-Android journeys, and other end-to-end
scenarios are also a later CI tier. See [End-to-End Tests](e2e-tests.md).

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
  tools/ci/test_junit_summary.py \
  product/preprocessor/scripts/test_build_multi_version_publication.py \
  product/preprocessor/scripts/test_pipeline_health.py \
  product/preprocessor/scripts/test_watch_build_log.py
```
