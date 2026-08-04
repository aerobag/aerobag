# Production Deploy

Aerobag production is deployed from the dev machine with:

```bash
cd /root/aerobag-preprocessor/aerobag
tools/deploy_prod.py --config deploy/aerobag-prod.json
```

By default, deploy starts the long cycle product publication asynchronously,
then builds the web static tree and Android APK synchronously. A web or Android
build failure makes `deploy_prod.py` fail.

Use `--skip-build` to install/update prod, restart live-feeds, and skip both the
async cycle product publication and the synchronous web/Android build.

Use `--runtime-config-only` to refresh env files, generated helper scripts,
nginx, and systemd runtime units without touching the source checkout or
currently running product build.

The deploy tool is intentionally dev-pushed. `aerobag-prod` does not need git
credentials back to dev or GitHub. The tool creates a local git bundle with all
refs, copies it to prod, fetches all heads/tags into `/opt/aerobag`, checks out
the configured production ref, and leaves the other refs available for
multi-version publication worktrees.

## Config

The checked-in prod config is `deploy/aerobag-prod.json`.

Important fields:

- `checkout_ref`: the source ref used for the running prod checkout and
  live-feeds daemon.
- `additional_publication_refs`: older product-contract branches to include in
  the merged cycle publication, such as `nav6-sunset`.
- `artifact_root`: the persistent product build root. It owns `cache/`,
  `live-feeds/`, `published/`, `logs/`, `locks/`, `state/`, `scratch/`, and
  `worktrees/`.
- `ui_target_root`: persistent web build workspace and final static output.
- `cargo_target_dir`: persistent Rust target dir shared across deploys.
- `cloud_server_listen`: localhost ACS listener. Production uses `127.0.0.1:8099`
  because `8096` is already the client-debug receiver.
- `cloud_server_data_root`: persistent ACS SQLite/blob state, outside published
  artifacts.
- `cloud_server_policy_source` and `cloud_server_policy_target`: the versioned,
  validated runtime policy and its installed location.
- `cloud_server_secret_source` and `cloud_server_secret_target`: the
  operator-owned 32-byte service secret and its service-readable installed
  location.
- `nginx_trusted_upstream_proxies`: exact addresses of public-edge proxies whose
  client-address assertion host nginx may trust.

The package list is not host config. It lives in
`deploy/prod-packages.txt` and is installed from the checked-out repo on prod.
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
`deploy_prod.py` installs the Android command-line tools, platform 34,
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
CARGO_TARGET_DIR=/var/cache/aerobag-build/target
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
/mnt/aerobag-data/artifacts/logs                 build/watch logs
/mnt/aerobag-data/artifacts/locks                publication locks
/mnt/aerobag-data/artifacts/published            public cycle publication root
/mnt/aerobag-data/artifacts/scratch              transient build scratch
/mnt/aerobag-data/artifacts/state                operational manifests and markers
/mnt/aerobag-data/artifacts/worktrees            multi-version build worktrees
/mnt/aerobag-data/ui-target/web/dist             static web app
/var/cache/aerobag-build/target                  Rust build cache
/etc/aerobag/deployed-rev                        deployed checkout commit
/etc/aerobag/deploy-config.json                  deployed ref summary
```

The public cycle publication contract is:

```text
/mnt/aerobag-data/artifacts/published/current_artifacts.json
/mnt/aerobag-data/artifacts/published/<publish-label>/<timestamp>/packaged/
/mnt/aerobag-data/artifacts/published/<publish-label>/<timestamp>/unpacked/
```

`current_artifacts.json` is always a JSON list and is written only by
`preprocessor-cli merge-current-artifacts`.

## Services

The deploy installs these systemd units:

- `aerobag-build-product.service`: one-shot multi-version cycle publication and
  cache GC.
- `aerobag-build-product.timer`: runs the product build every 2 hours.
- `aerobag-live-feeds.service`: continuous live-feeds daemon.
- `aerobag-cloud-server.service`: localhost-only Aerobag Cloud daemon running as
  the dedicated `aerobag-cloud` user with state under
  `/mnt/aerobag-data/cloud`.
- `aerobag-client-debug-log.service`: localhost-only receiver for browser
  `POST /__debug_log` batches.
- `aerobag-build-watch.service`: localhost-only web dashboard and JSON endpoint
  for the product build log.
- `aerobag-health.service` and `aerobag-health.timer`: refresh machine-readable
  health status every minute.
- `nginx.service`: public HTTP server on port 80.

The generated live-feeds unit runs the daemon in production mode by omitting
simulation/fixture flags:

```bash
"$CARGO_TARGET_DIR/release/aerobag-live-feedsd" \
  --live-root "$ARTIFACT_ROOT/live-feeds" \
  --scratch-root "$ARTIFACT_ROOT/scratch/live-feeds" \
  --fetch-cache-root "$ARTIFACT_ROOT/cache/fetch" \
  --fetch-cache-mode fill \
  --listen "$AEROBAG_LIVE_FEEDS_LISTEN"
