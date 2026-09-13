<!-- SPDX-FileCopyrightText: 2026 Aerobag contributors -->
<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->

# Live-Feed Provider Cutover

`shared.live-feed-provider-cutover` is a shared native Android/headless web p0
journey, selected by the existing main/PR and release registry. It uses the
ordinary fixture server's `cutover` profile and `POST /__control` hooks. No
production runtime Python or platform sharing behavior is involved.

The profile replaces METARs with two independently seeded, immutable resource
maps. Other products and the NAV25 packages come from the pinned compact
release fixture. Unknown resources return 404; they are never synthesized.
The journey establishes a flight plan and layer choices, renders `CUTOVEROLD`,
holds an old snapshot response, switches the binding at the same URL while
retaining its SSE stream, and observes an old-only manifest's 404. It releases
the outstanding old response, explicitly drains SSE, and requires real rendered
`CUTOVERNEW` and `CUTOVERNEXT` weather with preserved plan, viewport, map family,
and layers. The new manifest advertises a retired independent delta base; the
client must fetch its full snapshot, not request that delta.

Fixture HTTP tests verify the held response completes across the switch. Core
tests separately prepare an old response, install new state, and prove the old
completion is rejected. An old response may finish before the replacement
catalog is installed; the app test does not assert that the pre-catalog display
never changes. It asserts convergence and continued updates without reload or
reset after initial setup.

## Dependencies And Commands

HTTP/harness tests need Node 20+, loopback listeners, and no downloaded data.
Core tests need the existing Rust workspace dependencies and no external
fixtures. Neither test adds dependencies to the existing test framework:

```sh
node --test tools/e2e/live-feed-cutover.test.mjs tools/e2e/fixture-isolation.test.mjs
mkdir -p /tmp/cutover-no-artifacts
AEROBAG_ARTIFACT_READ_PATH=/tmp/cutover-no-artifacts cargo test \
  --manifest-path ui/core-rust/Cargo.toml -p app-core --test live_feed_provider_cutover
./ui/web-app/scripts/run-target-workspace.sh inner:test:harness
```

Real runs need the pinned compact NAV25 release fixture, an E2E-enabled web
bundle and Chrome, or an E2E APK plus its matching semantic-driver APK and
Android SDK/API 34 emulator. Use the existing fixture fetch/materializer from
`docs/testing/hosted-ci.md` and `.github/workflows/e2e-ci.yml`. Do not use
`fixture-build` with its production-data defaults. For APK builds, use the
workflow's disposable `keytool` signing key, not production credentials.

Example isolated web lane (choose unused ports before starting):

```sh
export AEROBAG_RELEASE_JOURNEY_FIXTURE=/path/to/materialized/fixture.json
export PACKAGE_SOURCE_PORT=22223
export AEROBAG_RELEASE_JOURNEY_LAB_STATE_DIR=/tmp/cutover-web-lab
export AEROBAG_E2E_URL=http://127.0.0.1:22223/
tools/e2e/release_journey_lab.sh web-build
tools/e2e/release_journey_lab.sh fixture-start-web cutover
AEROBAG_E2E_ARTIFACT_DIR=/tmp/cutover-red AEROBAG_E2E_CUTOVER_SUPPRESS_RESYNC=1 \
  tools/e2e/release_journey_lab.sh web-dist-test shared.live-feed-provider-cutover
AEROBAG_E2E_ARTIFACT_DIR=/tmp/cutover-green \
  tools/e2e/release_journey_lab.sh web-dist-test shared.live-feed-provider-cutover
tools/e2e/release_journey_lab.sh fixture-stop
```

The negative command **must fail** at `cutover rendered METAR count 2`, after
old weather and retained-stream checks pass and a new SSE connection is
observed. `AEROBAG_E2E_CUTOVER_SUPPRESS_RESYNC=1` withholds new catalogs and events
at the fixture transport, including HTTP catalog fallback, while still
accepting reconnects and serving correct full-state bytes. It does not patch
the client. Keep its failing `result.json`, screenshot, and network evidence;
never accept an unrelated startup failure as the negative control.

