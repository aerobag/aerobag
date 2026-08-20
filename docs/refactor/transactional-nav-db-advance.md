# Transactional NAVDB Advance

Status: implemented.

Tracks `TASK-53`.

## Goal

Advance a running session from one NAVDB artifact to another without exposing a
mixture of old and new navigation data, losing an active flight plan, using a
deleted package, or retaining stale platform caches.

The advance is a core-owned transaction. Android and Web provide generic
resource effects and render core output; neither platform decides whether a
candidate NAVDB is safe to adopt.

## Safety Contract

The running application always observes one coherent NAV-data generation.

- An operation reads either the old generation or the new generation, never a
  mixture.
- A candidate NAVDB is provisional until core can project the current user
  state successfully against it.
- `NeedResources` is retryable and does not commit the candidate.
- Missing pages are not semantic failures. Core asks the platform for the
  required opaque resources and resumes the same transaction.
- A missing or changed object required by safety-relevant state aborts the
  transaction. Core must not drop, reinterpret, or silently reroute a flight
  plan.
- Harmless transient UI state may be reconciled by an explicit core policy. For
  example, an inspector for a vanished object may close. Platforms must not
  invent those policies independently.
- The old NAVDB remains attached and pinned until the candidate commits and all
  readers of the old generation have drained.
- Package GC cannot delete an artifact while a core session holds a lease on
  it.

## State That Must Be Rebuilt Or Validated

Constructing the ordinary session snapshot is necessary but not sufficient.
The candidate transaction must exercise every currently reachable
NAV-dependent projection:

- flight-plan display rows and actions
- resolved route components and resolved legs
- active leg and direct-to state
- CDI/guidance geometry
- sequencing and finish-line geometry
- flight-plan route rendering and waypoint highlighting
- chart-page compact selections
- raster map catalog, selected family, selected map, and raster tile plan
- map overlay and map-selection results
- airport, navaid, fix, and airspace inspector state
- airport document availability, plate folder state, and chart supplement state
- search/query state that remains live in the session
- magnetic variation and flight-data banner fields
- cycle freshness and Data Status projections
- NAV-derived METAR importance/cache state

Future NAV-dependent state must join this transaction through a common typed
dependency/generation mechanism. It must not require adding another platform
callback to a hand-maintained list.

## Transaction

### 1. Discover And Open A Candidate

Core receives the generic installed-artifact inventory and chooses the NAVDB
candidate according to the publication and validity contracts. The platform
only lists artifacts and supplies bytes requested by core.

Opening and contract validation happen before the running session changes. The
candidate has its own NAVKV handle and page cache.

### 2. Clone The Session

Clone the current `UiSession` into a candidate session. Attach the candidate
NAVKV only to that clone. Retain the current session and NAVKV unchanged.

Invalidate all NAV-derived caches in the candidate before rebuilding. Do not
copy stale derived values merely because their Rust types are cloneable.

### 3. Rebuild And Reconcile

Run one core-owned rebuild path over the candidate session. This path must:

- re-resolve or validate the flight plan
- rebuild guidance, CDI, sequencing, route, and overlay geometry
- reload the raster catalog and reconcile selected chart/map state
- validate current inspectors, airport-document state, and other retained
  selections
- rebuild NAV-derived caches and status records
- construct the complete `UiSessionSnapshot`
- construct or preflight all other currently reachable NAV-derived outputs

This must use the same production projection code used after commit. Do not
create a separate validation implementation that can drift from rendering.

### 4. Page The Candidate To Completion

If production projection reports `NeedResources`, retain the candidate
transaction, fetch all requested typed resources, ingest them into the
candidate store, and run the same rebuild path again.

No candidate mutation may leak into the live session during paging. Fatal
decode, contract, or semantic resolution errors abort the candidate.

### 5. Produce The Commit Result

A successful candidate produces, as one result:

- the complete replacement session snapshot
- the new active NAVDB identity and cycle
- a new monotonically increasing `nav_data_epoch`
- broad `NavData` invalidation plus the affected existing invalidations
- the package lease set after commit

The complete result must exist before the live session is modified.

### 6. Commit Atomically

Replace the live session with the candidate in one critical section. New work
receives the new epoch. Work already holding the old epoch may finish, but its
result cannot land in the new generation.

Do not destroy the old NAVKV handle while an old-generation operation is using
it. Use a lease/reference-counted generation boundary rather than relying on
timing.

