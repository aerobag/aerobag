#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
rust_toolchain="${RUST_TOOLCHAIN:-1.94.1}"

if rustc +"${rust_toolchain}" --version >/dev/null 2>&1; then
  cargo_command=(cargo +"${rust_toolchain}")
elif [[ "$(rustc --version)" == "rustc ${rust_toolchain} "* ]]; then
  # rustup may expose the pinned version only through its stable alias.
  cargo_command=(cargo)
else
  echo "Rust ${rust_toolchain} is required for formatting; found $(rustc --version)" >&2
  exit 2
fi

manifests=(
  "crates/Cargo.toml"
  "ui/core-rust/Cargo.toml"
  "product/preprocessor/Cargo.toml"
  "services/Cargo.toml"
)

for manifest in "${manifests[@]}"; do
  echo "Checking Rust formatting: ${manifest}"
  "${cargo_command[@]}" fmt \
    --manifest-path "${repo_root}/${manifest}" \
    --all \
    --check
done
