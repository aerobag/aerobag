# Session Ownership and Projection Roadmap

## Goal

Reduce the update, projection, and lifecycle blast radius of `UiSession` without
creating platform-specific domain policy or assuming that domain separation
requires additional threads.

The current session owns most application state, runtime caches, pending effects,
and every page projection. Most platform operations therefore serialize through
one oversized mutable object and many changes produce a complete UI snapshot.
Async work also has inconsistent ownership and rollback rules. The result is a
combination of avoidable work, stale-completion risks, and large modules whose
dependencies are difficult to review.

The target is one application session composed of explicit domain controllers.
Controllers are independently revisioned and projectable, but remain coordinated
by the application session for changes that must be atomic across domains.

## Non-Goals

- Do not create multiple user-visible sessions during normal operation.
- Do not add a lock per controller merely because state was split into structs.
- Do not move work to an Android thread or web Worker without measurements that
  include scheduling, transfer, landing, memory, and cancellation costs.
- Do not replace the full snapshot wire contract until both platforms can consume
  revisioned partial updates.

## Principles

- Core owns domain state, dependencies, revisions, invalidations, and scheduling
  policy. Platforms own JNI, Worker, coroutine, rendering, and storage mechanics.
- A session handle has one explicit lifetime. Work accepted by an old or closing
  session must not mutate a replacement session.
- A mutation either commits its declared state and effects or leaves them
  unchanged. A post-commit snapshot resource request is explicitly different
  from a pre-commit resource request.
- Controller extraction must create narrow typed APIs. Moving fields into new
  structs while preserving unrestricted cross-field mutation is not progress.
- Projection and serialization costs are measured before optimization.

## Roadmap

### 1. Establish Baselines

Instrument session-registry wait, per-session wait and hold time, operation count,
snapshot projection count and component timings, and serialized snapshot size.
Add deterministic tests for lifecycle races, mutation rollback, and map-query
isolation across independent session slots. Timing thresholds belong in perf
workloads; unit tests assert ordering and state behavior. Same-session map-query
availability while weather preparation is blocked is an acceptance test for the
Weather controller extraction; it cannot pass while one slot deliberately owns
one serialized `UiSession`.

### 2. Make Session Lifetime Explicit

Replace the process-wide map of mutable sessions with a thin handle table of
reference-counted session slots. Each slot has a unique generation and a
`running -> closing -> closed` lifecycle. Looking up a handle briefly locks only
the handle table; an operation then locks its session slot. Destruction removes
the handle first, waits for accepted work, and closes the slot deterministically.

Platform session owners must eventually own every scheduler, subscription,
resource pump, and callback associated with the native handle. Teardown stops and
joins those owners before destroying the core handle. Normal Android and web app
instances each own exactly one active session.

### 3. Standardize Transactions

Use one candidate/commit/rollback path for transactional model mutations. Restore
the prior model and truncate newly queued effects when mutation, projection, or
persistence fails. Return `NeedResources` only for work that did not commit.
Return `NeedSnapshotResources` only when the mutation committed and a subsequent
snapshot refresh needs resources. Cover both distinctions with tests.

### 4. Extract Domain Controllers

Extract controllers incrementally while retaining the existing full snapshot:

1. Settings
2. Weather and live-feed products
3. Map state and layers
4. Situation, ownship, replay, and map follow
5. Flight plan, guidance, and sequencing
6. Nav data, packages, cloud, and status

Remove `UiSession`'s `Deref`/`DerefMut` access as fields migrate. Cross-domain
changes go through an `AppSession` coordinator rather than reaching through one
controller into another.

### 5. Cache Domain Projections

Give each controller a monotonic revision and cached projection. Recompute only
dirty projections. Continue assembling the legacy full snapshot from those
cached projections until both platform adapters support partial updates.

### 6. Stage Work, Then Selectively Schedule It

Define expensive operations as preparation followed by a short validated commit.
Initially run both stages synchronously on the existing execution path. Measure
end-to-end cost and move preparation off-thread only where the observed latency
justifies transfer, memory, lifecycle, and cancellation complexity.

