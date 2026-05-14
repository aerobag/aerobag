---
id: TASK-65
title: Reduce zoom fallback phase-in
state: low
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - performance
  - core
  - cat:core
dependencies: []
ordinal: 65000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Evaluate adjustments to zoom fallback behavior to reduce startup phase-in effects and improve performance, if measurements show it matters.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Measure visible phase-in and fallback timing at startup.
- [ ] #2 Decide whether the effect is worth fixing.
- [ ] #3 If needed, adjust core tile planning without reintroducing blurry stale fallbacks.
<!-- AC:END -->

