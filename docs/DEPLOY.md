# Production Deploy

Aerobag production is managed from the dev machine with one operator CLI:

```bash
cd /root/aerobag-preprocessor/aerobag
tools/prod_manage.py --reconcile
```

Release assignments come exclusively from `deploy/releases.json`.
`prod_manage.py` maps stage, promotion, and recovery intent onto distinct
internal deployment operations; there is no second deployment CLI that can
bypass those semantics. Missing cycle products, web output, signed APK, and
live-feed binaries are built behind the currently served channel; no build
writes into active production.

The production controller is intentionally dev-pushed. `aerobag-prod` does not need git
credentials back to dev or GitHub. The tool creates a local git bundle with all
refs, copies it to prod, fetches all heads/tags into `/opt/aerobag`, checks out
`main`, and leaves the configured annotated release tags available for isolated
release worktrees.

## Config

The checked-in prod config is `deploy/aerobag-prod.json`.

Important fields:

- `checkout_ref`: the source ref containing the deployment controller. It does
  not select the production application release.
- `release_desired_state`: checked-in production/staging/sunset assignments.
- `release_live_port_base`: start of the loopback port range allocated to
  isolated release live-feed daemons.
- `artifact_root`: the persistent product build root. It owns `cache/`,
  `live-feeds/`, `published/`, `logs/`, `locks/`, `state/`, `scratch/`, and
  `worktrees/`.
- `ui_target_root`: persistent web build workspace and final static output.
- `cargo_target_dir`: persistent Rust target dir shared across deploys. It must
  be a child of `data_root`, not the small container root filesystem.
- `cargo_target_max_bytes`: maximum retained Cargo target size after a
  successful release reconciliation. Above this threshold the deployment
  prunes reusable compiler artifacts while preserving runnable top-level
  binaries.
- `cloud_server_listen`: localhost ACS listener. Production uses `127.0.0.1:8099`
  because `8096` is already the client-debug receiver.
- `cloud_server_storage_root`: persistent ACS storage, outside published
  artifacts. It contains `live/`, hourly hard-linked `snapshots/`, preserved
  pre-restore trees under `recovery/`, and stable process locks under `locks/`.
- `cloud_server_policy_source` and `cloud_server_policy_target`: the versioned,
  validated runtime policy and its installed location.
- `cloud_server_secret_source` and `cloud_server_secret_target`: the
  operator-owned 32-byte service secret and its service-readable installed
  location.
- `nginx_trusted_upstream_proxies`: exact addresses of public-edge proxies whose
  client-address assertion host nginx may trust.

The package list is not host config. It lives in
`deploy/prod-packages.txt` and is installed from the checked-out repo on prod.

## Release Workflow

`deploy/releases.json` remains the authoritative desired state. The normal
operator interface creates the required tag and desired-state commits, shows
the exact commands and a colored config diff, and asks before changing anything:

```bash
tools/prod_manage.py --stage
tools/prod_manage.py --qualification-status
tools/prod_manage.py --promote
tools/prod_manage.py --promote --force
tools/prod_manage.py --reconcile
```

For routine releases, start with `--stage`. A full local or hosted candidate
run is not a prerequisite.

`--prequalify` is optional deeper local assurance before staging. It runs the
complete ordinary-CI and release-journey workload **once** against one immutable
app bundle and pinned fixture. All P0, P1, and P2 web and Android journeys remain
covered. Four fresh Android shards use a configurable pool of local emulator
workers and restore one run-scoped app-data baseline. Browser and emulator
phases are isolated to avoid host contention. Receipts bind the exact commit
and workflow definitions; fast-check results are cached for a later `--stage`
of the same commit. This command does not push to GitHub, wait for hosted CI,
require a GitHub API token, or contact production.

