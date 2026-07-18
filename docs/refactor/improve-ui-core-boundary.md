# Improve UI/Core Boundary

Status: implemented.

## Problem

We hit the same two bugs twice:

- A Rust panic crossed a UI boundary and left the UI runtime in a permanently bad state.
- A session API returned a plain `UiSessionSnapshot` even though snapshot projection could require lazy HAD/nav-db pages.

The second bug created the first. Fixing individual entry points would only
leave more latent copies, so the boundary now makes an unpaged session snapshot
unrepresentable in production code.

## Contract

All exported UI/core calls must obey these rules:

- No exported core, WASM, FFI, or JNI-facing code may panic for recoverable conditions.
- Every production session operation that returns a snapshot returns
  `HadOperationOutcome`; there is no pure/local snapshot allowlist.
- `HadOperationOutcome::NeedResources` means the operation has not committed.
  Fetch the resources and retry that same operation.
- `HadOperationOutcome::NeedSnapshotResources` means the mutation has committed
  exactly once, but projecting its resulting snapshot needs resources. Fetch the
  resources and resume the generic snapshot operation. Never replay the
  mutation.
- UI platforms fetch resources and render snapshots. They do not participate in model mutation logic.

## Architecture

Core uses these shapes:

- `session_snapshot_outcome(session)` for read-only paged snapshot projection.
- `changed_session_snapshot_outcome_with_invalidations(...)` after a mutation
  commits. It advances the revision once, preserves invalidations, and reports
  `NeedSnapshotResources` if projection faults.
- Staged mutation helpers when the mutation itself can fault before commit. A
  pre-commit `NeedResources` outcome must leave the session unchanged.

Both platform runners implement the same state machine:

1. Run the requested operation.
2. On `NeedResources`, load pages and retry it.
3. On `NeedSnapshotResources`, retain invalidations, load pages, and replace the
   active continuation with the generic paged snapshot operation.
4. On `Complete`, publish the snapshot and accumulated invalidations.

## Implementation Checklist

- [x] Convert every production snapshot-producing session API to a paged outcome.
- [x] Distinguish pre-commit resource faults from post-commit projection faults.
- [x] Resume generic snapshot projection without replaying committed mutations.
- [x] Delete duplicate plain/paged WASM, FFI, JNI, TypeScript, and Kotlin APIs.
- [x] Delete web and Android plain-snapshot adapter escape hatches.
- [x] Add a regression proving a committed mutation survives a snapshot page
  fault and is not applied twice.
- [x] Add boundary tests that reject plain production session snapshots and
  platform adapters without the generic snapshot continuation.

## Prevention

The resistance to regression is structural rather than an allowlist:

- Production `session.rs` must not contain a function returning
  `AppResult<UiSessionSnapshot>`.
- Platform adapters must not expose a plain snapshot decoder/runner.
- Every platform paged runner must provide the generic snapshot-resume
  continuation.
- A new snapshot-producing operation therefore has one available boundary
  shape: `HadOperationOutcome`.
- Do not add `expect`, `unwrap`, `panic!`, `unreachable!`, or `todo!` in exported
  boundary code. Convert recoverable failures to the boundary error type.
