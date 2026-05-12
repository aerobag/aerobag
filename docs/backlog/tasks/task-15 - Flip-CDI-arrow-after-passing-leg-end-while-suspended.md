---
id: TASK-15
title: Flip CDI arrow after passing leg end while suspended
status: Inbox
assignee: []
created_date: '2026-05-12 16:20'
updated_date: '2026-05-12 17:49'
labels:
  - navigation
  - core
  - bug
  - cat:core
dependencies: []
priority: low
ordinal: 15000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The CDI arrow should flip after passing the leg-end waypoint, especially when sequencing is suspended and the active leg does not advance.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add a core guidance test for crossing the leg end while suspended.
- [ ] #2 CDI guidance flips to the correct from/to presentation after crossing.
- [ ] #3 Web and Android render the same core guidance state.
<!-- AC:END -->