Repetition testing is separate: explicitly run
`tools/ci/local_candidate_qualification.py --repetitions 5`, or request a hosted
manual stability run. Stability evidence cannot substitute for ordinary
qualification, and failures are not retried into green. See
[hosted CI](testing/hosted-ci.md#stability-testing-is-not-a-release-gate).
`--candidate-status` still reports legacy/manually requested hosted candidate
results; it does not report local receipts.

`--stage` automatically runs a cached, commit-bound, emulator-free preflight before
creating release intent or a tag. The preflight runs the ordinary unit, static,
generated-source, web, and Android JVM lanes; it deliberately excludes fixture
materialization and UI journeys so a successful release does not pay for the full
qualification twice. Running `--prequalify` first remains an optional way to run
the complete journey matrix before the slower release-tag round trip. `--stage`
chooses the first unused UTC-date release name such as `2026-08-22.1`. It
requires a clean synchronized `main`, then commits the staging assignment,
creates an immutable annotated tag at that same commit, atomically pushes both,
and reconciles production. Production remains on its current generation while
the candidate builds, starts a separate live-feed daemon, and is qualified at
`https://aerobag.org/staging/`. If the current commit is already the assigned
staging release, the command exits and directs the operator to `--reconcile`.

Staging qualification has two independent parts, running in parallel after
the release push. The production reconciler
checks the bytes and routes actually exposed under `/staging/`. A release-tag
run of `.github/workflows/e2e-ci.yml` builds immutable web and Android clients
for that exact tag and runs every P0, P1, and P2 journey. The ordinary `CI`
workflow must also pass for the same commit. `--qualification-status` reports
both deployed-staging and GitHub results, including links to the matching runs.
`--stage` atomically pushes its new main commit and release tag to the configured
GitHub repository after updating the shared origin, which starts both workflows.
An ordinary main-branch E2E run is intentionally not accepted as a substitute
for the full tag run.

`--promote` requires a clean `main` synchronized with `origin/main`, and checks
that the configured candidate is active and qualified on staging and that both
exact-commit GitHub workflows passed. It commits the production pointer change,
clears staging, retains outgoing production in sunset for **14 days**, pushes,
synchronizes only the new release intent, and activates the qualified channel generation. It does not
install host packages, refresh products, or synchronously run GC.
With no staging assignment it exits locally without contacting production.
Before confirmation it shows the outgoing tag and exact UTC sunset deadline in
the note and proposed diff. That same deadline is committed; existing sunset
entries and deadlines are preserved. No manual desired-state edit is needed.
Use `--promote --sunset-days 30` to override the default, or explicitly choose
`--promote --sunset-days 0` to retire outgoing production immediately.

Only immediate retirement prints a red warning about removing installed clients'
tag-scoped package and live-feed URLs. It also reads the canonical product and
live-feed contract registries from the production and staging Git tags and
identifies contracts changed by the candidate. Releases with the JSON client inventory use it; historical tags
without that file use their own Rust `PRODUCT_CONTRACTS` registry and Python
`LIVE_FEEDS_CONTRACT_PATH` constant. This is inspection of immutable history,
not a fallback to today's contract values. This check is source-only and fails
closed on unreadable Git objects, malformed inventories, or unknown legacy
layouts. Retaining the outgoing release avoids this removal warning and does
not require inspecting old contract inventories.

Combined channel discovery contains one publication per exact contract set,
preferring the controlling release, then the first matching sunset entry. Each
retained release still has its own discovery, package/web/APK endpoints, live-feed
route and GC roots, including releases omitted from combined discovery.

Every completed `prod_manage.py` operation ends with a bold green success or red
failure/incomplete/interrupted result, after any retained-log path. `NO_COLOR`
disables ANSI coloring. A successful plain `--stage` says deployment succeeded
without claiming hosted qualification passed; use `--watch` for that result.

`--promote --force` is the temporary operator escape hatch when qualification
infrastructure is unavailable. It bypasses deployed-staging qualification and
exact-commit GitHub CI only. The exact candidate must still have completed its
build, have running live feeds, and be the active staging release. The forced
tag is scoped to one controller invocation, and the promotion commit explicitly
records that qualification was bypassed. An ordinary `--promote` remains
fail-closed. Forced promotion uses the same default sunset retention and accepts
the same `--sunset-days` override; `--force` alone does not retire old clients.

Successful forced activation also records `qualification_status: "bypassed"`,
`qualification_bypassed_at_utc`, and `qualification_bypass_reason` in observed
release state. The channel generation carries the same audit so recovery after
a controller restart preserves it. Bypass never counts as passing qualification.
A later successful qualification clears the bypass fields; product refreshes
can invalidate qualification and reset the status to `pending`.
Routine production/sunset deployment checks do not grant staging qualification
or clear this audit.

`--reconcile` never edits, commits, tags, or pushes desired state. It first
compares the checked-in assignments with observed state and the installed
runtime. It also compares a fingerprint of runtime/controller Python sources,
deployment generators, the FAA calendar, and deployment configuration/policy
with `/etc/aerobag/runtime-inputs.sha256`. Missing or changed fingerprints use a
runtime update: sync the controller checkout, install generated scripts and
configuration, and restart support services. This preserves release assignments
and does not invoke application builds, product refreshes, GC, apt, or SDK setup.
The fingerprint is written only after installation completes, so interrupted
updates remain retryable. The first reconciliation of an older deployment
installs the runtime and establishes its fingerprint.

A converged server produces a green success message without deployment;
service-only drift runs the runtime-config repair path. Missing host state,
release artifacts, or channel state runs the idempotent full deployment. All
paths verify convergence before reporting success. Use `--reconcile` to deploy
tooling changes, resume interrupted staging, finish promotion, repair host drift,
or recover a replaced container.

Convergence is defined by release intent and runtime health, not by whether the
deployment-owned source mirror equals the caller's latest unrelated `main`
commit. App source, documentation, and `deploy/releases.json` are excluded from
the runtime fingerprint; release assignments are reconciled separately. Commit
and push tooling changes, then run `--reconcile`; no new application tag or
staging build is needed. Host-package installation remains part of full host
reconciliation; there is no independent deployment entry point.

`prod_manage` captures subprocess command traces and output in a private
per-invocation file under `/tmp`. Successful and operator-aborted invocations
delete that file. A failure retains it and prints the path, keeping routine
output concise while preserving complete diagnostics for inspection or handoff.

All operations reject an active release reconciliation before making changes.
The intent-changing commands repeat that check after confirmation.
The internal deployment module independently closes the systemd-timer race and rejects a held
reconciler lock; deployment no longer kills an in-progress release build. Run
`--reconcile` after the prior reconciliation finishes instead of repeating an
intent-changing operation.

The first controller deployment preserves the pre-deploy
`/etc/aerobag/deployed-rev` in release state before updating the source
checkout. The configured initial production tag must resolve to that exact
commit; otherwise legacy adoption fails rather than guessing which bytes are
live.

Promotion is another desired-state commit: move the staged tag to `production`,
clear `staging`, and retain the prior production under `sunset` with an explicit
expiration (14 days from the promotion proposal by default). The next periodic
reconciliation after that deadline retires the sunset release. A qualified
promotion is a channel-pointer change and graceful nginx reload, not a rebuild.
Rollback assigns the retained prior tag to production. GC roots production,
staging, unexpired sunset releases, and the previous generation.

## Aerobag Cloud Backups

`aerobag-cloud-backup.timer` checks every 15 minutes whether the policy-defined
backup interval has elapsed. `backup-if-due` serializes the due check and backup
under the reclamation lock, then creates online snapshots under
`$AEROBAG_CLOUD_SERVER_STORAGE_ROOT/snapshots/`. The daemon remains available:
the backup briefly pins a WAL read snapshot while copying SQLite, then releases
that read transaction before hard-linking the immutable blobs protected by
`locks/blob-reclamation.lock`. External backup software should archive complete
`snapshot-*` directories, never `live/` directly.

Operator commands use the deployed release binary and policy:

```bash
aerobag-cloud-serverd backup-now --storage-root "$AEROBAG_CLOUD_SERVER_STORAGE_ROOT" --policy "$AEROBAG_CLOUD_SERVER_POLICY"
aerobag-cloud-serverd backup-if-due --storage-root "$AEROBAG_CLOUD_SERVER_STORAGE_ROOT" --policy "$AEROBAG_CLOUD_SERVER_POLICY"
aerobag-cloud-serverd verify-backup --storage-root "$AEROBAG_CLOUD_SERVER_STORAGE_ROOT" --policy "$AEROBAG_CLOUD_SERVER_POLICY" SNAPSHOT
systemctl stop aerobag-cloud-server.service
aerobag-cloud-serverd restore --storage-root "$AEROBAG_CLOUD_SERVER_STORAGE_ROOT" --policy "$AEROBAG_CLOUD_SERVER_POLICY" SNAPSHOT
systemctl start aerobag-cloud-server.service
```

### Cloud workload validation

The ACS workload harness uses disposable storage and the real Axum protocol
router. It crosses request signing, middleware, JSON, SQLite and filesystem
blob placement, root CAS, SSE, online backup, and GC without touching a running
dev or production daemon. The CI profile shrinks policy thresholds so quota,
SSE saturation, read-only recovery, filesystem pressure, and pipeline-health
alarms are exercised cheaply:

```bash
cargo run --manifest-path services/Cargo.toml -p aerobag-cloud-server \
  --features workload \
  --bin aerobag-cloud-workload -- --profile ci \
  --output /tmp/aerobag-cloud-workload-ci.json
python3 tools/verify_acs_workload_report.py \
  /tmp/aerobag-cloud-workload-ci.json
```

The longer profile retains `deploy/aerobag-cloud-policy.json`, runs optimized,
and reports per-stage p50/p95/p99 latency, throughput, stage-to-stage p95
falloff, SSE delivery, backup and GC pauses, and process RSS:

```bash
cargo run --release --manifest-path services/Cargo.toml \
  -p aerobag-cloud-server --features workload --bin aerobag-cloud-workload -- \
  --profile production --output /tmp/aerobag-cloud-workload-production.json
python3 tools/verify_acs_workload_report.py \
  /tmp/aerobag-cloud-workload-production.json
```

`production` describes the policy and workload shape; it still uses an
isolated temporary store. It never sends traffic to a deployed ACS. CI enforces
generous catastrophic latency ceilings. Stage-to-stage falloff remains in both
reports for diagnosis, but is not a CI gate because shared hosted runners are
not stable microbenchmark environments. The production report is likewise a
characterization artifact rather than a machine-dependent pass/fail benchmark.

Restore refuses to run while the daemon owns `locks/serve.lock`, verifies the
entire snapshot first, and retains the replaced `live/` tree under `recovery/`.
Returning a read-only service to normal operation uses checked
`resume-writes`; the separately named `force-resume-writes --reason ...`
records an operator audit event.

Dev-stack performs the same `backup-if-due` check once per minute from a
supervisor thread. The schedule exists only while dev-stack is running, and an
active check is terminated with the other children on `Ctrl-C`; persistent ACS
state still decides whether a backup is due after restart.
The deploy script installs only a tiny bootstrap set before the checkout exists:
`ca-certificates`, `git`, and `rsync`.

## NMS NOTAM Credentials

NMS API OAuth credentials are operator-owned and live outside the repository.
The dev stack uses staging credentials directly from:

```text
/root/aerobag-credentials/dev-stack/nms-notams-staging.json
```

Production deploys copy the configured production credential file to:

```text
/etc/aerobag/secrets/nms-notams.json
```

Enable production NOTAM ingestion only after production NMS credentials exist:

```json
{
  "nms_notams_enabled": true,
  "nms_notams_credential_file": "/root/aerobag-credentials/nms-notams-production.json",
  "nms_notams_prod_config": "/etc/aerobag/secrets/nms-notams.json"
}
```

The credential JSON declares `sourceEnvironment`. The NMS client requires each
environment to use its exact FAA API and token endpoints, so relabeling staging
credentials as production cannot route a production daemon back to staging.
Production deployment independently requires the production marker, production
endpoints, and non-empty credentials before copying the secret.

Production APK builds use the Android SDK under `/usr/lib/android-sdk`.
They require a full JDK, not just a JRE, because Android Gradle transforms use
`jlink` while processing platform modules. Prod installs `openjdk-21-jdk` from
`deploy/prod-packages.txt`; local builds may also use a full Java 17 JDK.
The full `prod_manage.py --reconcile` path installs the Android command-line tools, platform 34,
build-tools 34.0.0, platform-tools, accepts SDK licenses, installs NDK
`26.3.11579264`, installs the Rust `x86_64-linux-android` and
`aarch64-linux-android` targets, and writes `ui/android-app/local.properties`.

The deploy writes `/etc/aerobag/env` on prod. The important publication values
are:

```sh
SOURCE_ROOT=/opt/aerobag
ARTIFACT_ROOT=/mnt/aerobag-data/artifacts
AEROBAG_ARTIFACT_WRITE_PATH=/mnt/aerobag-data/artifacts
AEROBAG_ARTIFACT_READ_PATH=/mnt/aerobag-data/artifacts/published
AEROBAG_UI_TARGET_ROOT=/mnt/aerobag-data/ui-target
CARGO_TARGET_DIR=/mnt/aerobag-data/build-cache/cargo-target
AEROBAG_CARGO_TARGET_MAX_BYTES=34359738368
AEROBAG_WEB_DIST=/mnt/aerobag-data/ui-target/web/dist
ANDROID_HOME=/usr/lib/android-sdk
ANDROID_SDK_ROOT=/usr/lib/android-sdk
```

`AEROBAG_ARTIFACT_READ_PATH` points at the public publication root, not the full
artifact root. Clients see it through `/packages/`.

## Prod Layout

The prod container uses:

```text
/opt/aerobag                                      git checkout
/mnt/aerobag-data/artifacts/cache                build/fetch cache
/mnt/aerobag-data/artifacts/live-feeds           live-feed publication
/mnt/aerobag-data/artifacts/release-builds        immutable web/APK/daemon releases
/mnt/aerobag-data/artifacts/channel-generations   immutable serving views
/mnt/aerobag-data/artifacts/channel-current       active generation symlink
/mnt/aerobag-data/artifacts/logs                 build/watch logs
/mnt/aerobag-data/artifacts/locks                publication locks
/mnt/aerobag-data/artifacts/published            public cycle publication root
/mnt/aerobag-data/artifacts/scratch              transient build scratch
/mnt/aerobag-data/artifacts/state                operational manifests and markers
/mnt/aerobag-data/artifacts/worktrees            multi-version build worktrees
/mnt/aerobag-data/ui-target                       shared UI build cache
/mnt/aerobag-data/build-cache/cargo-target       bounded Rust build cache
/etc/aerobag/deployed-rev                        deployed checkout commit
/etc/aerobag/deploy-config.json                  deployed ref summary
```

Immutable cycle publications remain under `published/`. Public discovery is
channel-specific:

```text
/mnt/aerobag-data/artifacts/published/<publish-label>/<timestamp>/packaged/
/mnt/aerobag-data/artifacts/published/<publish-label>/<timestamp>/unpacked/
/mnt/aerobag-data/artifacts/channel-current/production/packages/current_artifacts.json
/mnt/aerobag-data/artifacts/channel-current/staging/packages/current_artifacts.json
/mnt/aerobag-data/artifacts/channel-current/releases/<tag>/packages/current_artifacts.json
```

Each discovery file is a distinct JSON list validated and written by the
production release's `preprocessor-cli merge-current-artifacts --output`.
Artifact subtrees are shared through symlinks; discovery files are not.

## Pipeline health

`/pipeline-health/` checks availability of the merged production publication,
but derives production cycle-product errors and warnings from
`channel-current/releases/<production-tag>/packages/current_artifacts.json`.
Sunset releases have independent diagnostic counts. Two releases with 153
warnings each therefore report 153 in each scope, and an increase to 154 warns
in the affected scope. The comparison is against the previous distinct
publication, not a fixed allowance. Old merged history totals are excluded from
the new release-specific comparison; old single-publication baselines remain
usable.

**Deployed release checks** describes current channel health, separately from
staging admission and GitHub qualification. The controller records
`deployment_status`, `deployment_record`, and `deployment_error` for production,
staging, and every retained sunset release. The existing
`release.qualification_status` dashboard metric ID remains stable, but reads
this deployment status (old controller snapshots retain their old behavior).
`pending` is critical for production and a warning for staging/sunset; `failed`
is critical for every role and includes the check error.

After product refresh or channel activation, reconciliation checks each active
channel's public web, About (when supported by an old release), package discovery,
live-feed status, and APK metadata endpoints. It verifies HTTP status, content
type, and exact static bytes; no Chrome or hosted journeys run on production.
Receipts bind the release, product manifest, channel role, origin, and served
package manifest. Stale/missing receipts are checked again, including records
created before deployment receipts existed. Successful production/sunset checks
do not alter historical staging qualification. Only staging checks can qualify
a candidate for promotion. Forced admission remains a separate visible
`release.qualification_bypass` alarm even after deployment checks pass.

This lifecycle repair is a controller/monitor runtime update through
`tools/prod_manage.py --reconcile`; it does not require a new application release.
Build and live-feed health remain separate signals.

Live-feed alarms describe **ongoing degradation**, not a historical hiccup.
`live_feed.<product>.failure_duration_seconds` warns after **2 minutes** of an
unrecovered failure and becomes critical after **10 minutes**. Another failure
does not restart that clock. Successful recovery clears the alarm on the next
monitor sample (normally within a minute); a failure and recovery between
samples need not alarm at all. Product-specific stale-data thresholds remain
independent and still catch a hung worker or an unavailable publication.

The two-hour failure rate and consecutive-failure graphs are informational.
Past failures remain in the dashboard's two-hour details and historical numeric
graphs; they no longer latch the current alarm. For longer-term flakiness audits,
use the 14-day pipeline-health JSONL histories to locate affected periods and
the release's `aerobag-live-feeds-release@<tag>` journal for individual errors.
Do not count a two-hour rolling failure count at every sample as a new incident.
Data-integrity/quality alarms (for example unresolved rejected NOTAM rows) remain
separate from transport failure history.

Production runs one worker per live product, with one operation in flight per
product. Fetch/build work is independent; shared catalog publication and GC
are serialized. A slow feed no longer holds a batch barrier before other feeds
can publish or poll again. Simulation retains its ordered fixture batches.
Live-product curl transfers have a **10-second connection** and **120-second
total** deadline per transfer, including redirects. Existing three-attempt
retries remain; FAA cookie handoffs can add a second bounded transfer per attempt.
NMS requests have 10-second DNS/connection and 300-second end-to-end deadlines
(including large initial loads). Other bulk fetches have 15-second connection
and 30-minute total deadlines. These are network bounds, not whole-build bounds.

NOTAM builds capture an owned projection/journal snapshot in one read
transaction, including checkpoint records only for initial publication. Later
ingestion cannot change the build's promised state, and acknowledging that
snapshot does not consume newer journal entries. See the
[v3 operational status contract](contracts/live-feed-status-v3.md) for independent
source/publication recovery and legacy monitoring behavior.

The monitoring rule update can be installed with `tools/prod_manage.py --reconcile`.
The Rust worker, snapshot, and deadline changes require a newly built release;
reconciling alone does not replace an old production or sunset daemon binary.

Every metric has a detail row, including string statuses and missing values.
**Weather camera sites** reports the minimum published inventory across each
release's cycles and warns below **960** unique sites (baseline: 974, including
218 Canadian sites). A missing or invalid **promised** count warns. Older known
producer contracts without that measurement show gray **Not instrumented**,
not a passing value or an alarm. Unknown contracts produce a separate telemetry
coverage warning. Expectations are pinned in immutable release metadata, with
an exact-commit bootstrap registry for already-existing releases; publication
claims cannot select a weaker contract. See [producer telemetry contracts](../contracts/telemetry/README.md)
for contract ownership, future feature additions, and reviewed coverage changes.
See
[weather camera ingestion](WEATHER_CAMERAS.md) for the two-source union and how
to obtain your own FAA API credential by email and MoU.

