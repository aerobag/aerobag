# User action latency and feedback

## Find Route investigation, 2026-09-14

Reported case: KRNT KLVN, click Find Route, wait roughly four seconds before the
map opens. The exact reported browser sample has not been identified; the
measurements below are a fresh local Chrome reproduction, not that sample.

The action opens the core routing editor, resolves its endpoints, loads the
published airway graph, searches, and projects the editor with junction symbols.
Only after the resulting snapshot lands does either platform open the map.
`NeedSnapshotResources` resumes the projection rather than replaying the command.

Local optimized-WASM browser, fresh profile and real NAV25 publication:

- Before: 714 ms from action to map editor summary observed in DOM.
- The routing manifest required one page, then 129 graph pages were discovered
  together. The graph frontier took 648 ms. One additional junction-symbol page
  was needed for the snapshot.
- 131 pages including metadata/symbols transferred 698,352 compressed bytes.
- Actual graph loading/validation: 19 ms; route search: 6 ms, 8,823 nodes.
- The insertion queue yielded through `setTimeout(0)` before every page.
  Between insertion calls, 114 gaps were exactly 4 ms. This is the
  [HTML nested timer minimum](https://html.spec.whatwg.org/multipage/timers.html),
  not network time. The six fetch slots also remained occupied until installation.
- After replacing timer yields with MessageChannel tasks: 191 ms cold,
  26 ms and 26 ms for two warm repeats. Graph frontier: 129 ms; graph decode:
  16 ms; search: 5 ms. These are diagnostic samples, not statistical guarantees.
- A later fresh-profile run during startup feed arrival took 520 ms cold,
  73 ms and 27 ms warm. Its graph frontier was 433 ms, graph decode 17 ms,
  and search 5 ms. The repeated 4 ms insertion gaps stayed gone, but there was
  one 270 ms inter-insertion gap overlapping other work, including a 34 ms
  prepared NOTAM ingest. The trace does not account for that entire gap, so
  do not attribute it all to the ingest or claim the timer fix removes all
  contention. The harness begins when the map appears, not when feeds quiesce.
- Network concurrency remains six, and the first action still needs the complete
  graph. Higher RTT can still be costly across 129 requests. Do not call the
  entire frontier time network latency: it includes queueing, download,
  decompression, installation, and intervening worker tasks.

No graph format, routing algorithm, startup prefetch, or contract change was
needed for this fix. The page scheduler fix benefits all NAVKV consumers while
preserving a real event-loop yield for every installation. It is not an
empty-result shortcut, batching synchronous work, or removing responsiveness.

## Repeating the experiment

Run `node ui/web-app/scripts/airway-routing-perf.mjs` against 8085.
Optional environment: AEROBAG_E2E_URL, AEROBAG_PERF_ROUTE (two endpoints),
AEROBAG_PERF_OUTPUT. The output defaults to /tmp/aerobag-airway-routing-perf.json.
An isolated browser profile accepts the disclaimer, enters the route through
flight-plan UI, then opens/cancels the editor three times. It waits for each
cancellation to land so an old summary cannot satisfy a later measurement.

Spinner implementation and the wider UI audit are deferred in
[foreground-action-feedback-plan.md](foreground-action-feedback-plan.md).
Graph sizing and fetch options are in [airway-graph-sizing.md](airway-graph-sizing.md).
