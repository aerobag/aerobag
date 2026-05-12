---
id: TASK-3
title: Support overlapping publication contract versions
status: Next
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - publication
  - android
  - core
  - mvp
  - cat:productionization
dependencies: []
priority: high
ordinal: 3000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
During publication contract upgrades, support simultaneous products long enough for users to update Android over a cycle or two without being stranded by a server-side contract cutover.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Define the supported overlap window for product contract versions.
- [ ] #2 Core/platform startup rejects unsupported contracts with a recoverable, user-visible error.
- [ ] #3 Android can continue using a still-supported previous product while the current product has advanced.
<!-- AC:END -->

