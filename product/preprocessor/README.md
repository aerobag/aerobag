# Aerobag Preprocessor

This workspace builds the authoritative Aerobag product artifacts. Product
builds and app/core tests are the contract.

## Common Commands

Build the cycle product:

```bash
cargo build -p preprocessor-cli
../../aerobag-artifacts/target/debug/preprocessor-cli build-product
```

Run a local live-feed daemon:

```bash
cargo build -p live-feeds-daemon
../../aerobag-artifacts/target/debug/aerobag-live-feedsd \
  --live-root ../../aerobag-artifacts/live-feeds \
  --scratch-root ../../aerobag-artifacts/private-work/live-feeds \
  --listen 127.0.0.1:8095
```

Run tests:

```bash
cargo test --workspace
```

Show the full internal/debug CLI surface:

```bash
../../aerobag-artifacts/target/debug/preprocessor-cli --long-help
```
