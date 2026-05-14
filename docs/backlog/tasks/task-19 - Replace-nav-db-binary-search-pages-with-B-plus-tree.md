---
id: TASK-19
title: Replace nav-db binary search pages with B+ tree
state: done
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - performance
  - preprocessor
  - core
  - data
  - cat:preprocessor
dependencies: []
ordinal: 19000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Startup and viewport teleport latency is dominated by dynamic vector fetches requiring many round trips through binary search. Replace the current offset/keys/values layout with a B+ tree shaped nav-db so lookup round trips are log_k rather than log_2, while retaining a prefetch story for known key policies.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Define a breaking nav-db page contract using B+ tree interior and leaf nodes.
- [ ] #2 Update the preprocessor builder and core reader together with no legacy fallback.
- [ ] #3 Preserve or improve prefetch by mapping expected keys to touched B+ tree pages.
- [ ] #4 Measure RTT reduction versus the current binary-search page layout.
<!-- AC:END -->

