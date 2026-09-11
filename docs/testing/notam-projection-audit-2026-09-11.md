# NOTAM projection expectation audit — September 11, 2026

## Finding

Heavy Fixture CI was consistently red, not flaky. The first red run on September 5
and the September 10/11 runs all produced 118 transitions versus an expectation
of 96, with the same final-state hash. NEXRAD passed independently.

The relevant behavior change is `f944d8886eb01df76f94afc608cfebbaebc4e061`
("Fix NOTAM association and safety ordering"). It replaced keyword-based airport
admission with structured facility evidence, added airport safety effects, and
excluded named non-airport facilities. The raw capture did not change, but its
derived publication expectations were not updated.

## Controlled comparison

Both versions used the exact `notams/nms-api-trace` input at artifact commit
`93b3a0e975fb8a9f9d36fd0b810d2905a6d3cbc2`, with captured poll completion times:

- Before: `64976c3d8b3991b59615521a29bf17e17c0e7e1f` (parent of the change).
- After: `4162491b231e368a1a5d4b9d9d2c20fea82bf0d9`.
- Both replayed 32,630 baseline source records, 498 polls, and 580 updates through
  the real collector and both full-reconciliation and incremental publication stores.
- Temporary diagnostic instrumentation exported the initial checkpoint, each
  timestamped mutation, final checkpoint, and normalized baseline source. It did
  not change assertions or processing and is not part of the implementation.
- The before version passed the existing expectations and all 114 recovery paths.
  The after version reproduced the hosted assertion with identical counts/hash.

| Measurement | Before | After |
| --- | ---: | ---: |
| Client baseline records | 11,521 | 28,962 |
| Publication transitions | 96 | 118 |
| Mutations | 820 | 1,407 |
| Removals | 695 | 1,178 |
| Repeated mutation IDs | 69 | 89 |

Before final-state hash:
`f75cfd5f1b93151208feb6f4f31adb50fd3936fd8d7502332f8de2e3e7eff884`.

After final-state hash:
`88b960aa5aec2728b561f42df3cef93c004005ced16d3e148c51d8eebaba1862`.

All 96 old transition timestamps remain. Matching mutations by timestamp,
operation, and record ID preserves all 820 old mutations; no common upsert changes
text, effective times, identity, or procedure keys. Changes to common upserts are
limited to airport association and airport effects. The 587 additional mutations
are AIRSPACE 239, COM 57, NAV 79, OBST 157, SVC 51, and unclassified 4.

The baseline adds 17,442 previously undisplayable records. Of 246 changed common
records, all change effects and 57 also change airport identity. One formerly
published APRON record, `NMS:1776757345252425`, is excluded because its structured
facility name is `TOR-NDB`, matching the explicit non-airport-facility policy.
Its raw notice remains in source state; this audit checks the documented
projection policy, not the real-world correctness of FAA facility naming.

The full replay intentionally tests the base projection without a NAV airport
catalog. Runtime catalog filtering has separate unit coverage; these figures
must not be presented as production airport-admitted record counts.

## Every additional transition

The 22 extra boundaries contain 30 mutations: two upserts, one source
cancellation, and 27 expirations grouped into 19 expiry-only boundaries.
Their notice categories are AIRSPACE (22 mutations), NAV (3), OBST (4), and
SVC (1), all with structured airport identities.

The ABQ cancellation is explicit in the raw poll at 17:47:48:
`fnse:canceled=2026-07-24T17:45:00.000Z`, payload SHA-256
`42046e02be53f81c3ad703f5a90d5e72e84f70da3fbf829e04782b3494649f11`.
It removes the ABQ notice before its scheduled expiry. Every other extra removal
occurs at the first captured poll after the record's effective end.

