# Incremental NOTAM Publication Plan

## Status

The incremental contract, shared state implementation, SQLite journal and Merkle
projection, immutable publisher, immediate daemon wakeup, core routing, main-core
application, and Android immutable-resource acknowledgement path are implemented
under live-feed schema/NOTAM contract 3. The old 60-second NOTAM publication
coalescing and replace-everything client contract are gone; there is no legacy
fallback. Delta publication now switches `current.json`, emits SSE, and advances
the SQLite cursor before checkpoint compaction runs. Compaction may therefore do
full-state work without delaying delivery of the triggering delta.

This design supersedes the replace-everything NOTAM publication target in
`docs/NOTAMS.md`, but it does not change SWIM collection, raw-message
acknowledgement, or normalized NOTAM record semantics.

The external fixture is frozen at raw-ingest cursors `97623..98802`. It contains
1,179 real applied SWIM messages that production normalization turns into 906
published record mutations, including 213 removals and 266 identifiers mutated
more than once. Its golden final state ID is
`dc0646b17451b3a2f4639062598ec57f6f63fa10e9a619eb0cbdac87be498a9a`.

Ordinary CI currently covers golden hash vectors, incremental/full Merkle
equivalence, 256 randomized materialization schedules over synthetic mutations,
SQLite migration/journal replay, publication ordering and recovery, 100-mutation
checkpoint rotation, core routing/application/work counters, immutable Android
resource restoration, and both platform transport boundaries. The ignored
external-fixture acceptance test adds 114 checkpoint, catch-up, and
resynchronization schedules through production SQLite normalization, XZ/JSON,
Postcard, and main-core application. Allocation and exhaustive fault-injection
coverage remain separate follow-up work described below.

## Motivation

The current dev database contains approximately 20,000 current NOTAM records and
22 MiB of compact record JSON. A published pretty JSON state is approximately
24 MiB before compression and 1.5 MiB after XZ compression. Today a small source
change causes the producer to read, decode, serialize, hash, compress, and diff
the entire state.

The target design makes ordinary work proportional to changed records. Full-state
work happens only when creating a checkpoint.

## Goals

- Give each logical NOTAM state one deterministic, content-derived identity.
- Derive small immutable deltas directly from the SQLite projection transaction.
- Publish each committed logical update without a wall-clock batching delay.
- Let a current client fetch only the newest immutable delta.
- Let a lagging client replay a bounded suffix of immutable deltas.
- Let a new or too-old client start from one checkpoint and replay later deltas.
- Create a new checkpoint when replay since the checkpoint exceeds 100 record
  mutations.
- Recover deterministically from a crash at every database/filesystem boundary.
- Keep all update selection, verification, replay policy, and mutable NOTAM
  state in shared Rust core, not in Android or web platform code.

## Non-Goals

- Do not expose FAA XML, JMS details, or raw source messages to clients.
- Do not rewrite one growing delta-chain payload after every update.
- Do not publish a new full state for every delta.
- Do not use wall-clock checkpoint intervals as the primary compaction policy.
- Do not preserve the old live-feed contract as a fallback. Producer and clients
  roll to the new contract together.
- Do not convert every live-feed product to incremental Merkle publication in the
  first change. Implement the generic contract where appropriate, but prove it on
  NOTAMs first.

## Terms

- **Record mutation:** one ordered upsert or removal applied by a client. The
  replay cost of a delta is the length of its ordered mutation list.
- **State ID:** the deterministic Merkle root of the complete logical NOTAM state.
- **Checkpoint:** one materialized full state representing a particular state ID.
- **Delta:** an immutable transition from one state ID to another.
- **Recent delta suffix:** the ordered immutable deltas retained for lagging
  clients. Its replay cost is bounded by policy.
- **Head:** the newest published state ID.
- **Publication journal:** durable SQLite rows describing logical transitions not
  yet, or recently, published.

## State Identity

### Fixed Merkle Shape

The Merkle index is separate from the physical JSON layout. Published records may
remain a lexicographically sorted map.

Use a fixed three-level shape:

1. Compute `SHA256(UTF-8(notam_id))` and use its first 10 bits as a bucket number
   in `0..1024`.
2. Sort records within each bucket by NOTAM ID and hash the framed ID and leaf
   hash sequence.
3. Divide the 1,024 buckets into 32 fixed groups of 32 consecutive buckets.
4. Hash the 32 ordered group hashes plus state metadata to obtain the state ID.

This structure never rotates or rebalances. Hashing identifiers distributes the
current approximately 20,000 records to about 20 records per bucket on average.

### Canonical Hash Encoding

Implement the client-visible NOTAM record contract, hash rules, mutable state,
and mutation application once in a new shared `crates/notam-state` crate
consumed by the preprocessor and app core. Do not independently recreate record
types, framing, canonicalization, Merkle maintenance, or mutation semantics.
Source-only diagnostics may remain in producer types, but they are not part of
the published record or state identity.

