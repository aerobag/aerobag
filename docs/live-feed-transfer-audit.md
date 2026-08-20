# Live-feed transfer audit

Date: 2026-08-20

## Bottom line

Yes: Aerobag constructs deltas first, serializes them, and then XZ-compresses the
finished delta. Full record states are compressed the same way. NEXRAD and NavKv
products use already-compressed binary/package representations instead.

The warm Android path is currently about **365 MiB/day/client** in the observed
weather, excluding TCP/TLS/IP overhead. Roughly 279 MiB is NEXRAD and 49 MiB is
winds aloft. While subscribed, web has a fixed warm rate of about **36.5 MiB/day**
for the record, NOTAM, and TFR products, then adds viewport/query-dependent
NEXRAD, winds, and obstacle resources. Today web stops its entire live-feed
subscription after 60 minutes without interaction (or immediately when the
document becomes hidden), so that rate must not be extrapolated across an
abandoned three-day browser tab. Web therefore does not have one honest
platform-wide daily number without specifying activity, visibility, map, and
flight-plan workload.

There are several clean savings. TFR semantic no-op suppression plus a keyed-array
delta was implemented during this audit. There is not evidence that a naive
NEXRAD or winds delta would help; both need product-specific work.

We do not need to leave an app running for a day. We do need to observe the
producer for a day once if we want a faithful, replayable corpus of changes that
have not happened yet. After that, the existing simulation mode can replay the
captured corpus at arbitrary speed.

## What is on the wire

The JSON record path is:

```text
normalized state
  -> compare with previous normalized state
  -> construct record / NOTAM / NavKv delta
  -> JSON serialization
  -> XZ level 6, one thread, CRC64
  -> immutable delta artifact
  -> small plain-JSON version manifest
  -> uncompressed SSE invalidation
```

The implementation uses `serde_json::to_vec_pretty` before XZ. That formatting is
not free, but XZ removes most of it.

Product transport shapes:

| Product           | Warm payload                                                                        |
| ----------------- | ----------------------------------------------------------------------------------- |
| METAR, PIREP, TAF | XZ-compressed keyed-record JSON delta; full XZ JSON on recovery                     |
| NOTAM             | XZ-compressed ordered mutation delta; roughly 2 MiB XZ checkpoint on recovery       |
| Obstacles         | XZ-compressed NavKv delta when changed; NavKv install package on recovery           |
| TFR               | Baseline: full XZ JSON every source version; no delta                               |
| NEXRAD web        | Plain state manifest plus just-in-time PNG tiles for the displayed viewport/history |
| NEXRAD Android    | Complete ZIP package for each new frame; seven frames on cold start                 |
| Winds web         | Plain state manifest/root plus just-in-time XZ NavKv pages                          |
| Winds Android     | Complete NavKv ZIP package for each changed forecast cycle                          |

The live-feed daemon sends stored bytes as-is. It does not negotiate HTTP gzip or
Brotli. `current.json`, version manifests, and SSE are consequently plain JSON;
XZ, ZIP, and PNG bodies should not receive another HTTP content encoding.

Every static live-feed resource is currently sent with `Cache-Control: no-cache`,
including immutable version, state, delta, package, and tile URLs. The production
nginx proxy does not change that behavior.

## Measurement basis

The dev-stack producer had already been running for about 26 hours when this audit
started. Its status history contains exact compressed state/delta artifact sizes
for the last 256 content versions per product:

| Product     | Samples |   Measured span | Full/state bytes | Delta bytes |
| ----------- | ------: | --------------: | ---------------: | ----------: |
| METAR       |     256 |         21.44 h |       38,959,296 |   6,040,680 |
| NEXRAD      |     256 |         21.47 h |      264,866,129 |        none |
| NOTAM       |     256 |         12.80 h |      528,363,500 |     569,076 |
| PIREP       |     256 |         21.47 h |       18,447,308 |   2,139,612 |
| TAF         |     256 |         21.44 h |       27,957,380 |     661,932 |
| TFR         |     256 |         21.44 h |       15,766,020 |        none |
| Winds aloft |       5 |         21.02 h |       63,676,502 |        none |
| Obstacles   |       1 | daemon lifetime |        2,419,926 |     239,204 |

