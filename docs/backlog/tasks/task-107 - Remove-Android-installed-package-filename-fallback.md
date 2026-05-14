---
id: TASK-107
title: Remove Android installed-package filename fallback
state: done
assignee: []
created_date: '2026-05-12 21:10'
labels:
  - android
  - core
  - refactor
  - cat:android
dependencies: []
references:
  - docs/refactor/core-platform-ui-boundary-audit.md
ordinal: 107000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Android `InstalledPackages.kt` still accepts ZIP files with missing metadata by deriving the artifact id from the filename. Package identity is manifest/metadata-owned; permanent filename guessing can resurrect stale or malformed package behavior. Ignore or quarantine metadata-free ZIPs instead of silently treating them as valid installed artifacts.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Metadata-free ZIPs are not reported as installed packages to core planning.
- [x] #2 Metadata-free local files are ignored rather than guessed into cleanup candidates with invented artifact ids.
- [x] #3 Existing Android package tests pass with filename fallback removed; future metadata recovery should be an explicit task, not permanent compatibility logic.
<!-- AC:END -->