On web, releasing a Rust lock does not make a single Worker concurrent. Prefer to
keep large state where it is consumed and transfer compact commands, projections,
or transferable encoded buffers. Do not shuttle expanded METAR, NOTAM, or other
large product state between Workers merely to claim concurrency.

### 7. Introduce Revisioned Session Updates

Generate a wire model such as:

```text
SessionUpdate {
    session_revision,
    situation?,
    flight_plan?,
    map?,
    weather?,
    settings?,
    cloud?,
    status?,
}
```

Send a full snapshot only for startup and explicit resynchronization. Core owns
which domains changed. Platforms render the supplied projections without
re-deriving dependencies.

### 8. Decompose Platform Surfaces

Split large platform adapters and page modules along the generated controller and
projection boundaries. Render active pages from their domain revisions; avoid
reconciling hidden pages after unrelated updates. Keep platform lifecycle and
transport code separate from UI rendering.

## Completed First Slice

The approved first slice is steps 1 through 3:

- add session instrumentation and deterministic lifecycle/rollback/isolation
  coverage;
- introduce explicit reference-counted session slots with lifecycle phase and
  generation;
- consolidate transactional mutation semantics and migrate the durable settings
  mutations to that path.

This slice intentionally retains the full snapshot and synchronous execution.

## Completed Second Slice

The second slice extracted `SettingsController` as the first narrow domain
boundary:

- the controller owns settings preferences, action validation, disclaimer
  acceptance, display policy, settings-page projection, and flight-data banner
  filtering;
- settings mutations advance a controller-local monotonic revision and
  invalidate a cached settings projection;
- the full session snapshot is unchanged, but repeated snapshots reuse the
  settings projection until either settings or a declared projection input
  changes;
- `UiSession` remains responsible for platform capabilities and for coordinating
  the aggregate persisted document that contains both settings and cloud state;
- boundary tests prevent raw settings state and projection policy from migrating
  back into `session.rs`, and transaction tests cover revision rollback when a
  durable write fails.

## Completed Third Slice

The third slice extracted one `WeatherController` with two private storage
classes: a lightweight, cloneable `WeatherModel` and a non-cloned
`WeatherRuntime` containing decoded products, indexes, and tile caches. Session
transactions checkpoint and roll back only the model while preserving runtime
allocations.

The controller now owns live-feed protocol and connection state, materialized
weather products, nav-derived weather caches, NEXRAD frame selection and
animation timing, and a revisioned weather projection. `UiSession` supplies
map-layer visibility and wall-clock inputs and continues to coordinate shared
status records, map configuration, resource effects, and assembly of the legacy
full snapshot. Mutable runtime access advances the weather revision and
invalidates the projection cache. Boundary and transaction tests preserve these
ownership and rollback rules.

This slice did not add a lock, thread, Worker, or platform wire change.

## Completed Fourth Slice

The fourth slice extracted `MapController` with a lightweight, checkpointed
`MapModel` and a non-cloned `MapRuntime`. It owns map-layer availability and
visibility policy, raster catalog selection, vector-manifest configuration,
obstacle-layer configuration supplied by weather, vector and terrain caches,
and a revisioned snapshot projection.

NAVDB candidate adoption now moves the map controller without copying its heavy
caches. Failed or page-faulted candidates roll back the map model and clear the
moved cache allocations before returning them to the live session. Session code
continues to coordinate typed weather visibility/status consequences, package
policy, NAVDB reads, and resource effects.

The map slice establishes the boundary required for a later same-session
map-query concurrency test, but separate locking or scheduling remains a
measurement-driven follow-up rather than part of extraction.

This slice did not add a lock, thread, Worker, or platform wire change.

## Completed Fifth Slice

The fifth slice extracted `SituationController` around ownship source
registration and selection, live samples, replay, plan preview, bad-autopilot
simulation, and map-follow state. The controller owns a revisioned, cached
situation projection consumed by the aggregate session snapshot.

`AppState` remains a public compatibility DTO for existing reducer callers and
core-only snapshot diagnostics, but it is no longer the session's internal
storage. `UiSession` assembles that DTO only at the compatibility boundary and
projects the UI from controller-owned ownship output plus the remaining
flight-plan and content fields. Session and NAVDB-candidate transactions
checkpoint and roll back the situation model with the other controllers.

