# Live-feed operational status v4

Version 4 preserves all v3 measurements and requires NOTAM delivery accounting
in `products.notams.quality` after NOTAM publication:

- `source_record_count`: retained canonical source records, including notices
  not yet effective. Not an upstream completeness guarantee.
- `client_record_count`: records in the published NOTAM client projection.
- `server_only_records_by_keyword`: disjoint counts of source records absent
  from that projection, keyed by source keyword; `(none)` denotes no keyword.
  Its sum must equal source minus client. All values are nonnegative integers.
- `delivery_audit.tfr_version`: the latest successfully published TFR version
  examined by this daemon.
- `delivery_audit.tfr_overlap_count`: distinct excluded source IDs also carried
  with display text in that TFR state. Multiple polygons do not multiply notices.
- `delivery_audit.without_delivery_count`: excluded minus TFR overlap.
- `delivery_audit.error`: null for a completed audit, otherwise a diagnostic.
  Counts are null until both publication inputs are known, or if TFR decoding
  fails. An empty successfully decoded TFR state is known, not unavailable.

The NOTAM publisher captures excluded IDs in the same SQLite read transaction
as the counts. Those IDs remain private to the daemon. The audit refreshes on
either product's successful publication, including source-only NOTAM changes
that do not change the client version. A changed TFR state is decoded outside
the status mutex and its ID set is cached; status HTTP requests do no file I/O
or record decoding. Removing/replacing TFRs recomputes overlap rather than
accumulating historical matches. Failed reads never retain a previous version's
successful count as evidence for the new version.

These are delivery gauges, not proof that every notice's anchor resolves to an
actual UI control. Transport freshness and source failure alarms stay separate.
No client payload schema changes for these measurements.

Pipeline Health shows source/client counts, raw exclusions with seven category
series (AIRSPACE, No keyword, NAV, OBST, COM, SVC, Other), TFR overlap, and the
remaining gap. Unknown/new keywords contribute to Other, with their exact
counts retained in metric details. The series use existing bounded history and
five-minute buckets. Gauges are informational; invalid promised accounting is a
coverage warning, not zero. Older v2/v3 daemons can supply their existing raw
counts, but do not promise a TFR cross-check and show Not instrumented for it.

Like v3, this is an explicit operational envelope contract, not a migration into
the independently pinned product-facts telemetry adapter. Missing/invalid v4
measurements alarm; the monitor never guesses an older version from absent data.
