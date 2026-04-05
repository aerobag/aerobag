# Legacy Capture Scaffold

This directory is the first pass of the compatibility capture harness described in [NEXT_SESSION.md](/root/aerobag/NEXT_SESSION.md).

It does four things:

- builds one shared legacy-tooling container instead of relying on the four repo-local Dockerfiles,
- runs a narrow representative capture set,
- records stdout and stderr per legacy job,
- snapshots output hashes, ZIP member lists, tile paths, and structured provenance for later Rust parity tests.

## Capture set

The default run executes:

- `charts/sec.py`
- `charts/tac.py`
- `charts/enr_l.py`
- `tpp/tpp.py NE`
- `csup/csup.py`

That matches the immediate handoff target closely enough to get golden artifacts without trying to boil the ocean.

## Usage

From the workspace root:

```bash
chmod +x legacy-capture/run_legacy_capture.sh legacy-capture/capture_inside_container.sh
RUNTIME=docker CPU_JOBS=16 ./legacy-capture/run_legacy_capture.sh
```

If you already have the native tools installed in the current environment, use the direct runner instead:

```bash
chmod +x legacy-capture/run_legacy_capture_direct.sh legacy-capture/capture_inside_container.sh
CPU_JOBS=16 ./legacy-capture/run_legacy_capture_direct.sh
```

Useful overrides:

- `RUNTIME=podman`
- `RUN_ID=20260405T153000Z`
- `OUTPUT_ROOT=/fastssd/aerobag-runs/20260405T153000Z`
- `CACHE_ROOT=/fastssd/aerobag-cache`
- `IMAGE_TAG=aerobag/legacy-capture:2026-04-05`

To inspect a run quickly:

```bash
python3 legacy-capture/run_status.py runs/<run_id>
```

## Provisioning

If you want host-level provisioning instead of building the optional wrapper image:

```bash
sudo python3 legacy-capture/provision_legacy_host.py
```

For cloud-init based machines, use:

- [cloud-init.legacy-capture.yaml](/root/aerobag/legacy-capture/cloud-init.legacy-capture.yaml)

## Output layout

The run writes into `runs/<run_id>/` by default:

- `logs/`
  - per-job stdout and stderr
- `artifacts/`
  - copied ZIPs, manifest text files, databases, and tile trees
- `meta/*.members.txt`
  - ZIP member lists
- `meta/*.tile-paths.txt`
  - sorted tile paths for tiled chart families
- `meta/*.sha256`
  - file hashes for working trees and copied outputs
- `meta/provenance/<label>/source_urls.jsonl`
  - crawler results and explicit source URLs
- `meta/provenance/<label>/downloads.jsonl`
  - downloaded filenames, content hashes, sizes, and extracted archive members
- `meta/provenance/<label>/package_outputs.jsonl`
  - emitted package filenames and hashes
- `meta/manifest.json`
  - machine-readable summary of the run
- `meta/*.summary.json`
  - condensed per-capture provenance and output counts

The manifest format is described by [manifest.schema.json](/root/aerobag/legacy-capture/manifest.schema.json).

## Current limits

- The image is shared and reproducible enough for local work, but it is not yet pinned to an immutable base digest.
- Source URL capture is still indirect through stdout and downloaded filenames; the next pass should instrument the Python download helpers directly.
- The current run that is already in flight predates the provenance hooks, so these JSONL files will appear on the next rerun rather than the active one.
- `data/` is not included yet because the current handoff prioritized charts, TPP, and CSUP first.
- This scaffold has not been executed in this environment because container runtime access was not requested during this turn.
- The direct runner still requires the same native packages as the container image: GDAL, Ghostscript, ImageMagick, exiftool, SQLite, and the Python dependencies used by the legacy repos.
