---
id: TASK-6
title: Preserve active leg by row UID after flight plan edits
status: MVP
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - core
  - navigation
  - bug
  - mvp
  - cat:core
dependencies: []
priority: high
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Entering `KRNT SEA V2 ELN KYKM`, activating `SEA -> VAMPS`, then deleting `KRNT` changes the active leg to `VAMPS -> BANDR`. Active leg identity appears to be recomputed by vector index after mutation instead of staying attached to stable row UIDs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add a regression test for deleting a row before an active leg that survives the edit.
- [ ] #2 Active leg identity remains unchanged whenever the edit does not remove either endpoint row.
- [ ] #3 The fix is general and does not reconstruct identity from post-edit indices.
<!-- AC:END -->

