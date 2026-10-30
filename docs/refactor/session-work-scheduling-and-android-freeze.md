# Session Work Scheduling and Android Freeze Plan

State ownership, lifecycle, transactions, and partial projection are tracked in
[`session-ownership-and-projection.md`](session-ownership-and-projection.md).
This document remains focused on input responsiveness and execution scheduling.

## Problem

The black tablet freeze/ANR showed Android running expensive session work on paths
that are allowed to block input:

- `MapExplorerPage` calls `uiSession.queryMapSelection(...)` directly from pointer
  handling after a tap.
- `queryMapSelection`, `queryMapOverlay`, terrain, NEXRAD, and similar calls can
  enter `NavKvStore.runPagedSessionOperation`, which repeatedly calls core,
  fetches missing resources, ingests them, and tries again until complete.
- The ANR stack showed the main thread blocked in
  `get_map_selection_in_session_with_point_display_scale_at_epoch_ms`.
- The same logs showed many dispatcher workers blocked in
  `get_map_overlay_in_session_with_point_display_scale_at_epoch_ms`, which means
  Android can create a herd of expensive session calls behind the same session
  state instead of coalescing them.

This is not a "move one call to IO" bug. The general bug is that platform UI code
can treat resource-paging core calls as ordinary local functions. The fix should
make that impossible or visibly wrong.

## Web Patterns To Preserve

The web side already solved much of this class of bug.

Relevant code:

- `ui/web-app/src/domain/workerAppCoreAdapter.ts`
- `ui/web-app/src/domain/appCore.worker.ts`
- `ui/web-app/src/App.tsx`
- `ui/web-app/scripts/vector-drag-perf.mjs`
- `ui/web-app/scripts/terrain-replay-perf.mjs`
- `ui/core-rust/crates/app-core/src/ui_work_scheduler.rs`

Patterns worth copying:

- Web app code talks to core through a worker-backed `UiSession`. Expensive
  `UiSession` methods are promises, not synchronous UI-thread calls.
- The worker sends `responseReady` before the payload, so logs can distinguish
  core compute time from main-thread receive/clone/landing delay.
- Map overlay queries use a length-one/latest-wins pump:
  - viewport changes only mark a pending request;
  - at most one overlay query is active;
  - if newer work arrives, the next loop consumes the newest request;
  - stale results are discarded unless they are safe to land;
  - landing cost is measured separately from query cost.
- The existing core `SessionSnapshotRefreshScheduler` already owns debouncing,
  viewport quiet time, active gesture delay, and in-flight coalescing for snapshot
  refresh. That is the right shape for policy that must not diverge across
  platforms.
- Web perf scripts already automate worst-case drag/replay workloads and parse
  debug logs against thresholds. Android needs the same discipline, not manual
  flight testing as the only detector.

## Original Android Gaps

The items below describe the state that motivated this roadmap. Map overlay,
map selection, nav-ref inspection, terrain planning/rendering, and NEXRAD
planning/tile preparation, and chart asset resolution/fetching now run through
one retained `UiSessionWorkRunner`; direct access to their low-level
`NativeUiSession` methods requires an error-level opt-in.

Current Android code has useful pieces, but no single enforced boundary:

- Some expensive calls are manually wrapped with `withContext(Dispatchers.IO)`.
- Some expensive calls are still direct from UI/input code, notably map
  selection and nav-ref inspection.
- `LaunchedEffect` cancellation is not enough once native code is inside a
  synchronous resource-paging loop. It can prevent landing a stale result, but it
  cannot guarantee the in-flight core call stops quickly.
- Multiple effects can enter the session path concurrently, which creates worker
  herds and session-lock contention instead of backpressure.
- There is no Android equivalent of the web worker instrumentation that
  separates queue wait, core time, resource fetch time, serialization time,
  payload receive time, and Compose landing time.

## Can Web Discipline Apply To Android?

Yes, conceptually:

- UI event handlers enqueue intent and return.
- Expensive session work runs through one sanctioned executor.
- Results carry request tokens and can be stale-dropped.
- Viewport-driven work is coalesced with latest-wins semantics.
- Input-priority work can preempt or be scheduled ahead of background work by
  core-owned policy.
- Landing results on the UI thread is measured and kept cheap.

No, not literally:

