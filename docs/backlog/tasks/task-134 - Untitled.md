---
id: TASK-134
title: Deploy
assignee: []
created_date: '2026-06-03 21:07'
labels:
  - cat:productionization
dependencies: []
state: done
ordinal: 134000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Let's change the division of labor:

jonh iac creates the CT that hosts the deployment, but doesn't push the binary/launch it

aerobag dev has scripts (in the aerobag repo) and credentials (NOT checked in -- just .ssh keys) that allow it to push a new version up to the CT

If I have to recreate the CT, I'll need to ask aerobag-dev to push the binary.
Actually, maybe what the iac should do is push enough of the repo to install itself from master!
(ever from a branch, actually? 'prod'?)
Then, when we change prod, we can ssh into aerobag-dev to have it fetch git (read-only perms) & restart the various pieces.

Pieces are:
- periodic cycle product build (cron)
   - cycle product status (emitted alongside build)
   - cycle product GC (ensure we're not leaking)
   - define set of active contract branches
- live-feedsd
   - upstream feed & publication pipeline
   - SSE server
   - define multiple SSE servers for active contract branches (someday)
   - GC story
- publication path to CDN
- archives
   - on a big cheap spinny disk
   - of cycle products
   - of live-feed products
<!-- SECTION:DESCRIPTION:END -->
