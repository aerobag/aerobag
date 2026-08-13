---
id: TASK-9
title: Validate known approach leg geometry edge cases
state: done
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - core
  - navigation
  - plates
  - bug
  - cat:core
dependencies: []
ordinal: 9000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`CEC ILS or LOC RWY 12` Instead of intercepting CEC R-166, we fly direct CHIDE. -- still open

HYR ILS 21 has an "OBBEY JODES" intersection and a "DAIVE TARRO". I thought we eliminated that case
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add focused tests or diagnostics for the listed approach cases.
- [ ] #2 Classify each case as correct, source-data limitation, or core geometry bug.
- [ ] #3 Fix core geometry bugs or emit appropriate cautions for untrusted procedure interpretation.
<!-- AC:END -->

