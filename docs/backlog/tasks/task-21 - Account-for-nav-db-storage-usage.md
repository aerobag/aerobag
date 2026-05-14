---
id: TASK-21
title: Account for nav-db storage usage
state: medium
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - performance
  - data
  - preprocessor
  - cat:performance
dependencies: []
ordinal: 21000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Do detailed accounting for what consumes space in the nav-db so optimization work is guided by measured byte costs rather than guesses.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Produce a size breakdown by major nav-db key/value families.
- [ ] #2 Identify the largest avoidable overheads.
- [ ] #3 Record recommendations for compression, schema, or tiling changes.
<!-- AC:END -->

