# NOTAM Feed Notes

This documents the current understanding of FAA NOTAM ingestion via SWIM/SCDS for Aerobag preprocessor work.

It intentionally does not include live credentials.

## Source

The realtime-ish machine-readable source we now have is FAA SWIM Cloud Distribution Service (SCDS), via a SWIFT subscription.

Observed subscription/product:
- product family: `AIM NMS Publication`
- subscription scope we requested: active Domestic + FDC NOTAMs
- transport: Solace JMS over TLS

This is not the public `notams.aim.faa.gov` search UI. It is the supported machine feed.

## Connection Model

The feed is consumed as a Solace JMS queue.

Required connection fields:
- provider URL
- queue name
- connection factory name
- username
- password
- message VPN

Credential storage convention:
- keep them out of the repo
- environment bundle: `/root/aerobag-credentials/swim-notams.environments.json`
- dev generated credential: `/root/aerobag-credentials/dev-stack/swim-notams.json`
- prod installed credential: `/etc/aerobag/secrets/swim-notams.json`
- dev and prod entries must use separate SWIM subscriptions

Important implementation note:
- FAA’s portal currently presents the broker URL as `tcps://...`
- Solace’s Java client is happier when this is normalized internally to the Solace SMF-over-TLS form
- the collector does that normalization automatically

## Collector

Standalone collector location:
- [swim-notams-fetch](/root/aerobag-preprocessor/aerobag/product/preprocessor/swim-notams-fetch)

Purpose:
- connect to the FAA queue using the vendor-supported Java/Solace path
- capture raw messages without putting secrets into the Rust workspace
- write raw messages to SQLite for Rust normalization and live-feed publication

Build:
- `gradle installDist`

Run:

```bash
product/preprocessor/swim-notams-fetch/build/install/swim-notams-fetch/bin/swim-notams-fetch \
  --config /root/aerobag-credentials/dev-stack/swim-notams.json \
  --sqlite /path/to/swim-notams/state/current.sqlite
```

Outputs:
- committed rows in SQLite table `raw_notam_messages`

The collector uses client acknowledgement, so reading from the queue is real consumption, not a harmless peek.

The live-feeds daemon snapshots the SQLite current state and emits:
- `notams.json`
- `notams_<label>.manifest.json`
- `notams_<label>.zip`

## What We Observed

Live test runs succeeded against FAA’s broker.

Observed behavior:
- connection succeeds over TLS
- messages arrive as `SolTextMessage`
- queue messages are persistent
- captured messages commit cleanly to SQLite before acknowledgement

Observed payload shape:
- body is FAA AIXM 5.1 XML
- top-level document is `AIXMBasicMessage`
- this is not a JSON feed

Observed normalized output shape:
- stable record id like `D:HLN:2026:N:198`
- JMS/NMS identifiers
- source type / status / function / keyword
- airport linkage:
  - `icao_id`
  - `location_designator`
  - `airport_name`
  - `airport_position`
- NOTAM number / year / type
- issued/effective timestamps
- plain `text`
- `local_text`
- `icao_text`
- FAA extension fields like:
  - classification
  - account id
  - cross-over account id
  - cross-over NOTAM id

Observed useful JMS properties:
- `us_gov_dot_faa_aim_fns_nds_ICAOId`
- `us_gov_dot_faa_aim_fns_nds_LocationDesignator`
- `us_gov_dot_faa_aim_fns_nds_NOTAMFunction`
- `us_gov_dot_faa_aim_fns_nds_NOTAMKeyword`
- `us_gov_dot_faa_aim_fns_nds_NOTAMStatus`
- `us_gov_dot_faa_aim_fns_nds_SourceType`
- `m_msg_last_updated`
- `m_msg_nms_id`

These properties look useful for first-pass indexing/filtering before deeper XML parsing.

## Sample Content Shape

Examples seen in live messages:
- communications outages such as remote communication outlet unserviceability
- runway/FICON condition updates

Payload content includes:
- airport / heliport objects
- runway / runway-direction objects
- event objects
- text NOTAM content
- translations including local-format text and ICAO-style formatted text
- FAA extension fields with classification and cross-reference identifiers

Useful XML elements observed:
- `event:NOTAM`
- `event:text`
- `event:effectiveStart`
- `event:effectiveEnd`
- `event:translation`
- `fnse:EventExtension`

FAA extension fields observed in XML:
- classification
- account ID
- ICAO location
- airport name
- cross-over NOTAM ID
- last-updated timestamp

## Implications For Preprocessor Design

The live-feed product path should not assume a simple REST/JSON source. The likely shape is:

1. Java collector drains queue to raw capture files.
2. Rust-side normalization step parses captured AIXM XML.
3. Fast `notams` package is emitted as JSON artifacts for clients.

Likely indexing layers:
- all NOTAM records
- airport-indexed records
- plate/procedure-indexed records where mapping is trustworthy

Plate attachment is desirable for many FDC/procedure NOTAMs, but airport fallback will still be necessary because not every NOTAM maps cleanly to a single chart or procedure object.

## Caveats

- SCDS is not intended for FAA/NAS-impacting operational use.
- For Aerobag’s advisory/non-certified use, this is acceptable enough to continue prototyping.
- Queue consumption is stateful; test runs can drain messages from the subscription queue.
- We have a Rust normalizer for captured messages, but not a live stateful live-feed publisher yet.

## Next Step

Build a persistent current-state store from the event stream, then publish that state as a real `notams` live-feed product.

We should not wire the live SWIM queue directly into the live-feed builder until that state model exists, because the queue is event-driven and stateful rather than a cheap snapshot feed.
