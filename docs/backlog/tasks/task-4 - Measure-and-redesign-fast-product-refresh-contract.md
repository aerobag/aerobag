---
id: TASK-4
title: live-feed transition cleanup
state: high
assignee: []
created_date: '2026-05-12 16:20'
labels:
  - publication
  - performance
  - data
  - cat:data
dependencies: []
ordinal: 4000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
- Audit active docs/backlog text for retired rolling-product terminology and old
  operational commands.
- Real future streaming adapters like SWIM/NOTAMs are not implemented yet.
- Production serving details remain: supervisor, reverse proxy, and SSE scaling.
- Android now consumes live-feed packages through its cache/SSE path. Remaining
  Android gaps are product UI-specific, currently winds-aloft display and debug
  tile labels.
- NEXRAD PNG delta encoding remains deferred under TASK-121.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Active docs describe rolling data as live-feed products, not static package rows.
- [x] #2 Backlog tasks use live-feed terminology except when explicitly recording historical context.
- [x] #3 Old operational commands are marked historical or removed from current instructions.
- [ ] #4 Remaining live-feed production gaps are tracked as concrete follow-up tasks.
<!-- AC:END -->
