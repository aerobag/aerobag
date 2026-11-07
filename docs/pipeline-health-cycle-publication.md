# Cycle Publication Health

Pipeline Health uses the independent FAA cycle calendar and the publication
actually assigned to production. Staging or sunset products cannot satisfy
production's cycle deadline.

## Alert Policy

- Upcoming cycle absent: WARNING at exactly 17 days before its effective date;
  CRITICAL at exactly 14 days. Earlier absence is not an alert. These are elapsed
  time thresholds from UTC midnight on the calendar's effective date.
- Current cycle absent or unusable: CRITICAL immediately. Publishing the next
  cycle cannot cover a gap in the currently effective cycle.
- Failed cycle build with the current production cycle still available: WARNING
  immediately. The upcoming-cycle countdown independently escalates at 14 days.
- Failed build without verified current production coverage: CRITICAL. Missing
  calendar or publication evidence cannot be assumed healthy.

Publication coverage requires NAVDB product facts, without reported product
errors for that cycle or input read errors. Chart-only records do not establish
usable cycle coverage. Product/telemetry validation alarms remain independent.
The build result stays `fail`; this policy does not erase unsuccessful attempts.

Exported alert messages begin with their individual WARNING/CRITICAL severity so
a downstream joined summary cannot make a warning look critical merely because
another alert is critical. Raw metric messages remain unchanged for dashboard
tables and disposition evidence. No downstream alarm-policy change is needed.

## Calibration

The FAA advertises next-edition d-TPP availability 20 days before effectiveness:
<https://www.faa.gov/air_traffic/flight_info/aeronav/digital_products/dtpp/>.
Aerobag's builder also begins considering upcoming cycles at 20 days. Starting
the absence warning then leaves no allowance for downloading or processing.

First successful cycle-manifest entries in retained production journals,
examined on November 6, 2026:

```text
Cycle  Effective   First successful build (UTC)  Days ahead
2608   2026-08-06   2026-07-19 00:56                  17.96
2609   2026-09-03   2026-08-16 03:22                  17.86
2610   2026-10-01   2026-09-11 19:46                  19.18
2611   2026-10-29   2026-10-10 07:59                  18.67
```

These are Aerobag completion times, not exact FAA availability dates. July and
August include local build failures; four samples do not establish a reliable
tail distribution. The operator selected 17/14 days to give normal publication
some slack and leave two weeks for urgent repairs.

## November 2026 ZIP Failure

Cycle 2612's `DDTPPE_261126.zip` used DOS backslash separators in 55 of 56
member names. All members passed integrity checks and extracted, but Info-ZIP
returned warning status 1. The fetcher treated that warning as a fatal failure.

The fetcher now uses the existing Rust ZIP dependency, normalizes separators,
and reads every member through its CRC-checking reader. Unsafe paths, symlinks,
normalized-name collisions, and corrupt data remain failures. A file is installed
only after its read succeeds; replacing an existing file does not mutate other
hardlinks to cached data. Provenance records the installed, normalized names.

A generated DOS-origin ZIP reproduces the old failure in the fixture-free test
suite. The downloaded production ZIP was also extracted with the new code and
all 56 members compared byte-for-byte on November 6. The FAA archive itself is
not a checked-in test dependency.

## Deployment

Monitoring policy is controller runtime code: after integration, the operator
can install it with `tools/prod_manage.py --reconcile` without an app rebuild.
The ZIP fix is producer code. Cycle builds use each release's pinned revision,
so stage and promote a release containing this fix to use it for production
cycle refreshes. Reconciliation alone does not patch old producers, and old
sunset releases may continue reporting their own refresh failures.

In particular, the November ZIP failure remains a per-release WARNING on an old
sunset producer. Promotion does not automatically acknowledge these failures. Sunset
retention defaults to four days (`--sunset-days` can override it). Until retirement,
the old release keeps serving its last successfully published cycle. The existing
`accepted-on-sunset` disposition adapter covers chart-quality findings, not generic
build errors; extending it would require precise failure evidence and a separate
operator acceptance, not blanket suppression of sunset refresh alarms.

## Release Attribution

The shared build watcher observes the most recent build across all releases, not
necessarily production. Its `build.publish_label` identifies the coordinator's
release tag. When that exact tag matches one deployment-managed channel whose
observed build or refresh already reports the failure, Pipeline Health emits the
channel's alarm without a duplicate `cycle_build.latest_result` global alarm.
The raw watcher result remains `fail`; it is not relabeled as passed. A retained
refresh error remains visible during a retry, until successful activation clears it.

Missing, ambiguous, unmanaged, or healthy release evidence does not suppress a
failed watcher result. Standalone builds retain their global result checks.
Unknown statuses, unavailable sources, dead in-progress build processes, and
production's independent current/next-cycle coverage checks remain monitored.

A completed failure from an explicitly retired release cannot reappear as a
global alarm. Retirement uses the same controller post-GC evidence as chart
monitoring (stopped service, removed release/publication paths, valid tag/commit),
not a guess that any unrecognized build must be obsolete.

This attribution fix is also controller runtime code: integrate it, then run
`tools/prod_manage.py --reconcile`. No new release or product rebuild is required.

No client format, product format, or producer telemetry contract changed.