| Captured completion time (UTC) | Added mutations |
| --- | --- |
| 2026-07-24 17:35:48 | upsert AIRSPACE KABQ (NMS:7848917985080673) |
| 2026-07-24 17:38:50 | upsert AIRSPACE KAEJ (NMS:9625960796674008) |
| 2026-07-24 17:47:48 | remove AIRSPACE KABQ (NMS:7848917985080673) |
| 2026-07-24 18:05:50 | remove AIRSPACE 07N (NMS:1784905453206595) |
| 2026-07-24 19:05:50 | remove AIRSPACE KIAH (NMS:6916556843845736) |
| 2026-07-24 20:11:48 | remove NAV KMIA (NMS:1784845958314205); remove NAV KMIA (NMS:1784845996791025) |
| 2026-07-24 20:56:48 | remove OBST KVTA (NMS:1784753767038548) |
| 2026-07-24 21:17:50 | remove SVC KSME (NMS:1784910548902421) |
| 2026-07-24 21:50:50 | remove AIRSPACE KLRD (NMS:9031893929280518) |
| 2026-07-25 00:14:50 | remove AIRSPACE KLZD (NMS:1777412173936308) |
| 2026-07-25 01:32:50 | remove OBST PHTO (NMS:1784838449821175); remove AIRSPACE KPAE (NMS:3214405404451529) |
| 2026-07-25 01:56:50 | remove AIRSPACE KOGD (NMS:1784577586226221) |
| 2026-07-25 02:05:48 | remove AIRSPACE KOWB (NMS:1784902128223730) |
| 2026-07-25 02:17:48 | remove AIRSPACE KHPN (NMS:1784842905363987) |
| 2026-07-25 02:32:48 | remove AIRSPACE KSGR (NMS:1784637475428309); remove OBST KGYR (NMS:1784910508747142) |
| 2026-07-25 03:20:50 | remove AIRSPACE KCVH (NMS:2552969221477873) |
| 2026-07-25 04:59:48 | remove AIRSPACE KMOX (NMS:1955514441074239); remove AIRSPACE KBDH (NMS:3353712912717825); remove AIRSPACE KBDH (NMS:6421907804675984) |
| 2026-07-25 05:50:50 | remove AIRSPACE KIDG (NMS:1435201552102176); remove AIRSPACE KIDG (NMS:8167352730567005); remove AIRSPACE KIDG (NMS:8406535844545688) |
| 2026-07-25 06:56:50 | remove AIRSPACE W36 (NMS:1447454262207147); remove AIRSPACE KRNO (NMS:2525676779733026) |
| 2026-07-25 06:59:50 | remove AIRSPACE KVGT (NMS:1784788568214708) |
| 2026-07-25 07:02:50 | remove NAV PASC (NMS:1784860126415457) |
| 2026-07-25 08:26:48 | remove OBST KSTC (NMS:1781079968380288) |

## Fix and prevention

Update only the derived `expected.json` in a new immutable artifact-repository
commit and pin that commit in `test-artifacts.lock.json`. Preserve all raw gzip
files, capture hashes, capture provenance, and the version-2 capture format.
No application behavior, assertion threshold, or timeout needs changing.

The expectation-only artifact revision is
`82543408e591054279bfbcb15546f4587d9f73ce`. The complete updated replay passed
locally with the hosted nextest command in 322.233 seconds, including all
checkpoint/catch-up schedules. Four new small regressions passed together in
under a second, and all 13 cheap-preflight suites passed in 76.3 seconds before
the final lock-pin update.

`product/preprocessor/nms-notams-fetch/src/projection_test.rs` adds four
fixture-free tests for AIRSPACE, NAV, OBST, and SVC. Each runs tiny generated AIXM
through the real collector, both publication paths, and the client state reader.
They assert admission, timestamp-controlled expiry, duplicate suppression,
explicit cancellation, server-only ARTCC retention, source-cursor advancement,
and exact convergence for individual versus collapsed deltas. An obstruction
version of the regression fails against the pre-change code because only the RWY
notice reaches the client.

These tests join ordinary preprocessor CI and the existing cheap preflight.
They detect semantic regressions cheaply; they do not prove the large pinned
golden is current. Changes to admission, identity, effects, or expiry still need
a full fixture replay and a reviewed expectation update in the same change.
Never replace a failed count or hash merely because the observed value is stable.

## Evidence

- [First red run](https://github.com/aerobag/aerobag/actions/runs/33942542905)
- [September 10 run](https://github.com/aerobag/aerobag/actions/runs/34514799117)
- [September 11 run](https://github.com/aerobag/aerobag/actions/runs/34619630704)

Local comparison traces were retained under
`/tmp/aerobag-notam-audit-BzsCAV/`; this document records the durable findings.