- Web uses a JavaScript Worker and structured clone.
- Android uses coroutines, JNI, Kotlin serialization, Compose state, and local
  package/resource adapters.

The mechanical execution is platform-specific, but the scheduling policy and
event names should be shared as much as practical.

## What Should Be Unified

The web overlay pump is not just an implementation trick. It contains product
policy that is currently trapped in web UI code. Extract that policy into core
before Android reimplements it.

Unify policy in core:

- Work categories: overlay, selection, nav-ref inspection, snapshot refresh,
  terrain, NEXRAD, live-feed/status refresh.
- Work priority: input, timely, background.
- Coalescing policy: latest viewport wins, selection click wins over old
  selection, background refresh waits for viewport quiet.
- In-flight policy: normally one session operation per session unless core says a
  category is safe to parallelize.
- Stale-result policy: when a stale overlay can land, when it must be dropped,
  and which request token is current.
- Debug/perf event names and payload shape.

Keep platform mechanics in the platform:

- Web worker postMessage and transferable payload behavior.
- Android coroutine dispatchers, JNI call sites, and Compose state landing.
- Resource byte fetching and persistence effects requested by typed core
  resource requests.

Likely core API direction:

- Extend or generalize `SessionSnapshotRefreshScheduler` into a
  `UiSessionWorkScheduler`.
- Move the web map-overlay pump's policy into that scheduler: one active overlay
  request, one pending latest request, queue-wait timing, request IDs,
  superseded-result handling, and stale-result land/drop decisions.
- The scheduler returns decisions such as `idle`, `schedule_after`, `start`,
  `drop_stale`, and `may_land_stale`.
- Both web and Android feed it viewport activity, gesture state, request
  completions, and explicit user inputs.
- Unit tests live in Rust so the hard policy is tested once.

## Android Architecture Plan

Introduce an Android session work runner and make direct expensive calls private
to it.

Proposed shape:

- `NativeUiSession` remains the low-level bridge to core.
- A new runner owns calls to expensive `NativeUiSession` methods:
  - `queryMapOverlay`
  - `queryMapSelection`
  - `queryMapSelectionForNavRef`
  - `queryTerrainOverlay`
  - `queryNexradOverlay`
  - other `runPagedSessionOperation` users after audit
- UI code submits typed requests to the runner.
- The runner assigns request IDs, calls the core scheduler, executes native work
  off main, fetches/ingests resources, and publishes results back to Compose only
  if the token is still current.
- Pointer handlers must never call `NativeUiSession.query*` directly.
- Debug builds should assert if an expensive `NativeUiSession` method starts on
  Android main thread.
- Add a simple static/grep test that fails if UI files call expensive
  `NativeUiSession` methods directly outside the runner.

Initial Android behavior targets:

- A tap queues map selection and returns immediately.
- Repeated drag/pinch events produce at most one active overlay query and one
  pending latest overlay query.
- A map selection request can be prioritized over stale overlay work.
- Stale overlay and selection results cannot overwrite newer UI state.
- Resource fetch failures are reported through the existing core failure path,
  not by blocking input.

## Web Follow-Up

Do not rewrite web just to match Android.

Useful web follow-ups:

- Keep the worker boundary.
- Move the overlay pump policy into shared core scheduling. The goal is to
  preserve the current web semantics while deleting web-owned scheduling policy,
  not to create a second Android copy.
- Add selection tokens if logs ever show stale selection results landing after
  a newer click.
- Keep worker `responseReady` instrumentation; mirror its event names on Android
  with Android-specific fields where needed.

## Repro Workload

We need an automated workload that reproduces the freeze class without waiting
for a real flight.

The bad case is:

1. Dense vector viewport with airspace, labels, METAR/PIREP/observation symbols,
   and enough nav-db pages to make overlay and selection non-trivial.
2. Rapid viewport changes to trigger overlay work.
3. A tap/inspect action while overlay work is in flight.
4. Optional resource cache coldness to force `need_resources` paging.

Candidate scenario:

- Start Android on the chart page.
- Use a dense terminal-area viewport such as SFO, Seattle, or Portland Bravo.
- Enable vector-heavy layers and observations.
- Clear or cold-start only the relevant in-memory nav-db page cache, without
  deleting installed packages.
- Run a scripted drag/pinch sequence for several seconds.
- During or immediately after the drag, synthesize a tap selection.
- Repeat with cold and warm caches.