Only numeric and boolean values have graphs. Alert links select the metric's
scope and scroll to its row. Verify dashboard behavior locally with:

```bash
python3 product/preprocessor/scripts/smoke_pipeline_health_dashboard.py
```

This uses local Chrome with fixture data and a stub plot renderer; it makes no
dashboard or CDN requests. Set `CHROME_BIN` to select another Chrome executable.

## Services

The deploy installs these systemd units:

- `aerobag-build-product.service`: one-shot desired-state reconciliation.
- `aerobag-build-product.timer`: refreshes release products every 2 hours.
- `aerobag-live-feeds-release@<tag>.service`: isolated release daemon. Nginx
  routes production, staging, and release-scoped clients to selected instances.
- `aerobag-cloud-server.service`: localhost-only Aerobag Cloud daemon running as
  the dedicated `aerobag-cloud` user with state under
  `/mnt/aerobag-data/cloud-storage`.
- `aerobag-cloud-backup.service` and `.timer`: online ACS snapshots on the
  policy-defined interval.
- `aerobag-client-debug-log.service`: localhost-only receiver for browser
  `POST /__debug_log` batches.
- `aerobag-build-watch.service`: localhost-only web dashboard and JSON endpoint
  for the product build log.
- `aerobag-health.service` and `aerobag-health.timer`: refresh machine-readable
  health status every minute.
