---
id: TASK-60
title: CUTLINES! Render Honolulu TAC inset
state: done
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - bug
  - plates
  - data
  - cat:navigation
dependencies: []
ordinal: 60000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Honolulu TAC inset is not currently rendered.
Probably needs a cutline.


  - right now, TACs have their legends/margins flopping out, which overhangs adjacent sectionals (since
  our TAC layer is TAC-over-sectional).
  - I'd like to actually trim out one (or all, if they're different?) of the legends for every chart type
  and make them available to the user! Legends are legendary.
  - Many charts have insets I'd like to be able to access like a legend, such as the half-dozen subcharts
  for how to cross LAX.
  - we're missing a honolulu inset; I'd like to track that down.
  - where's the grand canyon chart?
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Locate the Honolulu inset in the chart source/product.
- [ ] #2 Preproc/core exposes it through the normal chart tile path.
- [ ] #3 Web and Android render it when the relevant TAC is selected.
<!-- AC:END -->

