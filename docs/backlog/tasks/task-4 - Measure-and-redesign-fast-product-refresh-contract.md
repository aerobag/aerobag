---
id: TASK-4
title: live-feeds transition
state: high
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
- Some stale docs/backlog text still mentions fast-products and old commands.
  - Some CLI-hosted integration tests still live under preprocessor-cli; helpers
    are moved, tests are not fully relocated.
  - Real future streaming adapters like SWIM/NOTAMs are not implemented yet.
  - Production serving details remain: supervisor, reverse proxy, SSE scaling,
    health/status reporting.
  - Android live-feed consumption still trails web.
  - NEXRAD PNG delta encoding remains deferred under TASK-121.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Measure cost of refetching fast products through the current contract.
- [ ] #2 Measure frequency and volume of fast product updates.
- [ ] #3 Compute a target data volume for continuous refresh.
- [ ] #4 Propose whether current-artifacts, watch-like discovery, or hanging-get invalidations should be the durable contract.
<!-- AC:END -->

