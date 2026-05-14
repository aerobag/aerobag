---
id: TASK-53
title: Handle nav-db current cycle switching
state: medium
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - core
  - data
  - mvp
  - cat:core
dependencies: []
ordinal: 53000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Clarify how the app knows which nav_db is active, how it switches when a new nav_db becomes current, and how it avoids stale cached/unpacked data from the previous nav_db. Determine whether session recreation is sufficient.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core exposes the active nav_db identity/cycle.
- [ ] #2 Switching nav_db invalidates stale unpacked/cached state safely.
- [ ] #3 The user-visible state after a nav_db switch is deterministic and recoverable.
<!-- AC:END -->