- `nginx.service`: public HTTP server on port 80.

After a successful product/release reconciliation, the controller measures the
shared Cargo target. If it exceeds `cargo_target_max_bytes`, it removes Cargo's
reusable `deps`, `build`, `.fingerprint`, and `incremental` trees for the debug
and release profiles. Runtime binaries at the profile root remain in place, so
running services and later service restarts remain safe; the next changed build
recompiles whatever was pruned.

Each generated release live-feed unit uses its immutable binary and isolated
mutable roots while sharing only the fetch cache:

```bash
"$AEROBAG_RELEASE_ROOT/bin/aerobag-live-feedsd" \
  --live-root "$AEROBAG_RELEASE_LIVE_ROOT" \
  --scratch-root "$AEROBAG_RELEASE_LIVE_SCRATCH" \
  --tfr-detail-backfill-state-root "$AEROBAG_RELEASE_LIVE_FEEDS_STATE_ROOT/tfr-detail-backfill" \
  --fetch-cache-root "$ARTIFACT_ROOT/cache/fetch" \
  --fetch-cache-mode fill \
  --listen "$AEROBAG_LIVE_FEEDS_LISTEN"
```

`AEROBAG_RELEASE_LIVE_FEEDS_STATE_ROOT` is the sole controller-owned root for
daemon-private persistent state. TFR detail state lives below
`tfr-detail-backfill/`; when NMS NOTAM ingestion is enabled, its collector and
publication state live below `nms-notams/`.

