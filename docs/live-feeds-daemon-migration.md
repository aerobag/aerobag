# Live Feeds Daemon Migration

Live feeds should become a long-running daemon, not a cron-style CLI contract.
Tests still need one-shot invocation hooks, but those hooks should call library
interfaces rather than shelling out to an operational command.

## Target Shape

```text
aerobag-live-feedsd
├── source adapters
│   ├── real pollers
│   ├── real streaming upstreams
│   └── fixture/simulation sources
├── live-feed engine
│   ├── product-specific state builders
│   ├── delta builders over Aerobag-shaped state
│   └── publisher
├── SSE broker
└── internal/test harness API
```

Publication ordering is part of the contract:

1. Build the new full Aerobag-shaped state.
2. Read the currently published state/version for that product.
3. Build `delta(current, new)` when applicable.
4. Write state and delta to the paths clients will fetch.
5. Atomically update the product version manifest and `current.json`.
6. Notify the SSE broker that product/version is current.
7. Emit SSE invalidations only after the referenced files are visible.

## Slices

1. Extract live-feed engine code from `product_build.rs`.
   Create daemon-oriented modules for product state building, delta building,
   publishing, and current-state discovery. Keep these callable from tests
   without shelling out.

2. Define source/publisher interfaces.
   Add clear boundaries for `UpstreamSource`, `ProductBuilder`,
   `LiveFeedPublisher`, `SseBroker`, `Clock`, and `CycleDataProvider`.

3. Add `CycleDataProvider`.
   Live feeds ask for current shared cycle-derived datasets, such as towered
   METAR station IDs, through an explicit interface. The production daemon CLI
   does not take a cycle publication root; product builders that need
   cycle-derived data must receive that dependency explicitly and must not read
   the cycle build cache.

4. Split live-feed scratch space by product.
   Use `private-work/live-feeds/<product>/...` for transient production work.
   Do not use the cycle-package build cache for live-feed product output.

5. Add fixture compilation cache.
   For simulation/testing, compile raw fixtures into Aerobag-shaped live-feed
   states and deltas once:

   ```text
   private-work/live-feeds-fixtures/<fixture-cache-key>/
     manifest.json
     timeline.json
     states/<product>/<version>/...
     deltas/<product>/<from>__<to>.json
   ```

   The cache key includes raw fixture hashes, product builder version/config,
   and dependency snapshots such as towered station IDs.

6. Add simulation source.
   Simulation mode reads the compiled fixture timeline, applies time
   acceleration and optional "phase to now" timestamp shifting, and emits
   compiled state events into the same publisher/SSE path as production.

7. Add daemon process.
   `aerobag-live-feedsd` runs product schedulers, upstream pollers, streaming
   upstream clients, publisher, and SSE broker. It supports production and
   simulation source configurations.

8. Move dev SSE out of Vite.
   Vite should only serve the app and static assets. Dev live-feed SSE should
   come from `aerobag-live-feedsd --simulation ...`.

9. Remove CLI batch mode.
   Keep one-shot test hooks as library/test harness calls, not as an operational
   command.

10. Add end-to-end tests.
    Test one production-style tick with fake upstreams, fixture compilation
    reuse, accelerated simulation playback, delta reconstruction, atomic
    publish ordering, and SSE invalidation after publish.

## Burn-Down Status

- Done: daemon-oriented engine interfaces exist in `preprocessor-live-feeds`.
  The shared engine owns clocks, upstream-source/product-builder/publisher/SSE
  traits, current/version manifests, keyed record deltas, canonical JSON state
  hashes, a filesystem publisher, and fixture compilation cache keys.
- Done: shared publish ticks exist in `preprocessor-live-feeds`. A tick polls
  due upstream events, builds Aerobag-shaped state, publishes state/delta/current
  files, and announces invalidations through `SseBroker`. Product failures are
  recorded without stopping other products.
- Done: the current shared cycle-derived dependency is explicit. Live feeds
  need towered airport IDs for METAR low-zoom thinning, and
  `PublishedCycleDataProvider` reads them from the published nav-db
  `navref/symbol/airport/*` keyspace.
