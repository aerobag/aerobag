---
id: TASK-10
title: Approach decoding quality warning
state: done
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - core
  - navigation
  - safety
  - feature
  - cat:productionization
dependencies: []
ordinal: 10000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Approach decoding quality warning
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core detects procedure-turn or hold-in-lieu segments that are not fully trusted.
- [ ] #2 Core detects other approach decoding quality concerns and emits structured warnings.
- [ ] #3 UI receives a structured warning for affected procedures.
- [ ] #4 The warning appears in the general caution/status mechanism.
<!-- AC:END -->