Every input is length-framed or fixed-width. `frame(bytes)` means
`u64_be(bytes.len) || bytes`. Every level has a distinct ASCII domain tag so
values from different levels cannot be confused.

```text
leaf_hash = SHA256(
  "aerobag/notams/leaf/v1\0" ||
  frame(notam_id_utf8) ||
  frame(canonical_record_bytes)
)

bucket_hash = SHA256(
  "aerobag/notams/bucket/v1\0" ||
  u16_be(bucket_number) ||
  u32_be(record_count) ||
  concat(frame(notam_id_utf8) || leaf_hash, sorted by notam_id)
)

group_hash = SHA256(
  "aerobag/notams/group/v1\0" ||
  u8(group_number) ||
  concat(32 bucket_hash values in bucket order)
)

state_id = SHA256(
  "aerobag/notams/state/v1\0" ||
  u32_be(live_feed_schema_version) ||
  u32_be(notam_product_contract_version) ||
  u64_be(notam_count) ||
  u64_be(airport_notam_count) ||
  u64_be(airport_notams_with_multiple_effects) ||
  u64_be(airport_notams_with_other_effect) ||
  concat(32 group_hash values in group order)
)
```

`canonical_record_bytes` must use a shared deterministic encoding of the exact
published `NotamRecord`. It must not hash SQLite page bytes, incidental JSON
whitespace, map insertion order, source-only fields discarded by the client,
timestamps that are not part of the client state, or a derived version label.
Add checked-in golden vectors before using the encoding as a contract.

Use the full lowercase 64-character state ID in manifests and verification.
Filenames may contain that full ID; correctness must not depend on a truncated
display label.

The XZ checkpoint checksum and XZ delta checksum remain SHA-256 hashes of exact
transport bytes. They are blob identities, not state identities.

## SQLite Projection And Journal

### Projection Additions

Extend `current_notams` with:

```text
record_hash BLOB NOT NULL
merkle_bucket INTEGER NOT NULL
```

Add fixed Merkle tables:

```text
notam_merkle_buckets(bucket PRIMARY KEY, record_count, bucket_hash)
notam_merkle_groups(group_id PRIMARY KEY, group_hash)
```

Store aggregate counters and the current state ID in metadata. Initialize all
1,024 buckets and all 32 groups, including deterministic empty hashes.

### Durable Publication Journal

Add a parent transition table and exact child operations:

```text
notam_publication_journal
  journal_seq INTEGER PRIMARY KEY
  source_first_ingest_seq
  source_last_ingest_seq
  observed_at_utc
  from_state_id
  to_state_id
  notam_count
  airport_notam_count
  airport_notams_with_multiple_effects
  airport_notams_with_other_effect
  mutation_count

notam_publication_operations
  journal_seq
  operation_index
  notam_id
  operation               # upsert or remove
  record_json             # exact target record for upsert; null for remove
  PRIMARY KEY(journal_seq, operation_index)
  UNIQUE(journal_seq, notam_id)

notam_publication_cursor
  singleton key
  published_through_journal_seq
  published_head_state_id
```

The journal must retain exact target record JSON. Storing only changed IDs is
insufficient because a later source update could overwrite the row before an
older transition has been published or replayed after a crash.

### Transaction Rules

Applying committed raw messages must perform the following in one SQLite
transaction:

1. Capture the starting state ID and aggregate counters.
2. Apply all source rows in the existing raw-message batch.
3. Remember the original and final value for every touched NOTAM ID.
4. Omit IDs whose final value equals their value at transaction start.
5. Update record hashes and bucket assignments for net changes.
6. Recompute only affected buckets, affected groups, and the root.
7. Update aggregate counters from old/new record contributions.
8. Sort the net operations lexicographically by NOTAM ID and append one journal
   transition with that exact operation order when the root changed.
9. Advance the raw ingest cursor and commit.

Multiple changes to one ID in the transaction collapse to its final net
operation. Upsert followed by removal becomes one removal; removal followed by
upsert becomes one upsert. The remaining operations are stored in one canonical
order, not split into independently ordered upsert and removal collections. A
transaction with no net logical change creates no publication transition.

Rejected messages do not affect the root. Repaired quarantined messages,
cancellations, schema reprojection, and every other path that mutates
`current_notams` must use the same projection mutation helper. A schema migration
that changes canonical record content must roll the live-feed contract and
bootstrap a new checkpoint rather than silently continuing the old identity
lineage.

### Existing Database Migration

Migration performs one consistent full scan:

1. Canonicalize and hash every current record.
2. Assign records to buckets.
3. Build all bucket and group hashes.
4. Compute aggregate counters and the root.
5. Insert a bootstrap journal/checkpoint marker.
6. Verify a second full recomputation equals the stored root before enabling
   incremental publication.

Migration is transactional. A failure leaves the pre-migration schema intact
and does not leave a partially initialized Merkle index.

## Incremental Publication

### Wakeup And Work Selection

