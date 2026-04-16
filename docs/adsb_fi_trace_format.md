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

- `items[0]`: elapsed seconds since the start of the trace
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

If upstream trace shape changes, this document and the parser comment in
`playback.rs` should be updated together.
