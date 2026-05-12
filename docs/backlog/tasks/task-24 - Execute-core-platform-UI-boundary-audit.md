---
id: TASK-24
title: Execute core/platform UI boundary audit
status: Next
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - refactor
  - core
  - web
  - android
  - cat:core
dependencies: []
references:
  - docs/refactor/core-platform-ui-boundary-audit.md
priority: high
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Work through the core/platform UI boundary audit and burn down remaining cases where platform UI owns business logic, data contract knowledge, or duplicated policy that should be core-owned.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Re-audit the current code against `docs/refactor/core-platform-ui-boundary-audit.md`.
- [ ] #2 Convert remaining actionable violations into tasks or fixes.
- [ ] #3 Remove stale audit entries once fixed.
<!-- AC:END -->