### 7. Retire The Old Generation

After old readers drain:

- release the old NAVKV handle and its page cache
- release the old artifact lease
- recompute the offline package plan in core
- allow a later GC phase to remove the old artifact

Fetch/install, runtime adoption, and GC are therefore separate phases. Sync
must never delete the active NAVDB and then ask the runtime whether the
replacement works.

## Failed Advance And Recovery

If candidate projection cannot preserve safety-relevant state:

- destroy the candidate generation
- leave the live session, snapshot, active NAVDB, and old package lease intact
- record the rejected candidate identity and diagnostic reason
- emit a non-hushable warning:

  `Could not advance to new NAVDB. Reload application when not flying.`

The warning includes a core-declared `Reload application` action. Core owns the
action ID, label, enabled state, and warning policy. Platforms only implement
the shell effect:

- Web performs a normal application reload.
- Android disposes and reconstructs the runtime/session without requiring the
  user to find an operating-system force-stop control.

Reloading currently clears volatile flight state, allowing startup to select
the new NAVDB. If flight plans become persistent, the recovery contract must be
revisited; reload must not repeatedly restore the same incompatible state and
fail again.

The first semantic rejection latches NAVDB advancement off for the lifetime of
the session. Later clock ticks, publication refreshes, and package syncs do not
try another candidate. Transient transport or resource-fetch failures do not
set this latch; platforms retry those effects without changing the live
generation.

Android does not require an operating-system force stop or swipe-away. The
core-owned warning action has ID `app:reload`; Android disposes the retained
runtime and constructs a fresh session, while Web reloads the page.

## Clock And Publication Triggers

Package sync is not the only adoption trigger. Each snapshot carries
`next_nav_db_maintenance_epoch_ms`, computed by core from:

- the effective time of the next eligible NAVDB candidate
- whether an already-effective eligible candidate differs from the attached
  generation
- the Web publication refresh deadline

Both platforms arm a timer from that field and call the same core maintenance
operation with the current wall clock. On Android, "eligible" means the
candidate is present in the generic installed-artifact inventory. Kotlin does
not identify NAVDB packages or compare cycles; it rescans installed artifacts
and runs the existing core-owned open controller when core requests an
advance.

Web polls `current_artifacts.json` every four hours while the session is
alive. The poll is a typed core resource request with cache revalidation, not a
second TypeScript publication parser. Ingesting a refreshed publication drops
bundle manifests no longer referenced by `current_artifacts.json`, pages any
new bundle manifests through the ordinary resource pump, and then lets core
select a candidate. Discovery does not bypass validity: a future candidate
schedules maintenance for its effective timestamp and remains inactive until
then. If an advance commits, the shared Web NAVKV generation is swapped before
`nav_data` invalidations are published; the old WASM handle is destroyed after
its in-flight readers drain.

A hidden or suspended platform may deliver the timer late. Maintenance uses
the supplied absolute clock, so resuming after a cycle boundary immediately
selects the currently effective candidate rather than replaying missed ticks.

## Platform Cache Contract

Every NAV-derived query and cache entry is associated with `nav_data_epoch`.
The epoch is core-owned and advances only on successful commit.

Core returns a broad `UiInvalidation::NavData` on commit. Existing targeted
invalidations still describe what should be repainted, but `NavData` is the
generation boundary and must not be replaced by a hand-maintained list of
platform cache pokes.

Both platforms must:

- cancel or supersede queued NAV-dependent work from the old epoch
- refuse to land old-epoch asynchronous results
- key NAV-derived decoded/render caches by epoch
- clear old raster, vector, route, overlay, search, inspector, plate, and
  document-resolution entries
- release old network/image URL cache entries after old readers drain

Web currently does not create blob/object URLs in `ui/web-app/src`, but any
future object URLs must be owned by a NAV-data generation and revoked when that
generation retires. Ordinary resolved image URLs and browser-side image caches
must still include or be invalidated by the epoch because their resolution can
come from NAVDB metadata.

Android and Web consume the same epoch and invalidation semantics. Platform
code may differ in cache mechanics, not in dependency policy.

## Core API Direction

Replace direct session attachment as a product operation with an explicit
advance controller/outcome:

1. create an advance from a session and generic artifact inventory
2. step the candidate transaction
3. return typed resource requests when pages are needed
4. ingest candidate resources
5. return either a complete commit result or a rejected-candidate result

