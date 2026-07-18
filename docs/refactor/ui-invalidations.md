# UI invalidations

Core owns the application model and the dependencies between derived UI queries.
When an operation changes core state that makes an already-rendered query stale,
the operation returns `UiInvalidation` values in its HAD completion outcome.

Platform and `*-ui` code may react to these invalidations by re-running the
affected render queries, but it must not infer feature-specific dependency rules
from publication paths, SSE payloads, package names, or other transport details.
Those are mechanisms; the invalidation decision belongs in core.

The current invalidation path is:

1. A core session operation returns invalidations with either `Complete` or
   `NeedSnapshotResources`.
2. If snapshot projection needs pages after the mutation committed, the
   platform runner retains the invalidations while it loads those pages and
   resumes the generic snapshot operation.
3. On completion, the platform runner surfaces all retained invalidations to
   the `UiSession` invalidation listener.
4. The UI bumps per-invalidation revisions and uses those revisions as render
   query dependencies.

This is the intended path for asynchronous core-owned state changes, including
live-feed SSE updates, future live product swaps, and future cycle/package
changes. Do not add one-off "poke the NEXRAD layer" or "refresh METARs here"
paths in platform code.

All production snapshot APIs use this outcome path. Do not add a direct
snapshot API or a second notification mechanism.
