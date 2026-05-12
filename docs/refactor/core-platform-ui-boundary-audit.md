# Core / Platform UI Boundary Audit

## Contract

Core owns:

- tile request planning
- tile fetch/load policy
- caching policy
- visible-feature assembly
- package/publication-contract interpretation
- flight-plan mutation policy
- chart/plate selection policy
- layer availability and status policy

Platform bridge provides:

- tile/resource source implementation
- local zip mode
- remote URL mode
- cancellable byte fetch for opaque core resource requests

UI receives:

- visible features
- warnings/status
- view models and opaque action ids

## Current State

The vector/weather overlay path is close to the intended architecture. Web and
Android call core for the map overlay, satisfy generic `need_resources` requests,
ingest those bytes back into core, and then render the visible features returned
by core.

Raster tile planning is also core-owned in the main map path on both platforms.
Both platforms call core for a raster tile plan and render the returned tiles.

Terrain warning source-tile fetch/load is now also core-owned. Core returns
generic resource requests for terrain source bytes, stores those bytes in the
session, and platforms ask core to render a terrain image by core tile key. The
remaining platform-side state is mechanical decoded-image/render scheduling.

This audit pass was refreshed on 2026-05-12. The concrete platform/core drift
items from TASK-24 have been burned down through TASK-110. TASK-25 remains as a
separate contract decision because eliminating all platform-visible package
member resolution would change the raster/plate asset transport contract.

## Remaining Contract Decision

Platform-visible package-member resolution still exists for raster tiles,
plates, thumbnails, and generic web publication resolver helpers. NEXRAD and
terrain now use core resource requests, but moving raster/plate assets to the
same opaque-resource contract would alter the current tile/plate handoff. Track
that explicitly under `TASK-25`.

## Completed Burn-Down

- Removed stale overlay `needed_*` fields from platform-facing web and Android
  UI types/logs. Core still keeps internal skipped fields for convergence and
  resource planning.
- Deleted the dead web raster planner and its tests. Android `MapViewport.kt`
  now contains viewport gesture math and render tile identity only, not raster
  tile planning.
- Moved terrain warning source tile fetch/load into the generic core resource
  loop. Platforms now provide bytes for opaque resource requests and call back
  into core to render by terrain tile key.
- Removed Android's old parent/child bitmap fallback drawing path; Android now
  paints core-planned raster tiles and uses platform code only to satisfy
  core-provided source candidates from local zip storage.
- Android raster rendering still iterates core-provided candidate sources in the
  platform zip bridge. That is transport/local-storage behavior, not policy, as
  long as source ordering remains core-provided and UI does not invent fallback
  choices.
- Web terrain warning still owns rendered-image cache lifetime, in-flight tile
  rendering, render queue pumping, and frame publication. That is currently
  classified as paint-pipeline mechanics, not source-selection policy; revisit
  only if perf or divergence evidence appears.
- NEXRAD manifest/frame loading moved into a core-planned resource loop.
- Web startup vector-manifest synthesis is gone; session startup uses a minimal
  bootstrap and core/HAD owns the real vector manifest.
- Android metadata-free package ZIPs are no longer guessed into installed
  artifacts.
- Web and Android chart-page fallback helpers were removed from platform UI.
- Platform-facing mirrored flight-plan mutation APIs were removed; route-entry
  preview/append now operate on the live session.
- Web DPR raster planning policy moved into core input/options.
