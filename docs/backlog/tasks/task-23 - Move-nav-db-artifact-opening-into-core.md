---
id: TASK-23
title: Move nav-db artifact opening into core
state: done
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - refactor
  - core
  - android
  - web
  - cat:core
dependencies: []
ordinal: 23000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Use a cleaner architecture where Android and web provide generic byte readers or zip-entry adapters, while core owns the higher-level workflow of opening a nav-db artifact and deciding whether it is readable.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Platform code exposes generic artifact bytes or zip member access without nav-db-specific interpretation.
- [x] #2 Core opens and validates nav-db artifacts.
- [x] #3 Artifact unreadability is reported by core through a recoverable boundary error.
<!-- AC:END -->
