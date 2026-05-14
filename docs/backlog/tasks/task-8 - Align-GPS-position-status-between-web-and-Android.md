---
id: TASK-8
title: Align GPS position status between web and Android
state: medium
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - parity
  - android
  - web
  - bug
  - cat:android
dependencies: []
ordinal: 8000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Android reports `LIVE POSITION` while web reports `NO GPS POSITION` in a comparable state. Audit whether the model/view/controller split diverged and ensure core-owned position-source state drives both UIs.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Reproduce the same startup/runtime state on web and Android.
- [ ] #2 Identify why the status labels diverge.
- [ ] #3 Both platforms render the same core-provided position-source status.
<!-- AC:END -->