```

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

The default timer command uses:

```bash
/opt/aerobag/product/preprocessor/scripts/build_multi_version_publication.py \
  --release \
  --profile production \
  --build-root /mnt/aerobag-data/artifacts \
  --target-dir /var/cache/aerobag-build/target \
  <additional refs...> main
```

It then runs:

```bash
/var/cache/aerobag-build/target/release/preprocessor-cli \
  gc-build-cache --profile production \
  --build-root /mnt/aerobag-data/artifacts \
  --bootstrap-from-build-manifests --execute
```

Normal deploy also runs this web/Android build synchronously after starting the
cycle product service:

```bash
/usr/local/bin/aerobag-ensure-android-sdk

cd /opt/aerobag/ui/web-app
npm run install:wasm-opt
npm run build:release

cd /opt/aerobag/ui/android-app
./scripts/build_prod_apk.sh
```

`build_prod_apk.sh` verifies that `JAVA_HOME` resolves to a full JDK with
`jlink` before invoking Gradle. On prod that should be
`/usr/lib/jvm/java-21-openjdk-amd64`.

Release-like WASM builds require Binaryen `version_129` or newer. The pinned
`install:wasm-opt` step installs it under `$AEROBAG_UI_TARGET_ROOT/tools/`,
where the WASM build script finds it automatically. Set `AEROBAG_WASM_OPT_BIN`
only if using a different install location.

The Android APK publisher runs after the web build because the web build empties
and recreates `$AEROBAG_WEB_DIST`. It writes a hash-named APK under
`$AEROBAG_WEB_DIST/downloads/` plus
`$AEROBAG_WEB_DIST/downloads/android-apk.json`, which the About page uses for
the download link. Do not publish a stable APK filename such as `latest.apk`.

The Android APK publisher:

1. builds the Android app with `ANDROID_PACKAGE_SOURCE_BASE_URL` defaulting to
   `https://aerobag.org/packages/`
2. builds with `ANDROID_LIVE_FEED_SOURCE_BASE_URL` defaulting to
   `https://aerobag.org`
3. preserves the stable Android app identity `org.aerobag.app`
4. builds the packaged Rust JNI library in release mode for `arm64-v8a` by
   default; override `ANDROID_BUILD_RUST_RELEASE` or `ANDROID_TARGET_ABIS` only
   for development diagnostics
5. copies exactly one versioned APK name into `$AEROBAG_WEB_DIST/downloads`,
   for example `aerobag-android-4dbd9ead.apk`
6. writes `$AEROBAG_WEB_DIST/downloads/android-apk.json`, which is what the
   `/about` page reads to show the current versioned link

The obsolete `build-fast-subset` path is not part of production deploy. Live
METAR/TFR/NEXRAD/winds/obstacle data is owned by `aerobag-live-feedsd`.

## Nginx

Prod serves:

- `/`: static web app from `/mnt/aerobag-data/ui-target/web/dist`
- `/downloads/`: Android APK metadata and versioned APK from the static web app
- `/packages/`: `/mnt/aerobag-data/artifacts/published/`
- `/live-feeds/`: proxied to `aerobag-live-feedsd`
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
- `published/current_artifacts.json` age, manifest count, and contract summary
- `live-feeds/v3/current.json` age
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