Manual product rebuild:

```bash
ssh root@aerobag-prod.iac.jonh.net systemctl start --no-block aerobag-build-product.service
```

Omit `--no-block` only when you intentionally want the shell to wait for the
full cycle product build to finish.

Inspect build progress:

```bash
ssh root@aerobag-prod.iac.jonh.net \
  /opt/aerobag/product/preprocessor/scripts/watch_build_log.py \
  /mnt/aerobag-data/artifacts/logs/orchestrator/published/master.log
```

Or use the web dashboard:

```text
http://aerobag-prod.iac.jonh.net/build-watch/
http://aerobag-prod.iac.jonh.net/build-watch/api/state
```

The timer invokes the desired-state controller:

```bash
/opt/aerobag/tools/reconcile_prod_releases.py \
  --desired /opt/aerobag/deploy/releases.json \
  --observed /mnt/aerobag-data/artifacts/state/releases-observed.json \
  --source-root /opt/aerobag \
  --artifact-root /mnt/aerobag-data/artifacts \
  --cargo-target-dir /mnt/aerobag-data/build-cache/cargo-target \
  --refresh-products
```

For each missing tag, the controller builds cycle publication with
`build_multi_version_publication.py --no-activate`, then builds immutable web,
APK, and daemon outputs with `tools/build_release.py`. Promotion only creates a
new channel generation and reloads nginx gracefully. Product refresh and
channel-aware GC are deferred to periodic reconciliation. Promotion does not
rebuild or requalify the release.