Implemented first scenario:

- Launch with `--es aerobag_perf_scenario map_selection_freeze`.
- The app switches to the map page, moves to an SFO terminal-area viewport,
  starts a burst of overlay queries, then performs map selection on the current
  main-thread path.
- The scenario logs under `AerobagPerfScenario`.
- On the black tablet, a 64-overlay burst reproduced the bad class of behavior:
  selection blocked the main thread for 698 ms and the watchdog reported a
  763 ms main-thread stall. This is not a full 10 s ANR, but it is a reliable
  pre-ANR threshold failure for the exact structural bug.

Additional scenarios to add:

- Flight-path replay: use the captured GPS trace from the black tablet flight,
  feed the recorded ownship path into the app, and drive viewport-follow mode
  the same way the real flight did.
- Terrain-enabled replay: run the same flight-path replay with terrain warning
  enabled, then compare stall and overlay timings against the no-terrain run.
- Cold-page-cache replay: clear only volatile nav-db/session page cache before
  the run, preserving installed packages.

The captured ANR logs do not implicate terrain in the observed freeze. The ANR
stack contains main-thread `get_map_selection...` and many worker-thread
`get_map_overlay...` calls; searches for terrain/NEXRAD scheduled-query symbols
in the ANR dumps were empty. Terrain may still be a separate performance issue,
but it was not on the smoking-gun stack for this freeze.

Required logs:

- request kind, request ID, coalesce key, priority
- queue wait
- native/core elapsed
- number of core `need_resources` rounds
- resource fetch count and bytes
- session invalidations emitted
- serialization/decode elapsed
- main-thread landing elapsed
- stale-drop or stale-land decision
- frame gaps or input callback latency
- active worker count per session

Implemented for `map_selection_freeze`:

- `UiSessionWorkRunner` emits request, queue, start, and terminal records only
  while an explicit Android perf scenario is active.
- Terminal records separate scheduler queue time, coroutine-dispatch wait,
  worker execution, worker-to-main delivery, scheduler completion, and UI
  landing.
- `NavKvStore` accounts for core calls, resource request rounds, cache hits,
  loaded bytes, fetch time, and ingest time inside each scheduled request.
- `AndroidFrameGapMonitor` records p95/maximum frame spacing and reports gaps
  beyond the scenario threshold.
- `scripts/analyze_perf_scenario.py` validates the lifecycle and prints
  per-kind p50/p95/maximum timings; `run_perf_scenario.sh` retains the complete
  log under `/tmp` for investigation.

Resource paging now treats each core `need_resources` response as a dependency
frontier:

- Core map selection probes all independently knowable prerequisites before it
  returns, deduplicates their missing NAVKV pages, and emits their union. It
  still returns before following dependencies that cannot be discovered until
  an earlier frontier is available, such as plate availability and terrain.
- Android launches every uncached load in a frontier immediately on a bounded
  16-worker loader. It then ingests results serially in request order because
  the native stores have deterministic mutation boundaries. NAVDB open, direct
  HAD operations, cycle replacement, session operations, and asynchronous
  session resource effects all use this policy.
- Web already fetched a returned frontier with `Promise.all`; it now emits the
  same debug frontier shape instead of changing that execution policy.
- Perf records include frontier IDs and source kinds, fetch critical-path wall
  time, summed fetch work, observed concurrency, and serial ingest time.

In two emulator runs before this change, map selection took 318 ms and 205 ms.
The corresponding post-change runs took 185 ms and 127 ms, a roughly 40 percent
reduction in this small sample. The NAVKV portion fell from five dependency
rounds to three; the fourth remaining round fetches terrain. In the slower pair,
core time fell from 79 ms to 39 ms, summed resource fetch time from 157 ms to
89 ms, and ingest time from 54 ms to 38 ms.

A separate map-overlay request demonstrated the platform concurrency directly:
its 16-page NAVKV frontier took 8 ms of fetch wall time for 30 ms of summed fetch
work with 16 concurrent loads. This is evidence that sibling reads overlap, not
an additional map-selection measurement. More repetitions are required before
treating either result as a stable benchmark.

The broader stress scenario then detected a separate 879 ms map-overlay core
outlier and a 1,183 ms frame gap. Session-operation timing showed that the map
query was waiting behind live-feed installation: obstacle and winds-aloft
products spent roughly 0.8 and 1.0 seconds respectively constructing and
validating durable state while holding the session lock.

