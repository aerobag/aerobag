---
id: TASK-75
title: Improve Android raster tile performance
state: medium
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - android
  - performance
  - cat:android
dependencies: []
ordinal: 75000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Improve Android raster performance by evaluating a bounded decoded-tile LRU, limiting concurrent zip reads/decode, prefetching on PLATE after leaving CHART, and measuring whether Android plans too many raster tiles compared with web.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Measure Android planned raster tile counts versus web for the same viewport.
- [ ] #2 Add bounded caching or concurrency limits only where measurements justify them.
- [ ] #3 Reduce PLATE-to-CHART reload pain without preserving whole UI state.
<!-- AC:END -->

