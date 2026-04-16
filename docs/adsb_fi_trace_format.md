# adsb.fi Trace Format Notes

These notes document the inferences currently baked into our playback parser for
`globe.adsb.fi` / `tar1090`-style `trace_full_<hex>.json` files.

This is not an official upstream schema document. It is a local description of
what we have observed in saved traces and what `app-core` currently depends on.

## Root shape

The trace file root is a JSON object. Relevant top-level keys we currently use:

- `r`: registration, e.g. `N550AR`
- `icao`: Mode S / ICAO hex, e.g. `A6FF7B`
- `t`: aircraft type, e.g. `C172`
- `trace`: array of positional trace rows

## Trace row shape

Each `trace` row is a positional JSON array.

For the sample trace we are actively using:

- file: `/root/aerobag-three/adsb-traces/n550ar/n550ar-2024-09-29.json`
- rows: `2548`
- tuple length `14`: `2548`

So in that file every row is length-14 and carries the positional fields below.

## Positional fields we currently rely on

The playback parser uses only the first six positions:

- `items[0]`: seconds after the root `timestamp`, which is normally UTC
  midnight for historical daily trace files
- `items[1]`: latitude
- `items[2]`: longitude
- `items[3]`: altitude feet
- `items[4]`: speed knots
- `items[5]`: track / orientation degrees

`app-core` currently ignores the rest of the tuple.

## Embedded named object

Some rows also carry an embedded object later in the tuple. In the sample file:

- rows with embedded object: `637`
- object index: always `8`
- rows where embedded object has named `track`: `633`

Where that embedded object exists and has `track`, it matched `items[5]` in the
sample exactly within normal rounding tolerance:

- `items[5] == object.track`: `633`
- mismatches observed: `0`

So we treat the named object as a sanity check on the positional tuple, not as
the primary schema contract.

## Current parser contract

`ui/core-rust/crates/app-core/src/playback.rs` assumes:

- root is an object
- `trace` is an array
- each usable row is an array with at least 6 elements
- the first 6 elements mean what is listed above

For playback, the parser normalizes `items[0]` by subtracting the first usable
row's timestamp, so UI clocks are relative to the first trace point rather than
UTC midnight. The original wall-clock gaps are preserved as relative gaps.

Rows separated by more than two minutes are treated as ADS-B reception gaps.
Those spans are exposed to the UI so it can draw a no-reception hash pattern,
and playback skips across them instead of interpolating fake aircraft motion.

If upstream trace shape changes, this document and the parser comment in
`playback.rs` should be updated together.
