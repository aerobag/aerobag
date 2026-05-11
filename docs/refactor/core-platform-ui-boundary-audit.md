# Core / Platform UI Boundary Audit

## Contract

Core owns:

- tile request planning
- tile fetch/load policy
- caching policy
- visible-feature assembly

Platform bridge provides:

- tile/resource source implementation
- local zip mode
- remote URL mode

UI receives:

- visible features
- warnings/status

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

## Violations To Burn Down

1. NEXRAD is platform-owned on both web and Android. Each platform fetches the
   manifest and frames directly, owns playback state, and owns error handling.
   It should become a core-planned resource flow.

2. Web adjusts the raster planning viewport for device pixel ratio before asking
   core. That is defensible as display geometry, but the cleaner contract is for
   UI to pass raw viewport plus display scale and for core to own the request
   planning decision.

3. Web terrain warning still owns rendered-image cache lifetime, in-flight tile
   rendering, render queue pumping, and frame publication. That may be acceptable
   as paint-pipeline mechanics, but it should stay out of source tile selection,
   source-byte caching, and fetch policy.

4. Android raster rendering still iterates core-provided candidate sources in
   the platform zip bridge. That is transport/local-storage behavior, not policy,
   as long as source ordering remains core-provided and UI does not invent
   fallback choices.

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

## Remaining Execution Order

1. Move NEXRAD manifest/frame loading into a core-planned resource loop.
2. Move web DPR shaping into core input/options.
3. Decide whether terrain rendered-image queue/cache belongs in core or remains
   platform paint mechanics.

Each step should keep the UI contract moving toward: platform asks core for what
to paint; platform supplies bytes when core asks for resources; platform paints
the result.
