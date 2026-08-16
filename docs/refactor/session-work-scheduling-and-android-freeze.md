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
planning/tile preparation now run through `UiSessionWorkRunner`; direct access
to their low-level `NativeUiSession` methods requires an error-level opt-in.

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

1. In progress: the Android stall watchdog, perf scenarios, and slow-call audit
   reproduce the original freeze class. Complete queue/core/fetch/result timing
   instrumentation as individual workloads need deeper diagnosis.
2. Completed: introduce the core `UiSessionWorkScheduler`, reusing the shape of
   `SessionSnapshotRefreshScheduler`.
3. Completed: add Android `UiSessionWorkRunner` and route map overlay plus map
   selection through it.
4. Completed: add an automated Android map-selection freeze workload and fail it
   on main-thread stalls.
5. In progress: nav-ref inspection, terrain planning/rendering, and NEXRAD
   planning/tile preparation now use the runner. Audit the remaining paged
   session calls before deciding whether chart assets, live-feed maintenance,
   or other service-owned operations belong on this UI scheduler.
6. Pending: update web to consume the shared scheduler while keeping the Worker
   execution boundary.
7. In progress: the seven scheduled Android map operations carry an error-level
   `RawUiSessionWorkApi` opt-in and a boundary test prevents UI code from calling
   them directly. Extend the boundary only as phase 5 identifies more UI work.

The background lane now retains one latest pending request per core-owned
coalescing key and runs the oldest pending key next. Map churn can replace old
map work without evicting or starving pending terrain or NEXRAD work. Input
selection retains its separate priority lane and may start while background
work is active.

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
