# Live-feed transfer audit

Baseline measured: 2026-08-20

Implementation status updated: 2026-08-22

## Bottom line

Yes: Aerobag constructs deltas first, serializes them, and then XZ-compresses the
finished delta. Full record states are compressed the same way. NEXRAD and NavKv
products use already-compressed binary/package representations instead.

The **historical baseline** warm Android path was about **365 MiB/day/client** in
the observed weather, excluding TCP/TLS/IP overhead. Roughly 279 MiB was NEXRAD
and 49 MiB was winds aloft. While subscribed, web's historical fixed warm rate
was about **36.5 MiB/day** for the record, NOTAM, and TFR products, plus
viewport/query-dependent NEXRAD, winds, and obstacle resources. These numbers
are deliberately retained as the before-optimization accounting baseline; they
no longer describe every current client policy.

Since that measurement, Aerobag has shipped TFR semantic no-op suppression and
deltas, removed unused product history from client control messages, enabled
production gzip for plain JSON, fixed immutable-resource cache headers, added
core-owned NEXRAD coverage/detail/cadence policies, and made Android winds-aloft
packages on-demand. Web still stops its entire live-feed subscription after 60
minutes without interaction (or immediately when the document becomes hidden).
Android's NEXRAD rate now depends on time spent shown, hidden, and asleep plus the
selected coverage and offline detail. There is therefore no honest single current
daily number until those mode times are specified or measured.

There is still no evidence that a naive NEXRAD or winds delta would help; both
need product-specific work. Winds no longer transfers automatically on Android,
so its codec is now a per-download optimization rather than a standing daily
cost. The completed and open work is tracked at the end of this document.

A repeatable corpus analyzer now closes the accounting loop for policies whose
requests are determined by the feed metadata. Under an explicit representative
Android day of 4 hours shown, 4 hours hidden, and 16 hours asleep, the current
on-demand-winds policy models at **58.29 MiB/day** with full-detail NEXRAD and
**26.63 MiB/day** with reduced-detail NEXRAD, between explicit forecast
downloads. Each requested winds refresh adds the currently advertised package,
about 12.2 MiB in this corpus. Viewport-only traffic remains inherently
route/view dependent and needs a client request trace rather than a producer
corpus.

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

Historical baseline product transport shapes:

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

The live-feed daemon still sends stored bytes as-is. Production nginx now enables
gzip for `application/json`, which covers `current.json` and version manifests;
XZ, ZIP, and PNG bodies correctly remain unrecompressed. SSE keeps
`no-transform` semantics and is not included in this gzip claim. Direct clients
of the development daemon likewise do not receive HTTP content encoding.

Version manifests, versioned states, deltas, packages, and tiles now receive
`Cache-Control: public, max-age=31536000, immutable`. Mutable `current.json`,
status responses, and the event stream retain no-cache/no-store semantics.

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

### 2. Non-NEXRAD history was repeated in every SSE event but not consumed — implemented

At baseline, current events carried up to 12 history entries for every product.
Core needs the history window for NEXRAD animation and durable NEXRAD
installation. Web and Android do not load historical states for the other
products; NOTAM recovery uses `recent_deltas` in its version manifest instead.

Baseline event sizes with and without history were:

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

This is now implemented in the shared product policy. NEXRAD advertises the six
prior frames consumed by its seven-frame animation window; every other product
advertises zero client history. The same rule is applied to both `current.json`
and SSE events without changing server-side retention.

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

### 3. Plain control JSON had no HTTP compression — implemented in production

The baseline `current.json` shrinks from 28,679 to 5,889 bytes with ordinary gzip
(79.5%), saving 22.8 KiB per bootstrap/reconnect. Retained NOTAM version manifests
shrink by 79.7% on average. Across the warm Android estimates, compressing version
manifests saves approximately **2.7 MiB/day/client** out of their present 3.74
MiB/day. NOTAM is currently control-plane-heavy: about 1.0 MiB/day of delta bodies
is surrounded by about 2.7 MiB/day of version manifests and 2.3 MiB/day of SSE.

Production nginx now enables gzip for ordinary JSON manifests/current responses.
It does not recompress `.xz`, ZIP, or PNG payloads. The direct daemon and SSE
remain uncompressed; streaming SSE compression still needs separate
latency/buffering validation and is not counted as completed work.

### 4. Immutable resources were marked `no-cache` — implemented

Version manifests, versioned states, deltas, packages, and versioned NEXRAD tiles
are immutable. They should use a long-lived immutable cache policy. Only
`current.json`, status, and the event stream need no-cache semantics.

