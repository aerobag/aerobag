---
id: TASK-43
title: Add live SUA activation schedules
status: Next
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - data
  - safety
  - feature
  - cat:productionization
dependencies: []
references:
  - https://sua.faa.gov/datafeed/suagw/n24sua?user=
  - https://sua.faa.gov/ops/docs/suagwDataFmt.html
priority: medium
ordinal: 43000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add live Special Use Airspace activation schedules from the FAA SUA gateway, including the `n24sua` feed for non-MTR reservations scheduled in the next 24 hours. Access may require contacting FAA support rather than self-service signup.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Determine access requirements for the FAA SUA gateway.
- [ ] #2 Preproc/core ingest activation schedules when credentials/data are available.
- [ ] #3 Airspace inspection/status can show active or scheduled status.
<!-- AC:END -->

