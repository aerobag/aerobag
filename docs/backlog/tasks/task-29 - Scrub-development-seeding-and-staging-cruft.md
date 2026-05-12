---
id: TASK-29
title: Scrub development seeding and staging cruft
status: Next
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - cleanup
  - web
  - android
  - core
  - cat:core
dependencies: []
priority: medium
ordinal: 29000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Audit web and Android for early-development staging, default seeding, and debug shortcuts that were useful during bring-up but should not be durable app behavior now that real channels exist.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Find all startup seed/default/debug-only data paths on web and Android.
- [ ] #2 Remove or gate them behind explicit debug flags.
- [ ] #3 Production startup begins from intended real state, such as an empty flight plan unless core says otherwise.
<!-- AC:END -->

