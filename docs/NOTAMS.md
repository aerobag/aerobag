# NOTAMS Plan

This is the current architecture plan for bringing FAA NOTAM data into Aerobag.

The incremental state-identity, delta-journal, and checkpoint publication design
in [notam-incremental-publication-plan.md](notam-incremental-publication-plan.md)
supersedes this document's replace-everything publication target. The SWIM
collection, durable raw-message handoff, and normalized projection described
here remain current.

This is intentionally a plan doc, not a feed-details doc. Raw SWIM/SCDS connection and payload notes live in [NOTAMS_FEED.md](/root/aerobag-preprocessor/aerobag/docs/NOTAMS_FEED.md).

## Goals

- ingest FAA NOTAM data from the SWIM/SCDS queue
- publish a replace-everything current-state live-feed product for clients
- keep queue and credential complexity out of app/web/core
- keep XML parsing and FAA-specific semantics out of clients

## Key Decision

The SWIM queue is an event-driven, stateful source. Our client contract should still be a replace-everything snapshot of current state.

That means we need three distinct layers:

1. queue ingestion
2. producer-owned current-state storage
3. snapshot/export into the normal package pipeline

## Architecture

### 1. Queue Ingestion

Queue access is handled outside the deterministic Rust build DAG.

Current implementation:
- Java collector under [product/preprocessor/swim-notams-fetch](/root/aerobag-preprocessor/aerobag/product/preprocessor/swim-notams-fetch)
- optional live-feed daemon supervisor enabled with `--swim-notams-config <path> --swim-notams-environment <dev|prod> --swim-notams-collector <path>`

Role:
- connect to the FAA SWIM/SCDS Solace JMS queue
- receive messages
- durably persist raw messages before acknowledging them
- apply committed raw messages into daemon-owned current state
- trigger the normal `notams` live-feed product builder after raw messages are applied

Important rule:
- the collector must not acknowledge a message before it has been durably written to local storage

The NOTAM supervisor is isolated from the other live-feed products. A broken SWIM queue connection records `notams` source health failures and retries, but it does not stop METARs, TAFs, NEXRAD, TFRs, obstacles, or winds-aloft from publishing.

SWIM subscriptions are stateful queues. Dev and prod must use separate
subscriptions. A daemon must be started with an expected environment and refuses
to consume a credential file that declares a different environment.

### 2. Producer-Owned State Store

We need a durable producer-side NOTAM state store that is separate from:
- fetch cache
- node cache

Why:
- fetch cache is for immutable pulled artifacts by URL/content
- node cache is for deterministic build-node outputs
- live queue-fed NOTAM state is neither of those things

Current storage shape:
- `artifact-root/state/swim-notams/subscription.json`
  - non-secret subscription identity for the SWIM queue this state belongs to
- `artifact-root/state/swim-notams/lock`
  - process lock protecting the queue subscription and local state
- `artifact-root/state/swim-notams/state/current.sqlite`
  - committed raw messages, applied raw cursor, and current normalized state

SQLite is the preferred state format because:
- idempotent upserts are easy
- airport / keyword / time indexing is easy
- crash recovery is straightforward
- snapshot export into a live-feed product is simple

SQLite is the internal communication boundary between ingestion and export: the
Java collector writes one raw row transactionally before acknowledging each JMS
message, Rust applies committed raw rows idempotently into current state, and the
`notams` live-feed builder snapshots SQLite current state.

### 3. Snapshot / Export

The live mutable state store is not itself a node output.

Instead:
- a DAG step reads a consistent snapshot of the current state
- exports a content-addressed artifact
- packages and publishes it through the normal live-feed publisher path

That means the content-addressed identity belongs to the exported snapshot, not the ever-changing SQLite DB.

## Ownership Split

### Outside The DAG

A long-lived ingest process owns the mutable state:
- consume FAA queue
- commit raw messages
- parse / normalize
- upsert current state
- maintain checkpoints

This process is conceptually something like:
- `notam-ingestd`

### Inside The DAG

A build step owns deterministic export:
- read current NOTAM state at a consistent moment
- emit `notams.json`
- emit manifest
- emit zip/package

This is what later becomes a normal live-feed product.

## Why We Are Not Wiring The Queue Straight Into The Live-Feed Builder

That would be the wrong abstraction.

Problems:
- queue consumption is stateful
- queue data is event-driven, not a cheap snapshot feed
- build nodes are supposed to be deterministic
- cache semantics become nonsensical if a node both mutates and consumes a live queue

So:
- queue ingest must remain outside the graph
- snapshot/export belongs inside the graph

## Raw Message Handoff Between Java And Rust

The Java-to-Rust handoff is the SWIM NOTAM SQLite file.

Current safe shape:
- Java inserts one `raw_notam_messages` row in a SQLite transaction
- Java acknowledges the JMS message only after that transaction commits
- Rust reads committed raw rows above `raw_ingest_cursor`
- Rust idempotently applies those rows into `current_notams`
- Rust marks raw rows applied and advances the cursor in the same apply flow

This avoids:
- acked-but-not-persisted message loss
- tailing partially-written files
- split state between collector output and daemon state

Duplicates are acceptable and expected around crashes/restarts.
The Rust-side normalization/state-upsert logic must therefore be idempotent.

## Current Normalization Status

We now have:
- Java queue collector
- first-stage live-feed daemon integration for a `notams` product
- durable NOTAM store with subscription lock, raw messages, applied cursor, and SQLite current state
- client/cache registry support for the `notams` record-json live-feed schema

This proves:
- SWIM connectivity works
- AIXM XML can be parsed in Rust
- we can emit a product-shaped `notams.json` artifact

The daemon publishes the normalized current record set as a live-feed product
from SQLite state. Committed raw rows remain available for a retention window so
we can diagnose recent ingestion and replay recent applies if the normalizer
changes.

## Client Contract

Target client contract remains:
- replace-everything current-state live-feed product

Not:
- queue deltas in clients
- XML in clients
- Java in clients

Likely package contents:
- `notams.json`
- manifest
- zipped live-feed install package where needed

Likely normalized fields:
- stable NOTAM id
- airport/location linkage
- keyword/status/function
- effective interval
- plain text / local-format text / ICAO-format text
- FAA extension fields useful for indexing/debugging

Likely secondary indexes later:
- airport
- plate
- procedure

Plate attachment is desirable for many FDC/procedure NOTAMs, but it should be an index layered on top of the normalized record set, not the only organization of the data.

## Open Questions

1. Recovery / backfill
- what recovery window does FAA queue retention actually provide?
- is there a complementary snapshot source, or do we rely entirely on our own durable state?

2. State identity
- what exact identity key should drive the SQLite upsert?
- likely a combination of FAA/NMS message identifiers and NOTAM identifiers, but this needs to be pinned down carefully

3. Export cadence
- how often do we refresh the live-feed `notams` state from producer-owned state?

4. Plate/procedure mapping
- how aggressively can we attach FDC NOTAMs to plates/procedures without introducing bad matches?

## Next Step

Before productizing:
- fix the FAA subscription/queue state if it returns `503 Queue Shutdown`
- add an authoritative initial-state/backfill source so a fresh server can learn complete NOTAM state instead of only messages delivered after subscription startup
- add indexes for airport/procedure lookup once the normalized record contract is consumed by clients
