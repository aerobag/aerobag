---
id: TASK-31
title: Audit and gate logging
status: Next
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - cleanup
  - deployment
  - web
  - android
  - cat:web
dependencies: []
priority: medium
ordinal: 31000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Review all logging and decide what should be removed, commented as useful debugging context, or guarded by a flag that can be re-enabled when diagnosing issues.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Inventory logging across web, Android, and core boundary paths.
- [ ] #2 Remove noisy or obsolete logs.
- [ ] #3 Gate valuable diagnostics behind explicit debug/developer controls.
<!-- AC:END -->

