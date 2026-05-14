---
id: TASK-9
title: Validate known approach leg geometry edge cases
state: medium
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
Check known suspicious approach cases: `CEC ILS or LOC RWY 12` around the 10 nm leg from SLAMM, `CYS RNAV RWY 13` sharp turn at EMOTY and possible hold/procedure-turn behavior, and `HYR ILS 21` where GRASS 250 heading currently appears to jump direct DAIVE.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Add focused tests or diagnostics for the listed approach cases.
- [ ] #2 Classify each case as correct, source-data limitation, or core geometry bug.
- [ ] #3 Fix core geometry bugs or emit appropriate cautions for untrusted procedure interpretation.
<!-- AC:END -->

