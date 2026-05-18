---
id: TASK-36
title: Rename Android Java namespace
state: done
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - android
  - cleanup
  - source
  - cat:productionization
dependencies: []
ordinal: 36000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Rename the Android Java/Kotlin namespace to the intended package, likely `org.aerobag.app`.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Decide the final Android application namespace.
- [ ] #2 Rename package declarations, Gradle namespace/applicationId, and tests consistently.
- [ ] #3 Android build and tests pass after the rename.
<!-- AC:END -->
