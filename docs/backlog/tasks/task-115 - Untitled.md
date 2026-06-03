---
id: TASK-115
title: live-feeds products blocked by cycle production lock
state: done
assignee: []
created_date: '2026-05-13 22:11'
labels:
  - cat:weather
dependencies: []
ordinal: 115000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Historical context before the live-feed daemon migration:

• Yes, bad news: build_product() took the publication lock near the start and held it for the
  whole product build.

  Specifically, aerobag/product/preprocessor/preprocessor-cli/src/product_build.rs:1239 creates logs, then
  immediately does:

  let _publication_lock = acquire_publication_lock(...)

  That guard stayed in scope until build_product() returned, after the whole scheduler run and final status
  write. So if build-product spent 2 hours building TPPs, the old live-feed CLI command blocked for that whole time.

  update_live_feeds() also took the same publication lock immediately at aerobag/product/preprocessor/
  preprocessor-cli/src/product_build.rs:2977, before it even fetches/builds METARs.

  So that locking model was wrong for live feeds.

  The fix was: split this into narrower locks and then remove the old CLI path.

  - build_product should not hold the publication lock while doing private/cache build work.
  - live-feed publication should not hold the cycle publication lock while fetching/building METARs.
  - Each path should acquire a lock only around the final mutation of public roots/current pointers.
  - Better: use separate locks for cycle/package publication and live-feed publication, since they write
    different public contract roots. That way a cycle publish window does not block METAR updates unless they
    truly touch the same file.
<!-- SECTION:DESCRIPTION:END -->

## Resolution

<!-- SECTION:RESOLUTION:BEGIN -->
The old live-feed CLI path was removed. Live feeds now
publish under their own daemon/library path instead of sharing the cycle package
publication lock, so cycle publication work no longer gates live-feed updates.
<!-- SECTION:RESOLUTION:END -->
