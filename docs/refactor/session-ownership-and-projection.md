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
coordinates local package preference changes across `PackageController` and the
cloud domain, and applies remotely reconciled cloud preferences back through the
package controller. Failed persistence rolls both representations back.

Android's standalone offline-package planner remains a pre-runtime editor so a
user can install a first NAVDB before an application session exists. It remains
core-owned and exchanges typed preferences and library updates with the running
session; it is not a second source of effective runtime package policy.

NAVDB candidate adoption remains coordinated by `UiSession`, with
`NavDataController` the sole owner of active NAVDB identity and runtime. Package
state participates in session transactions and candidate rollback without
changing the public snapshot or platform wire contracts. This slice did not add
a lock, thread, Worker, or platform-specific domain policy.

## Completed Ninth Slice

The ninth slice extracted `CloudController` around the cloud persistent model,
provider workflow runtime, request/effect scheduling, synchronization status,
and cloud-page projection. The existing `CloudEngine` is now a private
implementation detail held behind a copy-on-write controller model, so session
transactions checkpoint a small reference and can restore cloud mutations
without copying provider state during unrelated operations.

Cloud page, status summary, and status-record projection now share one cached
projection keyed by the controller revision, wall clock, and QR-scanner
capability. Provider completions expose typed flight-plan and offline-package
updates to `UiSession`; the session remains the coordinator that applies those
updates atomically to `FlightPlanController` and `PackageController`. NAVDB
candidate adoption moves and restores the cloud controller with the other
domains.

The public snapshot and platform wire contracts remain unchanged. This slice
did not add a lock, thread, Worker, or platform-specific domain policy.

## Completed Tenth Slice

The tenth slice extracted `DataStatusController` around status-record ownership,
hushing, status actions, package-warning interpretation, and both status-strip
and Data Status page projection. The controller owns a copy-on-write,
transaction-checkpointed model with a monotonic revision and separately cached
strip and page projections.

`UiSession` supplies one typed page-input value assembled from NAVDB, package,
cloud, weather, platform, and clock facts. Status presentation policy, live-feed
freshness thresholds, chart validity interpretation, and package warning text no
longer live in the coordinator. Shared chart-validity helpers remain controller
owned and are reused when the coordinator creates the displayed-chart warning,
avoiding a second policy implementation.

NAVDB candidate adoption moves and restores the controller with the other
domains, and aggregate session transactions checkpoint its lightweight model.
Boundary tests prevent raw status state or page policy from returning to
`SessionModel`; cache tests prove repeated snapshots reuse unchanged status
projections. The public snapshot and platform wire contracts remain unchanged.
This slice did not add a lock, thread, Worker, or platform-specific domain
policy.

## Completed Eleventh Slice

The eleventh slice removed `UiSession`'s `Deref`/`DerefMut` access to residual
coordinator state. That state is now named `SessionCoordinatorModel`, and every
access is explicit as `session.coordinator.<field>`. A boundary test prevents the
implicit field shortcut from returning and fixes the remaining coordinator field
list as a reviewable architectural boundary.

`SettingsController`, which predated the common controller composition pattern,
now sits directly on `UiSession` with the other controllers. It checkpoints only
its preferences and revision during aggregate transactions and invalidates its
projection cache on rollback.

The aggregate snapshot, owner/revision, projection-input, and specialized-query
invalidation inventory is recorded in
[`session-update-dependency-inventory.md`](session-update-dependency-inventory.md).
The public snapshot and platform wire contracts remain unchanged. This slice did
not add a lock, thread, Worker, or platform-specific domain policy.

## Completed Twelfth Slice

The twelfth slice added core-owned monotonic projection versions for the twelve
groups in the dependency inventory. Each version observes a typed dependency
stamp assembled from controller revisions and explicit coordinator inputs;
serialized snapshot comparison and platform-side change inference are excluded.

Projection version state participates in aggregate transaction rollback and
NAVDB candidate adoption. Tests prove chart-only, settings-only, clock-driven,
and debug-only changes leave unrelated versions untouched, and an architectural
guard fixes the group list and keeps the versions out of the current full
snapshot wire contract.

This slice did not add a platform contract, lock, thread, or Worker.

## Completed Thirteenth Slice

The thirteenth slice defined `UiSessionUpdate` and
`UiSessionProjectionPatch` in the canonical Rust UI-contract crate, generated a
JSON Schema and TypeScript/Kotlin bindings, and made core assemble patches from
pre/post projection versions. The mandatory envelope carries the UI contract
and session revision; eleven optional groups carry their own monotonic version
and the exact top-level snapshot fields owned by that group.

