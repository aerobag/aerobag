# Deployment Compatibility Evidence

`aerobag-live-feedsd --describe-compatibility <product_artifacts.json>` prints a
schema-v1 requirements envelope and exits. It needs no listener, NMS config,
credentials, mutable state or upstream access. `wire_contracts` is this
executable's complete Rust-owned compiled inventory. `notam_catalog` contains
the loaded union's `schema_version`, canonical `sha256`, and `airport_count`.
`startup_publication` contains the resolved manifest `path` and its exact-byte
`sha256`. Both object and array publication manifests use every listed bundle.
Unknown manifest/catalog schemas and invalid airport identifiers are rejected.

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
