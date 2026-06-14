## Live feeds:

Let's talk about correcting how we think about live feeds.
Up until now, they've inherited almost everything from the slow (cycles & stable) pipeline.
A big chonky build, the client polls via current_artifacts, if the pointer moves, the
client fetches the updated bundles. We have hacky stuff in the clients to poll and refresh
the live feeds, different on each client, eww.

Let's completely revisit how we do this stuff, and make a better **plan**.  Path I see:

How do we detect that upstream products have released updated data?
Probably polling, mostly. The NOTAM product has some fancy watch-shaped interface.
We may want some sort of adaptive polling that learns that product X is always updated
every five minutes, so we start polling 0.9*5*60 after the last publication time.

How do we advertise available products to clients? Presently, we couple the notion of
"family available" to "version available" via current-artifacts. Instead we should decouple
these: Some static "live-feeds-manifest.json" will provide the URLs to the client-facing
endpoints for the live feeds; that knowledge will last the life of the application session.

How does the client detect that new data is available? And how does the client retrieve the
new data? These might be coupled, because I suspect for some data streams (TFRs, NOTAMs, maybe
METARs) the changes might often be smallish, and re-sending the whole thing is inefficient.
(We care a little about efficiency because clients may be trying to sip data through a crappy
4G connection at 6,000 MSL.) Options:

1. Do what we do now, maybe per-product: have a distinguished URL; client polls it for changes
with an HTTP cache-invalidation query.

2. Hanging get: client keeps a hanging get open so the server can notify it right away on data,
reducing latency (important for weather!) and obviating wasteful frequent polling. If the hanging
get dies, we fall back to #1. (How would we know if it had died? A heartbeat? Is that as bad
as re-polling? :grimacing:). On invalidation, the client gets a whole copy of the new data. That
is, the GET responds with the data.

3. Watch (Adya-style) protocol: Client hangs on a GET, server and client have agreed what data
version thet client has. On new data, server transmits a delta that transforms client's data into
a newer version.

4. polling-delta: Client has version X. It polls the server for "newest-version-based-on-X".
If the server has version Z, and has a good delta, it might send Z-X. Otherwise it just sends Z.

Perhaps, before we build anything complicated, we should measure! we have a few hours of
historical live-feed data on root@aerobag-prod.iac.jonh.net. Let's go study each product:
- How frequently does the product change?
- How big is the delta between adjacent copies of the product? (Answering this may require
writing per-product diffs.)

We should also discuss how the client will manage these products.
- All the management should be in core. Whatever fancy thing we do, if we need help from web-
or android- to open an HTTPS connection or a websocket or a hanging get, fine. But the platform-
part should not have any idea which product it's helping with or what the payload is. All the
mechanism goes in core so it doesn't diverge between platforms.
- core needs a way to be woken up on invalidation (or an a timer, if we don't have invalidations
from the server), and a way to prompt the ui layer to repaint the new product.
- core should already manage which layers are visible. Core should have a way to control
which are being fetched (and we may surface that in the UI). That is, the user may not want
to waste bandwidth fetching NEXRAD they're not looking at -- or, they may want to keep that
data hot just in *case* they want to look at it.

# product-independent protocol

## invalidation

Use Server-Sent Events as the client invalidation channel. The client opens one long-lived HTTP
stream, and the service emits small product-version notifications when fresh data is published.
This is a standard fit for one-way server-to-client updates: browsers support `EventSource`,
native clients can consume it as streaming HTTP, reconnect behavior is well understood, and
`Last-Event-ID` gives us a useful resume hook.

The invalidation event should be tiny and product-independent:

```text
id: metars-20260513T184000Z
event: product-update
data: {"product":"metars","version":"20260513T184000Z"}
```

The event does not carry product data. It only wakes core and tells it that a product version may
be newer than the version installed locally. Core then decides whether to fetch a delta, a full
state, or nothing. Platform layers only provide the transport for the SSE stream and payload fetches;
they do not know product names, schemas, delta rules, cadence, or publication contract details.

For recovery and startup, also publish a small current-state document:

```text
/live-feeds/current.json
```

`current.json` is the durable source of truth for the latest version of each product. On startup,
or after an SSE disconnect where `Last-Event-ID` cannot bridge the gap, core fetches `current.json`
and reconciles local product versions. The SSE stream is the low-latency wakeup path; `current.json`
is the correctness and recovery path.

