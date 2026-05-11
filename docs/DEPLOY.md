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
cargo build --release -p preprocessor-cli
```

Run the product build once before installing timed executions. The fast-subset
build depends on the cycle product existing first.

```bash
time "$CARGO_TARGET_DIR/release/preprocessor-cli" build-product --source-root "$SOURCE_ROOT"
```

Then run the fast subset build once:

```bash
time "$CARGO_TARGET_DIR/release/preprocessor-cli" build-fast-subset --source-root "$SOURCE_ROOT"
```

After these complete, `$ARTIFACT_ROOT` should contain the published artifact
contract the web build consumes:

- `current_artifacts.json`
- `published_packaged/`
- `published_unpacked/`

## Build the web static tree

The web release build reads from `$AEROBAG_ARTIFACT_READ_PATH` and writes under
`$AEROBAG_UI_TARGET_ROOT`.

```bash
cd "$SOURCE_ROOT/ui/web-app"
npm run build:release
```

That command:

1. validates the current artifact set that will be served under `/packages`
2. builds the Rust/WASM adapter in release mode
3. runs TypeScript checking
4. runs `vite build`
5. writes the deployable static tree to `$AEROBAG_WEB_DIST`

The static tree includes the app chunks and WASM. Product content is served
through the publication contract rooted at `/packages`; do not publish or rely
on legacy content routes such as `/plates`, `/thumbnails`, `/nav-kv`, or
`/sectional-packages`.

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

Once the one-shot product and web build are working, schedule content refreshes.

Every 2 hours, run `build-product`:

```bash
time "$CARGO_TARGET_DIR/release/preprocessor-cli" build-product --source-root "$SOURCE_ROOT"
```

Every 5 minutes, run `build-fast-subset`:

```bash
time "$CARGO_TARGET_DIR/release/preprocessor-cli" build-fast-subset --source-root "$SOURCE_ROOT"
```

When the published artifacts change, rebuild and redeploy the web static tree:

```bash
cd "$SOURCE_ROOT/ui/web-app"
npm run build:release
```

## Smoke checks

After deploying nginx, verify:

```bash
curl -I http://localhost/
curl -I http://localhost/packages/current_artifacts.json
```

Then verify at least one representative chart tile, plate image, fast-product
file, and shaded-relief tile from the current published artifact set.
