---
id: TASK-44
title: Fix ownship-driven sequencing and suspend
state: done
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - navigation
  - core
  - bug
  - mvp
  - cat:core
dependencies: []
ordinal: 44000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Ownship-driven sequencing and suspend behavior is not working correctly. Use the existing test description as the starting point and make sequencing state core-owned and test-covered.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add or update core tests for ownship-driven sequencing and suspended sequencing.
- [ ] #2 Sequence and suspend transitions match the expected navigation behavior.
- [ ] #3 Web and Android render the same sequencing state from core.
<!-- AC:END -->

