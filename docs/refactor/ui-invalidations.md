# UI invalidations

Core owns the application model and the dependencies between derived UI queries.
When an operation changes core state that makes an already-rendered query stale,
the operation returns `UiInvalidation` values in its HAD completion outcome.

Platform and `*-ui` code may react to these invalidations by re-running the
affected render queries, but it must not infer feature-specific dependency rules
from publication paths, SSE payloads, package names, or other transport details.
Those are mechanisms; the invalidation decision belongs in core.

The current invalidation path is:

1. A core HAD session operation returns `HadOperationOutcome::Complete` with
   zero or more `UiInvalidation` values.
2. The platform operation runner surfaces those values to the `UiSession`
   invalidation listener.
3. The UI bumps per-invalidation revisions and uses those revisions as render
   query dependencies.

This is the intended path for asynchronous core-owned state changes, including
live-feed SSE updates, future live product swaps, and future cycle/package
changes. Do not add one-off "poke the NEXRAD layer" or "refresh METARs here"
paths in platform code.

Known older direct snapshot APIs still return plain snapshots. When those APIs
grow asynchronous resource dependencies or need to invalidate derived query
results, migrate them to HAD outcomes instead of adding a second notification
mechanism.
