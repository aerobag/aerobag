# Airway graph sizing and loading options

Measured 2026-09-14. The audit below describes the NAV25 baseline; the selected
NAV26 implementation and subsequent browser measurements are described first.
Spinner work is deferred separately in
[foreground-action-feedback-plan.md](foreground-action-feedback-plan.md).

## Implemented: One first-use graph page frontier (NAV26)

- The complete graph is one Postcard value at `airway/routing/graph` in the
  ordinary HAD tree. The builder splits it across existing 64 KiB pages;
  publication XZ-compresses each page, and the existing ZIP Stored package
  contains those same pages. No extra member, bundle, or transport is used.
  The short-lived `airway-routing.postcard.xz` implementation was removed
  before publication.
- Startup prefetch includes the graph's **lookup path only**, using the new
  generic `NavKvPrefetch::Lookup` policy. The page containing the key records
  its external byte range, so the first Find Airways can identify every graph
  value page without another tree-walk waterfall. Graph value pages are not
  deliberately prefetched; a boundary page shared with another startup value
  may already be resident.
- Shared HAD `get_bytes` checks all external-page presence before allocating
  or copying a large value. It reports the complete missing frontier, not only
  the first missing page. Partial arrival returns only the remaining pages.
- Web starts every request in that frontier immediately, with in-flight
  deduplication and bounded transport retries. The application six-request
  cap is gone. Installation remains serialized, newest-request-first, and
  yields through MessageChannel, avoiding the per-page nested-timer clamp.
  A blocked installation does not gate network dispatch.
- Android reads the same pages from the installed NAVDB ZIP through its
  existing resource path. No Android-specific graph handler remains.
- Core decodes and validates Postcard schema 2, then caches the graph in
  NavDataController. Editors share an Arc. Reopening and endpoint changes
  reuse it; attaching/advancing NAVDB discards the cache.
- All nodes, edges, fields, precision and routing rules are retained. There
  is no old JSON graph fallback. Positional shape changes require another
  immutable NAVDB contract revision; HAD storage itself is unchanged.
- Core logs `airway_routing.graph_load` with decoded size, HAD read/copy time
  and Postcard decode/validation time. Page decompression is measured by the
  ordinary page-install path; search retains its separate timer.
- One discovered frontier is not a guarantee of one elapsed RTT. HTTP
  connection/stream limits, bandwidth and competing requests still apply.
  Junction symbols and flight-plan materialization can need other NAVDB pages.

### Regression evidence

Before the fixes, the new web test requested 32 pages but observed only 6
fetches start. The new shared HAD test expected 11 missing pages but got only
the first one. Both now pass. Additional tests cover metadata-only prefetch,
partial page arrival, graph cache reuse and NAVDB replacement, invalid Postcard
and graph references, ordinary package roundtrip, and producer graph-key output.

The full-graph audit in `/tmp/airway-graph-audit-nav26-pages/report.json`
roundtrips all 8,823 nodes and 31,080 edges through the ordinary HAD reader and
per-page XZ decoder. Its graph-only layout needs 19 value pages, totaling
548,208 compressed bytes for 1,196,453 Postcard bytes. Its isolated lookup page
adds 120 compressed bytes. Actual publication alignment and neighboring records
will change those page/byte totals slightly; these are not measured network
latencies. The published NAV25 baseline is 130 pages / 694,808 compressed bytes.

Pre-publication verification: 1,014 core tests passed (27 ignored or
fixture-dependent), including 49 platform-boundary tests; 84 shared contract,
HAD and package/fixture tests passed; 5 focused producer tests passed; web
TypeScript and all 233 unit tests passed; WASM target type-check passed.
Android's 9 NAVDB paging/replacement JVM tests passed, with core rebuilt for
arm64-v8a and x86_64. No physical-device run or new-publication browser run was
performed in this change.
These are not real-device/real-publication latency results.

### NAV26 publication and browser results

Publication `main-56c813c3cc44/20260914T163556Z`, cycle 2609, contains
NAVDB `c16b4a0ddfa2cf6810de7540f5edf225e698b3af6be55ee0f975408a2aa74abb`.
The shared HAD prefix audit confirms one graph value of 1,196,453 bytes over
19 value pages, plus one lookup page. Startup already installs that lookup page.

