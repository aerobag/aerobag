# Live Feed SSE Plan

Live feeds use immutable state payloads plus Server-Sent Events (SSE) for low
latency invalidation. `current.json` remains the bootstrap and reconnect source
of truth; SSE only tells clients that a newer version is available.

## Publication Shape

For each product:

```text
/live-feeds/v2/current.json
/live-feeds/v2/versions/<product>/<version>.json
/live-feeds/v2/states/<product>/<version>.json
/live-feeds/v2/deltas/<product>/<from>__<to>.json
```

`current.json` maps product id to the current version and version manifest URL.
The version manifest names the full state payload and, when available, the delta
from the previous state. Payload references include strong hashes so clients can
verify both fetched full states and reconstructed delta targets.

## SSE Events

The SSE stream emits product update events:

```json
{
  "product": "metars",
  "version": "abc123",
  "version_manifest_url": "versions/metars/abc123.json"
}
```

Clients treat events as invalidations. The canonical data remains the version
manifest and payload hashes.

## Client Rules

- Bootstrap by fetching `current.json`.
- Keep displaying the current valid state while fetching newer data.
- On an SSE event, fetch that version manifest.
- If the manifest delta starts at the local version, fetch and apply the delta.
- If the delta does not chain from local state, fetch the full state.
- Verify full-state hashes and delta-reconstructed target hashes.
- Ignore stale events for versions already seen or applied.
- On SSE disconnect, keep the current state and use `current.json` as fallback
  until the stream reconnects.

## Tests

Core live-feed behavior should be tested without production wall-clock time:

- Deterministic core tests use a fake clock, fake event stream, and fake fetcher.
- Local integration tests use an in-process scripted SSE server that owns its
  event queue, disconnects, and `current.json` timeline.

The METAR three-hour fixture is the first trace for these tests. It verifies
delta application over real successive states and exercises SSE update ordering,
stale events, disconnect fallback, and fetch failure behavior.