After a transaction commits a journal row, notify the publisher immediately.
Remove the 60-second NOTAM publication interval. On daemon startup and after any
notification, inspect the journal for rows after the publication cursor.

Normally publish one immutable delta per committed logical transition. If the
publisher falls behind, it may combine currently unpublished consecutive journal
rows into one net delta, provided that:

- the combined `from_state_id` equals the published head;
- the combined `to_state_id` equals the final journal root;
- operations are collapsed in journal order to their final net value and the
  resulting mutation list is sorted into the canonical NOTAM-ID order;
- replay cost counts the operations actually carried by the combined delta; and
- the original journal rows are not marked published until the combined delta is
  current.

This is backpressure recovery, not a time-based batching policy.

### Delta Payload

Each delta is an immutable, canonically encoded payload containing:

```text
schema_version
product = "notams"
from_state_id
to_state_id
source sequence/timestamp facts
target aggregate counters
mutations: ordered list<NotamMutation>
mutation_count
```

`NotamMutation` is a tagged enum with exactly two forms:

```text
upsert { record: NotamRecord }
remove { notam_id }
```

The mutation list order is part of the transport contract. Mutations are
strictly ordered by NOTAM ID with no duplicate ID, making the encoding canonical
and unambiguous, and core applies it from first to last. Do not represent a delta
as a map of changed records plus a separate list of removals; that representation
does not specify their relative application order.

Write it to a content-addressed path containing the from/to state IDs. Compress
it with the production XZ encoder. Its manifest reference includes exact byte
length and blob SHA-256.

Never rewrite a prior delta and never rewrite one growing chain blob. An
up-to-date client receives and fetches only the newest delta.

### Publication Ordering

For an ordinary transition:

1. Read and validate consecutive unpublished journal rows.
2. Build the immutable delta.
3. Write, fsync, and atomically promote the delta blob.
4. Write and atomically promote an immutable head manifest referencing the
   checkpoint and recent individual deltas.
5. Atomically update `current.json` to the new head manifest.
6. Emit the SSE invalidation.
7. Advance the SQLite publication cursor.

The filesystem and SQLite cannot share one transaction, so ordering and
idempotence provide recovery. If `current.json` already names the journal target
after a restart, validate all hashes, re-emit the idempotent SSE invalidation,
and advance the cursor without emitting a different transition. Duplicate SSE
invalidations are harmless; silently missing the invalidation between the
`current.json` switch and cursor advancement is not. If SQLite and the published
chain do not join at the same state ID, fail loudly; do not guess or fall back.

SSE remains an invalidation mechanism. It names the new head manifest. A current
client fetches that small manifest, sees that the newest delta starts at its
installed state ID, and fetches only that delta.

## Checkpoints And Retention

### Compaction Policy

Track replay cost from the current checkpoint to the head as the sum of mutation
list lengths for those deltas. When it reaches or exceeds 100 record mutations,
schedule checkpoint generation immediately.

Do not delay publishing a delta while the checkpoint is built. The chain may
temporarily exceed the target under load.

Checkpoint generation:

1. Open a consistent SQLite read transaction and capture its state ID and
   corresponding journal sequence.
2. Export all records in lexicographic ID order with aggregate counters.
3. Fully recompute the Merkle root from exported content and require it to equal
   the captured state ID.
4. Write and fsync the full XZ state and its immutable checkpoint manifest.
5. Require the retained/pending journal to provide a contiguous chain from the
   previously published head through the captured journal sequence.
6. Atomically publish a head manifest using the new checkpoint, the useful
   recent suffix through that checkpoint, and any deltas committed after the
   captured state.

If a single source transition exceeds 100 mutations, keep it atomic, publish it,
and checkpoint afterward.

### Recent Suffix

The head manifest advertises:

- the current head state ID;
- one checkpoint reference;
- an ordered recent suffix of individual immutable delta references; and
- source/publish timestamps used by health monitoring.

Keep at most approximately 100 record mutations ending at the current head,
including useful deltas before the checkpoint. This allows a recently lagging
client to catch up directly even after checkpoint rotation. A newcomer starts at
the checkpoint and applies only deltas after that checkpoint.

The 100-mutation limit is a replay-work target. One atomic delta may exceed it.
Trim only at delta boundaries.

Do not initially add aggregate delta-chain blobs. If measurements show that many
small HTTP requests materially hurt catch-up, a later contract may advertise
additional immutable sealed segments. Individual deltas remain canonical so a
current client is never forced to download an old segment.

### Garbage Collection

Publication GC traces the current head manifest, checkpoint, and advertised
deltas. It also keeps superseded manifests/blobs for a short publication grace
period so a client that fetched the previous `current.json` can finish its
requests. After grace, unreferenced checkpoints, deltas, and manifests are
deleted.

Journal rows may be pruned only after their immutable delta is safely outside
all crash-recovery and publication-grace requirements. The current projection
and Merkle tables are not journal history and must never be pruned.

## Live-Feed Contract