- Done: first-pass product poll intervals are encoded in the live-feed engine:
  NEXRAD 60 seconds, METARs/TFRs 5 minutes, winds aloft 1 hour, obstacles 6
  hours.
- Done: simulation scaffolding exists in `preprocessor-live-feeds`. Compiled
  fixture timelines can be replayed as accelerated `UpstreamSource` events.
  Each product's fixture start maps to the same daemon-start zero point, emitted
  versions and top-level timestamps are rewritten onto the virtual source clock
  so product time advances by the recorded fixture offsets, and only delivery is
  accelerated. The daemon restarts the fixture timeline when the shortest
  nontrivial fixture span is exhausted.
- Done: `aerobag-live-feedsd` exists as the daemon package and binary. It owns
  static live-feed file serving, an in-process SSE broker, the production
  polling loop, and the accelerated simulation loop. Production pollers build
  METARs, NEXRAD source-grid tiles, TFRs, winds aloft, and obstacles through
  `preprocessor-live-feeds` builders, publish via the shared filesystem
  publisher, and announce through the broker after publication.
- Done: source adapters now cover periodic polling, queued/push-style upstream
  events for future streaming sources, and fixture timeline replay. Future
  SWIM/NOTAM-style long-lived upstream clients should push `UpstreamEvent`s
  into `QueuedLiveFeedSource` and reuse the same product builder/publisher path.
- Done: reusable live-feed product helpers moved out of the CLI layer into
  `preprocessor-live-feeds::products`. The CLI tests that still exercise
  product-build interactions import those helpers instead of carrying local
  duplicate NEXRAD/winds/state-building code.
- Done: Vite no longer synthesizes live-feed timelines or SSE frames. In dev,
  `restart-vite-dev.sh` starts `aerobag-live-feedsd` beside Vite and proxies
  `/live-feeds`.
- Done: the retired live-feed CLI operational command and its CLI-owned batch
  builder bridge have been removed.
- Done: the remaining live-feed integration tests moved out of
  `preprocessor-cli` into `preprocessor-live-feeds`, so the CLI no longer owns
  live-feed build/publish behavior.
- Done: METAR app-side handoff was measured in a browser/WASM bakeoff using a
  real 5,044-record METAR fixture. The bakeoff separated serializer choice from
  indexing strategy. The useful result is that the indexing strategy dominates:
  moving tile-index construction off the latency-sensitive core worker cuts
  install time from milliseconds to below 1 ms. `serde-postcard` with
  early-indexed data matches the custom binary format while being smaller and
  avoiding a bespoke production wire format.

  ```text
  serializer          indexing         payload      encode   avg install    min     max
  -----------------   --------------   ----------   ------   -----------   -----   -----
  serde-json-delta    late-indexed      714.66 KiB   0.0ms    18.72ms       17.1    41.0
  serde-json          late-indexed      1.63 MiB     0.0ms     6.21ms        6.0     6.6
  serde-json-typed    late-indexed      1.63 MiB     0.0ms     3.65ms        3.5     3.9
  serde-bincode       late-indexed      891.30 KiB   0.7ms     2.61ms        2.4     3.5
  serde-postcard      late-indexed      690.16 KiB   0.6ms     2.54ms        2.4     3.0
  custom-bin          late-indexed      732.00 KiB   0.3ms     2.24ms        2.1     2.6

  serde-json          early-indexed     1.32 MiB     1.5ms     4.58ms        4.3     5.9
  serde-json-typed    early-indexed     1.32 MiB     1.5ms     1.72ms        1.5     2.6
  serde-bincode       early-indexed     931.97 KiB   0.4ms     0.73ms        0.6     1.0
  serde-postcard      early-indexed     699.00 KiB   0.6ms     0.69ms        0.5     1.0
  custom-bin          early-indexed     827.58 KiB   0.3ms     0.69ms        0.5     0.8
  ```

  Production direction: use `serde-postcard + early-indexed` for the METAR
  worker-to-core handoff. The METAR worker should receive/update the product,
  build the query-shaped record table plus tile index, serialize that prepared
  structure with postcard, and hand compact bytes to the latency-sensitive core
  worker for installation.
- Next slice: add a production supervision/deployment wrapper when deployment
  policy is chosen.
