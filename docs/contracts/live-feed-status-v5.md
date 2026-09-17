# Live-feed Operational Status v5

Version 5 preserves every v4 measurement and adds required NOTAM metadata-error
accounting to `products.notams`, independent of `quality` and publication success:

- `metadata_error_count`: nonnegative integer; number of diagnostic entries in
  `metadata_errors`. This counts detected catalog/projection initialization
  problems, not affected notices or failed poll attempts.
- `metadata_errors`: array of nonempty diagnostic strings naming invalid IDs,
  aliases, conflicting targets, cycles, excluded catalog inputs, or an
  unrecoverable initialization error. No secrets or upstream payloads.

These fields are present from startup, including before the first publication.
Successful publications and unchanged-source heartbeats do not clear metadata
errors. The catalog is pinned for a process lifetime: restarting with repaired
metadata recomputes both the usable catalog and the errors. Simulation, which
does not consume a publication airport catalog, reports zero detected errors.

Pipeline Health renders `live_feed.notams.metadata_error_count` as a gauge and
alarms **critical immediately when the count is greater than zero**. Diagnostics
are retained in metric details. Zero is healthy; absent, invalid, or inconsistent
promised accounting is a coverage warning with a null value, never zero. Older
v2/v3/v4 producers show Not instrumented for this measurement. This operational
envelope remains separate from the pinned product-facts telemetry adapter.

Recoverable catalog defects omit only untrustworthy associations and their
dependent aliases; other NOTAMs continue publishing. Omitted records remain in
canonical source storage and the existing server-only delivery audit. An
unreadable catalog or bundle can be omitted when other catalogs remain usable.
Readiness/provenance describe the actual retained catalog, not the rejected
inputs. Strict offline qualification still rejects metadata errors.

This is not permission to serve incorrectly decoded data: an unreadable entire
publication, no usable airport catalog, or corrupt projection storage can still
prevent NOTAM publication. Other products remain available; publication freshness
and failure metrics distinguish this from partial, current delivery.

The daemon also emits these two fields for other product status entries, currently
with no recoverable metadata diagnostics. No client payload or NAVDB format is
changed by this operational status version.