The daemon now marks the immutable versioned trees with a one-year immutable
cache policy while leaving mutable endpoints uncached. This does not change the
steady connected-client table because core ordinarily requests a version once.
It avoids needless retransfers across web reloads, route revisits, and
reconnect/recovery paths. The Android durable cache already provides stronger
application-level persistence for installed products.

### 5. Pretty JSON before XZ leaves a small amount on the table — open

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

### 6. NEXRAD dominated Android — acquisition controls implemented; temporal encoding open

The Android client intentionally installs a complete CONUS frame so it has a
durable offline animation window. The observed producer emitted 256 changes in
21.47 hours: about one every 5.05 minutes, or 285 frames/day. At roughly 0.98
MiB per package, that costs about 279 MiB/day in this sample. Web already avoids
this by fetching visible tiles only.

That unconditional behavior was the historical baseline. Core now owns both
platform policies. Web fetches no NEXRAD tile bodies until the layer is shown,
then uses retained metadata to fetch only missing tiles for the visible viewport
and animation window. Hiding the layer retains fresh tiles for likely reuse but
fetches no new tile bodies. The web application still stops all live feeds after
60 minutes of inactivity or while the document is hidden.

Android now offers full-offline and visible-area-only coverage. Full-offline mode
has separate shown, hidden, and asleep cadences plus full and reduced detail
profiles; visible-area-only mode reuses the same JIT viewport planner as web.
Decimated schedules fetch only the newest schedule-allowed frame and animate the
sparse retained set. They do not backfill skipped producer frames. The default is
full detail with every update while shown, every 30 minutes while hidden, and no
NEXRAD acquisition while asleep.

At the observed package size, useful order-of-magnitude settings labels are:

| Acquisition cadence            | Transfer while in that mode |
| ------------------------------ | --------------------------: |
| Every producer update (~5 min) |                ~280 MiB/day |
| Every 10 min                   |                ~140 MiB/day |
| Every 20 min                   |                 ~70 MiB/day |
| Every 30 min                   |                 ~47 MiB/day |
| Every 60 min                   |                 ~23 MiB/day |
| Never                          |                   0 MiB/day |

Those are retained as the old all-resolution-package estimates. The new publisher
puts only the selected base level in an offline package; core derives coarser
overviews after download. In the 139 profiled manifests retained by the completed
capture, the advertised package sizes were:

| Offline profile  | Average package | Every update | Every 10 min | Every 30 min |
| ---------------- | --------------: | -----------: | -----------: | -----------: |
| Full (`res0`)    |       0.790 MiB |   9.46 MiB/h |   4.76 MiB/h |   1.62 MiB/h |
| Reduced (`res1`) |       0.225 MiB |   2.70 MiB/h |   1.36 MiB/h |   0.46 MiB/h |

These rates come from exact schedule replay across the 139 publications in the
11.60-hour profile-compatible part of the capture, rather than average package
size multiplied by a nominal cadence. Reduced detail was 28.6% of full-detail
bytes in that sample. Settings display per-mode MiB/h using the currently
advertised package sizes rather than summing shown, hidden, and asleep, which are
mutually exclusive conditions. A personalized daily figure still requires time
spent in each condition; recording actual bytes and condition residence over a
rolling 24-hour window remains optional product telemetry, not a gap in the
policy simulator.

Tests on six adjacent current five-minute frames found:

- exact same-coordinate tile reuse: median 50 of 136 tiles, but only 0.7% of bytes
  because the identical tiles were almost empty;
- raw palette-index XOR compressed with zlib: median 1.29 times the target PNG
  bytes, so it was worse than sending the full package.

The existing 809-frame upstream analysis found a 0.60 median delta/full ratio for
a different whole-grid, two-minute representation. That remains encouraging for a
purpose-built temporal format, but it does not justify bolting an XOR delta onto
the current tiled PNG package.

The two immediate representation/policy ideas are implemented: offline packages
transfer one base level and deterministically derive coarser levels, and Android's
visible-area-only option trades the complete-CONUS offline guarantee for the JIT
viewport path. A purpose-built temporal codec remains open and should be revisited
only after measuring actual residence-weighted usage under the new policies.

### 7. Winds needed an acquisition policy more than a generic delta — implemented

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