The intended scaling shape is: one tiny live notification fanout, followed by clients independently
fetching immutable state/delta payloads from static storage or a CDN. A static-only service can
host `current.json` and all payloads, but true low-latency invalidation still needs a live push
mechanism somewhere, such as SSE fanout.

## deltas and payload verification

Use immutable full states plus adjacent deltas as the baseline protocol. When a publisher discovers
fresh product data, it constructs state `N+1`, constructs `delta(N, N+1)`, uploads the immutable
state, delta, and version manifest, then updates `current.json`, then emits the SSE invalidation.
That ordering ensures that any client hearing about `N+1` can immediately fetch the referenced
payloads.

The basic invariant is:

```text
apply(delta from A to B, state A) == state B
sha256(canonical_state_B) == B.state_sha256
```

Each product version should have a manifest with enough information to fetch the full state, walk
backward through recent adjacent versions, and verify every byte and decoded state:

```json
{
  "product": "metars",
  "version": "20260513T184000Z",
  "previous": "20260513T183500Z",
  "state": {
    "url": "states/20260513T184000Z.json.zst",
    "bytes": 1234567,
    "blob_sha256": "...",
    "state_sha256": "..."
  },
  "delta_from_previous": {
    "from_version": "20260513T183500Z",
    "from_state_sha256": "...",
    "to_version": "20260513T184000Z",
    "to_state_sha256": "...",
    "url": "deltas/20260513T183500Z__20260513T184000Z.json.zst",
    "bytes": 34567,
    "blob_sha256": "..."
  }
}
```

Hash the compressed payload bytes and the canonical logical state separately. `blob_sha256`
detects a corrupt or wrong download. `state_sha256` proves that decoding a full state, or applying
a delta chain, produced the intended product state independent of compression format.

Client behavior:

- For a full state, fetch the blob, verify `blob_sha256`, decode it, canonicalize the logical
  state, verify `state_sha256`, then install it.
- For a delta, verify the local state matches `from_version` and `from_state_sha256`, fetch the
  delta blob, verify `blob_sha256`, apply it, canonicalize the result, verify `to_state_sha256`,
  then install it.
- If the client can walk from its local version to current through available adjacent deltas, it
  applies the chain.
- If any delta is missing, too old, fails hash verification, or produces the wrong resulting state
  hash, discard the partial result and fetch the full current state.

Adjacent-only deltas are the first target. Skip deltas, such as `N-12 -> N`, can be added later if
measurements show catch-up through adjacent deltas is too slow or too expensive.

# products

Summary model: `cold read` is the first full product fetch. `daily data volume` assumes the cold
read has already been paid and the app remains open for a day, receiving the modeled update stream.

```text
Product      Mode                              Cold read (MB)  Daily data volume (MB)
TFRs         record delta                      0.035           0.05
Obstacles    delta                             9.10            0.001

METARs       station delta                     1.18            10-21
Winds Aloft  full                              10.2            41
NEXRAD       16:1 reduction + palette + delta  0.032           14

NEXRAD       raw source tif.gz                 3.02            2174
NEXRAD       palette compression               0.90            648
NEXRAD       palette compression + delta       0.90            386
NEXRAD       lossless RGBA delta               3.02            1090
NEXRAD       16:1 reduction                    0.27            197
NEXRAD       16:1 reduction + palette          0.032           24
```

Too cheap to meter: TFRs, Obstacles
The user should probably be in control of the other pulls: do we really
need ~2 minute updates on NEXRAD when using the web browser to flight plan?

Delta is critical for all but winds aloft.

## METARs

Measured against the local snapshot in `~/aerobag-five/tmp-fast-product-analysis`:

- 695 METAR packages from 2026-05-08T23:19Z through 2026-05-12T01:10Z.
- Probe cadence was usually 6 minutes, so measured update rate is capped by that cadence.
- Whole-package size is stable: median 1.174 MB, mean 1.176 MB.
- A station-keyed diff is much smaller. Comparing adjacent `metars.json` files by station id,
  using `(raw_text, observed_at_utc)` as the record identity, then gzipping a JSON payload of
  changed/added station records plus removed station ids:

