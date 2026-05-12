---
id: TASK-55
title: Scan vector density and tune thresholds
status: Next
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - performance
  - data
  - preprocessor
  - cat:preprocessor
dependencies: []
priority: medium
ordinal: 55000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Run a nationwide density scan for each vector object type, point the map at the densest areas, and decide whether vector thresholds need adjustment.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Generate density metrics by feature type and geography.
- [ ] #2 Create reproducible links/viewports for densest areas.
- [ ] #3 Adjust preproc/core thresholds if density is visually or performance-wise wrong.
<!-- AC:END -->