Optimized WASM on 8085, headless Chrome on the dev server, KRNT-KLVN:

- Three independent fresh browser profiles: first Find Route reached the
  editor summary in 103, 88, and 87 ms.
- Each cold graph request discovered all 19 pages in one frontier. Fetch,
  decompression and installation of that frontier took 49, 50, and 50 ms.
- HAD value assembly took 0-1 ms; Postcard decode/validation took 2-3 ms;
  shortest-path search took 4-5 ms.
- A separate snapshot resource needed one additional page, taking 2-3 ms.
- Six warm reopenings took 28, 25, 24, 23, 23, and 35 ms. They did not fetch
  or decode the graph again; searches took 3-5 ms.

The endpoint is DOM-observed editor summary availability, polled at 20 ms,
not an after-paint measurement. Cold means fresh browser and core caches;
the server's disk cache is warm. These are local, unthrottled measurements,
not evidence of laptop/Firefox or high-RTT performance. Do not compare them
as a controlled speedup against the user's earlier four-second observation.
The first run saved valid timings but failed during temporary-profile cleanup;
bounded removal retries, as used by the other browser labs, fixed that race.
The next two complete harness runs passed. Reports are
`/tmp/aerobag-airway-routing-nav26-run2.json` and `-run3.json`.

After integrating the new guided-tour feature and rebuilding 8085, the standalone
probe initially failed to establish its two-airport plan because it operated
behind the automatic introduction. It now calls the shared `dismissFirstUseTour`
helper, closing the actual tour through pointer input before route entry.
The final integrated-app run passed: 138 ms cold (99 ms for the 19-page frontier,
2 ms graph decode, 5 ms search), then 26 and 35 ms warm. That report is
`/tmp/aerobag-airway-routing-perf.json`. This additional run used the new startup
flow, so it is recorded separately from the three earlier profiles.

Both compact fixtures were genuinely rebuilt from this publication. Another
checkout concurrently published artifact commit
`ef74b3f0587a7e77ec6637bfbedd3d05d42d4027`; its fixture data matches our independent
rebuild byte-for-byte. Use that published commit, now pinned in the application,
rather than publishing a duplicate. Live-feed samples are unchanged. Both
fixture contract checks and release-fixture materialization passed. This closes
the temporary NAV25/NAV26 fixture mismatch accepted for the producer handoff.
The permanent logical rollover fixture generates the new graph as Postcard
inside HAD. Full browser/emulator release journeys were not run for this handoff.

## What the NAV25 first action reads

The 131-page KRNT-KLVN sample loaded **100% of the routing graph**. It did not
fault only the graph explored by the shortest-path search. Graph::load first
reads airway/routing/manifest, discovers pages for every chunk together, then
loads and validates all chunks before searching. This is the same dataset for
a short route. A Draft holds its Graph in an Arc; editing pins does not clone
all graph data. A fresh editor reconstructs it from already cached NAVDB pages.

Publication inspected:

- Producer revision: 271934ad9670, publication 20260912T153651Z, cycle 2609, NAV25.
- NAVDB identity: a701d991587f0ba3726bcd6d0b15322a7affef297821becb2d499ea3b4c289e3.
- 69 chunks of at most 128 nodes, plus one manifest record.
- 8,823 nodes and 31,080 directed edges (not undirected links).
- Nodes: 8,113 fixes, 690 navaids, 20 coordinate-only nodes, no airports.
- Published values: 8,414,281 bytes of compact JSON, not pretty-printed JSON.
- Manifest/key leaf: page 236. Values: pages 3117 through 3245 inclusive.
- These 130 pages total 694,808 bytes after existing per-page XZ compression.
- The browser's 131st page is a junction-symbol page (660), not another graph
  page. Including it gives the previously reported 698,352 bytes.
- None of these graph pages is in the existing startup prefetch set.
- The entire NAVDB has 3,681 pages; that is distinct from the routing graph.

## Is the content bloated?

Not a dump of airport records, descriptions, map geometry, or every source
column. The routing index is an adjacency list and contains mostly useful data.

- Node id: index into the node array; validated as contiguous. This could be
  implicit in a different representation, but is tiny compared with the edges.
