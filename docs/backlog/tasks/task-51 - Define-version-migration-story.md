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


I propose we give versions names that encode the product, so comparisons can't confuse across product lines. NAV7, TPP6. Use short prefixes for each product family, 3-4 letters. The numbers at the end we'll increment as we break contracts, but we don't need to encode comparison since the only meaningful test is exact comparison.

Okay, start the project:
- Add version-identity into package names
- Be sure every package's manifest "supports" a "ui-warning" field. "Support" might just mean "teach core to look for that field in every package manifest and surface it through the warning widget." (We have a generalized warning widget now.)
- Agree on your conclusion about publisher-level version aggregation. Let's defer that part of the work until we have a way to generate versions at all. Then we'll roll the version numbers for no good reason and make the publisher code do the right thing.
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