`state_bytes` for directory products is the complete directory, not necessarily a
client request. For NEXRAD, the actual Android package is 98.78% of the directory
size in the retained sample; the Android estimate below applies that correction.
For winds, the four steady six-hour updates after startup totaled 48.74 MiB of
actual packages.

The status ring records only content-version changes. SSE counts were checked
against the attempt history as well; obstacles, for example, issues about four
metadata refresh events per day while retaining the same content version.

The estimates below include payloads, version manifests, and SSE event bodies.
They exclude response/request headers, TCP/IP/TLS overhead, retries, reconnects,
and carrier accounting.

### Warm Android estimate

| Product     |            Payload MiB/day | Manifest MiB/day | SSE MiB/day | Total MiB/day |
| ----------- | -------------------------: | ---------------: | ----------: | ------------: |
| METAR       |                       6.45 |             0.25 |        0.85 |          7.55 |
| NEXRAD      |                     278.95 |             0.21 |        0.68 |        279.84 |
| NOTAM       |                       1.02 |             2.68 |        2.34 |          6.04 |
| PIREP       |                       2.28 |             0.25 |        0.85 |          3.38 |
| TAF         |                       0.71 |             0.25 |        0.84 |          1.79 |
| TFR         |                      16.83 |             0.10 |        0.84 |         17.77 |
| Winds aloft |                      48.74 |       negligible |        0.01 |         48.75 |
| Obstacles   | no observed content update |       negligible |  negligible |    negligible |
| **Total**   |                 **354.98** |         **3.74** |    **6.41** |     **365.1** |

A cold Android cache currently installs about **23.6 MiB**: 12.15 MiB winds,
6.70 MiB for seven NEXRAD frames, 2.34 MiB obstacles, 1.98 MiB NOTAMs, and less
than 0.4 MiB for the remaining current products and control files.

### Web estimate

While actively subscribed, the fixed portion—METAR, PIREP, TAF, NOTAM, and
TFR—is about **36.5 MiB/day**. NEXRAD is much smaller than Android when only
visible tiles are requested. For the current frame, the whole-CONUS `res3` level
is 16.5 KiB, while all four levels total about 967 KiB. A local detailed view
requests a subset of the 98 `res0` tiles; those tiles range from nearly empty
hundreds-of-byte PNGs to about 65 KiB in active weather. Winds and obstacles
likewise depend on the queried route and viewport because web requests NavKv
pages rather than install packages.

This is not the behavior of an abandoned tab today. `useWebIdleState` considers
pointer, keyboard, wheel, touch, focus, and visibility input. After 60 minutes of
no activity—or immediately while hidden—the app tears down the entire live-feed
subscription. That saves bytes, but it is a blunt, web-local policy that also
stops metadata and non-NEXRAD updates. A graduated NEXRAD acquisition policy
should replace it rather than being layered on top of it.

## Findings and opportunities

### 1. TFR published semantic no-ops and had no delta — implemented

This is the cleanest payload win. Four consecutive real states in the sample had
identical canonical content after removing `version_label`; they were nevertheless
published and transferred as distinct full states. Across the 35 retained
transitions:

- 17 were semantic no-ops;
- full transfer was 2.11 MiB;
- a prototype keyed delta, including the no-op deltas, was 0.024 MiB;
- suppressing semantic no-ops reduced it to 0.021 MiB, a 99.0% payload saving.

