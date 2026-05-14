---
id: TASK-11
title: Build general caution status system
state: high
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - core
  - safety
  - feature
  - mvp
  - cat:core
dependencies: []
ordinal: 11000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a clickable caution signal that propagates from detailed warnings up through status pills to a top-level status pill. Opening it should show active warnings such as procedure-turn cautions and stale ADSB, NEXRAD, METAR, NOTAM, or TFR data.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core exposes active warnings with type, severity, age/source data, and display text.
- [ ] #2 UI shows an aggregated caution/status pill when warnings exist.
- [ ] #3 Clicking the caution opens a page or tray listing the active warnings.
- [ ] #4 Warnings clear when core recomputes that the unsafe condition no longer exists.
<!-- AC:END -->

