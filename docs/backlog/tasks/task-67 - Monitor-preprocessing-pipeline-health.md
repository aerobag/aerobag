---
id: TASK-67
title: Monitor preprocessing pipeline health
state: high
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - preprocessor
  - deployment
  - data
  - cat:preprocessor
dependencies: []
ordinal: 67000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add health monitoring for the preprocessing pipeline: whether charts and fast products arrive on time, latency, gaps, and enough data to tune poll periods.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Monitor chart arrival timeliness.
- [ ] #2 Monitor fast product latency and gaps.
- [ ] #3 Produce metrics usable for tuning polling/update cadence.
<!-- AC:END -->

