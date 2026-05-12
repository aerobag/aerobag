---
id: TASK-1
title: Android plate LOAD APPCH tray exposes no selectable procedure options
status: MVP
assignee: []
created_date: '2026-05-12 15:58'
labels:
  - android
  - plates
  - parity
  - bug
  - mvp
  - cat:android
dependencies: []
references:
  - tools/parity/run-flight-plan-inspect-journey.mjs
priority: high
ordinal: 1000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The parity journey can select KPAE on the plate page and observes LOAD APPCH becoming enabled, but opening the Android LOAD APPCH tray exposes no selectable parity:tray-option nodes. Web executes the same KPAE plate-load path successfully.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Android exposes selectable procedure load options when LOAD APPCH is enabled.
- [ ] #2 The web-vs-Android parity journey can load a KPAE approach on both platforms.
<!-- AC:END -->
