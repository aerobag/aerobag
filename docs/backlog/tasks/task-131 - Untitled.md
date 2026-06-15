---
id: TASK-131
title: Live feed contract story
assignee: []
created_date: '2026-06-03 05:05'
labels:
  - cat:data
dependencies: []
state: medium
ordinal: 131000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
cycle data has a multi-contract publication story that allows multiple contracts to coexist, so old clients can hang around a while until their users upgrade them (or we sunset the old contract eventually).

We need some analogous story for live-feeds.
I guess that means having parallel publishers, which is a bummer since the live-feeds daemon owns data from the upstream poll all the way to publication. Ideally, we'd have the two publishers share the incoming upstream data but be separate binaries (built from separate commits). Of course, that makes things difficult if we want to actually change the upstream fetch code. Hrmm.

Another bummer is that, in cycles, two diverging navdb contracts can share charts & tpps (whose contracts change MUCH less often). I'm not sure how to do that in live feeds.

We'd also need an ultimate "merge" story, analogous to the python merger for cycles. For live feeds, this probably happens at SSE: as the feedsd produces updates and publishes them to the SSE server, it only updates its contract; the feedsd server keeps publishing the latest values for the "other" contract until the other publisher emits new data.

Maybe I should do something much simpler until this is an actual problem: run two end-to-end live feed publish + SSE servers at different endpoints. Newer clients just learn the appropriate endpoint. Rather than one endpoint that all clients can use to get a "directory of contracts".
<!-- SECTION:DESCRIPTION:END -->
