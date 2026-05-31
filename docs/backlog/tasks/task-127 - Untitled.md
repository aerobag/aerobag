---
id: TASK-127
title: Obstacle data
assignee: []
created_date: '2026-05-20 22:03'
labels:
  - cat:data
dependencies: []
state: high
ordinal: 127000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Obstacles are currently packaged as zip-vector-tiles, which is a bit of a poor fit for live-feeds.
Proposed change:
- Raw data gets transformed into a canonical HAD-tile database rows.
- HAD is modified to include range iteration. Reading the obstacle had in order is equivalent to printing its entire (K,V) table in canonical (key-sorted) order. That provides a way to precisely define the hash of a HAD-encoded data set.
- Given HADX and HADY, the delta can be computed as a set of (K,V) pairs to be replaced in HADX to produce HADY. We'll need a distinguished V that means "actually, delete K." (Add tests that cover deleting records, ensuring that they really get deleted and that hashes converge.)
- The live-feedsd will, for obstacles, have a converter that transforms upstream obstacle data into an obstacles-HAD, and will use the delta code above to compute obstacle-HAD-deltas. It will publish both the unpacked HAD pages (for incremental web consumption) and the complete zipped HAD and HAD-delta (for Android to grab when online).
- Core will use the SSE to learn which HAD root is current. Upon learning about a new HAD, discard its old HAD handle, and create a new one to answer future viewport obstacle queries. it should invalidate the viewport on this event to update the obstacle renderings.
- Based on the startup policy, on web core will open each HAD handle as a dynamically-fault-pages HAD (just like the main nav-db), but pointing to the unpacked dir specified by the recent SSE version info. On Android, core will fetch the whole-package delta locally, reconstitute the new HADY value, confirm the hash, then open that HAD file locally (so no network async is required).
<!-- SECTION:DESCRIPTION:END -->
