---
id: TASK-116
title: Get legit magnetic variance data source
state: done
assignee: []
created_date: '2026-05-13 22:27'
labels:
  - cat:data
dependencies: []
ordinal: 116000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
We used to derive terrain geoid offsets from a CSV copied from Avare, of
unknown provenance. The product pipeline now fetches NGA EGM2008 geoid data
through the source cache and carries source/effective-date metadata into terrain
artifacts.
<!-- SECTION:DESCRIPTION:END -->
