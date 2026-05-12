---
id: TASK-25
title: Eliminate platform-visible package member resolution
status: Next
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - refactor
  - core
  - web
  - android
  - cat:core
dependencies: []
priority: medium
ordinal: 25000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Web no longer knows most publication contract details, and terrain/NEXRAD source loading has moved into the generic core resource loop. Raster tiles, plates, thumbnails, and generic resolver helpers still expose package ids/member paths to platform UI. Replacing those remaining paths with core resource requests is a separate asset-contract decision.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Core returns opaque resource requests for terrain/NEXRAD data.
- [ ] #2 Web and Android no longer resolve package member URLs/paths in UI-facing code.
- [ ] #3 Existing map layers still fetch and render through the generic resource path.
<!-- AC:END -->
