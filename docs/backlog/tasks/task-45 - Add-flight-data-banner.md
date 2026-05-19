---
id: TASK-45
title: Add flight data banner
state: done
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
Add a data banner for various flight data fields
- GPS or baro altitude
- vertical speed
- ground track (magnetic)
- desired track (magnetic) from active leg
- distance to waypoint
- time to waypoint
- distance to final
- time to final (ETE)
- ETA at final

In other tools, this is a grid (grid lines, labels, and values drawn as white strokes with black contrast strokes). Wer'e going to want to be able to move the grid around and change its aspect ratio to deal with responsive layout. Users may want to make it bigger or smaller -- one user might want three important fields and save the rest of the space for the chart to show through; another user might want 6 or 8 things, which might be best in a 6x1 column on a side edge, or a 2x3 or 1x6 row at the top, depending on the screen layout.

The set & order of the displayed fields will (eventually) be user-configurable.
The layout of the fields will be some sort of responsive magic that makes the display look reasonable when a tablet is rotated to a different orientation or a browser is resized. "reasonable" means "not covering any controls/widgets like CDI, ownship pill" and "prefer to cover chart farther from the center (leaving a more-square visible chart area"
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core exposes banner fields with validity/age state.
- [ ] #2 Web and Android render the same selected fields.
- [ ] #3 Missing or stale inputs are shown explicitly.
<!-- AC:END -->

