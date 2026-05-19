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

  - Legacy fast bundle plumbing in preproc: bundle_fast_*,
    PublishedFastProductResult, build_or_reuse_fast_product, sync/status/tests
    around fast bundles.
  - Old NEXRAD package builder still exists, but runtime NEXRAD is now the live
    tiled/source-grid path.
  - Naming cleanup: preprocessor-fast, fast_product_* helpers, and node names
    like fast-metars / fast-tfrs are now misleading where they’re reused for
    live-feed builders.
  - Web dev legacy route: Vite still has a /fast-products legacy/404 route.



  - Wire the daemon’s real production loop: run schedulers/pollers, call the
    shared publish tick, and announce SSE invalidations as products update.
  - Finish simulation mode: compile/load fixture timelines and pump accelerated
    events from the daemon, not Vite.
  - Move the remaining live-feed test-only helpers out of preprocessor-cli/src/
    product_build.rs when those tests get relocated.
  - Add real upstream adapters for the daemon path, including future streaming
    sources like SWIM/NOTAMs.


  - Productionize serving: supervisor/reverse proxy details, SSE scaling, and
    health/status reporting.
  - Android live-feed consumption still trails web.
  - NEXRAD PNG delta encoding remains deferred under the TASK-121 placeholder.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Measure cost of refetching fast products through the current contract.
- [ ] #2 Measure frequency and volume of fast product updates.
- [ ] #3 Compute a target data volume for continuous refresh.
- [ ] #4 Propose whether current-artifacts, watch-like discovery, or hanging-get invalidations should be the durable contract.
<!-- AC:END -->

