---
id: TASK-64
title: Add airport city and friendly name search
status: Next
assignee: []
created_date: '2026-05-12 16:35'
labels:
  - feature
  - navigation
  - data
  - cat:navigation
dependencies: []
priority: low
ordinal: 64000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Support searching airports by city and friendly name, using either a full-text scan or a more efficient prefix index.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Define search behavior and ranking for ident, city, and friendly names.
- [ ] #2 Implement search in core with prepared data from preproc if needed.
- [ ] #3 Web and Android use the same core search results.
<!-- AC:END -->

