---
id: TASK-76
title: Help: Add disabled action reason messages
state: medium
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - feature
  - core
  - cat:ui-affordances
dependencies: []
ordinal: 76000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Whenever core disables a button/action, include a reason message explaining why, such as `Bad AP unavailable because no active FP leg`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core action models include optional disabled reason text.
- [ ] #2 Web and Android expose the reason through hover, long-press, or an equivalent platform affordance.
- [ ] #3 Existing disabled actions get useful initial reason messages.
<!-- AC:END -->