Example native lane, using only its own AVD and device reverse mapping:

```sh
export VNC_PORT=5963 DISPLAY_NUM=:63 EMULATOR_CONSOLE_PORT=5680 EMULATOR_ADB_PORT=5681
export ANDROID_SERIAL=emulator-5680 AVD_INSTANCE_NAME=aerobag34-cutover-5963 EMULATOR_HEADLESS=1
export PACKAGE_SOURCE_PORT=22225 ANDROID_PACKAGE_SOURCE_DEVICE_PORT=18093
export AEROBAG_RELEASE_JOURNEY_LAB_STATE_DIR=/tmp/cutover-native-lab
export AEROBAG_RELEASE_JOURNEY_ORIGIN=http://127.0.0.1:22225
export ANDROID_PACKAGE_SOURCE_BASE_URL=http://127.0.0.1:18093/packages/
export ANDROID_LIVE_FEED_SOURCE_BASE_URL=http://127.0.0.1:18093/live-feeds/
export ANDROID_CLOUD_SERVER_BASE_URL=http://127.0.0.1:18094/cloud/
./ui/android-app/scripts/start_emulator_stack.sh
tools/e2e/release_journey_lab.sh fixture-start cutover
AEROBAG_E2E_ARTIFACT_DIR=/tmp/cutover-native-red AEROBAG_E2E_CUTOVER_SUPPRESS_RESYNC=1 \
  ./ui/android-app/scripts/run_e2e.sh --apk /path/to/aerobag-release-e2e.apk \
  --driver-apk /path/to/aerobag-e2e-driver.apk --clear-app-data --sync-all-available-packages \
  --release-fixture "$AEROBAG_RELEASE_JOURNEY_FIXTURE" --test shared.live-feed-provider-cutover
tools/e2e/release_journey_lab.sh fixture-start cutover
AEROBAG_E2E_ARTIFACT_DIR=/tmp/cutover-native-green \
  ./ui/android-app/scripts/run_e2e.sh --skip-install --clear-app-data --sync-all-available-packages \
  --release-fixture "$AEROBAG_RELEASE_JOURNEY_FIXTURE" --test shared.live-feed-provider-cutover
tools/e2e/release_journey_lab.sh fixture-stop
./ui/android-app/scripts/stop_emulator_stack.sh
```

Native hosted/grouped runs retain the existing warmed-package baseline rules;
the direct commands above intentionally start from clean app data. App reset
and fixture reset happen only between journeys, never during a cutover.

## Pre-Drain Observation

Add `AEROBAG_E2E_CUTOVER_OBSERVE_BEFORE_DRAIN_MS=15000` to a positive run to
observe the old SSE after its resource returns 404. This optional bounded probe
records `diagnostics.before_drain` and then drains normally. It is diagnostic,
not an assertion that clients must or must not reconnect early.

On September 13 both real platforms retained old SSE without fetching the new
catalog during this 15-second window. Web made 143 old-only manifest requests;
native made one. Both rendered new weather and the next update after explicit
drain (about 3 seconds web, 5 seconds native to reconnect). Do not assume 404
alone repairs a provider cutover during an hour-long SSE drain grace.

Initial evidence is in `/tmp/aerobag-cutover-evidence/`: `web-red` and
`android-red` are intended failures; `web-predrain` and `android-green` passed
all seven checks. `web-green` preserves an earlier fixture-encoding failure,
not a product regression: unrounded JS coordinate arithmetic changed a
canonical state hash when decoded by Rust. Coordinates now use six decimal
places, and both native and WASM decoders accepted the corrected snapshots.

This suite does not qualify nginx activation, release-prefix rewrites, staging
isolation, or daemon process provenance. Those belong to the separate real
proxy/Python integration owned by the deployment lane. Core's
`nexrad_provider_cutover_replaces_history_and_rejects_late_old_tiles` test
separately covers replacement history, distinguishable decoded tile pixels,
failed old requests and rejection of a late prepared old tile package.
A new Chrome-on-Android cutover run is not claimed here; the existing
transport-recovery journey remains separate.