- Node nav_ref and lat/lon: endpoint identity, nearest-node connectors, drag
  targets, display and flight-plan materialization. Twenty coordinate-only
  references repeat their coordinates; this is not material bloat.
- Edge to and distance_nm: graph traversal and route scoring.
- airway_name: eligibility, airway-change tie breaking, display and materialization.
- branch_key and from/to_sequence: group the result into actual contiguous
  airway segments; branch_key also participates in crossing-point matching.
  These are not unused source bookkeeping. Branch keys are only zero or one
  characters long; there are three distinct values, not huge embedded IDs.
- mea_ft and gnss_mea_ft: mode eligibility and altitude presentation.
- crossing_altitude_ft and crossing_point: MCA markers at the proper fix.
- maximum_altitude_ft and signal_gap: published but **not read by this consumer**.
  The first is present on 30,980 edges and the second true on 176 edges. These
  are potentially meaningful constraints/cautions, not random junk; deciding
  whether to expose/enforce them is preferable to silently discarding them.

Removing those last two fields saves 1,460,412 raw JSON bytes, but only 41,012
bytes with the existing synthetic per-page XZ layout. In a whole Postcard+XZ
object it saves just 7,368 bytes for the full graph, or 6,236 bytes for the
low-edge variant. Deleting fields is not the major opportunity.

There is avoidable content: J has 3,418 directed edges and Q has 3,144. Both
current navigation modes reject these, yet we publish/download them. They are
21.1% of the edges. The retained low-airway edges are V=17,064, T=7,352, G=88,
and R=14, totaling 24,518.

JSON representation is the largest raw overhead: edge field names and their
quotes/colons/commas alone account for about 5.78 MB. XZ already compresses that
repetition well, so a sevenfold reduction in raw bytes is not sevenfold less
network traffic. Binary values nevertheless mean far fewer HAD pages and much
less decompressed data.

## Orthogonal size comparison

All numbers are **bytes**, not KiB. All rows use identical XZ -6, CRC64, one
thread, matching production. No coordinate/distance rounding, string interning,
custom binary encoding, undirected-edge folding or routing algorithm change.
Postcard is the existing standard serializer already used in core.

- full: all original nodes, edges and fields.
- low-edges: remove J/Q edges, keep all original nodes and node IDs.
- low: additionally remove nodes not incident to retained edges and reindex.
- used: omit only maximum_altitude_ft and signal_gap.
- Pages: complete graph-only HAD built by the shared builder, including its
  key/manifest page, excluding its tiny root. Values keep 128-node chunks.
- Page XZ: sum of separately compressed HAD pages, no ZIP/HTTP overhead.
- Whole XZ: the same semantic graph serialized as one chunk and compressed once.
  This is a candidate single-object representation, not a ready client format.

```text
Variant                    Raw values B  Pages   Page XZ B   Whole XZ B
full-json                       8412379    130      681700       598484
full-used-json                  6951967    108      640688       531048
full-postcard                   1196663     20      548940       471260
full-used-postcard              1041833     17      536644       463892
low-edges-json                  6801464    105      577928       507136
low-edges-used-json             5649316     88      545952       453744
low-edges-postcard               997580     17      478380       409108
low-edges-used-postcard          875512     15      467984       402872
low-json                        6680440    103      554072       484996
low-used-json                   5528292     86      522124       431840
low-postcard                     960635     16      448880       386148
low-used-postcard                838567     14      440020       380084
```

The synthetic full-JSON page total (681,700 B) is slightly below the actual
publication (694,808 B): it re-serializes typed values, and the graph-only HAD
has different page alignment and no neighboring keys/values. Use the published
number for current traffic, and synthetic rows for controlled comparisons.

**Node pruning caveat:** Graph::connections treats an exact graph-node anchor
differently from a nonmember anchor. Removing a high-only fix could change a
formerly unreachable exact anchor into a nearest-low-airway connector. Keep all
nodes/IDs initially (409,108 B whole Postcard+XZ); the extra saving from node
pruning is only 22,960 B and needs explicit endpoint-policy tests. Do not call
the pruned-node row behaviorally equivalent just because serialization roundtrips.

## Transport alternatives considered