The implementation now gives every area an explicit stable `area_id` derived from
NOTAM identity plus polygon identity because `notam_id + area_index` is not unique
in the actual state. It derives the version from normalized semantic output,
keeps full snapshots in the old array shape for compatibility, and applies the
shared record-delta envelope to that array by stable ID. The first publication
after an old snapshot is a full migration checkpoint; subsequent versions use
deltas. Older clients do not advertise TFR delta support and continue to fetch the
backward-compatible full array. This should reduce the baseline 17.8 MiB/day to
well below 1 MiB/day in similar conditions, including reduced control traffic.

### 2. Non-NEXRAD history is repeated in every SSE event but not consumed

Current events carry up to 12 history entries for every product. Core needs the
history window for NEXRAD animation and durable NEXRAD installation. The current
web and Android acquisition paths do not load historical states for the other
products; NOTAM recovery uses `recent_deltas` in its version manifest instead.

Current event sizes with and without history are:

| Product     | Current bytes | Without history | Repeated history |
| ----------- | ------------: | --------------: | ---------------: |
| METAR       |         3,129 |             369 |            2,760 |
| PIREP       |         3,129 |             369 |            2,760 |
| TAF         |         3,073 |             361 |            2,712 |
| TFR         |         3,073 |             361 |            2,712 |
| NOTAM       |         5,126 |             572 |            4,554 |
| Winds aloft |         3,305 |             353 |            2,952 |

Keeping history only for products whose client policy consumes it would save about
**5.1 MiB/day/client** at the observed update rates. It also reduces the startup
`current.json` from 28.7 KiB to roughly 5.5 KiB. Server retention does not require
exposing those entries: the pruning code independently retains version manifests
for each product's configured retention window.

For example, this is the shape of a real 3,129-byte METAR event captured at
04:46Z (hashes shortened here only for readability):

```text
id: metars:23da6b98e767082c
event: live-feed-current
data: {
  "schema_version": 3,
  "product": "metars",
  "version": "23da6b98e767082c",
  "version_manifest_url": "versions/metars/23da6b98e767082c.json",
  "state_url": "states/metars/23da6b98e767082c.json.xz",
  "state_sha256": "4156...",
  "collected_at_utc": "2026-08-20T04:46:00Z",
  "history": [ /* 12 prior version/state descriptors */ ]
}
```

The same event is 369 bytes without `history`; 2,760 bytes are repeated prior
versions that neither METAR client consumes. The complete captured record is in
the capture's `events.jsonl`.

### 3. Plain control JSON has no HTTP compression

The current `current.json` shrinks from 28,679 to 5,889 bytes with ordinary gzip
(79.5%), saving 22.8 KiB per bootstrap/reconnect. Retained NOTAM version manifests
shrink by 79.7% on average. Across the warm Android estimates, compressing version
manifests saves approximately **2.7 MiB/day/client** out of their present 3.74
MiB/day. NOTAM is currently control-plane-heavy: about 1.0 MiB/day of delta bodies
is surrounded by about 2.7 MiB/day of version manifests and 2.3 MiB/day of SSE.

Enable HTTP gzip for ordinary JSON manifests/current responses, either in the
daemon or explicitly in the production proxy. Do not recompress `.xz`, ZIP, or PNG
payloads. Streaming SSE compression needs separate latency/buffering validation;
trimming unused history is the safer first SSE optimization.

### 4. Immutable resources are marked `no-cache`

Version manifests, versioned states, deltas, packages, and versioned NEXRAD tiles
are immutable. They should use a long-lived immutable cache policy. Only
`current.json`, status, and the event stream need no-cache semantics.

This does not change the steady connected-client table because core ordinarily
requests a version once. It avoids needless retransfers across web reloads, route
revisits, and reconnect/recovery paths. The Android durable cache already provides
stronger application-level persistence for installed products.

### 5. Pretty JSON before XZ leaves a small amount on the table

Across all currently retained deltas, compact JSON before the same XZ settings
would save:

