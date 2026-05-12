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
Web no longer knows most publication contract details, but terrain/NEXRAD APIs can still expose `product_id` plus member path and require platform `resolvePackageMemberUrl` helpers. Replace those paths with core resource requests so UI never needs package member concepts.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core returns opaque resource requests for terrain/NEXRAD and similar data.
- [ ] #2 Web and Android no longer resolve package member URLs/paths in UI-facing code.
- [ ] #3 Existing map layers still fetch and render through the generic resource path.
<!-- AC:END -->

