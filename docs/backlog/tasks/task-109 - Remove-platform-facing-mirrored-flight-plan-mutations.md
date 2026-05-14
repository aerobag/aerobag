---
id: TASK-109
title: Remove platform-facing mirrored flight-plan mutations
state: done
assignee: []
created_date: '2026-05-12 21:10'
labels:
  - core
  - android
  - refactor
  - cat:core
dependencies: []
references:
  - docs/refactor/core-platform-ui-boundary-audit.md
ordinal: 109000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Android still exposes bridge calls such as `removeFlightPlanLegJson`, `replaceFlightPlanStateJson`, and materialized airway/procedure mutation APIs that accept mirrored `FlightPlan` payloads from platform UI. Keep internal core helpers if needed, but remove platform-facing mutation paths in favor of session row actions, opaque action ids, and paged session operations.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Platform UI no longer passes whole flight-plan models back to core for normal mutation flows.
- [x] #2 Android bridge bindings for obsolete mirrored-plan mutations are removed or made test-only/internal.
- [x] #3 Boundary tests reject new platform-facing mutation exports that accept full plan state when a session/action API exists.
<!-- AC:END -->
