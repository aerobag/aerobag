---
id: TASK-62
title: Visualize chart cutlines and seams
status: Next
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - feature
  - plates
  - cat:navigation
dependencies: []
priority: medium
ordinal: 62000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add visible cutline/seam rendering, including pink dashed strokes at chart cutline borders and dashed strokes around TAC or chart seam edges. This should make overlap seams obvious and help diagnose label issues such as the Oak Ridge NSA label lost under a chart cutline.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core exposes chart boundary/cutline geometry needed for seam rendering.
- [ ] #2 UI can toggle or render seam strokes without obscuring chart content.
- [ ] #3 TAC margins/legends can be clipped or handled using the same boundary knowledge.
<!-- AC:END -->

