---
id: TASK-105
title: Move NEXRAD into core resource flow
status: Done
assignee: []
created_date: '2026-05-12 21:10'
labels:
  - weather
  - core
  - refactor
  - cat:core
dependencies: []
references:
  - docs/refactor/core-platform-ui-boundary-audit.md
priority: high
ordinal: 105000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
NEXRAD is still platform-owned on both web and Android: each UI fetches the manifest, resolves frame URLs, owns availability/error policy, and advances frame playback. Move NEXRAD discovery and frame planning into a core-planned resource flow so platforms only supply bytes for opaque resource requests and render the returned frame model.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Core owns NEXRAD manifest parsing, availability status, and frame list; platform code only advances the displayed frame as paint scheduling.
- [x] #2 Web and Android satisfy opaque core resource requests instead of fetching `/fast-products` or package members directly for NEXRAD.
- [x] #3 Both platforms render the same core-provided NEXRAD frame state.
<!-- AC:END -->
