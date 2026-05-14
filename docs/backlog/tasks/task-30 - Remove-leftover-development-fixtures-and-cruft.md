---
id: TASK-30
title: Remove leftover development fixtures and cruft
state: low
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - cleanup
  - source
  - cat:productionization
dependencies: []
ordinal: 30000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Search the source tree for leftover fixtures, staging artifacts, and dead development code that can now be removed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Identify stale fixtures and cruft with references proving they are unused.
- [ ] #2 Delete unused artifacts.
- [ ] #3 Keep test fixtures that still have a clear purpose and document that purpose if needed.
<!-- AC:END -->