```text
Metric                                Value
comparisons                           694
full_package_median_MB                1.174
full_package_mean_MB                  1.176
station_delta_gzip_median_MB          0.030
station_delta_gzip_mean_MB            0.035
station_delta_gzip_p90_MB             0.071
station_delta_gzip_max_MB             0.193
```

The hourly ATIS/METAR rollover wave is real, but in this sample the quietest period was around
`:49` through `:53`; the heavy churn lands mostly around `:56` through `:05`.

```text
Phase          n    median_MB  p90_MB  max_MB  median_touched
quiet_49_53    63  0.007      0.009   0.093   192
rollover_56_05 121 0.072      0.104   0.188   1914
middle_rest    510 0.026      0.049   0.193   698
```

Network-demand model:

```text
HourlyModel                           median_MB  p90_MB  max_MB  median_updates_per_hour
station_delta_gzip_actual_6min_probe  0.35       0.40    0.56    10
full_package_actual_6min_probe        11.71      12.97   15.43   10
station_delta_gzip_12x_mean_update    0.43       0.86    n/a     12
full_package_12x_mean_update          14.11      n/a     n/a     12
```

Interpretation:

- Whole-package polling every 5 minutes is roughly 1.2 MB/update, about 14 MB/hour.
- Station-level gzipped deltas are typically 0.03 to 0.04 MB/update, with rollover-wave updates
  around 0.07 MB median and 0.10 to 0.19 MB in the heavy cases.
- Expected METAR flow with station deltas is roughly 0.4 to 0.9 MB/hour.

Plan:

- Make METARs delta-native using station id as the key.
- Keep an explicit version id for the full METAR set and for each delta.
- Delta payload should include changed station records, added station records, and removed station
  ids. Core applies the delta and owns all product state.
- Server may fall back to a whole snapshot when the client's base version is too old or unknown.
- Platform layers only provide transport; they do not know METAR schema, delta semantics, product
  cadence, or publication contract details.

## TFRs

Measured against the same local snapshot:

- 700 TFR packages from 2026-05-08T23:19Z through 2026-05-12T01:10Z.
- Probe cadence was usually 6 minutes.
- Each package currently contains one `tfrs.json`.
- A logical TFR delta can key records by `(notam_id, area_index)`, then send changed/added area
  records plus removed keys.

```text
Metric                              Value
areas                               median=58.0 p10=55.8 p90=63.0 min=55.0 max=66.0
full                                median=34599.0 p10=33474.4 p90=36500.4 min=32446.0 max=37490.0
changed                             median=0.0 p10=0.0 p90=0.0 min=0.0 max=1.0
added                               median=0.0 p10=0.0 p90=0.0 min=0.0 max=5.0
removed                             median=0.0 p10=0.0 p90=0.0 min=0.0 max=5.0
touched                             median=0.0 p10=0.0 p90=0.2 min=0.0 max=7.0
delta_gz                            median=136.0 p10=135.0 p90=138.6 min=133.0 max=1478.0
empty_delta_count                   629
empty_delta_pct                     90.0
```

Network-demand model:

```text
HourlyModel                         median_MB  p90_MB  max_MB  median_updates_per_hour
tfr_delta_gzip_actual_6min_probe    0.001      0.003   0.004   10
tfr_full_package_actual_6min_probe  0.335      0.356   0.430   10
tfr_delta_gzip_12x_mean_update      0.002      n/a     n/a     12
tfr_full_package_12x_mean_update    0.399      n/a     n/a     12
```

Interpretation:

- Whole-package polling every 5 minutes is already cheap: roughly 34 KiB/update, about
  0.4 MB/hour.
- TFRs are extremely stable in the sampled data: 90% of adjacent probes had no logical changes.
- Delta payload body cost is effectively noise: around 0.002 MB/hour at 12 checks/hour in this
  dataset. A "no change" response would make the common case mostly request/header overhead.

Plan:

- TFRs can use the same delta protocol shape as METARs: base version, target version, changed,
  added, removed.
- Because full TFR snapshots are tiny, implementation complexity should stay modest. It is
  acceptable to fall back to full snapshots aggressively.
- The biggest win is not bandwidth alone; it is having a uniform live-feed update mechanism
  where the server can cheaply say "no change" for 90% of checks.

## NEXRAD

