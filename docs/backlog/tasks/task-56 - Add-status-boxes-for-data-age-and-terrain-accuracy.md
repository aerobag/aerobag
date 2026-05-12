---
id: TASK-56
title: Add status boxes for data age and terrain accuracy
status: Next
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - safety
  - weather
  - terrain
  - feature
  - cat:productionization
dependencies: []
priority: medium
ordinal: 56000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add status boxes such as `NEXRAD age: 7 minutes` computed relative to original data time, and `Terrain: +/- 300ft`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core exposes data age and accuracy status fields.
- [ ] #2 UI renders compact status boxes consistently across platforms.
- [ ] #3 Stale or degraded status can feed the caution system.
<!-- AC:END -->

