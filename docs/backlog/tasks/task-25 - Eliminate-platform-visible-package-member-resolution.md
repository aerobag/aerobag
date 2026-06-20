---
id: TASK-25
title: Eliminate platform-visible package member resolution
state: done
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - refactor
  - core
  - web
  - android
  - cat:core
dependencies: []
ordinal: 25000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Web no longer knows most publication contract details, and terrain/NEXRAD source loading has moved into the generic core resource loop. Plates and thumbnails must resolve through core-owned operations rather than platform-visible package ids/member paths. Platform-facing chart records must not carry package ids or member paths; core keeps those raw nav-db records internal.

Raster tiles are an explicit performance exception: core still owns source selection and package/member resolution, but platform renderers use tile-specific fetch/decode paths. The raster code must carry loud comments pointing future resource work back to the normalized core-driven resource path.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Core returns opaque resource requests for terrain/NEXRAD data.
- [x] #2 Web and Android no longer receive or resolve plate/thumbnail package member URLs/paths in UI-facing code.
- [x] #3 Raster tile paths document their explicit performance exception and point to the normalized core resource path for new resources.
<!-- AC:END -->
