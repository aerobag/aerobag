---
id: TASK-27
title: Add undo for flight plan actions
state: low
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - feature
  - navigation
  - core
  - cat:core
dependencies: []
ordinal: 27000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add undo support for flight plan mutations so accidental deletes, moves, inserts, and procedure changes can be reversed.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core records enough mutation history to undo recent flight plan changes.
- [ ] #2 Undo restores row UIDs and active/direct-to state consistently.
- [ ] #3 Web and Android expose the same core-provided undo availability and action.
<!-- AC:END -->

