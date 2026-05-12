---
id: TASK-61
title: Support arbitrary lat/lon waypoints
status: Next
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - feature
  - navigation
  - cat:navigation
dependencies: []
priority: medium
ordinal: 61000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add arbitrary lat/lon waypoint mode, but do not create arbitrary waypoints by dragging route waypoints off real features.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Define explicit UI affordance for creating a lat/lon waypoint.
- [ ] #2 Core models arbitrary waypoints distinctly from nav-db references.
- [ ] #3 Dragging existing real-feature waypoints does not silently convert them to arbitrary points.
<!-- AC:END -->