The core transaction owns the candidate session and candidate NAVKV. Raw
`attach_nav_kv_store_to_session` may remain as an internal construction helper,
but platforms must not compose a NAVDB advance from attach, package-ID update,
catalog reload, and snapshot calls.

The existing flight-plan commit path, which clones a session, builds guidance,
projects a snapshot, and commits only on success, is the architectural model.

## Test Artifacts

The immutable production artifacts are captured in the test-artifacts commit
pinned by `test-artifacts.lock.json`. Their NAVDB contract must match the
client's single supported contract. The regression opens those exact ZIP bytes
through the production NAVKV reader and pages them to completion; it does not
depend on a publication server or wall clock.

```text
nav-db/advance-2608-to-2609/
  README.md
  fixture.json
  source/current_artifacts.json
  source/packaged/bundle_cycle_2608_01_....json
  source/packaged/bundle_cycle_2609_01_....json
  source/packaged/nav_db_<contract>_2608_01_....zip
  source/packaged/nav_db_<contract>_2609_01_....zip
```

`fixture.json` records the exact source publication identity, filenames,
hashes, byte sizes, cycles, and contracts. The real-artifact regression
qualifies both NAVDBs through production HAD APIs and verifies:

- a rich route and approach procedure present in both cycles
- stable airports, navaids, fixes, airways, charts, and airport-document
  records used by the positive E2E scenario
- expected package hashes and NAVDB contract IDs

The qualified scenario is `KRNT SEA KPAE` with `KPAE VOR-A ECEPO`; it exercises
ordinary waypoints, procedure geometry, an arc, a hold, active guidance, a
selected plate, and raster-family preservation.

## Regression Coverage

`real_nav_db_2608_to_2609_advance_preserves_rich_session` drives the production
session transaction and real paged NAVKV artifacts. Set
`AEROBAG_TEST_ARTIFACTS_ROOT` (or `AEROBAG_TEST_ARTIFACTS`) when the sibling
fixture repository is not discoverable automatically.

The test constructs the rich plan on 2608, selects its VOR-A plate, activates
the procedure hold, loads the raster catalog and guidance, then advances the
same live session to 2609. It asserts exact flight-plan/guidance preservation,
fresh procedure arc geometry, active route projection, selected-family
preservation, candidate identity, and a single epoch increment.

Focused Rust tests cover atomic commit, side-effect-free page faults, rejection
of a missing required `NavRef`, old-artifact pinning, warning/reload semantics,
and epoch behavior. Android tests cover candidate rejection and old-generation
reader availability while candidate ZIP I/O is deliberately blocked. Platform
generation keys and component lifetimes mechanically prevent delayed old map,
raster, route, and terrain results from landing after a commit.

## Implemented Shape

1. Core clones the live session, attaches the candidate only to the clone,
   rematerializes waypoint, airway, and procedure dependencies through the
   production HAD paths, rebuilds guidance and UI projections, then commits in
   one session critical section.
2. Candidate page faults are side-effect free and batched by the existing
   paged-operation contract.
3. Android sync is ordered as fetch/install, runtime adoption, then GC. The old
   artifact is retained on rejection and old readers drain before its NAVKV
   handle is destroyed.
4. A successful change increments `nav_data_epoch` once and emits `NavData`
   plus targeted invalidations. Android and Web remount generation-owned map
   workers and reject late old-epoch results.
5. A rejected candidate leaves the live generation intact and emits the
   non-hushable reload warning and core-owned action described above.
6. Synthetic tests cover commit, paging atomicity, missing required plan data,
   rejection/lease behavior, and concurrent old-generation reads during
   candidate I/O. The production-byte test covers the rich 2608-to-2609 case.
7. Core emits a NAVDB maintenance deadline. Android attempts an installed
   candidate at cycle turnover, and Web periodically refreshes publication
   metadata before running the same candidate transaction.

## Completion Criteria

- Core owns candidate selection, validation, adoption, warning policy, and
  package leases.
- Platforms contain no NAVDB-specific swap policy.
- A rich active session advances from the real 2608 fixture to the real 2609
  fixture without losing or mixing state.
- An incompatible live session rejects the candidate without changing visible
  or navigational state.
- Web and Android consume the same epoch/invalidation contract.
- Old NAVDB packages cannot be GC'd while a live or draining generation uses
  them.
- The existing Android-only attach/reload workaround is removed rather than
  retained as a compatibility path.
