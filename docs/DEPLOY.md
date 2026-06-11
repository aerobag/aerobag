# Deploy 1

This is the first-pass deployment path for a VM that has the git repo checked
out and enough disk for the generated artifacts plus the built web tree.

## apt prereqs

Install the VM packages:

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  curl \
  unzip \
  zip \
  python3 \
  gdal-bin \
  python3-gdal \
  python3-numpy \
  python3-pil \
  python3-pypdf \
  imagemagick \
  ghostscript \
  libimage-exiftool-perl \
  systemd \
  poppler-utils \
  sqlite3 \
  openjdk-21-jre-headless \
  nodejs \
  npm \
  rustup \
  nginx
```

Use `rustup` for Rust. Do not install the distro `cargo`/`rustc` packages for
this deployment path; they can lag behind what the repo needs and do not manage
the WASM target cleanly.

```bash
rustup default stable
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
```

Release-like WASM builds use Binaryen `wasm-opt` by default and require
Binaryen `version_129` or newer. Install that from the upstream Binaryen release
assets, not from an older distro package. The web app has a pinned installer:

```bash
cd "$SOURCE_ROOT/ui/web-app"
npm run install:wasm-opt
```

That installs Binaryen under `$AEROBAG_UI_TARGET_ROOT/tools/`, where the WASM
build script finds it automatically. Set `AEROBAG_WASM_OPT_BIN` only if using a
different install location. Release-like builds fail loudly if `wasm-opt` is
missing, too old, or produces a module that fails startup.

## Define paths

Set these somewhere you can source before running the commands below.

Point this at the git clone:

```bash
export SOURCE_ROOT=/wherever/aerobag
```

Point this where the preprocessor should write published packaged/unpacked
artifacts:

```bash
export ARTIFACT_ROOT=/mnt/aerobag-data/artifacts
```

Point this where the web build should put generated WASM, staged static inputs,
node workspace files, and the final deployable `dist/` tree:

```bash
export AEROBAG_UI_TARGET_ROOT=/mnt/aerobag-data/ui-target
```

Derived paths:

```bash
export CARGO_TARGET_DIR="$ARTIFACT_ROOT/target"
export AEROBAG_ARTIFACT_WRITE_PATH="$ARTIFACT_ROOT"
export AEROBAG_ARTIFACT_READ_PATH="$ARTIFACT_ROOT"
export AEROBAG_WEB_DIST="$AEROBAG_UI_TARGET_ROOT/web/dist"
```

With these values:

- source lives under `$SOURCE_ROOT`
- content artifacts live under `$ARTIFACT_ROOT`
- web build outputs live under `$AEROBAG_UI_TARGET_ROOT`
- the static site root is exactly `$AEROBAG_WEB_DIST`

## Build the preprocessor

```bash
cd "$SOURCE_ROOT/product/preprocessor"
cargo build --release -p preprocessor-cli -p live-feeds-daemon
```

Run the product build once before installing timed executions. The live-feed
daemon reads the published cycle product for shared inputs such as towered
airport metadata.

```bash
time "$CARGO_TARGET_DIR/release/preprocessor-cli" build-product --source-root "$SOURCE_ROOT"
```

After this completes, `$ARTIFACT_ROOT` should contain the published artifact
contract the web build consumes:

- `current_artifacts.json`
- `published_packaged/`
- `published_unpacked/`

## Build the web static tree

The web release build reads from `$AEROBAG_ARTIFACT_READ_PATH` and writes under
`$AEROBAG_UI_TARGET_ROOT`.

```bash
cd "$SOURCE_ROOT"
tools/deploy_prod
```

That command builds products, builds the web tree, then publishes the Android
APK into that fresh tree. Pass `--skip-product` only when the current published
artifact set is already known-good and should be reused.

The web release portion:

1. validates the current artifact set that will be served under `/packages`
2. builds the Rust/WASM adapter in release mode
3. runs TypeScript checking
4. runs `vite build`
5. writes the deployable static tree to `$AEROBAG_WEB_DIST`

The static tree includes the app chunks and WASM. Product content is served
through the publication contract rooted at `/packages`; do not publish or rely
on legacy content routes such as `/plates`, `/thumbnails`, `/nav-kv`, or
`/sectional-packages`.

## Publish the Android APK

Build the Android APK on the production host after the web release build. The
web build empties and recreates `$AEROBAG_WEB_DIST`, so the APK publisher must
run after the web build. `tools/deploy_prod` does this ordering automatically.

```bash
cd "$SOURCE_ROOT/ui/android-app"
./scripts/build_prod_apk.sh
```

That script:

1. builds the Android app with `ANDROID_PACKAGE_SOURCE_BASE_URL` defaulting to
   `https://aerobag.org/packages/`
2. builds with `ANDROID_LIVE_FEED_SOURCE_BASE_URL` defaulting to
   `https://aerobag.org`
3. preserves the stable Android app identity `org.aerobag.app`
4. copies exactly one versioned APK name into `$AEROBAG_WEB_DIST/downloads`,
   for example `aerobag-android-4dbd9ead.apk`
5. writes `$AEROBAG_WEB_DIST/downloads/android-apk.json`, which is what the
   `/about` page reads to show the current versioned link

Do not publish a stable APK filename such as `latest.apk`. Each published app
version gets a hash-named APK, and the About page points at that versioned file.

## Serve the web tree

Point the static web server at:

```text
$AEROBAG_WEB_DIST
```

Minimal nginx sketch:

```nginx
server {
    listen 80;
    server_name _;

    root /mnt/aerobag-data/ui-target/web/dist;
    index index.html;

    location / {
        try_files $uri $uri/ /index.html;
    }
}
```

Use the real value of `$AEROBAG_WEB_DIST` in the nginx config; nginx will not
expand the shell variable inside the config file.

## Schedule product builds

Once the one-shot product and web build are working, schedule content refreshes
and run the live-feed daemon.

Every 2 hours, run `build-product`:

```bash
time "$CARGO_TARGET_DIR/release/preprocessor-cli" build-product --source-root "$SOURCE_ROOT"
```

Run `aerobag-live-feedsd` continuously. The exact supervisor is deployment
policy; for a command-line test instance:

```bash
"$CARGO_TARGET_DIR/release/aerobag-live-feedsd" \
  --live-root "$ARTIFACT_ROOT/live-feeds" \
  --publication-root "$ARTIFACT_ROOT" \
  --scratch-root "$ARTIFACT_ROOT/private-work/live-feeds" \
  --listen 127.0.0.1:8095
```

Initial live-feed polling cadence is intentionally conservative and easy to
adjust: NEXRAD 60 seconds, METARs and TFRs 5 minutes, winds aloft 1 hour, and
obstacles 6 hours.

When the published artifacts change, rebuild and redeploy the web static tree:

```bash
cd "$SOURCE_ROOT"
tools/deploy_prod
```

To reuse the current published artifact set without running the preprocessor:

```bash
cd "$SOURCE_ROOT"
tools/deploy_prod --skip-product
```

## Smoke checks

After deploying nginx, verify:

```bash
curl -I http://localhost/
curl -I http://localhost/packages/current_artifacts.json
```

Then verify at least one representative chart tile, plate image, live-feed
manifest, and shaded-relief tile from the current published artifact set.
