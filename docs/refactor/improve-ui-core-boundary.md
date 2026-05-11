# Improve UI/Core Boundary

## Problem

We hit the same two bugs twice:

- A Rust panic crossed a UI boundary and left the UI runtime in a permanently bad state.
- A session API returned a plain `UiSessionSnapshot` even though snapshot projection could require lazy HAD/nav-db pages.

The second bug created the first. The first fix made `performFlightPlanRowAction` paged; the next failure showed `insertWaypointAtFlightPlanRow` had the same shape. Then the paged retry loop exposed a third invariant: paged mutations must not commit side effects before all required resources are available.

## Contract

All exported UI/core calls must obey these rules:

- No exported core, WASM, FFI, or JNI-facing code may panic for recoverable conditions.
- Any operation that can touch HAD/nav-db must return `HadOperationOutcome`.
- `HadOperationOutcome::NeedResources` must be side-effect free. Retrying the same operation after fetching resources must apply the logical mutation exactly once.
- Plain `UiSessionSnapshot` exports are allowed only for operations classified as pure/local: they must not perform HAD reads during mutation or projection.
- UI platforms fetch resources and render snapshots. They do not participate in model mutation logic.

## Architecture

Use these helpers as the default shapes:

- `session_snapshot_outcome(session)` for read-only paged snapshot projection.
- `commit_session_flight_plan_with_snapshot_outcome(session, plan)` for flight-plan mutations whose resulting projected UI may need HAD pages.
- Platform wrappers (`runCoreHadSessionOperation` on web and `runPagedSessionOperationElement` on Android) for every paged session operation.

The current transactional helper clones `UiSession` and commits the candidate only after snapshot projection succeeds. That is intentionally conservative. If it becomes a performance problem, replace the broad clone with a narrower staged state object, but preserve the invariant that `NeedResources` commits nothing.

## Implementation Checklist

- Convert row-action mutations to paged outcomes.
- Convert row insert mutations to paged outcomes.
- Stage flight-plan mutations until snapshot projection succeeds.
- Recover poisoned session locks instead of panicking forever.
- Recover poisoned WASM/FFI handle stores instead of panicking.
- Add boundary tests that fail if new exported session APIs return plain snapshots without allowlisting.
- Add boundary tests that fail if boundary modules gain panic-prone calls outside tests.
- Add a retry/idempotency test for paged flight-plan mutation.

## Prevention

When adding a new UI/core API:

- If it can read nav-db/HAD directly or indirectly, return `HadOperationOutcome`.
- If it mutates session state and returns `HadOperationOutcome`, make `NeedResources` side-effect free.
- If it returns `UiSessionSnapshot`, add it to the pure snapshot allowlist with a reason.
- Do not add `expect`, `unwrap`, `panic!`, `unreachable!`, or `todo!` in exported boundary code. Convert to `AppError`, `String`, or `JsValue`.