Live-feed installation now prepares and validates durable product state before
acquiring the session lock. The short commit installs the prepared value and
projects its update under the lock; map-query result encoding likewise happens
after releasing the lock. A deterministic core test pauses preparation and
proves that another operation can acquire the same session during that pause.

Two post-change emulator runs passed the unchanged `map_selection_freeze`
scenario while live feeds were active. The frame summaries were 206 frames at
83 ms p95/183 ms maximum and 216 frames at 83 ms p95/200 ms maximum, below the
250 ms threshold. Maximum map-overlay core time was 80 ms and 67 ms, and both
map selections landed within 113 ms and 108 ms end to end. Obstacle preparation
still took as much as 818 ms and winds-aloft preparation 1,062 ms, but maximum
session-lock hold was 37 ms and 39 ms. The original core outlier and frame-gap
failure are therefore resolved without hiding or relaxing either threshold.

The next measured workload is Android cold process startup with warmed offline
packages, ending when the first vector and raster chart content have both
reached a frame. The gated `cold_start_chart` scenario records runtime, native
session, map-surface, vector, and raster milestones. Its analyzer requires the
complete lifecycle, rejects scheduled work on the main thread, allows at most a
500 ms gap during the noninteractive first render, and enforces a 2.2 second
time-to-usable budget.

The terminal condition requires the first combined map-overlay result and at
least one decoded or cached raster tile to each pass through a frame, followed
by one final frame. It does not wait for terrain, NEXRAD, GPS, live-feed cache
restoration or connection, the complete raster tile batch, or flight-plan route
projection. The combined overlay result can contain immediately available
vectors, airspace/TFR, METAR/PIREP, traffic, and offline-region data; live-feed
products restored later invalidate it asynchronously. Terrain has a diagnostic
startup marker but is not a required stage.

The first trace showed that installed-runtime loading did not begin until the
first Compose effect and native session construction then occupied the UI
thread. Runtime and session preparation now form one retained, generation-
checked IO transaction that starts before `setContent`. Package reset cancels
and invalidates the transaction; a late completion closes its native resources
instead of publishing stale state. Compose only awaits the retained result.

On the isolated emulator, three post-change cold runs reached the usable chart
at 1.829, 1.805, and 1.740 seconds. Session preparation completed at 646, 659,
and 632 ms; map composition, vector completion, and first raster delivery each
moved roughly 230 to 250 ms earlier than the comparable traced run. The
remaining repeatable 433 ms gap occurs in the first Compose/map render before
the chart becomes interactive. It is now visible separately from runtime and
session construction and is the next startup-render target if cold-start work
continues.

Follow-up attribution split that render into loaded-app composition, the first
surface-sized map composition, root map layout, and root map draw-command
generation. Two x86 release-build runs reached the terminal condition at 1.278
and 1.420 seconds. In the faster run, runtime/NAVDB preparation took 112 ms,
native session preparation 86 ms, loaded-app composition 12 ms, and the sized
map composition/layout/draw stages 16/18/4 ms. The emulator still delivered a
433 ms maximum frame gap; Android frame statistics attributed its largest
debug-build frames primarily to Skia/OpenGL draw-command submission and render
synchronization. Removing a neutral north-up raster graphics layer did not
improve two A/B runs and was reverted. No further app optimization is justified
from this emulator-specific render delay without a representative-device trace.

Pass/fail thresholds should start conservative:

- No expensive session work starts on Android main thread.
- Pointer/touch handler work stays below a small budget, for example 16 to 50 ms.
- At most one in-flight overlay operation per session.
- Drag workload does not produce an ANR or sustained frame gaps above a chosen
  threshold.
- Selection result eventually lands or is intentionally stale-dropped.

## Test Strategy

Core tests:

- Scheduler coalesces repeated viewport work to one active plus one latest
  pending request.
- Input-priority selection is not blocked behind unlimited background overlay
  work.
- Stale results are dropped or allowed to land according to the same rule on
  both platforms.
- Active gesture and viewport quiet behavior match the existing snapshot
  scheduler expectations.

Android unit tests:

- Fake `NativeUiSession` blocks for a controlled duration; submitting selection
  from the UI layer returns immediately.
