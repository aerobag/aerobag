# Web Deploy

## Goal

Build the Aerobag web app as a static tree and serve it from a plain web server on an otherwise empty VM.

This document covers the current repo shape:

- repo is present on disk
- a published artifact snapshot is present on disk
- the VM does not already have the dev environment assembled

## What Gets Deployed

The deployed output is a static directory tree produced by:

```bash
cd ui/web-app
npm run build:release
```

That build:

1. stages the current artifact snapshot into the UI target tree
2. builds the WASM module in Rust release mode
3. runs `vite build`
4. materializes the artifact-backed static routes into the final web `dist/` tree

The final deployable tree is:

```text
<repo>/../ui-target/web/dist/
```

Given the current `ui/target-root.txt`, that means:

```text
ui-target/web/dist/
```

adjacent to the repo root.

## Prerequisites

Install these on the VM:

- `git`
- `node`
- `npm`
- `python3`
- Rust toolchain (`cargo`, `rustc`)
- Rust target `wasm32-unknown-unknown`
- `wasm-bindgen` CLI

### Rust setup

If Rust is not already installed:

```bash
curl https://sh.rustup.rs -sSf | sh
. "$HOME/.cargo/env"
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
```

## Inputs Required At Build Time

You need:

1. the repo
2. the published artifact root containing:
   - `published-packaged/`
   - `published-unpacked/`
   - `published-packaged/current_artifacts.json`

The web build reads artifacts from either:

- `AEROBAG_ARTIFACT_READ_PATH`
- or the path stored in `.aerobag-artifact-read-path`

For deployment, prefer the explicit environment variable.

## Build Steps

From the repo root:

```bash
cd ui/web-app
export AEROBAG_ARTIFACT_READ_PATH=/srv/aerobag-artifacts
npm run build:release
```

That writes the deploy tree to:

```text
../ui-target/web/dist/
```

relative to the repo root.

## What Is In The Static Tree

The Vite build plus the static-assets plugin emits:

- application HTML/CSS/JS chunks
- generated WASM bindings
- static app assets
- linked/copied published content trees under:
  - `sectional-packages/`
  - `plates/`
  - `afd/`
  - `thumbnails/`
  - `nav-db/`
  - `nav-kv/`
  - `vectors/`
  - `fast-products/`
  - `adsb-traces/`
  - `shaded-relief-products/`

So the deployed site can be served as ordinary static files.

## Serve It

Serve `ui-target/web/dist/` from nginx, Caddy, Apache, or any other static HTTP server.

Minimum requirement:

- the web root points at `ui-target/web/dist/`

Recommended:

- enable gzip/brotli for text/JSON/JS/WASM
- set long cache headers on hashed JS/CSS assets
- set reasonable cache headers on content trees according to your publication/update model
- configure SPA fallback to `index.html` if desired

## Example nginx Sketch

Not production-polished, but directionally correct:

```nginx
server {
    listen 80;
    server_name _;

    root /srv/aerobag/ui-target/web/dist;
    index index.html;

    location / {
        try_files $uri $uri/ /index.html;
    }
}
```

## Operational Notes

### 1. `npm run build` is not the deploy command

Current script split:

- `npm run build`
  - debug-style web build path
- `npm run build:release`
  - deploy-oriented path with release WASM

For deployment, use:

```bash
npm run build:release
```

### 2. The snapshot is consumed at build time

This deploy shape is static publication, not a runtime artifact server.

If the snapshot changes, rebuild the web tree and redeploy the new `dist/`.

### 3. The result should be portable

The build plugin materializes artifact-backed trees into the output directory using hardlinks where possible and copies when cross-device linking is not possible.

That means the final `dist/` tree should be portable as a standalone filesystem tree after the build completes.

## Recommended Deployment Sequence

1. copy repo to VM
2. copy published artifact snapshot to VM
3. install Node/Python/Rust/wasm-bindgen prerequisites
4. run:

```bash
cd ui/web-app
export AEROBAG_ARTIFACT_READ_PATH=/srv/aerobag-artifacts
npm run build:release
```

5. point your static web server at:

```text
ui-target/web/dist/
```

6. verify:
   - `/`
   - `/nav-kv/root`
   - `/vectors/vectors`
   - one sample plate/thumbnails/vector/fast-product path

## Follow-Up Improvements

The current deploy path is workable now. Good next hardening steps would be:

1. add a single top-level deploy verification script that asserts required static endpoints exist in `dist/`
2. add cache-header guidance per subtree
3. decide whether `npm run build` should eventually become release by default
4. document the production artifact-root layout next to the content publication docs
