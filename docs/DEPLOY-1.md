# apt prereqs

    cargo rustc curl unzip zip python3 gdal-bin python3-gdal imagemagick ghostscript libimage-exiftool-perl systemd poppler-utils sqlite3 openjdk-21-jre-headless

# define these  paths:

## point this at the git clone
```
export SOURCE_ROOT=/wherever/aerobag
```

## point this where we're going to land the binaries and built artifacts
```
export ARTIFACT_ROOT=/mnt/aerobag-data/artifacts
```

## these are defined based on those
```
export CARGO_TARGET_DIR="$ARTIFACT_ROOT/target"
export AEROBAG_ARTIFACT_WRITE_PATH="$ARTIFACT_ROOT"
export AEROBAG_ARTIFACT_READ_PATH="$ARTIFACT_ROOT"
```

Set these envs up somewhere that you can source them before you run the
commands below, so they're in env context when we need them.

# Build the binary

cd "$SOURCE_ROOT/product/preprocessor"
cargo build -p preprocessor-cli

# Schedule the product builds

## Every 2 hours, run build-product
time "$ARTIFACT_ROOT/target/debug/preprocessor-cli" build-product --source-root "$SOURCE_ROOT"

## Every 5 minutes, run build-fast-subset
time "$ARTIFACT_ROOT/target/debug/preprocessor-cli" build-fast-subset --source-root "$SOURCE_ROOT"
