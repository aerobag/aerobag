# NOTAM Records v9

Version 9 adds `airport_aliases`, a sorted set of authoritative airport IDs,
alongside the canonical `airport_id`. An alias cannot equal the canonical ID,
be empty or noncanonical, or exist without an airport association. It identifies
the same airport, not a navaid with a coincidentally matching identifier.

The publication-wide airport catalog follows explicit alias edges across
cycles. For example, cycle 2609 contains
canonical `FLT`; cycle 2610 contains canonical `PAFT` and alias `FLT -> PAFT`.
The merged catalog retains `PAFT` and that alias. Alias chains are flattened;
conflicting targets and cycles are errors. Live delivery omits the affected
associations and reports critical telemetry rather than stopping other NOTAMs.
Strict producer/qualification validation rejects these defects.

The producer builds the reverse alias index once per catalog. A notice for
either identifier is published once with `airport_id: "PAFT"` and
`airport_aliases: ["FLT"]`. Canonical and prepared client indexes install that
record under both IDs, remove every old association on replacement, and remove
all associations on cancellation. Counters still count notices, not aliases.

The canonical JSON includes the alias field in its record hash. NOTAM records
contract 9, SQLite derived projection 15, and prepared display projection 5 must
be used together. The existing canonical-source rebuild replaces older derived
projections atomically; no source fetch or NAVDB rebuild is required. This change
requires no new NAVDB contract; per-cycle catalog schema 2 is unchanged. Both
generated live-feed/client contract inventories declare the new record version.

## Metadata Errors And Availability

Malformed IDs, invalid aliases, conflicts, and cycles are quarantined at the
association level, including aliases that depend on a rejected association.
Unrelated valid associations continue publishing. No name-based matching or
arbitrary first/last-wins conflict resolution is introduced. Unbound notices
remain in canonical source storage and the existing delivery accounting.

Operational [status v5](live-feed-status-v5.md) exposes `metadata_error_count`
and diagnostics even before the first publication. Pipeline Health alarms
critical on any nonzero count. Publishing the remaining records does not clear
the error. Catalog/projection identities fingerprint the actual usable catalog;
strict offline compatibility checks and explicit `--check-config` still reject
metadata errors rather than certifying a degraded candidate as clean.

Only unrecoverable initialization failures (no usable catalog, unreadable entire
publication, corrupt projection storage) disable NOTAM publication. They also
enter the existing `publication` failure lane with phase `build`, and readiness
remains false. There is no new failure lane.

The HTTP catalog and SSE bootstrap omit unavailable products, individual SSE
updates are filtered, and their version/state/delta/package requests return
503. Cached bytes are not deleted as part of failure containment. Other feeds
and the status endpoint remain available. Repairing an initialization failure
requires restarting the daemon; this is stated in its status error. Restarting
with repaired metadata recomputes the catalog and error gauge.

Fixture-free tests cover the real daemon process, HTTP/SSE isolation, corrupt
SQLite startup, alias-chain validation, both cycle identities, prepared
Postcard transfer, retargeting, cancellation, and pipeline-health reporting.
They also cover partial publication, retained unbound source records, restart
consistency, conflict-order independence, and alarms surviving publication success.
