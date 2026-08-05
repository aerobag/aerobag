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

## First Slice

The approved first slice is steps 1 through 3:

- add session instrumentation and deterministic lifecycle/rollback/isolation
  coverage;
- introduce explicit reference-counted session slots with lifecycle phase and
  generation;
- consolidate transactional mutation semantics and migrate the durable settings
  mutations to that path.

This slice intentionally retains the full snapshot and synchronous execution.
The next slice starts controller extraction with `SettingsController`, then adds
controller revisions and cached projections before changing the platform wire
model.

## Relationship To Work Scheduling

[`session-work-scheduling-and-android-freeze.md`](session-work-scheduling-and-android-freeze.md)
addresses input responsiveness, coalescing, and platform execution mechanics.
This roadmap addresses state ownership, projection scope, and lifecycle. The two
meet at staged controller operations, but neither requires every controller to
have a separate lock, thread, or Worker.