| Product   | Saving |
| --------- | -----: |
| METAR     |   3.3% |
| PIREP     |   1.3% |
| TAF       |   2.7% |
| NOTAM     |   3.1% |
| Obstacles |  25.8% |

The obstacle percentage is large because binary values serialize as very large
pretty-printed number arrays, but obstacle changes are rare. This is a cheap and
safe cleanup, not a leading daily-byte win.

### 6. NEXRAD dominates Android, but a naive delta loses

The Android client intentionally installs a complete CONUS frame so it has a
durable offline animation window. The observed producer emitted 256 changes in
21.47 hours: about one every 5.05 minutes, or 285 frames/day. At roughly 0.98
MiB per package, that costs about 279 MiB/day in this sample. Web already avoids
this by fetching visible tiles only.

Android currently does this regardless of whether the NEXRAD overlay is visible.
The durable-cache request builder unconditionally requests the current frame and
the six advertised history frames. A cadence controller therefore must select and
retain the last acquired frames, not wake periodically and ask for the producer's
last six frames. The latter would backfill nearly every skipped update and erase
the intended saving. When a client becomes active it should fetch the newest frame
immediately, skip intervening producer frames, and let its animation buffer refill
with subsequently selected frames.

At the observed package size, useful order-of-magnitude settings labels are:

| Acquisition cadence            | Transfer while in that mode |
| ------------------------------ | --------------------------: |
| Every producer update (~5 min) |                ~280 MiB/day |
| Every 10 min                   |                ~140 MiB/day |
| Every 20 min                   |                 ~70 MiB/day |
| Every 30 min                   |                 ~47 MiB/day |
| Every 60 min                   |                 ~23 MiB/day |
| Never                          |                   0 MiB/day |

These should be displayed as per-mode rates, not simply summed: active, inactive,
and asleep are mutually exclusive portions of a day. A useful overall figure is
either actual bytes over the last 24 hours or a projection weighted by the recent
time spent in each mode. Package size varies with the weather, so estimates should
use a recent rolling average and say they are estimates.

Tests on six adjacent current five-minute frames found:

- exact same-coordinate tile reuse: median 50 of 136 tiles, but only 0.7% of bytes
  because the identical tiles were almost empty;
- raw palette-index XOR compressed with zlib: median 1.29 times the target PNG
  bytes, so it was worse than sending the full package.

The existing 809-frame upstream analysis found a 0.60 median delta/full ratio for
a different whole-grid, two-minute representation. That remains encouraging for a
purpose-built temporal format, but it does not justify bolting an XOR delta onto
the current tiled PNG package.

About 28% of a current package is the three downsampled overview levels. An Android
format that transfers the finest level and deterministically derives overview
levels could save meaningful bytes, at the cost of device CPU, install complexity,
and a different durable representation. A corridor/viewport-aware Android cache
could save more, but changes the current complete-CONUS offline guarantee. Both
need explicit product work and measurement.

### 7. Winds needs an acquisition policy more than a generic delta

For a real adjacent pair, all 903 logical values and all 492 page hashes changed.
The target install package was 12.80 MB. The existing pretty-JSON NavKv delta was
19.16 MB after XZ (149.6% of full); compact JSON still produced 14.83 MB (115.8%
of full). The Android path already refuses a delta larger than its full payload.

A winds poll runs hourly, but content changed on the four normal six-hour forecast
cycles in this sample. Each Android install package was about 12.2 MiB, yielding
48.74 MiB/day.

The current representation is already Protocol Buffers, not verbose JSON: each
8-by-8 spatial tile contains nine valid times and eight pressure levels, with
validity plus quantized little-endian signed 16-bit east wind, north wind,
temperature, and geopotential-height arrays. The protobuf values live in NavKv
pages compressed with XZ and then in the install ZIP. gRPC would only provide an
RPC transport around these bytes and would not make them intrinsically tighter.

A quick lossless domain-codec experiment on a real 12,743,198-byte package gave:

