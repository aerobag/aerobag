---
id: TASK-87
title: Delete first/last of airway
state: done
assignee: []
created_date: '2026-05-12 20:10'
labels:
  - navigation
  - core
  - cat:core
dependencies: []
ordinal: 87000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Delete first/last of airway

No, delete the waypoint and have the airway represent it. Garmin

Superseded by the explicit endpoint model: airways now reference pinned top-level
waypoint occurrences. Remove Airway retains those endpoints; children no longer offer
endpoint trimming. See [Airway Endpoint Ownership](../../refactor/airway-endpoint-ownership.md).
<!-- SECTION:DESCRIPTION:END -->
