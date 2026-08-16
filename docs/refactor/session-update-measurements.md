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

## 2026-08-15 Platform-Landing Result

The Android journey was repeated after the native adapter began decoding only
changed assignment paths. This run included active live-feed work, so its
payload mix differs slightly from the narrow-projection sample above; the
landing timings remain directly useful.

| Measurement | Result |
|---|---:|
| Mean update JSON | 3,042 bytes |
| Mean accumulated snapshot JSON | 32,767 bytes |
| Update/snapshot ratio | 9.3% |
| Mean accumulator merge | 1,653 us |
| Mean changed-group decode | 1,897 us |
| Mean snapshot publication | 171 us |
| Situation-only mean decode | 353 us |

The prior full Kotlin decode averaged 4,195 us. Changed-group decoding reduced
that mean by 55%; the common situation-only case was about 92% lower. No
ordinary update invoked the full `WireUiSessionSnapshot` decoder. The sample's
group counts were `situation=85`, `flight_data=37`, `status=20`,
`flight_plan=6`, `map=6`, and `charts=1`; 21 updates changed no visible group.

On web, the Worker now crosses one narrow projection message and returns a
revision marker. A marker serializes to 30 bytes at a one-digit revision and 32
bytes at revision 128, versus the roughly 33 KB accumulated snapshot previously
returned for each mutation. The initial session and explicit recovery still
cross as full snapshots. The headless Chrome platform journey passed with this
transport, including repeated shared ETA-mode mutations, map rendering, and map
inspection.

These measurements do not justify another lock, thread, or Worker. The next
candidate cost is aggregate UI publication: Android Compose and web React still
receive one top-level snapshot for every relevant update even though decoding
and transport are now narrow.

## 2026-08-15 Render-Invalidation Result

Both platforms now route the generated `ownship`, `situation`, and
`flight_data` groups to a high-rate render store without replacing the
application-shell model. Full-snapshot recovery still invalidates every owner;
revision-only updates invalidate none.

The Android `session_render_invalidation` emulator scenario pushed 32 synthetic
ownship samples at 40 ms intervals after warmup:

| Compose scope | Recomposition count |
|---|---:|
| Application root | 2 |
| High-rate timing effects | 32 |
| Active map content | 32 |
| Inactive chart content | 0 |

The browser platform journey pushed 20 synthetic positions while connected to
the normal development live feeds:

| React/store measurement | Count |
|---|---:|
| High-rate publications | 22 |
| Slower shell publications | 3 |
| `App` render attempts under StrictMode | 12 |
| Active `MapPage` render attempts | 122 |
| Inactive `ChartsPage` render attempts | 2 |

React StrictMode deliberately invokes renders more than once in development.
The actionable result is that root and inactive-page work tracks the much lower
shell publication rate rather than the ownship stream. Map-local fan-out is now
the next measured target; these results still do not justify another Worker.

## 2026-08-16 Active-Map Result

The browser journey was instrumented at commit level after the application
shell and high-rate session scopes were separated. For 20 synthetic ownship
samples, the representative baseline produced 54 active-map commits:

| Commit source | Count |
|---|---:|
| High-rate session snapshot | 20 |
| Followed viewport | 8 |
| Vector overlay and frame | 8 |
| Terrain overlay | 8 |
| Raster tile frame | 3 |
| Other or parent-local work | 9 |

Some sources land in the same commit, so the source counts are not additive.
The nested React Profilers showed that `VectorLayer` reconciled on all 54 map
commits and used about 197 ms of the 232 ms `MapSurface` duration. `RasterLayer`
also reconciled 54 times but used only about 4 ms, so commit count alone would
have selected the wrong target.

After isolating vector reconciliation behind its actual inputs, the same
journey produced 15 vector commits, about 75 ms of vector duration, and about
124 ms of total map-surface duration. Durations are development-build Chrome
measurements and vary between runs; the stable regression is that vector commit
count must fit a budget derived from viewport, vector-overlay, route, and shell
changes rather than total map commits.

## 2026-08-16 Map-Surface Result

The next browser run split the remaining map surface into terrain, situation,
flight-data, and control subtrees. Terrain rendering was not a meaningful
scheduling target: across 71 map commits it consumed about 6 ms. The expensive
behavior shared by the remaining subtrees was unnecessary reconciliation on
every raster, terrain, vector, and high-rate completion:

| Surface | Commits before | Duration before |
|---|---:|---:|
| Flight data | 71 | 10.3 ms |
| Terrain | 71 | 6.3 ms |
| Situation | 71 | 9.5 ms |
| Primary navigation | 71 | 5.7 ms |

Exact dependency boundaries now isolate those four surfaces. Terrain image
placement is also memoized from its frame and viewport inputs. A repeated run
measured:

| Surface | Commits after | Duration after |
|---|---:|---:|
| Flight data | 1 | 0.2 ms |
| Terrain | 15 | 1.6 ms |
| Situation | 27 | 4.9 ms |
| Primary navigation | 8 | 0.8 ms |

The absolute durations are development-build samples and varied with concurrent
live-feed landings; aggregate map duration in particular was dominated by the
number of vector-overlay landings during each run. Durations are therefore not
CI thresholds. The browser journey instead derives commit budgets from each
surface's actual sources: terrain frame and viewport, high-rate situation and
viewport, high-rate flight data, and shell navigation. It fails if unrelated
map-local work crosses those boundaries again.

Moving terrain state or rendering to another scheduler is not justified by this
measurement. The remaining control, raster, and vector subtrees each averaged
well below one millisecond per commit in this workload. The next optimization
should begin with a new measured workload rather than another active-map split.
