---
id: TASK-106
title: Remove web startup vector manifest synthesis
state: done
assignee: []
created_date: '2026-05-12 21:10'
labels:
  - web
  - core
  - refactor
  - cat:core
dependencies: []
references:
  - docs/refactor/core-platform-ui-boundary-audit.md
ordinal: 106000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
`ui/web-app/src/domain/appCoreAdapter.ts` still builds a synthetic vector manifest at startup and patches in optional obstacle/METAR live-feed metadata. The vector manifest contract now belongs to core/HAD, and live feeds need explicit core-owned contracts rather than web-side manifest mutation.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Web no longer synthesizes a base vector manifest before session creation.
- [x] #2 Core/HAD owns vector manifest loading for nav-db-backed layers.
- [x] #3 Live-feed layers use explicit core-owned discovery/resource contracts instead of web-only manifest patching.
<!-- AC:END -->