Roll the live-feed contract/schema and base path. Define the global live-feed
schema version and NOTAM product contract version in `crates/product-contracts`
so producer and core consume the same constants. Producer and core accept only
the new exact versions.

`current.json` should stay small and identify each product's head and immutable
head manifest. The NOTAM head manifest contains the checkpoint and ordered delta
references. Other products may initially represent their current full state as
a checkpoint at the head.

Conceptual NOTAM head manifest:

```json
{
  "schema_version": 3,
  "product": "notams",
  "head_state_id": "<64 hex>",
  "checkpoint": {
    "state_id": "<64 hex>",
    "url": "states/notams/<state-id>.json.xz",
    "bytes": 123,
    "blob_sha256": "<64 hex>"
  },
  "recent_deltas": [
    {
      "from_state_id": "<64 hex>",
      "to_state_id": "<64 hex>",
      "mutation_count": 1,
      "url": "deltas/notams/<from>__<to>.json.xz",
      "bytes": 123,
      "blob_sha256": "<64 hex>"
    }
  ]
}
```

The chain must be ordered and contiguous. The checkpoint state ID may appear
inside the recent suffix rather than at its beginning because older deltas may
be retained for lagging clients.

## Core Behavior

Both the background worker and the application main thread execute shared Rust
core code on web and Android. In this section, **background core** means the Rust
instance responsible for SSE, fetches, checksum verification, and network JSON
decoding. **Main core** means the Rust instance that owns the app session and
serves NOTAM queries. Web and Android platform code only transports bytes,
wakeups, prepared messages, and acknowledgements between those two instances.

### Single Materialized State

Main core owns the only long-lived materialized client state:

```text
NotamState
  state_id
  records: canonical map<notam_id, NotamRecord>
  by_airport: map<airport_id, ordered notam_id references>
  merkle: fixed bucket/group index
  aggregate counters
```

Secondary indexes hold IDs or compact sort keys, not duplicate full NOTAM
records. `NotamState` provides the app-facing airport and record queries and
updates only index entries affected by each mutation.

Background core does not maintain another `NotamState`, record map, airport
index, or Merkle tree. It holds only temporary decoded checkpoint/delta data and
the state ID most recently acknowledged by main core. Therefore an ordinary
update parses network JSON off the main thread but does not rebuild, clone,
serialize, or compare the full state there.

### Prepared Main-Thread Messages

Background core converts verified network JSON into one of two postcard
messages using the shared `NotamRecord` and `NotamMutation` types:

```text
InstallNotamCheckpoint
  expected_to_state_id
  ordered_records: list<NotamRecord>
  aggregate counters

ApplyNotamMutations
  expected_from_state_id
  expected_to_state_id
  ordered_mutations: list<NotamMutation>
  target aggregate counters
```

Checkpoint records are ordered lexicographically by NOTAM ID. Delta mutations
retain the exact canonical order published by the server. The ordinary delta
message is proportional to the changed records; it never contains a materialized
full state. Background core requires the state IDs inside a delta blob to match
the immutable manifest reference, then passes the server-declared target ID
unchanged to main core. It does not substitute a locally predicted identity.

Prepared messages are delivered one at a time. Background core does not deliver
the next transition until main core acknowledges the prior one. It may fetch
independent immutable blobs concurrently, but it decodes and sends transitions
in chain order. A newer SSE invalidation arriving during application records the
newest desired head and is routed only after the current acknowledgement.

### Main-Core Application

The shared `NotamState::apply_mutation` implementation performs one ordered
operation at a time and updates the canonical record collection, affected
app-facing indexes, affected Merkle bucket/group hashes, aggregate counters, and
current root. Main core uses that exact implementation for every delta.

For `ApplyNotamMutations`:

1. Require the current state ID to equal `expected_from_state_id` before any
   mutation. A stale transition is rejected without changing the known-good
   state, and the acknowledgement includes the actual state ID so background
   core can route from that exact state.
2. Apply mutations strictly from first to last on the main thread. Session
   queries cannot interleave with the update.
3. After the last mutation, require the computed Merkle root and aggregate
   counters to equal the expected target values delivered from the server.
4. On success, retain the state and acknowledge the exact resulting state ID.
5. On malformed mutation, index invariant failure, counter mismatch, or final
   Merkle mismatch, emit a high-severity structured diagnostic, discard the
   entire `NotamState`, and acknowledge `NoState`.

The diagnostic records product/contract version, blob identity, expected base
and target IDs, computed ID when available, mutation count, and failure class.
It contains no NOTAM text. It enters the existing client diagnostic path so an
opted-in client may upload it rather than leaving the failure only in a local
console.

Discard-on-failure intentionally avoids a second full copy or a rollback log.
The app cannot query a partially applied state because application is serialized
on the main thread, and any failed postcondition removes the state before the
event returns.

For `InstallNotamCheckpoint`, main core starts with a new empty `NotamState`,
inserts the ordered records through the same mutation implementation, and
installs it only after its computed state ID and counters match the checkpoint.
This is the only full-state postcard path. It is used on initial startup and
explicit resynchronization, not for ordinary updates.

