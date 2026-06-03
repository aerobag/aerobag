# NOTAMS Plan

This is the current architecture plan for bringing FAA NOTAM data into Aerobag.

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

Queue access is handled outside the Rust DAG.

Current implementation:
- Java collector under [product/preprocessor/swim-notams-fetch](/root/aerobag-preprocessor/aerobag/product/preprocessor/swim-notams-fetch)

Role:
- connect to the FAA SWIM/SCDS Solace JMS queue
- receive messages
- durably persist raw messages before acknowledging them

Important rule:
- the collector must not acknowledge a message before it has been durably written to local storage

### 2. Producer-Owned State Store

We need a durable producer-side NOTAM state store that is separate from:
- fetch cache
- node cache

Why:
- fetch cache is for immutable pulled artifacts by URL/content
- node cache is for deterministic build-node outputs
- live queue-fed NOTAM state is neither of those things

Planned storage shape:
- `artifact-root/state/notams/raw/`
  - append-only raw segment files
- `artifact-root/state/notams/state.sqlite`
  - current normalized state
- `artifact-root/state/notams/checkpoints.json`
  - ingestion / export checkpoints

SQLite is the preferred state format because:
- idempotent upserts are easy
- airport / keyword / time indexing is easy
- crash recovery is straightforward
- snapshot export into a live-feed product is simple

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
- append raw segments
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

The Java-to-Rust handoff must not be a single mutable file.

Planned safe shape:
- Java writes append-only segment files
- each segment is fsynced/closed
- then marked complete
- Rust ingests only complete segments
- Rust tracks its own checkpoint

This avoids:
- acked-but-not-persisted message loss
- races on a single shared mutable file

Duplicates are acceptable and expected around crashes/restarts.
The Rust-side normalization/state-upsert logic must therefore be idempotent.

## Current Normalization Status

We now have:
- Java queue collector
- Rust offline normalizer from captured messages

Current Rust tool:

```bash
/root/aerobag-artifacts/target/debug/preprocessor-cli normalize-swim-notams \
  --input-jsonl <captured_messages.jsonl> \
  --output-dir <out_dir> \
  --version-label <label>
```

This proves:
- SWIM connectivity works
- AIXM XML can be parsed in Rust
- we can emit a product-shaped `notams.json` artifact

But this is still offline normalization from captured files, not the persistent current-state pipeline yet.

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

Build the persistent producer-side NOTAM state store:
- append-only raw segments
- SQLite current-state DB
- checkpoints

Then add an export step that snapshots that state into a real `notams` live-feed state.