- Repeated overlay submissions collapse to the newest request.
- A stale result cannot overwrite a newer result.
- Direct main-thread expensive session calls throw in debug builds.

Android instrumentation/perf test:

- Add a debug-only scripted chart workload similar to
  `ui/web-app/scripts/vector-drag-perf.mjs`.
- Drive the emulator/tablet through drag and tap using adb or Compose test
  hooks.
- Parse logcat for `AerobagPerf` events and fail on ANR-like stalls, main-thread
  calls, or excessive in-flight work.

Web regression tests:

- Keep `vector-drag-perf.mjs` and `terrain-replay-perf.mjs`.
- If scheduler policy moves into core, add web perf runs before and after to
  confirm the worker/pump behavior was preserved.

## Implementation Phases

1. Completed for the map-selection freeze workload: the Android stall watchdog,
   perf scenario, and slow-call audit reproduce the original freeze class. The
   dense map-selection workload has queue/core/resource/delivery/landing
   timings and frame-gap thresholds and passes after resource-frontier and
   live-feed prepare/commit fixes. Extend the same gated instrumentation to
   additional workloads only when they need deeper diagnosis.
2. Completed: introduce the core `UiSessionWorkScheduler`, reusing the shape of
   `SessionSnapshotRefreshScheduler`.
3. Completed: add Android `UiSessionWorkRunner` and route map overlay plus map
   selection through it.
4. Completed: add an automated Android map-selection freeze workload and fail it
   on main-thread stalls.
5. Completed for UI-owned paged work: nav-ref inspection, terrain
   planning/rendering, NEXRAD planning/tile preparation, and chart assets now
   use one retained runner. Raster planning and the one-hertz inspector-distance
   refresh remain direct bounded reads: neither can page resources, and the
   latter already executes on `Dispatchers.IO`. Live-feed maintenance and
   offline package synchronization retain their service-owned schedulers.
6. Completed: each web WASM UI session retains a runner backed by the same core
   scheduler. Chart resolution, map overlay/selection, terrain planning, the
   legacy terrain-tile entry point, and NEXRAD planning are scheduled inside the
   existing app-core Worker. Raster planning and inspector-distance refresh
   retain the same bounded-read exceptions as Android; browser-loaded NEXRAD
   image URLs do not cross the session resource-work boundary. Scheduler wire
   requests and decisions live in `app-ui-contracts`; generated TypeScript and
   Kotlin types prevent the two runners from copying that protocol.
7. Completed for the audited surface: the eight scheduled Android operations
   carry an error-level `RawUiSessionWorkApi` opt-in and a boundary test prevents
   UI code from calling them directly. The runner is owned by the retained core
   session and closes before the native session, rather than being recreated by
   individual Compose pages.
8. Completed: resource-frontier instrumentation distinguishes dependency depth
   from frontier width. Android fetches each known frontier concurrently and
   ingests deterministically; core map selection combines independent first-level
   prerequisites so platform scheduling can exploit that concurrency.

The background lane now retains one latest pending request per core-owned
coalescing key and runs the oldest pending key next. Map churn can replace old
map work without evicting or starving pending terrain or NEXRAD work. Input
selection retains its separate priority lane and may start while background
work is active. If a page coroutine is cancelled while its request is still
pending, the retained runner discards the payload so a stale thumbnail or map
request cannot later consume resource I/O merely to be dropped. The web runner
rejects replaced promises with a typed cancellation and completes failed active
work through core before starting the next retained request.

## Receiver Replay Measurements, 2026-10-30

The sampling-dependent assertion from hosted run `41085733398` on `8cd26da2`
was fixed separately in `43402773`; see
[`hosted-ci.md`](../testing/hosted-ci.md). The measurements here investigate
the underlying Android cost, not another change to the journey's deadline.

In the failed run's 15-second observer window (app PID 6557, device log time
19:53:51.400 through 19:54:06.100):

- Ten map-overlay jobs occupied 7,902 ms of worker time. JNI/core calls accounted
  for only 313 ms; no resource paging occurred. Median worker time was 860 ms,
  p90 1,271 ms, maximum 1,334 ms. These calls also waited 1,441 ms in aggregate
  for main-thread completion delivery.
- Eleven radar queries used 41 ms of worker time, including 3.4 ms in core.
  Their background-queue wait was median 1,220 ms, p90 1,764 ms, maximum
  2,019 ms. Both radar images were tiny local files, not network transfers.