### Routing And Resynchronization

Background core owns route selection but bases it only on the state ID
acknowledged by main core:

1. If the acknowledged state ID equals the head, do nothing.
2. If it is a `from_state_id` in the recent suffix, fetch all later required
   individual deltas. Fetches may run concurrently; delivery remains ordered.
3. If main reports `NoState`, or its state ID is outside the retained suffix,
   fetch the checkpoint followed by deltas after the checkpoint state ID.
4. Verify every blob checksum before decoding and every chain join before
   preparing a main-thread message.
5. After checkpoint installation, continue only from the state ID acknowledged
   by main, never from an identity merely assumed by background core.

A computed-root mismatch should never occur outside deliberately corrupt tests.
The resynchronization path is nevertheless explicit: main discards the suspect
state, background core fetches the current checkpoint, and normal ordered replay
resumes after the checkpoint acknowledgement. Repeated failure of the same
checkpoint or transition is a hard reported error, not an infinite retry or a
legacy-format fallback.

Android persistence must not serialize the full materialized state after every
mutation. Before delivery, its durable cache stores the already downloaded,
checksum-verified immutable checkpoint/delta blobs. After a successful main-core
acknowledgement, it atomically advances only small installed-head metadata. On
restart, background core replays those durable resources through the same
checkpoint/mutation messages, and treats no state ID as installed until main
core acknowledges it. Web and Android do not implement product-specific replay
logic outside shared Rust.

## Implementation Areas

- `product/preprocessor/preprocessor-live-feeds/src/notam_store.rs`
  - schema migration, canonical record hashing, Merkle maintenance, journal,
    cursor, and consistent checkpoint reads
- `product/preprocessor/preprocessor-live-feeds/src/products.rs`
  - incremental NOTAM delta and checkpoint builders
- `product/preprocessor/preprocessor-live-feeds/src/engine.rs`
  - new contract structures, immutable delta/checkpoint publisher, retention,
    and GC rooting
- `product/preprocessor/live-feeds-daemon/src/main.rs`
  - immediate journal wakeup, restart recovery, checkpoint scheduling, health
    facts, and removal of the 60-second coalescing mitigation
- `crates/notam-state`
  - exact published record and mutation types, canonical leaf encoding, fixed
    Merkle implementation, main-core `NotamState`, app-facing secondary indexes,
    and ordered mutation application
- `ui/core-rust/crates/app-core/src/live_feeds.rs`
  - background-core contract parsing and route selection, prepared postcard
    messages, acknowledgement protocol, main-core installation, and structured
    mismatch diagnostics
- `ui/core-rust/crates/app-core/src/live_feed_cache.rs`
  - durable immutable checkpoint/delta storage and small acknowledged-head
    metadata; no per-update full-state serialization

## Captured Large Fixture

Use a real NMS Initial Load plus poll trace for schedule-invariance and
work-accounting tests. Keep it in the separate `aerobag-test-artifacts`
repository under `notams/nms-api-trace/`; do not add it to the source
repository.

The fixture starts with the raw compressed DOMESTIC and FDC Initial Load
responses captured on 2026-07-24. It then records 498 poll boundaries and the
580 unique raw AIXM updates first observed during those polls through
2026-07-25. Empty polls and their completion times are retained because expiry
behavior depends on time. Source receive counts are diagnostic metadata; the
trace intentionally omits repeated overlap payloads already represented by
their content hash.

The fixture contains no API URL, OAuth credential, access token, local source
path, or retired SWIM data. A manifest records source environment, timestamps,
record counts, byte counts, and SHA-256 hashes. The test parses the raw NMS
Initial Load, replays every update through the production NMS collector,
synchronizes source-state changes through the publication store, and builds
every client checkpoint and delta using production code.

The repeatable capture command is:

```bash
(cd product/preprocessor && cargo run -p nms-notams-fetch -- \
  capture-fixture \
  --initial-load /path/to/nms-initial-load-capture \
  --state-root /path/to/nms-collector-state \
  --output /path/to/aerobag-test-artifacts/notams/nms-api-trace \
  --captured-by-commit "$(git rev-parse HEAD)")
```

Capture reads the collector SQLite state read-only, validates every retained
payload hash, writes into a temporary directory, and publishes only by atomic
rename to a previously nonexistent output path.

## Tests

### Hash Contract Unit Tests

- Checked-in golden vectors for canonical record bytes, ID bucket assignment,
  leaf hash, empty/nonempty bucket hash, group hash, and root state ID.
- Producer and core run the same vectors through the shared implementation.
- Record insertion order does not affect bucket hashes or the state ID.
- Two records intentionally assigned to the same bucket remain deterministic
  when inserted in opposite orders.
- Changing a logical record field changes its leaf and state ID.
- Changing irrelevant JSON whitespace or object insertion order does not change
  its canonical bytes or state ID.
