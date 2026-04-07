# Rust Preprocessor Workspace

This workspace is the first executable scaffold for the Rust replacement described in:

- [RUST_PREPROCESSOR_DESIGN.md](/root/aerobag/RUST_PREPROCESSOR_DESIGN.md)
- [GOLDEN_COMPARISON_CONTRACT.md](/root/aerobag/legacy-capture/GOLDEN_COMPARISON_CONTRACT.md)

## Crates

- `preprocessor-cli`
  - initial CLI for inspecting capture runs and printing current baselines
- `preprocessor-core`
  - shared manifest types and baseline comparison constants
- `preprocessor-fetch`
  - initial manifest-path and fetch-oriented helpers
- `preprocessor-tools`
  - initial comparison-target selection helpers

## Immediate purpose

This is not the full implementation yet. The point of the first scaffold is to:

- load the legacy capture manifest as a typed Rust structure,
- encode the current tile-count baselines in code,
- give the next iteration a real workspace to extend with fetch/cache and tool orchestration.

## First commands

```bash
cd rust-preprocessor
cargo run -p preprocessor-cli -- print-baseline
cargo run -p preprocessor-cli -- inspect-run --run-root ../runs/20260405T154700Z
```