- Twelve terrain-tile jobs used 1,658 ms of worker time, including 441 ms in
  core. Terrain contributes contention, but is not the largest measured task.

Existing `core_us` excludes Kotlin decoding. Added gated fields on the existing
`AerobagSessionWork` completion log, without adding per-feature log records:
`outcome_json_chars`, `outcome_decode_us`, `result_decode_us`,
`result_mapping_us`, `invalidation_publish_us`, and `work_cpu_us`.
The CPU sample surrounds the synchronous payload on one IO-dispatcher thread.
Two controlled-clock unit tests cover stage separation/accumulation, immutable
snapshots and preservation of failed-stage timing and the original exception.

A release-mode x86_64 APK with those measurements passed `shared.receiver-replay`
on a fresh local Google ATD API 34 emulator using the failed run's retained
publication, receiver fixture and app-data baseline. Fifty page-cache-hit map
queries during that journey used 2,937 ms total worker time:

- Rust/JNI: 160 ms.
- Parse JSON into a generic tree: 536 ms.
- Decode that tree into typed Kotlin wire objects: 2,137 ms.
- Map wire objects into UI objects: 77 ms.
- Publish invalidations: 10 ms.
- Residual/rounding: 18 ms.

The two decoding stages account for 91% of this sample. Worker CPU time was
2,531 ms, so this is predominantly CPU/allocation work, not waiting for network
or the session lock. Result envelopes were approximately 80-155 thousand
characters, not megabytes. The generic tree is built in
`NavKvStore.runPagedSessionOperation`; typed decoding and UI conversion happen
in `NativeUiSession.queryMapOverlay` after the resource loop returns.

A second, already-warmed run resumed the same receiver trace from 18 seconds
through 43.2 seconds, without map interactions. It completed 240 map queries
and 240 radar queries in about 25.6 seconds. Every map result envelope was
154,796 characters and none requested resources. Mean map worker time was
23.8 ms, comprising 3.6 ms Rust/JNI, 6.0 ms generic JSON parsing, 13.3 ms typed
decoding, 0.5 ms UI mapping and 0.4 ms other work/rounding. Decoding still accounts
for 81% after warmup. Median/p90 map worker time was 22/33 ms; worker CPU totaled
5.28 seconds. Map completion delivery added median/p90 35/45 ms. Radar query
work was median/p90 1/1 ms but queueing was 62/67 ms.

These are two local traces, not device-population statistics. The local machine
is substantially faster than the hosted failure, so do not transplant the local
stage percentages into the old CI log as though CI measured them. The new
instrumentation makes that breakdown available on future hosted runs. Warmup
also matters: do not compare the first run's mixed startup sample directly with
the second run and call it an optimization.

### Implemented Follow-Ups

1. Android map overlays now decode directly into a typed paged envelope in one
   streaming JSON pass. The existing resource loop is generic over its result
   decoder; raw callers still use that same loop with `JsonElement`. There is
   no second pager, tree reserialization, binary format or contract change.
   A nullable payload on the envelope permits resource faults without a result;
   completed typed responses still require a valid result. This also avoids a
   polymorphic discriminator falling back to a DOM when `state` is last.
2. Core receiver replay now invalidates layers when their inputs change, or
   when the historical clock crosses their deadlines. Quiet clock ticks do not
   redraw the map. Ownship motion/altitude, traffic changes/expiry, station
   weather inputs, radar animation/source/age deadlines and rewind still trigger
   the relevant layers. Radar queries record their next historical deadline;
   the replay clock consumes it, not a host-clock timer. Replay commands already
   return a projected session update, so they no longer request a duplicate
   full snapshot.
3. Android's `SessionWorkQueue` serializes calls to the existing core scheduler
   away from Main. Completion releases the execution slot and posts ordered UI
   delivery before starting the successor. A busy Main thread can delay landing
   without holding that slot. Core still owns input priority, per-key coalescing
   and stale-result policy. Cancellation/retirement claim each result at most
   once, and queued work cannot enter a retired native session.

### Matched Local Measurements

