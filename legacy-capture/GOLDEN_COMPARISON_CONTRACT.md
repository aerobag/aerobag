# Golden Comparison Contract

Snapshot date: 2026-04-05
Reference run: [runs/20260405T154700Z](/root/aerobag/runs/20260405T154700Z)

This file defines the minimum comparison contract that the Rust replacement should satisfy against legacy captures.

## Comparison levels

Use four levels of comparison, in this order:

1. Source provenance
2. Package contract
3. Tile-path coverage
4. Sampled content equivalence

The first implementation should pass the first three levels before spending time on richer image diffs.

## Required captured artifacts

For each captured job, preserve:

- stdout and stderr logs
- explicit source URL lists
- downloaded filenames with content hashes and sizes
- extracted archive member lists
- package ZIP filenames and hashes
- manifest filenames and hashes
- ZIP member listings
- tile-path listings for tiled chart families

These now map to:

- `meta/provenance/<label>/source_urls.jsonl`
- `meta/provenance/<label>/downloads.jsonl`
- `meta/provenance/<label>/package_outputs.jsonl`
- `meta/*.members.txt`
- `meta/<label>.tile-paths.txt`
- `meta/<label>.outputs.sha256`

## Rust parity requirements for V1

For a given capture label, Rust output should match legacy on:

- same package names
- same manifest names
- same ZIP member paths
- same tile-path set
- same region split set
- same cycle string on manifest first line

The first pass does not need byte-identical ZIP archives or byte-identical imagery, but it must not introduce coverage gaps.

## Current observed baselines

From the in-flight reference run:

- `charts-sec`
  - package count: `9`
  - tile-path count: `35494`
- `charts-tac`
  - package count: `9`
  - tile-path count: `7174`
- `charts-enr-l`
  - package count: `9`
  - tile-path count: `27428`

These numbers are not universal forever, but they are the concrete baseline for the current cycle and should be treated as the immediate golden target.

## Package contract checks

For each capture, compare:

- expected package filename set
- expected manifest filename set
- ZIP member path set per package
- manifest body path set per package

For tiled families, compare both:

- union tile-path set for the whole job
- per-region ZIP member tile-path sets

For standalone image families, compare:

- output image path set
- per-region ZIP member path sets
- EXIF `UserComment` presence and parseability where applicable

## Provenance checks

Rust should record enough metadata to answer:

- which source URLs were considered
- which URLs were selected
- whether each file was downloaded or reused from cache
- the SHA-256 of each fetched source file
- which archive members were extracted

This lets us separate true rendering regressions from upstream-input drift.

## Suggested Rust test layout

- `provenance_parity`
  - assert selected source URL set matches expected legacy capture
- `package_contract_parity`
  - assert package names and member paths match
- `tile_path_parity`
  - assert exact tile-path set match for tiled families
- `sampled_visual_parity`
  - compare a bounded sample of rendered outputs once structural parity is stable

## What not to require yet

Do not require these in the first compatibility gate:

- byte-identical ZIP files
- byte-identical PNG or WebP files
- matching timestamps inside ZIP metadata
- matching compression ratios

Those are useful diagnostics, but they are not the primary compatibility target.
