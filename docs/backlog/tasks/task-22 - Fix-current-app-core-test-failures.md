---
id: TASK-22
title: Fix current app-core test failures
state: done
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - tests
  - core
  - bug
  - cat:core
dependencies: []
ordinal: 22000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`cargo test -p app-core` was noted as failing in six tests around HAD previews, generated KPAE VOR-A UI state, replay source selection, row action/direct-to behavior, and injected CDI guidance geometry. Re-run against current master, update the list, and fix remaining failures.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Re-run `cargo test -p app-core` and record the current failing tests.
- [ ] #2 Fix failures without weakening the behavior under test.
- [ ] #3 `cargo test -p app-core` passes.
<!-- AC:END -->

