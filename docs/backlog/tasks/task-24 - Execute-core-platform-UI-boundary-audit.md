---
id: TASK-24
title: Audit core/platform UI boundary
state: done
assignee: []
created_date: '2026-05-12 16:20'
updated_date: 2026-05-12 21:10
labels:
  - refactor
  - core
  - web
  - android
  - cat:core
dependencies: []
references:
  - docs/refactor/core-platform-ui-boundary-audit.md
ordinal: 24000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Work through the core/platform UI boundary audit and burn down remaining cases where platform UI owns business logic, data contract knowledge, or duplicated policy that should be core-owned. 2026-05-12 audit pass completed. Remaining actionable violations were recorded in `docs/refactor/core-platform-ui-boundary-audit.md` and split into follow-up tasks `TASK-25`, `TASK-105`, `TASK-106`, `TASK-107`, `TASK-108`, `TASK-109`, and `TASK-110`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Re-audit the current code against `docs/refactor/core-platform-ui-boundary-audit.md`.
- [x] #2 Convert remaining actionable violations into tasks or fixes.
- [x] #3 Remove stale audit entries once fixed.
<!-- AC:END -->
