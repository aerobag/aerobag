---
id: TASK-49
title: Add NOTAM support
state: medium
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - feature
  - safety
  - data
  - cat:productionization
dependencies: []
ordinal: 49000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Add NOTAM ingestion, display, inspection, and freshness warning support. The upstream side has been proofed out and should be carried through the app contract.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Preproc/core ingest NOTAM data relevant to chart and airport inspection.
- [ ] #2 UI can show relevant NOTAMs without platform-side filtering logic.
- [ ] #3 Stale NOTAM data contributes to the caution/status system.
<!-- AC:END -->
