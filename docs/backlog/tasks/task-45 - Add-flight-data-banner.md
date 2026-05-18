---
id: TASK-45
title: Add flight data banner
state: high
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - feature
  - navigation
  - cat:navigation
dependencies: []
ordinal: 45000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add a data banner for GPS/baro altitude, track, DTK, ETE, and related flight data.

In other tools, this is a grid (grid lines, labels, and values drawn as white strokes with black contrast strokes). Wer'e going to want to be able to move the grid around and change its aspect ratio to deal with responsive layout. Users may want to make it bigger or smaller -- one user might want three important fields and save the rest of the space for the chart to show through; another user might want 6 or 8 things, which might be best in a 6x1 column on a side edge, or a 2x3 or 1x6 row at the top, depending on the screen layout.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core exposes banner fields with validity/age state.
- [ ] #2 Web and Android render the same selected fields.
- [ ] #3 Missing or stale inputs are shown explicitly.
<!-- AC:END -->

