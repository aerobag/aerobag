# Aerobag Preprocessor

This workspace builds the authoritative Aerobag product artifacts. Product
builds and app/core tests are the contract.

## Common Commands

Build the cycle product:

```bash
cargo build -p preprocessor-cli
../../aerobag-artifacts/target/debug/preprocessor-cli build-product
```

Build fast products:

```bash
cargo build -p preprocessor-cli
../../aerobag-artifacts/target/debug/preprocessor-cli build-fast-subset
```

Run tests:

```bash
cargo test --workspace
```

Show the full internal/debug CLI surface:

```bash
../../aerobag-artifacts/target/debug/preprocessor-cli --long-help
```
