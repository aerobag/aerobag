#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="${AEROBAG_REPO_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)}"

python3 "$ROOT_DIR/ui/scripts/generate_content_fixture.py"
