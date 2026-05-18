---
id: TASK-4
title: live-feeds transition
state: high
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - publication
  - performance
  - data
  - cat:data
dependencies: []
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
What’s left in “fast-products”:

  - Winds aloft: still old fast-product style. Likely convert to a live feed
    with full-state updates; low cadence makes deltas less important.
  - Obstacles: still not migrated. This one probably needs real delta handling
    because the measured deltas were tiny compared to full state.
  - Legacy fast bundle plumbing in preproc: bundle_fast_*,
    PublishedFastProductResult, build_or_reuse_fast_product, sync/status/tests
    around fast bundles.
  - Old NEXRAD package builder still exists, but runtime NEXRAD is now the live
    tiled/source-grid path.
  - Naming cleanup: preprocessor-fast, fast_product_* helpers, and node names
    like fast-metars / fast-tfrs are now misleading where they’re reused for
    live-feed builders.
  - Web dev legacy route: Vite still has a /fast-products legacy/404 route.

  Runtime-wise, METARs, NEXRAD, and TFRs are now live-feed driven. The remaining
  real product migrations are winds aloft and obstacles.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Measure cost of refetching fast products through the current contract.
- [ ] #2 Measure frequency and volume of fast product updates.
- [ ] #3 Compute a target data volume for continuous refresh.
- [ ] #4 Propose whether current-artifacts, watch-like discovery, or hanging-get invalidations should be the durable contract.
<!-- AC:END -->