Two independent levers are available:

1. **Transport only:** measured a Stored ZIP containing the 130 existing,
   already-XZ-compressed pages unchanged. It is 707,050 B, only 12,242 B (1.8%)
   above separate files, and can be fetched in one request. No change to graph
   values or routing is needed. A production resource-group descriptor would
   identify its URL and page IDs; readers would feed pages through the same
   NAVKV installation/cache path, retaining bounded yields. This would not
   eliminate the current 8.4 MB decompression/JSON workload.
2. **Format plus transport:** full graph as Postcard+XZ is 471,260 B; with only
   the supported edges and all nodes retained it is 409,108 B. The latter is
   about 41% less than current graph traffic and under 1 MB after decompression.
   Merely switching values to Postcard still leaves 17 HAD pages; getting one
   request requires a bundle or explicit whole-graph resource as well.

The old six-slot fetch queue made 129 value pages roughly 22 request waves,
even though core discovered them in one batch. At 100 ms RTT that suggests
about 2.2 s of request latency before byte transfer/installation. This is a
simplified model, not a measured explanation of the user's four-second sample.

We chose ordinary multi-page HAD delivery rather than either bundled transport.
A single-object Postcard value reduces the page count; metadata-only startup
prefetch removes graph lookup dependencies; removing our cap lets the browser
schedule the whole frontier. Production HTTPS negotiated HTTP/2 when checked;
local Vite HTTP negotiated HTTP/1.1. HTTP/1.1 browser connection limits can
still serialize requests, whereas HTTP/2 permits multiplexed streams.

## Eager loading tradeoff

Today's root-prefetch set is 21 pages / 113,980 B. NavDbOpenController::step
asks for this set before finishing database open. Adding the present graph
would increase that prefetch payload about 7.1x, to 808,788 B. Even the 409 KB
single-object candidate adds about 5.45 s of byte transfer at 600 kbit/s (ignoring
RTT and competing traffic). It is not free enough to add to the blocking startup
set without a new startup measurement.

Prefer first-use bulk page delivery; optional cache warming after startup
or when entering the flight-plan workflow can be evaluated independently. If
warming is added, keep it out of the required startup readiness barrier and
avoid competing with the chart/navigation content needed immediately. The data
supports early **descriptor** fetch much more clearly than mandatory graph fetch.

## Reproduce and verify

From the repository root, with NAVDB naming an unpacked publication directory:

```sh
cargo +1.94.1 run --locked --manifest-path ui/core-rust/Cargo.toml \
  --target-dir ../ui-target/shared/rust-target -p app-core \
  --example airway_graph_audit -- "$NAVDB" /tmp/airway-graph-audit
```

The example uses the shared NavKvDirectoryReader, HAD reader/builder, current
product-contract Rust structs, Postcard and production XZ settings. It writes
report.json, experimental graph objects, and published-pages.zip under the
output directory. It validates manifest counts, node IDs, typed serialization
roundtrips, and the selected Postcard-in-HAD graph through per-page XZ decoding,
metadata-only prefetch and one missing-page frontier with the shared reader. Nothing
is written into the publication or loaded into a live session.

This offline audit is size evidence, not a native-to-WASM performance extrapolation.
After the local browser measurements above, further comparison should cover
cold/warm latency and startup under realistic RTT/bandwidth. Verify retained route/altitude/crossing results
with golden tests, test concurrent requests and failed fetch retry, and retain
short installation work units so transport bundling does not become UI blocking.

Code owners inspected:

- ui/core-rust/crates/app-core/src/airway_routing.rs: Graph::load, connections,
  navigation-mode eligibility and shortest-path search.
- ui/core-rust/crates/app-core/src/routing_editor.rs: Arc graph ownership,
  route_crossings, summary and materialize.
- crates/product-contracts/src/airway_routing.rs: serialized graph schema.
- product/preprocessor/preprocessor-cli/src/product_build/airway_routing.rs:
  graph producer; nav_db.rs: unfiltered branch inputs and binary graph value.
- ui/core-rust/crates/app-core/src/had_ops.rs: NavDbOpenController prefetch barrier.
- ui/web-app/src/domain/navKv.ts: unrestricted fetch dispatch and installation yields.
