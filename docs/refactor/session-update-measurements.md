# Session Update Measurements

## Purpose

This baseline measures the revisioned update path after ordinary mutation
results stopped carrying a full `UiSessionSnapshot`. It is intended to decide
which projection and platform boundaries to split next; it is not a release
performance threshold.

Core permanently records update projection count, total and maximum projection
time, and per-group frequency in `UiSessionDiagnostics`. Android and web have
opt-in landing diagnostics that record changed groups, update JSON bytes,
accumulated snapshot JSON bytes, merge time, full platform-model decode time,
and publication time. Android exposes the core aggregate through the same
native diagnostics boundary. Diagnostic JSON serialization remains disabled in
normal builds.

## 2026-08-14 Android Baseline

The workload was the hermetic `android.flight-plan-route-smoke` journey on an
Android 14 x86_64 emulator. It installed the compact fixture publication,
entered `KRNT KPWT`, opened the chart, centered on `KPWT`, and verified one
visible route segment. A warm 128-update sample after the journey measured:

| Measurement | Result |
|---|---:|
| Mean update JSON | 30,352 bytes |
| Mean accumulated snapshot JSON | 33,714 bytes |
| Update/snapshot ratio | 90.0% |
| Mean accumulator merge | 190 us |
| Mean full Kotlin model decode | 1,430 us |
| Mean snapshot publication | 26 us |

Group frequency in that sample was:

| Group | Updates | Frequency |
|---|---:|---:|
| `application` | 128 | 100.0% |
| `settings` | 128 | 100.0% |
| `status` | 128 | 100.0% |
| `cloud` | 127 | 99.2% |
| `situation` | 109 | 85.2% |

A fresh-session core checkpoint after 63 update projections reported 544,686
us total projection time, 12,942 us maximum, and these dominant counts:
`application=57`, `settings=57`, `status=55`, `cloud=44`, and `situation=19`.
The mean was 8,646 us per projection.

These timings come from an unoptimized debug Rust build on an emulator and must
not be treated as tablet release latency. The payload ratio and group frequency
are contract-shape measurements and remain actionable independent of hardware.

## Findings

The update transport works, but the current groups are not narrow enough. Raw
wall-clock and broad controller revisions cause `application`, `settings`,
`status`, and `cloud` to advance during routine ownship and live-feed activity.
`application` carries the aggregate `app_ui_state`; `settings` carries a full
settings page containing the dynamic flight-data cells. A small displayed-value
change therefore serializes most of the snapshot.

Both adapters then rebuild a full platform model. Android strictly decodes the
entire accumulated snapshot and publishes it to the snapshot listener. The web
Worker does the same accumulation and returns the full snapshot through
structured clone. Platform decomposition alone would leave the 90% wire
payload intact, while core-only group splitting would still leave avoidable
full-model landing work.

## Decision

Narrow core projection scope first, then decompose platform landing:

1. Split the aggregate application update into generated nested projections for
   flight data, ownship/situation-derived UI, flight-plan UI, and the remaining
   application shell. Keep startup and explicit recovery as full snapshots.
2. Replace raw-clock dependency tokens for application, settings, status, and
   cloud with semantic projection revisions or display-granularity tokens, so a
   clock sample advances a group only when its rendered value can change.
3. Separate the settings page's dynamic flight-data choices from otherwise
   static settings state, allowing routine flight-data updates to avoid sending
   the whole settings page.
4. Once those patches are narrow, move web update accumulation to the side of
   the Worker boundary that owns rendered state and add group-scoped Android
   decode/listener surfaces. Hidden and unrelated page models should not be
   decoded or reconciled for every ownship sample.
5. Repeat the same journey with web debug logging and Android release-like
   builds before deciding whether any measured core operation needs a staged
   prepare/validate/commit boundary or another thread/Worker.

No additional lock, thread, or Worker is justified by this baseline alone.

## 2026-08-15 Narrow-Projection Result

The same hermetic Android journey was repeated after replacing top-level field
replacement with strict generated path assignments and splitting the aggregate
application group into `application_shell`, `flight_plan`, `ownship`, and
`flight_data`. Data Status and Cloud now advance their wire projections only
when their rendered models change, and the settings group no longer owns
dynamic flight-data values.

A warm 128-update sample measured:

| Measurement | Before | After |
|---|---:|---:|
| Mean update JSON | 30,352 bytes | 2,612 bytes |
| Mean accumulated snapshot JSON | 33,714 bytes | 32,895 bytes |
| Update/snapshot ratio | 90.0% | 7.9% |
| Mean accumulator merge | 190 us | 488 us |
| Mean full Kotlin model decode | 1,430 us | 4,195 us |
| Mean snapshot publication | 26 us | 41 us |

The after-sample group counts were `situation=61`, `status=25`,
`flight_plan=6`, `map=5`, `flight_data=1`, and `charts=1`.
`application_shell`, `settings`, and `cloud` did not appear. Unchanged rendered
state commonly produced only a 48-byte revision envelope; routine situation
updates were about 760 bytes.

The emulator timing columns are noisy debug-build measurements and do not show
an Android decode improvement yet. In fact, Android still decodes the complete
accumulated Kotlin snapshot after every update. The payload result is the
actionable result of this slice: wire volume fell by about 91%, while core's
debug projection mean remained comparable to the baseline. The next measured
target is platform landing work, beginning with group-scoped Android decode and
publication and then moving web accumulation to the render-state side of the
Worker boundary.
