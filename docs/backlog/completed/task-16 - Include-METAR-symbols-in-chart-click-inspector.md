---
id: TASK-16
title: Include METAR symbols in chart click inspector
status: Done
assignee: []
created_date: '2026-05-12 16:20'
updated_date: '2026-05-12 17:54'
labels:
  - weather
  - core
  - feature
dependencies: []
priority: medium
ordinal: 16000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Chart click testing should include METAR stations from z7 weather tiles. When a METAR matches, paint its symbol in an inspector cell in the same row as SPOT, sorted after SPOT.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core includes nearby METAR stations in chart inspection results.
- [ ] #2 METAR cells render the same weather symbol used on the map.
- [ ] #3 METAR items appear in the SPOT row after the SPOT item.
<!-- AC:END -->
