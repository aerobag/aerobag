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
- [x] #1 Identify a reliable weather camera data source.
- [x] #2 Core exposes nearby/visible weather camera features.
- [x] #3 UI can inspect a camera and open/display useful image metadata.
<!-- AC:END -->

## Implementation Note

The default combines the FAA's supported `/api/redistributable/sites` API with
the website inventory to retain Canadian sites. Shared IDs use official metadata.
API access uses a token issued after signing the FAA's MoU. See
[weather camera ingestion](../../WEATHER_CAMERAS.md) for credentials, deployment,
and source selection. The published vector and core/UI contracts are source-independent.
