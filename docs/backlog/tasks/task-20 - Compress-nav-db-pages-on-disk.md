---
id: TASK-20
title: Compress nav-db pages on disk
status: Next
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - performance
  - data
  - preprocessor
  - cat:preprocessor
dependencies: []
priority: medium
ordinal: 20000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Investigate static compression for nav-db pages on disk so package size and fetch volume shrink without adding runtime complexity that hurts lookup latency.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Measure current uncompressed nav-db page size distribution.
- [ ] #2 Evaluate compression options compatible with random page access.
- [ ] #3 Prototype and measure decompression cost on web and Android.
<!-- AC:END -->

