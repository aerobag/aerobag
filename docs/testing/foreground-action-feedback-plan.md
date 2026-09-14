# Foreground action feedback plan

Status: deferred on 2026-09-14 to focus on airway graph loading. The previous
uncommitted spinner implementation was removed from web and Android; no spinner
feature is currently being claimed as delivered.

## Implementation when resumed

1. Extract the existing altitude-comparison spinner into a small WorkingIndicator
   renderer on each platform, retaining theme colors, thumb-based sizing, and
   accessible status/live-region semantics. Keep altitude planner background
   recomputations from flashing the user-action spinner.
2. Track a pending user command at its initiating tray, using the core action's
   label. Web awaits the Promise; Android uses UiSessionWorkRunner callbacks,
   never blocking the UI thread. Prevent duplicate submissions synchronously,
   before the next render. Clear pending on success and on error, including
   synchronous submission failures, and show the error rather than swallowing it.
3. Reuse standard trays/scrims. Find Route, endpoint selection, Add Airway, and
   row mutations should visibly acknowledge the action before page faults finish.
   Keep domain decisions and completed navigation effects in core. Do not drive
   this from generic session invalidations or background resource activity.
4. Test the pending state, duplicate taps, failure/retry, and stale completion
   after dismissal/navigation. In a real browser, hold the worker's
   performFlightPlanRowAction message, assert the working indicator is visible,
   then release it and assert the map editor opens. Do this at desktop and phone
   sizes. Held-command/screenshot times are not performance measurements.
5. Test Android's production pending-tray layout with Compose/Robolectric physical
   performTouchInput, ensuring taps on both the indicator and scrim cannot hit
   underlying controls, and that completion restores those controls. Reuse the
   testDebugUnitTest infrastructure. The preliminary broader test invocation
   found six pre-existing release-variant Compose failures because the test APK
   cannot resolve androidx.activity.ComponentActivity; that is a separate harness
   issue, not passing release coverage.

The preliminary web held-command check failed without the indicator and passed
with it. The prototype's pending-state/error tests and Android physical-touch
check also passed before being set aside. Recreate these tests with the resumed
implementation; they are not retained as dead code in the airway sizing work.

## UI audit and next priorities

This is a code-path audit of the user-facing categories, not an assertion that
every control has been exercised with cold pages. Relevant owners are
`ui/web-app/src/App.tsx`, `domain/appCoreAdapter.ts`, `domain/navKv.ts`, Android
`FlightPlanPage.kt`, `ChartsPage.kt`, `UiSessionWorkRunner.kt`, and
`domain/NativeAppCoreAdapter.kt`.

- **P0: Android procedure work on the UI thread.** `ChartsPage` directly calls
  `loadPlateProcedure` in a selection handler; `FlightPlanPage` directly lists
  procedures and fetches options in handlers. These can enter the synchronous
  paged adapter. Move them through an off-main runner before adding feedback;
  setting a loading boolean cannot paint while that same thread is blocked.
  Audit direct plan insert/control callbacks at the same time. The row-action
  decision lookup is nominally resource-free but can wait on core's session
  lock; it should also be scheduled off main with the operation it precedes.
- **P1: Open plate/legend/inset navigation.** Web `openChartAirport` and
  `selectChartReference` await data before navigation; the old page otherwise
  stays unchanged. Show action-scoped feedback until navigation commits or fails,
  or navigate immediately to a correctly owned loading destination. Preserve
  history and user selection; do not leave void/fire-and-forget wrappers that
  make the caller think the navigation is complete.
- **P1: Load procedure from a plate.** Web `loadProcedureOptions` has no pending
  feedback and an empty rejection handler. Keep the tray's action pending,
  expose the error, and prevent duplicate loads. Android needs the P0 dispatch
  correction as well. Apply the same treatment to choosing a procedure variant
  from the flight-plan picker.
- **P1: Flight-plan edits.** Find Route, Add Airway, endpoint choices, and core
  row mutations need action-scoped feedback. Append route, insert-before/after and
  procedure insertion have separate loading/submitting states; consolidate
  presentation while retaining each editor's input/error semantics. Cover
  success, rejected mutation, transport failure and repeat clicks.
- **P2: Search and inspector details.** Airport-info modals already open with
  unresolved state before `airportInfo` completes. Search previews already show
  searching/loading states. Reuse the indicator where useful, but preserve
  selection-generation checks so a late result cannot replace a newer query.
  Click-to-inspect itself deserves a cold-page test; the pending selection query
  must not be confused with the continuously refreshed map overlay.
- **P2: Settings and aircraft changes.** Android user mutations use the work
  runner, but most controls rely on the eventual snapshot. Identify settings
  that actually fault pages (for example a model-dependent projection), use
  local feedback, and keep ordinary instant toggles instant.
- **P2: Replay loads, offline packages, cloud and data-status actions.** These
  already have domain-specific progress/error states in various places. Keep
  byte/progress information rather than replacing it with an anonymous global
  spinner; verify pending/error completion at each user command boundary.
- **Retain: Altitude planner.** It already separates user-requested work from
  background forecast invalidation, retains useful previous results, and keeps
  computation off the rendering thread. The proposed shared indicator must preserve that.
- **Excluded:** map/terrain raster loading, vector refresh, observations,
  camera images and NEXRAD streaming. They progressively fill content and should
  not block the primary interaction UI.

Recommended next implementation slice: plate navigation plus procedure loading,
with held-page browser and Android component/journey tests. For each action,
measure request-to-first-feedback, page frontiers, synchronous core work, and
completion-to-visible-result separately. Also test dismissal/page changes while
waiting so stale completions cannot reopen a dismissed tray. Do not blanket-wrap
all session work: foreground intent belongs to the initiating UI action, while
domain decisions and presentation labels stay in core.
