#!/bin/bash
DEST=/root/aerobag-artifacts-snapshot
SOURCE_ROOT=/root/aerobag-artifacts

set -euo pipefail

rm -rf $DEST
mkdir -p "$DEST/published-packaged"

time cp -rl "$SOURCE_ROOT/published-packaged/production" "$DEST/published-packaged/production"
time cp -rl "$SOURCE_ROOT/published-unpacked" "$DEST/published-unpacked"
