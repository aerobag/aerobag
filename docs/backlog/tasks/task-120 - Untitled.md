---
id: TASK-120
title: regularize component escaping
assignee: []
created_date: '2026-05-16 19:13'
labels:
  - cat:data
dependencies: []
state: medium
ordinal: 120000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Even better: move HAD key-component escaping into one shared crate used by
  both preproc and core, then keep the contract test as a guard against
  publication/query drift. The same pattern should apply to other keyspaces
  where values carry their own IDs, e.g. plate/by-id, plate/cifp, package/by-id,
  maybe procedure geometry keys.
<!-- SECTION:DESCRIPTION:END -->
