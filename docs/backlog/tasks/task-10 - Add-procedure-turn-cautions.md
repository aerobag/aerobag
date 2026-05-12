---
id: TASK-10
title: Add procedure turn cautions
status: Next
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - core
  - navigation
  - safety
  - feature
  - cat:core
dependencies: []
priority: high
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Procedures containing procedure turns should surface a caution that the pilot must confirm the maneuver stays within published plate limits. This should be a core-generated warning, not a UI heuristic.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core detects procedure-turn or hold-in-lieu segments that are not fully trusted.
- [ ] #2 UI receives a structured warning for affected procedures.
- [ ] #3 The warning appears in the general caution/status mechanism.
<!-- AC:END -->

