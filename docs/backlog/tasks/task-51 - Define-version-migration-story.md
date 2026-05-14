---
id: TASK-51
title: Define product contract versioning and migration story
state: high
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - core
  - data
  - deployment
  - publication
  - android
  - mvp
  - cat:productionization
dependencies: []
ordinal: 51000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Define schema/product-contract migration behavior, including making Rust parsing robust to unused fields where intended and supporting overlapping published contract versions long enough for users to update Android without being stranded by a server-side cutover.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Document compatibility expectations for additive and breaking schema changes.
- [ ] #2 Rust parsers tolerate unused fields where that is the intended contract.
- [ ] #3 Breaking changes produce clear contract-version failures.
- [ ] #4 Define the supported overlap window for product contract versions.
- [ ] #5 Core/platform startup rejects unsupported contracts with a recoverable, user-visible error.
- [ ] #6 Android can continue using a still-supported previous product while the current product has advanced.
<!-- AC:END -->
