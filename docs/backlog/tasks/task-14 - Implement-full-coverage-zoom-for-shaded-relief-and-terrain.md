---
id: TASK-14
title: Implement full coverage zoom for shaded relief and terrain
status: Next
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - terrain
  - preprocessor
  - core
  - feature
  - cat:preprocessor
dependencies: []
priority: medium
ordinal: 14000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Shaded relief and terrain are missing full_coverage_zoom behavior. Fixing this likely requires preproc-level assembly across regions so the client can request complete coverage without region-edge gaps.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Define the preproc contract for full coverage shaded relief and terrain tiles.
- [ ] #2 Core tile planning uses the contract instead of hardcoded regional assumptions.
- [ ] #3 Region intersections do not produce missing or blurry terrain/shaded-relief coverage.
<!-- AC:END -->

