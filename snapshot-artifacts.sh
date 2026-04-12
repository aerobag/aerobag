#!/bin/bash
DEST=/root/aerobag-artifacts-snapshot
SOURCE_ROOT=/root/aerobag-artifacts

set -euo pipefail

rm -rf $DEST
mkdir -p "$DEST/product-builds"

time cp -rl "$SOURCE_ROOT/product-builds/production" "$DEST/product-builds/production"
time cp -rl "$SOURCE_ROOT/product-builds/shared" "$DEST/product-builds/shared"