That acquisition policy is now implemented in core. Android's durable cache
continues to fetch the current version manifest and roughly 1.2 KiB atmosphere
state manifest so core can show the available cycle, validity, age, and package
size. It does not fetch the NavKv install package until the user invokes the
core-issued action on the Altitude Planner. An older installed forecast remains
usable while a newer cycle is advertised, and the page clearly separates the
downloaded forecast from the available one. Core owns the visible
`DOWNLOAD REQUESTED`, `DOWNLOADING WINDS`, and `INSTALLING WINDS` phases;
Android reports transport progress but does not invent UI state. Installing the
advertised version clears the transient request automatically.

The downloaded NavKv ZIP is persisted byte-for-byte. Installation validates its
blob, manifest, and root without expanding its XZ pages or constructing a larger
client-side archive. When planning needs a page, core issues a package-member
resource request and Android reads and decodes only that member. Full-package
preparation and persisted-package restoration also run outside the cache locks;
only the validated-state swap is serialized.

The request is deliberately session-local navigation/application state. It is
not a setting, does not sync through cloud, and is not confused with the user's
selected `FORECAST` versus `NO WIND` planning model. Web retains its existing JIT
NavKv page acquisition. The Data Status page reports an uninstalled Android
forecast as informational `ON DEMAND`, not as a false unavailable-data alert.

### 8. Small protocol safeguards and startup duplication — open

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

A real 24-hour capture completed at:

```text
/root/aerobag-artifacts/analysis/live-feed-transfer-20260820T0335Z
```

It ran from `2026-08-20T03:32:19Z` through `2026-08-21T03:32:19Z` at a
15-second poll interval and finished with state `complete`: 5,740 polls, 3,335
product samples, and 1,946 observed current events. `samples.jsonl` and
`events.jsonl` are append-only measurements; the mirrored `live-feeds/v3`
directory is the replay corpus.

The corpus occupies about 1 GiB because it is a producer archive, not a trace of
one client's requests. It preserves full states, deltas, tile directories, and
install packages together. The largest components are 429 MiB of NEXRAD state
directories, 123 MiB of NEXRAD packages, 118 MiB of winds state data, and 110 MiB
of winds packages. A warm client chooses only one applicable transfer path.

The run crossed several implementation changes: TFR deltas, client-history
trimming, and NEXRAD profile publication landed during the capture. It is useful
for transition testing and codec experiments, but it is not a clean all-before or
all-after daily benchmark. The capture tool at the time also did not archive
`install_profiles`; it retained their manifests and underlying NEXRAD state tiles
but not the new profile ZIPs. The checked-in tool now archives those refs for
future captures. Retain this corpus; do not commit its approximately 1 GiB of
artifacts to git.

### Repeatable corpus accounting

`tools/analyze-live-feed-transfer.mjs` replays the captured event and artifact
corpus as a warm client. It reconstructs the historical and current SSE history
policies, accounts for each unique version manifest, models production gzip at
level 6, selects applicable deltas only when they beat the full payload, and
replays exact NEXRAD cadence decisions without backfilling skipped frames.

Run the checked-in analysis with:

```text
node tools/analyze-live-feed-transfer.mjs \
  --capture /root/aerobag-artifacts/analysis/live-feed-transfer-20260820T0335Z \
  --residence-hours 4,4,16
```

Use `--format json` for machine-readable output. Initial snapshots establish the
warm cache and are excluded from recurring totals. The analyzer recovered 42
NOTAM manifests archived under their equivalent post-compaction names and found
no unresolved manifests among 1,938 observed changes.

The exact full-day control-plane result is:

| Component         | Historical |  Current | Saving |
| ----------------- | ---------: | -------: | -----: |
| SSE history       |   7.00 MiB | 1.35 MiB |  80.7% |
| Version manifests |   5.91 MiB | 1.34 MiB |  77.4% |
| Combined control  |  12.91 MiB | 2.69 MiB |  79.2% |

This replaces the earlier projection with corpus replay: history trimming saves
5.65 MiB/day and manifest gzip saves 4.57 MiB/day in this capture. Gzip bytes are
a deterministic level-6 model over the exact archived entity bodies; HTTP
headers, TLS, retries, and carrier accounting remain excluded.

The full-day non-NEXRAD Android payload replay is:

| Product     | Historical | Captured current | Saving |
| ----------- | ---------: | ---------------: | -----: |
| METAR       |   6.51 MiB |         6.51 MiB |   0.0% |
| NOTAM       |   1.21 MiB |         1.21 MiB |   0.0% |
| PIREP       |   2.48 MiB |         2.48 MiB |   0.0% |
| TAF         |  766.5 KiB |        766.5 KiB |   0.0% |
| TFR         |  11.20 MiB |         9.08 MiB |  18.9% |
| Winds aloft |  48.56 MiB |        48.56 MiB |   0.0% |

