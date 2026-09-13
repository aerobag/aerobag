<!-- SPDX-FileCopyrightText: 2026 Aerobag contributors -->
<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->

# Contract Exports

Regenerate both checked-in artifacts from the Rust owners:

```sh
cargo run --manifest-path crates/Cargo.toml -p product-contracts --bin export-contract-inventory -- --output-dir crates/product-contracts/contracts
```

With no arguments the exporter still prints the unchanged schema-1
`client-data-contracts.json`. Its NOTAM-only legacy live-feed section is **not**
sharing evidence. `--live-feed-compatibility` prints only the new inventory.

Verify without overwriting either checked-in artifact:

```sh
python3 tools/ci/check_generated_contract_inventories.py
```

This standalone check uses the locked shared-crate workspace, a temporary output
directory and an explicitly empty artifact-data directory. Both filenames are
required even if the generator exits successfully without producing them.
Missing, stale and unexpected generated outputs fail. Cheap preflight runs it
before UI generation; ordinary release qualification and hosted shared-crate CI
also run it. Shared Rust tests independently check both exported files.

## Live-feed Compatibility v1

`live-feed-compatibility.json` is exactly the serialized return value of
`product_contracts::live_feed_compatibility_descriptor()`:

- `schema_version`: descriptor schema, currently 1.
- `protocols`: named `WireFormatContract { schema_version, encoding }` entries.
- `products`: every `LIVE_FEED_PRODUCT_POLICIES` ID, each with `formats` (the
  same named wire-format entries) and `parameters` (named strings).

The descriptor contains no release, publication, process, availability or
readiness claims. The release builder must bind this artifact to its exact
client build. The release's own executable must calculate publication
requirements from every supported NAVDB catalog. A controller must never
invent requirements for a historical client missing this artifact.

`LiveFeedCompatibilityDescriptor::require_exact_match` rejects incomplete,
unknown or unequal evidence. `require_live_feed_compatibility` also checks both
validated catalog identities for exact equality. These library helpers only
check evidence equality; the caller must still establish actual configured
product availability, projection/publication readiness, immutable launch
inputs and process identity before routing. Policy defaults belong to the
deployment controller, not this contract inventory.

### Format Owners

- Discovery, SSE catalogs/current events, version manifests, record deltas and
  NAVKV deltas use `live_feeds::v3` types and schema constants.
- `sse_service_bulletins` pins `service_bulletins::EVENT` and the shared
  `BulletinHint::SCHEMA_VERSION`. Hint v1 is the existing JSON object containing
  `publisher` and `revision`, without an on-wire schema field. It is an optional
  SSE notification, not a weather product: its configuration, file availability
  and current revision do not gate sharing readiness or alter the product roster.
  The independently served bulletin document is not a daemon weather payload.
- METAR/TAF/PIREP/TFR producer schemas live in `product-contracts` and are used
  by their builders. METAR's snapshot schema is 4, distinct from its producer
  manifest/contract revision 9. Record keys and array/object shape come from
  the shared product policies used by producer and core.
- NOTAM checkpoint/delta envelope schema is 3; records and ordered mutation
  semantics are contract 7, consumed by the shared `notam-state` crate.
- Obstacles have separate manifest, tile and logical-layout versions. The
  obstacle builder uses the shared constants without changing existing bytes.
- NEXRAD manifest/tile metadata is passed to the Python encoder by its Rust
  launcher. Tiles use bounded-palette PNG with RGBA8 overflow. Offline profile
  names come from the shared v3 contract.
- Atmosphere manifests, protobuf tiles, array ordering and quantization use
  the existing shared atmosphere constants and types.
- NAVKV storage version comes directly from `had-nav-kv`; pages support raw
  or XZ data and install packages use the shared stored-ZIP/XZ member layout.

Previously implicit SSE framing, archive/compression and state-integrity
assumptions are explicitly pinned as v1 protocol entries. A required framing,
key, encoding, hash algorithm or payload-shape change must revise its owning
contract and regenerate the artifact. This metadata addition does not bump a
weather payload or the NAVDB contract. Exact equality is deliberately
conservative; it is not a guarantee against behavior bugs behind unchanged
contracts. Client-only prepared caches and upstream FAA source formats are
not client-visible producer wire contracts and are not included.

## NOTAM Catalog Identity

`NotamAirportCatalog::identity() -> Result<NotamCatalogIdentity, String>`
fingerprints the exact loaded object. `NotamAirportCatalog::union` validates
every input and deduplicates all cycles before hashing. File loading stays with
the publication/daemon owner and must use `NavKvDirectoryReader`.

The serialized identity is `{ schema_version, sha256, airport_count }`.
`schema_version` is the logical airport-catalog schema, currently 1; descriptor
v1 fixes the hashing algorithm. `sha256` is lowercase hex. Zero airports,
unsupported schemas and IDs other than nonempty uppercase ASCII letters and
digits are rejected, never normalized. Count is diagnostic, not proof.

Canonical bytes, in order:

1. ASCII `aerobag/notam-airport-catalog/v1` followed by a NUL byte.
2. Catalog schema as big-endian u32.
3. Sorted unique airport count as big-endian u64.
4. For each sorted ID: UTF-8 byte length as big-endian u64, then its bytes.

Changing this representation requires a new compatibility descriptor schema.
For schema 1 and IDs `1S5`, `KJFK`, `KSFO`, the SHA-256 is
`5dce8d7e0cf97d5de13dacc38f0d7c0ff3d66b54c01a881ad41d3dc31b934b08`.

Fixture-free tests verify the export, roster, independent schema/encoding/
parameter mutations, malformed evidence, catalog canonicalization and changes,
union, and different NAVDB bytes yielding identical catalog identity. Producer
tests also compare generated synthetic record-product bytes to the inventory
and check every actual core product driver. Separate METAR/TAF/PIREP/TFR tests
publish snapshots and deltas, check their manifests and SSE events against the
inventory, and install the actual compressed bytes through core. Topology tests
remove or extend every nested object and reject malformed scalar fields.
