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