- Insert followed by removal restores the original root.
- Upsert followed by restoration of the original record restores the original
  root.
- Contract/schema version and each aggregate counter are committed by the root.
- Domain separation prevents a leaf byte sequence from being accepted as a
  bucket/group/root sequence.

### Incremental Merkle Property Tests

- Generate deterministic randomized insert/update/remove sequences and compare
  the incrementally maintained root with a full recomputation after every step.
- Include empty state, one record, all records in one bucket, changes spanning
  many buckets/groups, repeated no-ops, and cancellation/removal transitions.
- Run long sequences with periodic full recomputation to detect accumulated
  index drift.
- Verify only affected buckets and groups change for a single-record mutation.

### SQLite Migration And Transaction Tests

- Migrate a representative pre-Merkle database and compare its stored root with
  a full recomputation.
- Interrupt/fail migration and verify no partially initialized schema is
  committed.
- Verify all 1,024 buckets and 32 groups have deterministic initialized hashes.
- Apply one insert, update, and removal and verify projection, counters, Merkle
  tables, metadata root, raw cursor, and journal commit atomically.
- Roll back after each internal mutation stage and verify none of those surfaces
  advances independently.
- Collapse multiple changes to one ID to the final net operation.
- Omit a net no-op transaction from the publication journal.
- Preserve exact historical upsert JSON in the journal when the current row is
  changed again before publication.
- Verify duplicate raw source messages do not create duplicate logical deltas.
- Verify rejected messages do not change the root or create a transition.
- Verify repaired quarantined messages and cancellations use the same journaled
  mutation path.
- Verify aggregate counters update correctly for old/new airport-effect
  contributions without scanning the whole table.
- Verify subscription identity mismatch and store locking retain their current
  failure behavior.

### Delta Construction Tests

- Build a delta directly from one journal transition without reading all
  `current_notams` rows.
- Combine a backlog of consecutive transitions and verify last-write-wins net
  operations and exact first/last state IDs.
- Reject a backlog with a state-ID gap or branch.
- Verify the mutation list is strictly ordered by NOTAM ID, contains no duplicate
  ID, and includes upserts and removals in one sequence.
- Verify mutation count equals the ordered mutation-list length.
- Verify canonical delta bytes and blob hashes are deterministic.
- Apply each produced delta to a reference state and compare exact logical state,
  aggregate counters, and Merkle root with the SQLite projection.
- Verify an atomic delta larger than the 100-mutation target remains one valid
  transition and requests checkpointing afterward.

### Publisher And Fault-Injection Tests

- One committed logical update wakes publication without waiting 60 seconds.
- A current client update creates one new immutable delta, not a new full-state
  payload and not a rewritten chain blob.
- Re-publishing an existing from/to transition verifies and reuses identical
  bytes.
- Inject failure after journal commit but before delta write; restart publishes
  the missing delta.
- Inject failure after delta promotion but before head manifest; restart reuses
  the delta and finishes publication.
- Inject failure after head manifest but before `current.json`; restart finishes
  the atomic current switch.
- Inject failure after `current.json` but before SQLite cursor advancement;
  restart validates the published head and advances the cursor without creating
  another version.
- Inject failure after cursor advancement and verify published files were already
  durable and current.
- Reject any startup where the published head and unpublished journal do not
  form one exact chain.
- Emit SSE only after all referenced files and `current.json` are visible.
- Emit no SSE event for a net no-op source transaction.

### Checkpoint And Retention Tests

- Trigger checkpointing at 100 replayed record mutations, not at 100 files or a
  wall-clock interval.
- Permit a temporary overage while checkpoint generation runs.
- Build a checkpoint from a consistent read snapshot while later mutations
  commit concurrently; the new manifest includes post-snapshot deltas.
- Fully recompute the checkpoint Merkle root and reject publication on mismatch.
- Rotate a checkpoint without changing the checkpoint state's existing state ID.
- Keep useful pre-checkpoint deltas in the recent suffix for lagging clients.
- Trim the suffix at delta boundaries to approximately 100 replay mutations.
- Retain one oversized atomic delta even when it alone exceeds the target.
- GC retains every file referenced by the current head manifest.
- GC grace retains files referenced by the immediately superseded publication.
- GC removes old unreferenced checkpoints, deltas, manifests, and safely prunable
  journal history after grace.

### Core Routing And Verification Tests

- A client already at the head requests nothing.
- A current client requests exactly the one new delta, never the checkpoint or
  the preceding suffix.
- A client several transitions behind requests the matching suffix.
- Required delta blobs may be fetched concurrently but are applied in chain
  order.
- A client absent from the suffix requests the checkpoint and only
  post-checkpoint deltas.
- A client whose state precedes a checkpoint but remains in the retained suffix
  catches up directly without downloading the checkpoint.
- A checkpoint-only head installs correctly.
- Reject malformed ordering, gaps, duplicate transitions, branches, and a head
  not reachable from the checkpoint.
