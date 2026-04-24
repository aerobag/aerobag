# Refactoring Ideas

These are follow-up cleanups discovered during feature work. They are not part
of the current implementation unless explicitly pulled into scope.

## Core/UI Data Contract

- Move more app-ready view models behind core-owned queries so web and Android
  do not independently derive runtime UI state from producer-shaped artifacts.
- Replace UI-side `resource_index` derivations with core APIs backed by
  `nav_kv`/`nav_tiles`.
- Keep old compatibility exports only while migration is in progress; remove
  them once all call sites consume the core-owned contract.
- The initial `nav_kv` `chart/catalog` writer mirrors the old web
  `deriveMapViews` shape to make migration easy. Once the app consumes
  `chart/catalog` through core, remove the old UI-side resource-index adapter and
  keep the chart catalog schema owned by core/preproc.
- The shared Rust HAD reader now owns root parsing, binary search, page math,
  value extraction, page-byte caching, and domain key construction. The web
  `navHad.ts` helper layer is gone; keep pushing toward explicit
  `NeedHadPages`/resume APIs so platform code only fetches opaque page bytes.

## Async Transport Boundary

- Current web feature loading for map overlays still uses a clumsy
  query/need-data/fetch/ingest/requery loop:
  - UI asks core for feature state
  - core returns "I need X"
  - web fetches X and stuffs it back into core
  - UI asks again
- This works with synchronous wasm exports and asynchronous browser fetch, but
  it entangles transport orchestration with the UI layer.
- Preferred shape:
  - UI asks core for feature state
  - core requests transport through a platform callback or adapter-owned
    continuation loop
  - platform fetches bytes/JSON
  - core resumes and returns the final UI answer
- Near-term cleanup:
  - move fetch/ingest/requery orchestration out of React/UI code and into the
    web/android adapter boundary
  - expose generic core transport requests instead of feature-specific booleans
    like `needed_tfrs`
- Longer-term cleanup:
  - make the core/platform boundary properly async so core can suspend on host
    transport instead of forcing a manual continuation protocol