Important measurement distinction: the existing Aerobag NEXRAD package is a postprocessed
Avare-style product, not the upstream product. It fetches three upstream MRMS frames, warps them,
converts them to PNG, resizes them to 25%, and packages `nexrad.json` plus `frame_0.png`,
`frame_1.png`, and `frame_2.png`. That format bakes in an animation policy and creates
redundancy, so it is the wrong basis for deciding the long-term live-feed protocol.

The upstream product we should study is:

```text
https://mrms.ncep.noaa.gov/data/RIDGEII/L2/CONUS/CREF_QCD/
CONUS_L2_CREF_QCD_YYYYMMDD_HHMMSS.tif.gz
```

Prod currently polls NEXRAD mostly every 6 minutes, but each poll fetches three upstream frames:
latest, latest-approximately-10-minutes, and latest-approximately-20-minutes. Sorting all fetched
upstream filenames therefore reveals NOAA's underlying publication cadence, which is about
2 minutes.

```text
Thing                         Cadence
NOAA upstream frame cadence   ~2 min
prod polling cadence          ~6 min
current product fetch/run     3 frames/run
```

The locally copied upstream study run is:

```text
/root/aerobag-five/tmp-fast-product-analysis/upstream-nexrad/2026-05-11T170642Z_2026-05-12T202440Z
```

```text
count      818
bytes      2630211340
GB         2.45
first      CONUS_L2_CREF_QCD_20260511_170642.tif.gz
last       CONUS_L2_CREF_QCD_20260512_202440.tif.gz
median_MB 3.020
```

The copied tree contains some duplicate upstream frames because separate prod runs can fetch the
same NOAA frame. The delta analysis below de-duplicates by upstream filename and uses 809 unique
frames.

Full prod upstream cache size, before narrowing to the local study run:

```text
count             bytes        span_start            span_end
9156              38959553240  2026-04-28T19:38:39Z  2026-05-12T20:24:40Z

metric            value_MB
min               1.638
p10               2.968
median            4.095
mean              4.058
p90               5.142
max               5.890
```

The upstream `.tif.gz` files are not raw/uncompressed. They decompress to LZW-compressed GeoTIFFs:
7000x3500 RGBA, EPSG:4326, with georeferencing metadata. PNG is lossless, and full-resolution PNG
is about the same size as upstream `.tif.gz` for this data.

Sampled 40 frames from the local study run:

```text
Metric        tif_gz_MB  lzw_tif_MB  png_MB  png_vs_tif_gz
min           2.614      3.663       2.498   0.95x
p10           2.751      3.809       2.628   0.96x
median        3.032      4.061       2.958   0.97x
mean          3.079      4.099       3.003   0.97x
p90           3.418      4.419       3.401   0.99x
max           3.596      4.581       3.571   1.00x
sample_count  40
```

Interpretation before deltas:

- If clients receive every upstream frame at NOAA's ~2-minute cadence, full-frame delivery costs
  roughly 30 frames/hour * 3.1 MB/frame = about 90 MB/hour in the sampled run.
- If clients receive the latest frame every 5 minutes, full-frame delivery costs roughly
  12 frames/hour * 3.1 MB/frame = about 37 MB/hour.
- PNG alone does not materially improve full-resolution bandwidth versus upstream `.tif.gz`.
- The current Avare-style three-frame package is not a good long-term model: it sends an animation
  bundle instead of a stream of independently versioned frames.

Palette and delta analysis artifacts are preserved in:

```text
docs/nexrad/analysis/
```

The working upstream sample and generated reports also live outside the repo at:

```text
/root/aerobag-five/tmp-fast-product-analysis/upstream-nexrad/
```

The relevant preserved scripts/reports are:

```text
analyze_nexrad_palette.py
analyze_nexrad_index_deltas.py
whole-day-greedy-255-palette.json
whole-day-greedy-255-palette-report.json
whole-day-index-delta-report.json
```

The full-day upstream rendered rasters have far fewer colors than general RGBA imagery, but more
than a single PNG8 palette can represent exactly:

```text
frames                 818 copied files, 809 unique upstream filenames
unique_opaque_rgb      1566
alpha_values           [0, 255]
per_frame_min_colors   489
per_frame_median       586
per_frame_max_colors   698
```