- Reject blob checksum mismatch before decoding.
- Reject a noncanonical or duplicate-ID mutation list before mutation.
- Reject delta `from_state_id` mismatch before mutation, preserve the known-good
  state, and return its actual ID to background core.
- Apply every list one mutation at a time and update records, affected airport
  indexes, counters, and Merkle nodes through the same shared method.
- On a final Merkle or counter mismatch, discard the materialized state before
  returning, emit a structured high-severity diagnostic, and acknowledge
  `NoState`.
- Install a checkpoint by applying its ordered records to a fresh empty state;
  do not expose or install it before final identity verification succeeds.
- Ensure airport queries return exact expected results after inserts, record
  updates, airport moves, and removals without duplicating full records in the
  secondary index.
- Background core sends one prepared message at a time and advances its
  acknowledged state ID only from main-core acknowledgements.
- An SSE invalidation arriving while a message is in flight routes from the
  eventual acknowledged state, not the state background core expected.
- A normal delta postcard contains only its mutation records and fixed metadata;
  no untouched record or full-state payload is serialized.
- A `NoState` acknowledgement fetches and installs the current checkpoint, then
  applies later deltas in order.
- Repeated rejection of the same checkpoint or transition becomes a hard visible
  error instead of an unbounded retry loop.
- Android persists immutable resource blobs plus small acknowledged-head
  metadata, not a freshly serialized full state after each delta.
- Restart from the durable Android resources, reconstruct and verify main-core
  state, then continue a delta chain.
- Exercise the same routing through web session resources without platform-owned
  chain logic.
- Run the background-core/main-core message and acknowledgement protocol through
  both web-worker and Android transport adapters.
- Parse only the rolled contract version; old/new mismatches fail explicitly.

### Materialization-Schedule Invariance

Turn the captured trace into one canonical sequence of ordered logical
mutations. Build an independent reference oracle with a simple sorted record map
and a full Merkle recomputation at every logical boundary. Check in the expected
initial and final state IDs as golden values so producer and client cannot agree
on the same accidental new hash contract unnoticed.

Generate many deterministic paths from the initial boundary to the same final
boundary. A path is an ordered mixture of:

- a materialized checkpoint at a chosen boundary;
- one delta per logical transition;
- a delta combining several consecutive transitions according to the specified
  backlog-collapse rule; and
- checkpoint replacement followed by later deltas.

Include fixed schedules with no intermediate checkpoint, checkpoints every 7,
31, and 100 mutations, and checkpoints immediately before/after repeated-ID and
removal events. Exercise checkpoint-after-every-mutation on a focused real trace
window rather than pointlessly serializing the 24 MiB state thousands of times.
Also generate at least 256 seeded-random schedules over the full large trace,
varying checkpoint boundaries, combined-delta boundaries, client starting
points, and retained-suffix routes. Print the seed and path on failure so the
case is exactly reproducible.

Generate all schedules first and memoize each unique checkpoint boundary and
delta span. Every artifact is still built by production code, but two paths that
request the same immutable artifact reuse it as they would in publication. This
keeps test cost tied to unique server decisions rather than multiplying identical
XZ work by the number of client paths.

For every path:

1. Use the real producer checkpoint/delta builders and production XZ encoder to
   create network artifacts. Do not synthesize client messages directly.
2. Run every selected artifact through real background-core checksum validation,
   JSON decoding, typed conversion, and postcard serialization.
3. Run every postcard through real main-core checkpoint installation or ordered
   mutation application and acknowledgement.
4. At every path boundary, require the producer's incremental state ID, a full
   producer recomputation, and main core's state ID to equal the oracle ID for
   that logical boundary.
5. At the final boundary, require every path to have the same state ID and exact
   canonical record collection.
6. Compare all app-visible NOTAM outputs exactly: aggregate counters, lookup by
   NOTAM ID, the complete ordered airport index, and every airport query result.

Paths may skip intermediate logical states by using a combined delta or later
checkpoint. Compare only boundaries represented by that path, but require every
path to end at the same golden final state. This proves state identity depends on
content, not on when the server chose to materialize checkpoints or combine
pending transitions.

Run a smaller fixed schedule set in ordinary CI. Run the complete captured trace
when `AEROBAG_TEST_ARTIFACTS_ROOT` (or the existing alias) points to the external
fixture repository.

### Client Work Accounting

Add explicit work accounting to the shared background/main-core NOTAM path.
Tests must use structural counts rather than elapsed time as the correctness
gate. Record at least:

```text
BackgroundNotamWork
  compressed_bytes_read
  json_bytes_decoded
  records_decoded
  postcard_bytes_written

MainNotamWork
  postcard_bytes_read
  mutations_applied
  canonical_record_lookups
  secondary_index_removals
  secondary_index_insertions
  leaf_hashes_computed
  bucket_records_hashed
  bucket_hashes_computed
  group_hashes_computed
  roots_computed
  full_record_collection_iterations
  full_state_serializations
```