Cycle builds combine the FAA weather-camera API and website inventories. Deployment copies
`/root/aerobag-credentials/faa-weathercams-token` to the production secrets
directory; see [weather camera ingestion](WEATHER_CAMERAS.md) for token setup
and source selection.

`build_prod_apk.sh` verifies that `JAVA_HOME` resolves to a full JDK with
`jlink` before invoking Gradle. On prod that should be
`/usr/lib/jvm/java-21-openjdk-amd64`.

Release-like WASM builds require Binaryen `version_129` or newer. The pinned
`install:wasm-opt` step installs it under `$AEROBAG_UI_TARGET_ROOT/tools/`,
where the WASM build script finds it automatically. Set `AEROBAG_WASM_OPT_BIN`
only if using a different install location.

The Android APK publisher writes into the temporary release tree. The APK pins
`/releases/<tag>/packages/` and `/releases/<tag>/live-feeds/`; the exact staged
APK is linked from production after promotion. Do not publish a stable APK
filename such as `latest.apk`.

The Android APK publisher:

1. builds with release-scoped package and live-feed endpoints
2. preserves the stable Android app identity `org.aerobag.app`
3. builds the packaged Rust JNI library in release mode for `arm64-v8a` by
   default; override `ANDROID_BUILD_RUST_RELEASE` or `ANDROID_TARGET_ABIS` only
   for development diagnostics