A fixed palette with one transparent slot plus 255 opaque radar colors had this error across the
whole sampled day:

```text
palette_size              255 opaque + 1 transparent slot
max_rgb_channel_error     5
p50_rgb_channel_error     2
p90_rgb_channel_error     3
p95_rgb_channel_error     4
p99_rgb_channel_error     4
p999_rgb_channel_error    5
```

Source cadence is about 2 minutes, so a fully fresh stream is about 30 frames/hour. Measured median
network rates:

```text
Mode                                      MB/frame  MB/hour
raw source tif.gz                         3.021     90.6
palette compression                       0.901     27.0
palette compression + delta               0.537     16.1
lossless delta from original RGBA         1.512     45.4
16:1 reduction                            0.274     8.2
16:1 reduction + palette compression      0.032     1.0
16:1 reduction + palette + delta          0.019     0.6
```

Definitions:

- `palette compression` is a zlib-compressed 8-bit index frame using the fixed 255-color opaque
  palette above, with index 0 reserved for transparency. It is lossy relative to upstream RGBA,
  with max channel error 5 in this sample.
- `palette compression + delta` is an adjacent-frame byte delta of those palette indices. It is
  lossless relative to the palettized frames.
- `lossless delta from original RGBA` is an adjacent-frame byte delta of the decoded RGBA source
  and is lossless relative to the upstream rendered raster.
- `16:1 reduction` is 4:1 reduction in each axis, matching the scale of the current Avare-style
  downsample. This analysis used a local reducer and is not byte-identical to ImageMagick output.

Interpretation:

- If we want full upstream resolution at 2-minute freshness, fixed-palette compression cuts the
  stream from about 90 MB/hour to about 27 MB/hour.
- Temporal deltas on the full-resolution palette-indexed stream help, but only by about 40%;
  clouds move enough that this is not an order-of-magnitude win.
- A lossless RGBA delta is useful compared with raw source delivery, but still materially larger
  than the fixed-palette lossy stream.
- The current 4:1-per-axis downsample is the huge bandwidth lever. Combined with palette and delta
  coding, it makes a 2-minute stream well under 1 MB/hour in this sample.

Plan/questions for optimization:

- Keep analysis based on upstream MRMS frames, not the postprocessed PNG package.
- Deliver this product all-or-nothing for now; do not pursue viewport tiling or region clipping as
  the primary optimization.
- Decide what visual fidelity we want: full resolution with fixed palette, or the 4:1-per-axis
  downsample with fixed palette.
- If using a fixed palette, build it intentionally from source radar colors rather than relying on
  generic PNG8 quantization.
- Temporal deltas are worth keeping in the design, especially for reduced/indexed frames, but they
  should not be the only compression strategy.
- Preserve the option to provide ~2-minute fresh data to clients; prod currently does not poll that
  often, but NOAA appears to publish at that cadence.

## Winds Aloft

Winds aloft is a much lower-cadence product than METARs, TFRs, or NEXRAD. It is issued about every
6 hours, so whole-package delivery is only about 4 updates/day.

The local live-feed samples are already zipped packages around 9.6-9.8 MiB each:

```text
sample_count          9
package_size_range    9.63-9.80 MiB
approx_daily_rate     39 MiB/day
```

Interpretation:

- Delta delivery is probably not useful enough to prioritize. Each issuance is far apart in time,
  and the daily whole-package bandwidth is already modest.
- If winds aloft becomes part of a uniform live-feed delta protocol, it can participate, but it
  should not drive the design. Straight whole-package refreshes are likely fine for the first
  implementation.

## Obstacles

Obstacle updates are the opposite case from winds aloft: the product is a good fit for exact
record-level deltas, and the delta payload appears to be tiny compared with a full refresh.

The important measurement result is that accurate deltas were straightforward to compute, and the
delta size is on the order of `1e-4` of the full product size.

Interpretation:

- Delta delivery is critical for obstacles. Whole-package refreshes would waste nearly all of the
  bytes in the common case.
- The product should use stable obstacle identity plus exact added/changed/removed records, not a
  lossy or spatial approximation.
- Obstacles are a strong candidate for the shared live-feed delta protocol because they provide
  an outsized bandwidth win without requiring the more complex image-specific machinery needed for
  NEXRAD.

------------------------------------------------------------------------------

