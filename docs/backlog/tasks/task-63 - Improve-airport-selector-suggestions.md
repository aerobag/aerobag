---
id: TASK-63
title: Improve airport selector suggestions
status: Next
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - feature
  - navigation
  - cat:navigation
dependencies: []
priority: medium
ordinal: 63000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Airport selector lists should include flight plan airports, an LRU cache of recently touched airports, and possibly the first five towered airports by distance from ownship.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core owns airport suggestion ranking and sources.
- [ ] #2 Suggestions include flight plan and recent airports.
- [ ] #3 Nearby towered airport suggestions are evaluated and added if useful.
<!-- AC:END -->