Each warmed sample used the same retained fixture and app-data baseline on a
local Google ATD API 34 emulator, release x86_64 APK, replay from 18 seconds
through approximately 43.2 seconds, with no map interactions or resource faults.
Counts include the initial seek and draining its playback/pause results. In each
`warm-logcat.txt`, select successful completion records after the last submitted
`seekPlayback`: before PID 3620 from 17:12:42.030, decoder-only PID 3534 from
17:28:15.062, combined PID 4562 from 17:40:47.644 (all 2026-10-30 UTC).
The log files also contain older runs; do not aggregate the entire file.
These are individual local samples, not a statistical estimate of CI or tablet
performance.

- **Before:** 240 map queries and 240 radar queries in 25.6 seconds; map worker
  CPU totaled 5.28 seconds. Mean map worker time was 23.82 ms, with 19.30 ms in
  the two decode stages. Radar queue wait was median/p90 62/67 ms.
- **Decoder only:** 236 map/radar queries; mean map worker time 14.41 ms
  (median/p90 14/18 ms), including 9.80 ms single-pass decoding and 3.74 ms
  Rust/JNI. Mean worker CPU fell from 21.99 to 13.39 ms. This isolates an
  approximately 40% reduction in worker time without suppressing queries.
- **All three:** 26 map queries and 19 radar queries in the same roughly
  25.5-second replay window; map
  worker CPU totaled 0.417 seconds, about 92% less than before. Radar queue wait
  was median/p90 15/22 ms. There was one terrain plan and no terrain tile jobs.
  Mean map worker time was 16.54 ms across this much smaller sample; reduced
  warmup means it should not be interpreted as a decoder regression.

Main-thread delivery is still measurable: map median/p90 46/50 ms and radar
43/52 ms in the combined sample. This work removes its coupling to the next
background computation; it does not claim to fix or explain that UI delay.
`outcome_decode_us` now measures the entire direct typed decode for map queries;
the obsolete `result_decode_us` stage exists only in the before logs, not the
new implementation. Instrumentation remains gated by existing perf metrics.

### Regression Coverage

- The streaming-decoder test failed with the old generic-tree path, then passed
  with direct decoding. Tests cover a last-position `state`, strict unknown and
  malformed fields, missing results, resource faults, accumulated invalidations,
  committed-mutation snapshot resumption and the repeated-request guard.
- Quiet-tick/radar-boundary tests failed against the old every-tick invalidation
  path. Session tests also cover ownship movement/expiry, held-radar expiry and
  rewind, alongside the existing incremental-versus-seek equivalence tests.
  A partial-traffic-expiry case tests the deadline even while the retained
  aircraft array is unchanged; temporarily removing that deadline check made
  the test fail with a missing map invalidation, then restoring it passed.
- Queue tests initially failed while completion still waited on Main. They now
  cover progress while UI delivery is held, FIFO delivery, newest-pending
  coalescing, failure, cancellation before/after computation, and retirement
  before worker dispatch or after computation but before landing.
- Rebuilt Android release APK: `shared.receiver-replay` and
  `shared.nexrad-frames` pass. Rebuilt optimized web bundle:
  `shared.receiver-replay` passes. These are focused runs, not the full release
  journey suite or a physical-tablet benchmark.
- Full cheap preflight passes all 13 suites in 91.1 seconds, including the Rust
  and Android tests above. Logs: `/tmp/aerobag-cheap-preflight-d5oh0mt7/`.

Diagnostic artifacts: `/tmp/aerobag-radar-perf-instrumented/` contains the passing
journey result, `logcat.txt` and `warm-logcat.txt`. The original failure and exact
builds/fixtures are under `/tmp/aerobag-e2e-41085733398/`. Follow-up measurements
are in `/tmp/aerobag-decoder-only/` and `/tmp/aerobag-perf-all/`, each with
`warm-logcat.txt`. The extra radar and web results are in
`/tmp/aerobag-perf-all-radar/` and `/tmp/aerobag-replay-web-fixed/`. No resource
format, scheduling policy or journey deadline changed.

## Open Questions

- Should selection preempt an in-flight overlay call, or only jump ahead of
  pending work after the current call returns?
- Is one in-flight session operation per session always the right rule, given
  the session mutex and nav-db page cache?
- Which stale overlay results are visually acceptable to land? Preserve the web
  rule unless we prove a better one.
- Do any result payloads need compact/generated wire encoding before Android can
  land them cheaply?
- Can the repro clear only in-memory page state without disturbing installed
  packages, so cold-cache tests are repeatable but quick?
