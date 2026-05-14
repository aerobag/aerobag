---
id: TASK-17
title: Restore TFR altitudes and NOTAM text
state: medium
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - weather
  - safety
  - data
  - feature
  - cat:productionization
dependencies: []
ordinal: 17000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
TFRs lost altitude display. Add altitude ranges back and include NOTAM text, likely by ingesting the NOTAM feed and cross-referencing TFR areas to source notices.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 TFR feature data includes reliable lower/upper altitude text.
- [ ] #2 TFR inspection/detail UI exposes the associated NOTAM text.
- [ ] #3 Missing cross-reference data fails softly without removing TFR rendering.
<!-- AC:END -->

