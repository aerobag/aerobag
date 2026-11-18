# Deployment Compatibility Evidence

`aerobag-live-feedsd --describe-compatibility <product_artifacts.json>` prints a
schema-v1 requirements envelope and exits. It needs no listener, NMS config,
credentials, mutable state or upstream access. `wire_contracts` is this
executable's complete Rust-owned compiled inventory. `notam_catalog` contains
the loaded union's `schema_version`, canonical `sha256`, and `airport_count`.
`startup_publication` contains the resolved manifest `path` and its exact-byte
`sha256`. Both object and array publication manifests use every listed bundle.
Unknown manifest/catalog schemas and invalid airport identifiers are rejected.

Ordinary serving uses the same loader with per-association recovery: invalid
aliases, conflicts, and cycles are omitted while valid associations remain.
Bad catalog/bundle inputs can be omitted when other catalogs are usable. These
omissions are never treated as healthy: [operational status v5](../../../docs/contracts/live-feed-status-v5.md)
reports metadata errors and Pipeline Health alarms critical on any nonzero
count, even while NOTAM publication succeeds. Offline qualification and
`--check-config` stay strict. The runtime catalog identity always fingerprints
the actual retained associations. Unbound source notices remain on disk.

`GET /live-feeds/compatibility.json` on the direct daemon listener returns a
separate schema-v1 runtime envelope with `Cache-Control: no-cache, no-store`.
`HEAD` returns the same headers without a body. This is deployment evidence,
not an extension to the frozen status/telemetry schema.

Runtime fields:

- `schema_version`: runtime envelope version, currently 1.
- `executable_sha256`: SHA-256 of the actual running `/proc/self/exe` image.
- `release_tag`: startup `AEROBAG_RELEASE_TAG`, or null.
- `launch_instance_id`: startup `AEROBAG_RELEASE_LIVE_INSTANCE_ID`, or null.
- `process_instance_id`: fresh process UUID; different after automatic restart.
- `wire_contracts`: complete compiled inventory, identical to the offline CLI.
- `configured_products`: actual configured worker product IDs.
- `notam_catalog`: identity of the catalog object supplied to NOTAM projection,
  or null when NOTAM processing is not configured.
- `startup_publication`: startup resolved manifest path and exact-byte hash,
  or null when no catalog was loaded. Neither is reread on requests.
- `projection_ready`: true only after catalog-bound publication verification
  and acknowledgement, including a durable published-provenance write.
- `published_state_id`: verified NOTAM publication state ID, or null.
- `ready`: requires projection/publication readiness, loaded catalog/publication,
  configured NOTAM processing, release tag and launch-instance identity.

The launch controller supplies the two environment variables, pins the manifest
and every referenced NAVDB input, owns independent mutable roots, and retains
these inputs for restart. Editing files on disk does not reload an instance.
An executable digest, process ID or airport count alone never proves sharing.
Compare the complete contracts and catalog identity, product availability,
startup publication and current instance identity before activating a binding.

SQLite records derived catalog identity under `notam_catalog_identity_v1`.
The derived-state root also holds `published-provenance-v1.json`, bound to the
catalog, full inventory and published state ID. Unknown/mismatched provenance
invalidates cached NOTAM histories before the listener serves requests. When a
canonical baseline exists, startup rebuilds locally and queues a full product
without waiting for upstream events. Matching cached publications are verified
and can become ready on an unchanged update. Missing canonical baselines,
publication failures and provenance-write failures remain unready.

## PIREP source recovery

PIREPs normally use AWC's worldwide gzip/XML cache. HTTP success alone does
not establish source success: XML errors, missing data, inconsistent counts,
or malformed reports fail validation. The shared weather readers also reject
AWC XML errors for METARs and TAFs.

If the PIREP cache fails, the same worker queries AWC's
[`/api/data/pirep`](https://aviationweather.gov/data/api/) XML endpoint with the
whole-world bounding box `-90,-180,90,180`. It collects the preceding 90 complete
minutes at a fixed cutoff. A response containing 400 or more reports is treated
as potentially truncated and replaced by two smaller time-window queries.
One minute of requested overlap covers AWC's age rounding; half-open ownership
windows remove overlap without deduplicating genuine source records. XML text,
coordinate precision, report identities, and published wire contracts remain
unchanged. The current partial minute is collected on the next poll.

The fallback requires every leaf window to succeed. A failed request, invalid
response, exhausted budget, or still-capped one-minute window fails the update
and leaves the current publication untouched. HTTP 204 is a valid empty result;
an empty HTTP 200 body is an error. Valid empty snapshots include their fallback
timestamp in the version identity, so consecutive empty updates cannot collide
with an older immutable publication.

Bulk attempts have a 20-second transfer limit and use the fetcher's existing
three-attempt retry policy. API collection has a 120-second total deadline,
20-second per-request limit, at most 63 queries, and at least one second between
request starts. API requests have no internal retry. The normal worker schedule
retries a failed collection. Offline fixture mode never contacts the API.

The worker scratch directory retains individual API XML files under `input/api`
and URL, interval, HTTP status, and content hash records under
`meta/provenance/pireps/api`. `source-selection.json` in the latter's parent
records the selected source and bulk failure reason for a successful collection.
These are diagnostic files, not extensions to monitored status contracts.

For a manual live integration check, run from the repository root:

```sh
cargo run --manifest-path product/preprocessor/Cargo.toml \
  -p preprocessor-live-feeds --example pirep_source_probe -- /tmp/pirep-source-probe
```

The destination must not exist. The probe invokes the production builder and
file publisher locally, verifies an identical replay, and saves `result.json`
and source evidence. It does not start or modify a service. This uses live AWC
and belongs outside hermetic CI; deterministic collector and HTTP failure tests
run in ordinary CI. On 2026-11-18 at 19:18 UTC, the probe recovered 1,734 reports
from 15 API requests while the bulk cache returned `Error processing data`.

Deploy this code through a new release: pull the tested revision, run
`python3 tools/prod_manage.py --stage --watch`, and only after qualification
passes run `python3 tools/prod_manage.py --promote`. Staging gets its own live-feed
daemon, so it exercises the new collector even though client wire contracts did
not change. No client schema migration is required.