Snapshot-producing mutations expose the update as a transitional
`session_update` member while continuing to return the full snapshot consumed
by current Android and web adapters. NAVDB adoption and maintenance include the
same update beside their nested snapshot. Startup and explicit snapshot recovery
remain full-snapshot operations. The effect-only offline-preferences command
continues to return JSON `null` until adapters can consume updates directly.

Tests pin the generated wire shape, prove chart-only and settings-only mutation
scope, and require every production snapshot field to belong to exactly one
non-overlapping update group. Existing specialized query invalidations are
unchanged. This slice did not add a lock, thread, Worker, or platform policy.

## Completed Fourteenth Slice

The fourteenth slice taught both platform adapters to retain a raw startup
snapshot, validate generated `UiSessionUpdate` envelopes and group versions,
merge changed top-level fields, and run the result through their existing
snapshot decoders. Stale responses are ignored, revision gaps explicitly reset
from the accompanying transitional full snapshot, and a normally applied patch
must exactly reproduce that full snapshot. Paged-operation runners now report
when a committed mutation resumed through the full-snapshot continuation, so
that recovery path cannot be mistaken for a missing update.

The schema generator derives the patch-group inventory from the canonical
contract and emits it for TypeScript and Kotlin. The canonical `fields` type is
now a JSON object rather than arbitrary JSON. Both accumulators run the same
checked-in conformance sequence, including stale, gap, malformed, overlapping,
and envelope-replacement cases.

Offline-package preference persistence now returns a normal revisioned update;
the update-free model transaction helper was removed. This work also found and
fixed NAVDB maintenance publishing changed projections without advancing the
session revision. Full core, web, and Android tests pass, and the hermetic
Android `KRNT KPWT` route-rendering journey passes with the accumulator active.
Ordinary mutation results still carry the transitional full snapshot for the
next cutover slice. This slice did not add a lock, thread, or Worker.

## Completed Fifteenth Slice

The fifteenth slice removed the transitional full snapshot from ordinary
mutation results. Core now serializes `UiSessionUpdate` directly; NAVDB
maintenance and adoption envelopes carry only their action/disposition metadata
and an optional update. Startup, explicit refresh, rejected-command recovery,
and committed page-fault continuation remain explicit full-snapshot operations.

Android and web apply generated updates directly. A revision gap leaves the
cached model untouched, calls the existing paged full-snapshot API, installs the
returned snapshot, and resets projection-group version tracking. Both platform
accumulators test this recovery behavior, and core asserts that narrow mutation
payloads contain no snapshot fields and are smaller than the corresponding full
snapshot. NAVDB tests also reject any reintroduction of an embedded snapshot.

## Completed Sixteenth Slice

The sixteenth slice added core-owned aggregate diagnostics for update projection
count, total and maximum projection time, and frequency of every generated
update group. Android and web now have opt-in landing measurements for update
bytes, accumulated snapshot bytes, merge time, full platform-model decode time,
and publication or Worker-transfer timing. Android can read the same core
diagnostics through its native boundary, and its injected core clock is now
high-resolution and monotonic.

The hermetic Android `KRNT KPWT` journey remained green. Its warm update sample
showed that update JSON averaged 90% of accumulated full-snapshot JSON;
`application`, `settings`, and `status` appeared in every sampled update, and
Android continued decoding the full Kotlin model. The core checkpoint likewise
showed those groups advancing much more often than the narrower `situation`
group. Full methodology and results are in
[`session-update-measurements.md`](session-update-measurements.md).

This slice added no lock, thread, Worker, or platform domain policy.

## Completed Seventeenth Slice

The seventeenth slice made revisioned updates materially narrow. UI wire
contract version 2 replaced top-level field maps with generated, validated path
assignments. Core split `application` into `application_shell`, `flight_plan`,
`ownship`, and `flight_data`; the latter also owns the dynamic flight-data row
inside Settings. Static settings changes remain in the settings group.

Cloud and Data Status controllers now retain semantic projection revisions:
clock or model inputs can force a projection computation without advancing the
wire group when the rendered output is unchanged. Application dependencies use
the actual projected ownship, flight-plan, and flight-data models, so clock
ticks likewise emit patches only when visible values change. Shared web and
Android conformance data exercises top-level, nested-object, and nested-array
assignments and rejects missing, envelope, duplicate, and overlapping paths.

On the same 128-update Android journey, mean update size fell from 90.0% to
7.9% of the accumulated snapshot. Full results are in
[`session-update-measurements.md`](session-update-measurements.md).