Flight-plan guidance, sequencing, and route projection remain in the session
coordinator. Situation operations invoke those responsibilities through the
coordinator rather than making `SituationController` own flight-plan state.

This slice did not add a lock, thread, Worker, or platform wire change.

## Completed Sixth Slice

The sixth slice extracted `FlightPlanController` around the normalized active
plan, guidance geometry, guidance sequencing, and core-owned flight-plan row
actions. The controller owns monotonic model and route revisions plus cached
flight-plan UI/materialization and route projections. Projection cache keys
declare the external ownship, clock, and NAVDB-generation inputs supplied by the
session coordinator.

NAVDB candidate adoption moves the controller and checkpoints only its model.
Page-faulted and rejected candidates roll back that model while retaining the
moved controller allocation. The session still coordinates HAD page delivery,
chart-page consequences, cloud persistence, weather enrichment, and ownship
inputs. It commits candidate plans returned by the controller through the
existing transactional invalidation path.

The public snapshot and route wire contracts remain unchanged. This slice did
not add a lock, thread, Worker, or platform wire change.

## Completed Seventh Slice

The seventh slice extracted `NavDataController` around the attached NAVDB
artifact identity, byte-store handle, public epoch, internal cache generation,
advance-blocked state, candidate filtering, and maintenance timing policy. The
controller separates its checkpointed model from its reference-counted store
runtime and exposes a monotonic revision for diagnostics.

Cycle rollover remains transactional across domains. Core constructs a separate
candidate controller, rebuilds the flight-plan, map, weather, and chart
projections against it, and publishes the candidate only through the final
aggregate `UiSession` swap. A page fault or rejected candidate leaves the live
store, identity, epoch, and generation unchanged. Page faults also leave the
live revision unchanged; a fatal rejection changes only the lightweight model
revision when it marks the controller advance-blocked to prevent repeated
attempts. Boundary-time tests cover immediately before, exactly at, and
immediately after the effective instant.

The public snapshot and platform wire contracts remain unchanged. This slice
did not add a lock, thread, Worker, or platform wire change.

## Completed Eighth Slice

The eighth slice extracted `PackageController` around effective resource policy,
installed package IDs, publication resolution and offline-library metadata,
offline-package preferences, and the cached package projection. Package
availability filtering now has one owner: `NavDataController` consumes the
package controller's typed NAVDB candidate view instead of reimplementing
installed-package filtering.

Cloud retains the durable synchronized records, but it is no longer the source
used to project effective package preferences. `UiSession` transactionally
coordinates local package preference changes across `PackageController` and
`CloudEngine`, and applies remotely reconciled cloud preferences back through
the package controller. Failed persistence rolls both representations back.

Android's standalone offline-package planner remains a pre-runtime editor so a
user can install a first NAVDB before an application session exists. It remains
core-owned and exchanges typed preferences and library updates with the running
session; it is not a second source of effective runtime package policy.

NAVDB candidate adoption remains coordinated by `UiSession`, with
`NavDataController` the sole owner of active NAVDB identity and runtime. Package
state participates in session transactions and candidate rollback without
changing the public snapshot or platform wire contracts. This slice did not add
a lock, thread, Worker, or platform-specific domain policy.

## Next Slice

Continue roadmap item 4.6 with `CloudController` around cloud persistent model,
provider workflow runtime, request/effect scheduling, synchronization status,
and cloud-page projection. Keep flight-plan and package application of remote
records coordinated by `UiSession`; cloud should expose typed reconciled domain
updates rather than mutating those controllers. Data status remains a later,
separate controller slice.

## Relationship To Work Scheduling

[`session-work-scheduling-and-android-freeze.md`](session-work-scheduling-and-android-freeze.md)
addresses input responsiveness, coalescing, and platform execution mechanics.
This roadmap addresses state ownership, projection scope, and lifecycle. The two
meet at staged controller operations, but neither requires every controller to
have a separate lock, thread, or Worker.