4. copies exactly one versioned APK name into the immutable release downloads,
   for example `aerobag-android-4dbd9ead.apk`
5. writes release-scoped `downloads/android-apk.json`, which is what the
   `/about` page reads to show the current versioned link

The obsolete `build-fast-subset` path is not part of production deploy. Live
METAR/TFR/NEXRAD/winds/obstacle data is owned by `aerobag-live-feedsd`.

## Nginx

Prod serves:

- `/`, `/downloads/`, `/packages/`, `/live-feeds/`: active production channel
- `/staging/` and its package/download/live-feed subpaths: staging channel
- `/releases/<tag>/`: durable release-scoped resources
- `/cloud/`: proxied to the localhost ACS daemon with SSE buffering disabled
- `/icons/`: source-tree icon assets
- `/health.json`: machine-readable deploy/build/live-feed status
- `/__debug_log`: proxied to the client debug log receiver
- `/build-watch/`: proxied to the build log dashboard

The nginx config blocks internal `/packages/cache/`, `/packages/logs/`,
`/packages/locks/`, `/packages/scratch/`, `/packages/state/`, and
`/packages/worktrees/` paths.

Production has two proxy layers:

```text
client -> public aerobag.org proxy -> aerobag-prod nginx -> ACS
```

Before deploying ACS, configure the public proxy to overwrite, rather than
forward, any client-supplied identity:

```nginx
proxy_set_header Aerobag-Client-Address $remote_addr;
```

The browser `EventSource` transport carries a short-lived, one-use ACS ticket
in `/cloud/v1/events`'s query string. Disable access logging, or explicitly
redact the query string, for that exact route at every proxy layer. The
host-local generated nginx configuration already uses `access_log off` there;
the public edge must do the same so bearer tickets never enter log storage.

Do this before deploying the inner nginx configuration. Host nginx accepts the
header only from `nginx_trusted_upstream_proxies`, derives its trusted
`$remote_addr`, and overwrites the header again when calling ACS. ACS accepts
that assertion only from its policy allowlist. Direct clients cannot choose
their rate-limit identity.

`/cloud/v1/health` is the bounded public health response. Detailed
`/cloud/v1/status`, including opaque top contributors, is deliberately blocked
by nginx and accepted by ACS only from a direct loopback caller presenting a
bearer credential derived from the root-owned ACS server secret. Pipeline
health derives that credential locally; it never sends the master secret.
Pipeline health and root-operated containment commands use that host-local
operator boundary; there is no public ACS administration API.

## Health

Machine-readable status is written to:

```text
/mnt/aerobag-data/health/status.json
```

and exposed as:

```text
http://aerobag-prod.iac.jonh.net/health.json
```

It reports:

- deployed commit and configured publication refs
- systemd active/enabled states
- active production `current_artifacts.json` age, manifest count, and contracts
- desired/observed release state and per-release service state
- active production release `live-feeds/v3/current.json` age
- most recent orchestrator log path

The live-feeds daemon also serves:

```text
/live-feeds/status.json
/live-feeds/status.html
/live-feeds/v3/current.json
/live-feeds/v3/events
```

Browser debug logs are written as JSON lines under:

```text
/mnt/aerobag-data/client-debug/client-debug-YYYYMMDD.jsonl
```

## Smoke Checks

After deploy:

```bash
curl -I http://aerobag-prod.iac.jonh.net/
curl -I http://aerobag-prod.iac.jonh.net/packages/current_artifacts.json
curl -s http://aerobag-prod.iac.jonh.net/health.json
curl -s http://aerobag-prod.iac.jonh.net/live-feeds/status.json
curl -s http://aerobag-prod.iac.jonh.net/cloud/v1/health
```
