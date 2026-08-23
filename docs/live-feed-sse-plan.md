# Live Feed SSE Plan

Live feeds use immutable state payloads plus Server-Sent Events (SSE) for low
latency discovery and invalidation. Every SSE connection begins with an
authoritative full catalog; later events announce individual product updates.
Application clients never fetch `current.json`.

## Publication Shape

The daemon keeps a daemon-owned durable publication ledger and publishes immutable
resources for each product:

```text
/live-feeds/v3/versions/<product>/<version>.json
/live-feeds/v3/states/<product>/<version>.json
/live-feeds/v3/deltas/<product>/<from>__<to>.json
```

The daemon-owned `current.json` ledger maps product ids to current versions and lets
the daemon recover its publication state across restarts. The version manifest
names the full state payload and, when available, the delta from the previous
state. Payload references include strong hashes so clients can verify both
fetched full states and reconstructed delta targets.

## SSE Events

The SSE stream first emits a `live-feed-catalog` event containing the complete
v3 catalog. It then emits `live-feed-current` product update events:

```json
{
  "product": "metars",
  "version": "abc123",
  "version_manifest_url": "versions/metars/abc123.json"
}
```

The catalog is authoritative for product membership. Product events are
incremental invalidations. The canonical product data remains the immutable
version manifest and payload hashes.

## Client Rules

- Bootstrap and reconnect from the full catalog at the start of the SSE stream.
- Keep displaying the current valid state while fetching newer data.
- On an SSE event, fetch that version manifest.
- If the manifest delta starts at the local version, fetch and apply the delta.
- If the delta does not chain from local state, fetch the full state.
- Verify full-state hashes and delta-reconstructed target hashes.
- Ignore stale events for versions already seen or applied.
- On SSE disconnect, keep the current valid state. A reconnected stream supplies
  a fresh full catalog before any queued product events.

## Tests

Core live-feed behavior should be tested without production wall-clock time:

- Deterministic core tests use a fake clock, fake event stream, and fake fetcher.
- Local integration tests use an in-process scripted SSE server that owns its
  catalog, event queue, and disconnect timeline, and assert that clients never
  request `current.json`.

The METAR three-hour fixture is the first trace for these tests. It verifies
delta application over real successive states and exercises SSE update ordering,
stale events, catalog-based reconnect recovery, and fetch failure behavior.
