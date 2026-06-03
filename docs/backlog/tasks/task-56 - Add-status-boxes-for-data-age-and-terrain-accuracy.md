---
id: TASK-56
title: Product age warnings
state: high
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - safety
  - weather
  - terrain
  - feature
  - cat:productionization
dependencies: []
ordinal: 56000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Let's talk about how to surface various kinds of data quality problems. Here are some examples:
- no high-res chart data here (because we don't have the package and can't fetch the tile(s))
- a selected chart is expired
- a METAR sample is old
- METAR feed is old
- NEXRAD is old
- nav-db / vector data is expired
- terrain data is running too far behind ownship
- obstacle data is old
- a procedure / procedure leg has a data-quality issue attached to it
- (later) NOTAM feed is old
- TFR feed is old
- (later) ADSB data is old

Here are some possible presentations:
- a big yellow ⚠ pops up on the chart page (in a distinguished place, laid out to the left of the ownship source pill). Clicking it brings up a tray of the currently active warnings. Warnings have a "hush" button that keeps them from triggering the ⚠, but if the list is brought back (because another warning is available or from the Home page), the hushed warnings are still visible.
- perhaps some data should be explicitly removed from the chart (METARs, NEXRAD) if it's old enough to be misleading.
- perhaps warned layers in the layers menu get a disabled presentation, a ⚠ in place of the toggle, maybe clicking the button pops up a modal explaining the reason for disabling.
- Something like a single old NOTAM that belongs to a timely feed: the NOTAM display in the inspector pane detail should include a decoded "27 min ago"; when it's more than an hour old, that becomes bold and red.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Core exposes data age and accuracy status fields.
- [ ] #2 UI renders compact status boxes consistently across platforms.
- [ ] #3 Stale or degraded status can feed the caution system.
- [ ] #4 Product age warnings cover live-feed products such as NEXRAD, METAR, NOTAM, TFR, and ADS-B where applicable.
<!-- AC:END -->
