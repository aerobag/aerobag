---
id: TASK-108
title: Move chart-page selection fallbacks to core
state: done
assignee: []
created_date: '2026-05-12 21:10'
labels:
  - core
  - web
  - android
  - refactor
  - cat:core
dependencies: []
references:
  - docs/refactor/core-platform-ui-boundary-audit.md
ordinal: 108000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Web and Android both duplicate chart-page airport/chart fallback helpers (`resolveAirportId` / `resolveChartId`) while core also has chart-page state derivation. Platform UI should pass candidate ids/recent ids to core and render the returned selection so folder/chart-supp/default-chart rules cannot drift.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Web removes local chart-page airport/chart fallback resolution.
- [x] #2 Android removes local chart-page airport/chart fallback resolution.
- [x] #3 Core tests cover folder, chart supplement, candidate id, recent airport, and empty-state selection behavior.
<!-- AC:END -->
