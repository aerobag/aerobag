---
id: TASK-110
title: Move web DPR raster planning policy into core
status: Done
assignee: []
created_date: '2026-05-12 21:10'
labels:
  - core
  - web
  - refactor
  - cat:core
dependencies: []
references:
  - docs/refactor/core-platform-ui-boundary-audit.md
priority: medium
ordinal: 110000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Web currently adjusts the raster planning viewport for `window.devicePixelRatio` before asking core for a tile plan. That is display geometry, but the tile-count/overscaling decision is still planning policy. Pass raw viewport plus display scale to core so web and Android cannot diverge on retina tile selection.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Core raster planning accepts the display scale needed to plan against device pixels.
- [x] #2 Web stops reshaping the viewport before calling core for raster tiles.
- [x] #3 Core tile-count tests cover the device-pixel viewport cases that web now delegates to core.
<!-- AC:END -->
