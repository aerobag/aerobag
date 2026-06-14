---
id: TASK-127
title: Obstacle data
assignee: []
created_date: '2026-05-20 22:03'
labels:
  - cat:data
dependencies: []
state: done
ordinal: 127000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Obstacles are live-feed HAD data, not legacy zip-vector-tiles.

Implementation shape:
- Raw data is transformed into canonical HAD-tile database rows.
- Reading the obstacle HAD in key order defines the hash of the encoded data set.
- Given HADX and HADY, the delta is represented as key/value replacements plus a distinguished delete value; tests cover delete convergence.
- The live-feeds daemon transforms upstream obstacle data into an obstacle HAD and publishes unpacked HAD pages for incremental web consumption plus a complete zipped HAD package for Android installation.
- Core uses live-feed current/version state to learn which HAD root is current. When the obstacle current version changes, it discards the old HAD handle and cached obstacle tiles before answering future viewport obstacle queries.
- Web opens each obstacle HAD as dynamically-faulted pages. Android installs the whole HAD package or delta locally, verifies the hash, and opens the local HAD without dynamic network page faults.
<!-- SECTION:DESCRIPTION:END -->