The full-day TFR number is deliberately conservative because the capture began
before the implementation landed. In the fully implemented 11.60-hour window,
TFR delta selection reduced 2.29 MiB of full states to 170.4 KiB, a 92.7%
reduction; suppressed semantic no-op publications are absent and would save
additional bytes.

The same fully compatible window gives exact NEXRAD transfer counts:

| Profile | Cadence      | Frames |      Bytes | Observed rate |
| ------- | ------------ | -----: | ---------: | ------------: |
| Full    | Every update |    139 | 109.76 MiB |    9.46 MiB/h |
| Full    | 10 minutes   |     70 |  55.20 MiB |    4.76 MiB/h |
| Full    | 30 minutes   |     24 |  18.77 MiB |    1.62 MiB/h |
| Reduced | Every update |    139 |  31.33 MiB |    2.70 MiB/h |
| Reduced | 10 minutes   |     70 |  15.76 MiB |    1.36 MiB/h |
| Reduced | 30 minutes   |     24 |   5.36 MiB |    0.46 MiB/h |
| Either  | Never        |      0 |        0 B |    0.00 MiB/h |

For the explicit 4/4/16-hour residence split, combining exact full-day control
and non-NEXRAD observations with the fully implemented TFR rate and observed
NEXRAD rates produces:

| Scenario                           | Modeled 24-hour bytes | Saving vs reference |
| ---------------------------------- | --------------------: | ------------------: |
| Current-format reference           |            310.66 MiB |                0.0% |
| Current, full                      |            106.85 MiB |               65.6% |
| Current, reduced                   |             75.19 MiB |               75.8% |
| Current, full + winds on demand    |             58.29 MiB |               81.2% |
| Current, reduced + winds on demand |             26.63 MiB |               91.4% |

The reference uses raw control JSON, historical TFR full-state transfer,
automatic winds, and full-profile NEXRAD on every publication. It is lower than
the original 365.1 MiB/day historical baseline because this corpus can compare
the new single-base-resolution NEXRAD packages exactly but cannot reconstruct the
discarded older all-resolution ZIPs. Web and Android visible-area-only NEXRAD,
web winds, and web obstacles require viewport/route request traces and are
intentionally not assigned fabricated totals.

## Implementation ledger

| Item                                     | Status | Baseline accounting effect                                        |
| ---------------------------------------- | ------ | ----------------------------------------------------------------- |
| TFR semantic no-op suppression and delta | Done   | Delta-only replay saved 92.7% in the fully current window         |
| Client history only where consumed       | Done   | Measured saving of 5.65 MiB/day/client                            |
| Production gzip for control JSON         | Done   | Modeled manifest saving of 4.57 MiB/day/client                    |
| Immutable resource cache policy          | Done   | Avoids reload/recovery retransfers; no steady-state table change  |
| Core-owned NEXRAD acquisition policies   | Done   | Replaces one 279 MiB/day behavior with mode-dependent rates       |
| Completed day capture corpus             | Done   | Preserves the original evidence and source material               |
| Repeatable corpus policy analyzer        | Done   | Exact replay plus explicit 4/4/16-hour daily policy projections   |
| Compact JSON before XZ                   | Open   | 1-3% for active record deltas; 25.8% for rare obstacle deltas     |
| JIT delta-versus-full size guard         | Open   | No waste observed in the baseline; defensive correctness          |
| Winds on-demand immutable lazy package    | Done   | Removes 48.6 MiB/day baseline and avoids page expansion/repacking |
| Current/SSE bootstrap deduplication      | Open   | About 52 KiB per connection in the historical uncompressed form   |
| NEXRAD temporal codec                    | Open   | Potentially material, but new policy-weighted usage is unmeasured |

## Recommended next burn order

1. Switch XZ JSON inputs from pretty to compact serialization. It is a contained,
   low-risk cleanup with modest recurring savings and a large percentage win on
   rare obstacle deltas.
2. Apply the delta-versus-full size rule to the JIT path. This did not waste bytes
   in the baseline sample, so treat it as a protocol safeguard rather than a major
   rate reduction.
3. Reconsider a NEXRAD temporal codec only after actual mode-residence accounting
   shows that full-offline acquisition remains dominant. The naive tile XOR
   experiment remains a loss; any follow-up needs a domain-specific format.
4. Consider the measured winds spatial predictor only if explicit download size
   proves painful. It saved 29.4%, but no longer reduces an automatic daily cost.
5. Leave current/SSE bootstrap deduplication and possible SSE compression until
   the larger work is measured. They are control-plane polish, not leading wins.
