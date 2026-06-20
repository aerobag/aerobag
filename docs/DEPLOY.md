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
  `private-work/`, `live-feeds/`, and `published/`.
- `ui_target_root`: persistent web build workspace and final static output.
- `cargo_target_dir`: persistent Rust target dir shared across deploys.

The package list is not host config. It lives in
`deploy/prod-packages.txt` and is installed from the checked-out repo on prod.
The deploy script installs only a tiny bootstrap set before the checkout exists:
`ca-certificates`, `git`, and `rsync`.

Production APK builds use the Android SDK under `/usr/lib/android-sdk`.
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
/mnt/aerobag-data/artifacts/private-work         build logs and work dirs
/mnt/aerobag-data/artifacts/live-feeds           live-feed publication
/mnt/aerobag-data/artifacts/published            public cycle publication root
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
  --scratch-root "$ARTIFACT_ROOT/private-work/live-feeds" \
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
  /mnt/aerobag-data/artifacts/private-work/orchestrator-logs/published/master.log
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
  <additional refs...> master
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
- `/icons/`: source-tree icon assets
- `/health.json`: machine-readable deploy/build/live-feed status
- `/__debug_log`: proxied to the client debug log receiver
- `/build-watch/`: proxied to the build log dashboard

The nginx config blocks `/packages/cache/` and `/packages/private-work/`.

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
- `live-feeds/v2/current.json` age
- most recent orchestrator log path

The live-feeds daemon also serves:

```text
/live-feeds/status.json
/live-feeds/status.html
/live-feeds/v2/current.json
/live-feeds/v2/events
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
```
