---
id: TASK-4
title: Measure and redesign fast product refresh contract
state: medium
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - publication
  - performance
  - data
  - cat:data
dependencies: []
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Measure the network and update behavior of fast products discovered through the current contract, then decide whether the refresh path should stay as current-artifacts polling, use a watch-like contract, or support hanging invalidation requests.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Measure cost of refetching fast products through the current contract.
- [ ] #2 Measure frequency and volume of fast product updates.
- [ ] #3 Compute a target data volume for continuous refresh.
- [ ] #4 Propose whether current-artifacts, watch-like discovery, or hanging-get invalidations should be the durable contract.
<!-- AC:END -->