| Representation before XZ          |      Bytes | Saving vs current package |
| --------------------------------- | ---------: | ------------------------: |
| Current protobuf values, combined | 12,083,000 |                      5.2% |
| Raw field arrays                  | 11,807,640 |                      7.3% |
| 2-D planar spatial predictor      |  9,909,672 |                     22.2% |
| 2-D predictor plus signed varints |  9,001,520 |                     29.4% |

The predictor confirms that adjacent grid cells contain exploitable structure;
plain varints without prediction were worse. Even the best prototype would only
reduce the automatic rate from 48.74 to roughly 34.4 MiB/day. The bigger win is
on-demand acquisition: keep cheap catalog metadata current, distinguish the
latest available forecast from the version installed and in use, and let the
altitude planner offer an explicit download such as “Using 18Z forecast (7h old).
00Z available (1h old), 12.2 MiB. Update.” Web's just-in-time page path is already
closer to this bandwidth shape.

### 8. Small protocol safeguards and startup duplication

- The durable client compares advertised delta bytes with full bytes and chooses
  full when the delta is larger. The just-in-time web path does not make the same
  comparison. None of the 256-sample record deltas was larger than its full state,
  so this caused no observed waste, but the shared selection rule should protect
  both acquisition modes.
- A new connection fetches `current.json` and also receives an initial SSE frame
  containing the same current catalog. That is about 52 KiB uncompressed today.
  It does not duplicate payload downloads and is low priority, but one of the two
  control snapshots can eventually become authoritative for bootstrap.

## The 24-hour capture

`tools/capture-live-feed-transfer.mjs` records status samples, exact version
manifest and SSE sizes, and an event log. With `--archive-artifacts`, it hard-links
immutable states/deltas/packages before the normal retention pass removes them.
Hard links avoid a second physical copy on the current filesystem and remain valid
after the producer removes its original link.

A real 24-hour capture is running at:

```text
/root/aerobag-artifacts/analysis/live-feed-transfer-20260820T0335Z
```

It started at `2026-08-20T03:32:19Z`, is scheduled to stop at
`2026-08-21T03:32:19Z`, and uses a 15-second poll interval. The detached process
was PID 4144395 when launched. `capture.json` is its heartbeat/status file;
`samples.jsonl` and `events.jsonl` are append-only measurements; the mirrored
`live-feeds/v3` directory is the replay corpus.

The existing status ring is enough to identify and size the large opportunities
now. The completed capture is useful for validating a TFR implementation, testing
control-plane changes, and replaying a representative multi-product day at high
speed. Accelerated polling against upstream today would mostly redownload the same
published products and would not synthesize the future forecast/radar/NOTAM changes
that a faithful corpus needs.

## Suggested burn order

1. Done: suppress TFR semantic no-ops and add a backward-compatible stable-key
   array delta.
2. Emit current/SSE history only for products whose acquisition policy uses it.
3. Add gzip for plain JSON control resources and immutable caching for versioned
   resources.
4. Switch XZ JSON inputs from pretty to compact serialization.
5. Apply the delta-vs-full size rule to the just-in-time path too.
6. Add a shared core acquisition controller. For web, report interaction and
   visibility to core and use every-update / 20-minute / 60-minute NEXRAD modes at
   the 0 / 20 / 120-minute thresholds. For Android, persist separate active,
   inactive, and asleep NEXRAD cadences in user state; core enforces that active
   is at least as eager as inactive and commands package acquisition. Keep SSE
   metadata flowing even when a product payload is decimated.
7. Make winds forecast packages on-demand while continuing to advertise fresh
   availability metadata; retain the spatial-predictor codec as a second-stage
   optimization if its implementation complexity earns the remaining saving.
8. Use the completed day corpus to validate all estimates and benchmark compact
   JSON/XZ against binary record-delta envelopes. Test protobuf as a serialization
   format, not gRPC as a transport.