## Completed Eighteenth Slice

The eighteenth slice exploited narrow patches at both platform landing
boundaries. Android now decodes only the generated assignment paths carried by
changed groups, preserves unchanged Kotlin submodels, and supports
group-filtered snapshot listeners. Startup and explicit recovery still decode a
complete snapshot. A 128-update emulator sample reduced mean model decode from
4,195 us to 1,897 us overall and to 353 us for situation-only updates.

Web sessions now send ordered narrow projection messages across the Worker
boundary and accumulate them on the render-state side. Mutation responses carry
only a 30-to-35-byte revision marker instead of structured-cloning the complete
roughly 33 KB snapshot a second time. Projection messages received before the
main-thread facade exists are queued and drained in order; a marker whose
revision does not match the landed projection fails loudly. Explicit recovery
continues to cross the boundary as a full snapshot.

The browser journey exposed two contract bugs while exercising the real Worker.
The creation snapshot used a special empty settings model that could not accept
the first legal nested update, and the time-display action bypassed the common
web mutation accumulator. A core regression now applies every first-revision
assignment to the raw creation snapshot, and the unchanged browser smoke covers
the time-display symptom. Full measurements are in
[`session-update-measurements.md`](session-update-measurements.md).

## Completed Nineteenth Slice

The nineteenth slice narrowed platform render invalidation around the generated
projection groups. Both adapters now publish the accumulated snapshot together
with the exact changed-group set and explicit full-snapshot recovery marker.
`ownship`, `situation`, and `flight_data` feed a stable high-rate render store;
the application shell receives only the remaining groups. Revision-only updates
advance the stored model without invalidating either render owner.

Android Compose reads the high-rate store only in the active map/chart content
and its timing effects. Its main-thread latest-value delivery unions group sets
when collapsing queued publications, so an intermediate shell change cannot be
lost. A 32-sample emulator scenario produced 32 map recompositions, 32 timing-
effect recompositions, 2 root recompositions, and 0 chart recompositions.

Web React uses `useSyncExternalStore` for the high-rate model, map viewport, and
map query-invalidation revisions. Inactive page layers retain local state but
skip parent-driven reconciliation until activated. A headless Chrome journey
with 20 synthetic positions observed 22 high-rate and 3 slower shell
publications, 12 StrictMode root render attempts, 122 active-map render attempts,
and 2 hidden-chart render attempts. The journey fails if high-rate publications
begin driving root or hidden-page work directly.

This slice added no lock, thread, Worker, or platform-specific domain policy.

## Completed Twentieth Slice

The twentieth slice measured active-map commits by source and accumulated the
existing React Profiler durations during the synthetic-ownship browser journey.
The baseline produced 54 map commits. Direct high-rate publication accounted
for 20, followed viewport movement for 8, vector-overlay landing for 8, terrain
landing for 8, and raster landing for 3. The vector surface nevertheless
reconciled on all 54 commits and consumed about 197 ms of the 232 ms measured
map-surface duration.

The vector surface now has an explicit shallow dependency boundary. It
reconciles for viewport, vector model, route, selection, size, and vector debug
changes, but not for unrelated ownship, terrain, raster, or shell-local work.
The same journey reduced vector commits from 54 to 15, vector duration from
about 197 ms to 75 ms, and total map-surface duration from about 232 ms to 124
ms. The E2E budget is derived from observed vector-relevant commit sources and
fails if unrelated map-local work again reconciles that surface.

While starting the measurement, the browser journey exposed a lifecycle cycle
introduced by typed session-operation hardening: pre-NAVKV resource-policy
configuration had been routed through the NAVKV pager, while opening NAVKV
already required that policy. Pre-NAVKV configuration now uses a nominally
typed resource-free mutation completion and fails if it requests paging;
post-attach mutations retain the typed paged runner and snapshot continuation.

## Next Slice

Profile the remaining active map work. The vector drawing surface is now scoped
to vector-relevant inputs, but terrain completion and the high-rate map chrome
still schedule the page owner independently. Measure their render duration
before moving another surface or controller; raw commit count alone is not a
reason to split ownership.

## Relationship To Work Scheduling

[`session-work-scheduling-and-android-freeze.md`](session-work-scheduling-and-android-freeze.md)
addresses input responsiveness, coalescing, and platform execution mechanics.
This roadmap addresses state ownership, projection scope, and lifecycle. The two
meet at staged controller operations, but neither requires every controller to
have a separate lock, thread, or Worker.
