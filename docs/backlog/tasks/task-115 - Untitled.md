---
id: TASK-115
title: live-feeds products blocked by cycle production lock
status: Next
assignee: []
created_date: '2026-05-13 22:11'
labels:
  - cat:weather
dependencies: []
priority: medium
ordinal: 115000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
• Yes, bad news: build_product() currently takes the publication lock near the start and holds it for the
  whole product build.

  Specifically, aerobag/product/preprocessor/preprocessor-cli/src/product_build.rs:1239 creates logs, then
  immediately does:

  let _publication_lock = acquire_publication_lock(...)

  That guard stays in scope until build_product() returns, after the whole scheduler run and final status
  write. So if build-product spends 2 hours building TPPs, update-live-feeds will block for that whole time.

  update_live_feeds() also takes the same publication lock immediately at aerobag/product/preprocessor/
  preprocessor-cli/src/product_build.rs:2977, before it even fetches/builds METARs.

  So the current locking model is wrong for live feeds.

  The fix should be: split this into narrower locks.

  - build_product should not hold the publication lock while doing private/cache build work.
  - update_live_feeds should not hold the publication lock while fetching/building METARs.
  - Each path should acquire a lock only around the final mutation of public roots/current pointers.
  - Better: use separate locks for cycle/package publication and live-feed publication, since they write
    different public contract roots. That way a cycle publish window does not block METAR updates unless they
    truly touch the same file.
<!-- SECTION:DESCRIPTION:END -->
