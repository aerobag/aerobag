#!/usr/bin/env bash

# SPDX-FileCopyrightText: 2026 Aerobag contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo run \
  --quiet \
  --manifest-path "$repo_root/ui/core-rust/Cargo.toml" \
  -p app-ui-contracts \
  --features schema \
  --bin generate-ui-contract-schemas
