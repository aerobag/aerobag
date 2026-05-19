---
id: TASK-68
title: Fix Android vector density and label overlap
state: done
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - android
  - parity
  - bug
  - mvp
  - cat:android
dependencies: []
ordinal: 68000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Android shows bay-area intersections far too densely and appears not to apply overlapping-label removal that was supposed to be core-owned. Investigate whether Android pixel scaling or platform-side filtering caused the divergence.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Compare web and Android visible feature/label sets for the same viewport.
- [ ] #2 Ensure overlap removal and density policy are core-owned.
- [ ] #3 Android matches web density and label suppression for the same core state.
<!-- AC:END -->

