---
id: TASK-42
title: Add weather camera layer
state: low
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - weather
  - feature
  - cat:weather
dependencies: []
ordinal: 42000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add weather camera support, with SAC 150 at 13 nm called out as an example candidate.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Identify a reliable weather camera data source.
- [x] #2 Core exposes nearby/visible weather camera features.
- [x] #3 UI can inspect a camera and open/display useful image metadata.
<!-- AC:END -->

## Implementation Note

The first implementation uses the public site's undocumented
`weathercams.faa.gov/api/sites` inventory through an isolated preprocessor adapter. The endpoint
requires the same-origin `Referer` header sent by the site. It must be replaced with an official
supported inventory before criterion #1 can be checked; the published vector and core/UI contracts
are source-independent.