Production may use a no-op meter where measurement overhead matters, but the
same application functions must accept the test meter; do not create a separate
instrumented algorithm.

For every ordinary delta in the captured trace, assert:

- records decoded and postcard content equal only the upsert/remove operations
  in that delta;
- `mutations_applied == mutation_count`;
- `full_record_collection_iterations == 0`;
- `full_state_serializations == 0`;
- only affected airport-index entries are removed/inserted;
- only affected Merkle buckets/groups are recomputed; and
- postcard size is a function of envelope plus encoded mutations, not current
  state size.

Add a dedicated single-process allocation probe. Construct the full captured
20,000-record state, reset allocator counters, then pass a real one-record delta
through background decoding/postcard encoding and main decoding/application.
Assert a documented fixed allocation ceiling comfortably above one-record work
but far below the 24 MiB full-state representation. Run the same mutation
against small and full states and require the structural work counts to differ
only by the occupancy of the touched fixed Merkle bucket, never by total record
count. The allocation test is not a timing benchmark; it exists specifically to
catch reintroduction of full-state clone/serialization work.

Do not derive `Clone` or `Serialize` for `NotamState`, and keep its record
collection private behind mutation/query APIs. This makes a whole-state copy in
the ordinary delta path an explicit architectural change rather than an easy
accident.

### End-To-End Tests

- Feed a realistic captured SWIM sequence through raw ingestion, normalization,
  journal publication, core fetching, and installation.
- At every published head, compare the client's materialized typed NOTAM payload
  with a fresh full export from SQLite.
- Run through more than 100 record mutations, checkpoint rotation, suffix
  trimming, daemon restart, and client reconnect.
- Keep one client continuously current, one intermittently connected, and one
  new client; verify they converge to the same state ID through their expected
  routes.
- After every acknowledged update, compare main-core record equality, state ID,
  aggregate counters, and airport query results with a fresh SQLite export.
- Corrupt an operation after network decoding but before main-core application;
  verify the mismatch diagnostic, state discard, `NoState` acknowledgement,
  checkpoint resynchronization, and exact convergence.
- Verify source rejection/repair health facts survive the incremental path and
  remain visible to pipeline health monitoring.

### Performance And Size Measurements

These are measurement tests or ignored benchmarks, not timing-sensitive CI
assertions.

- Measure single-record SQLite apply plus Merkle maintenance.
- Measure single-record journal-to-XZ-delta publication.
- Measure core application and Merkle verification for 1, 10, and 100 record
  mutations.
- Measure network-JSON parsing and postcard encoding in background core
  separately from postcard decoding and mutation application in main core.
- Assert ordinary prepared-message bytes scale with changed records, not total
  NOTAM state size.
- Compare delta bytes and CPU with the current 24 MiB full-state rebuild.
- Measure checkpoint export/compression independently from ordinary updates.
- Confirm ordinary updates do not read or serialize all current records by
  instrumentation/counters, not merely elapsed time.
- Measure catch-up request count and transfer bytes for current, 10-mutation,
  100-mutation, and checkpoint-fallback clients before considering immutable
  sealed delta segments.

## Implementation Order

1. Implement the Rust fixture finalizer and freeze the staged live capture.
2. Add shared published record/mutation types, canonical encoding, Merkle index,
   `NotamState`, secondary indexes, and ordered apply implementation with
   golden/property tests.
3. Add SQLite migration, transactional Merkle maintenance, and durable journal.
4. Add direct journal delta construction and crash-safe publisher behind tests.
5. Add checkpoint generation, mutation-budget policy, retention, and GC roots.
6. Roll the live-feed contract and implement background-core routing, prepared
   messages, main-core ownership/application, acknowledgements, persistence, and
   resynchronization.
7. Update daemon wakeup/SSE behavior and remove 60-second coalescing.
8. Run schedule-invariance, end-to-end, and work-accounting tests over the
   captured fixture; retain storage, transfer, and allocation probes as explicit
   measurements.
9. Deploy producer and clients together under the new exact contract.

## Completion Criteria

- Ordinary NOTAM updates perform work proportional to changed records and do not
  materialize or compress a full state.
- An up-to-date client fetches exactly one immutable delta per logical update.
- Every installed head is verified against one deterministic content-derived
  state ID.
- Main core owns the only long-lived client record collection, Merkle index, and
  app-facing indexes; ordinary worker-to-main messages contain only mutations.
- More than one hundred checkpoint/delta materialization schedules over the
  captured trace converge to the same golden final ID and exact client-visible
  outputs.
- Structural work accounting and an allocation probe prove a one-record delta
  performs no full-state iteration, clone, serialization, or allocation-scale
  equivalent of the prior 24 MiB replacement path.
- New, lagging, restarted, and continuously connected clients converge to exact
  typed state equality.
- Checkpoint rotation bounds replay work without a wall-clock batching delay.
- Fault-injection tests cover every SQLite/filesystem publication boundary.
- Current and superseded publication data remain bounded under retention and GC.
