---
id: TASK-69
title: Fix Replay panel click-through z-order
state: medium
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - android
  - bug
  - cat:android
dependencies: []
ordinal: 69000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Replay panel play button can be clicked through, suggesting a z-order or touch-handling bug.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reproduce click-through on the Replay panel.
- [ ] #2 Fix z-order or event handling so the top control owns the tap.
<!-- AC:END -->

