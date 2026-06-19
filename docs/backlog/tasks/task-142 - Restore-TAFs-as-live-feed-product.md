---
id: TASK-142
title: Restore TAFs as a first-class live-feed product
assignee: []
created_date: '2026-06-19 00:00'
labels:
  - cat:weather
dependencies: []
state: done
ordinal: 142000
---
## Description

<!-- SECTION:DESCRIPTION:BEGIN -->

Aerobag still fetches and parses TAFs inside the METAR live-feed builder, and
core still has TAF model/UI plumbing, but the live-feed publication currently
advertises only the `metars` product. TAFs should be restored as their own
live-feed product rather than hidden in METAR sidecars.

Plan:

- Add a `tafs` live-feed builder that fetches `tafs.cache.xml.gz`.
- Publish `states/tafs/{version}.json` with a keyed-record delta policy using
  `tafs_by_station` and `taf_count`.
- Register `tafs` in the shared core live-feed product registry.
- Install the `tafs` product into `session.taf_payload`; the weather inspector
  TAF button already works once the payload is present.
- Stop including TAF and PIREP model bytes in the METAR product fingerprint.
  METAR version churn should reflect only the published METAR state.
- Add producer, core install/delta, persistent-cache, and map-selection tests.

<!-- SECTION:DESCRIPTION:END -->
