# NMS NOTAM Collector

This note records the NMS API behavior and the evidence behind Aerobag's NOTAM
collector. Operational architecture is in [NOTAMS.md](NOTAMS.md).

## API

Aerobag requests both `DOMESTIC` and `FDC`.

- Initial state: `GET /notams/il/{classification}?allowRedirect=false`
- Updates: `GET /notams?lastUpdatedDate=<RFC3339>`
- Authentication: OAuth2 client credentials
- Response format: AIXM

The Initial Load request returns a content URL. The client accepts relative
service URLs and HTTPS URLs. It only sends the bearer token to the API origin;
external signed content URLs do not receive credentials.

## Standalone Commands

Fetch and validate a diagnostic Initial Load capture:

```bash
cargo run -p nms-notams-fetch -- fetch \
  --config /operator/secrets/nms-notams.json \
  --output /operator/captures/nms-il-20260724T120000Z
```

Run the durable collector:

```bash
cargo run -p nms-notams-fetch -- collect \
  --config /operator/secrets/nms-notams.json \
  --state-root /operator/state/nms-notams
```

Optional collector controls are `--poll-seconds`, `--overlap-seconds`,
`--duration-seconds`, and `--max-polls`. Production defaults are a 180-second
poll interval and a 600-second overlap.

Inspect one AIXM document:

```bash
cargo run -p nms-notams-fetch -- inspect \
  --input update.xml \
  --classification DOMESTIC
```

Captures retain the compressed response, decompressed XML, canonical JSON, parse
diagnostics, checksums, source environment, API URL, and capture time. A capture
is published atomically only after record counts and normalization validate.

## State Rules

NMS IDs are environment-local and are the primary current-state keys. Human
location/year/type/number identities remain metadata and are used to resolve
referenced cancellations, but cannot be primary keys because the Initial Load
contains active collisions.

Poll updates are sorted by source `lastUpdated` time before application. Raw
payload hashes suppress overlap duplicates. Updates cannot overwrite a newer
stored record, cancellation removes either the record itself or its uniquely
matched referenced identity, and expired records are pruned. All mutations and
cursor advancement share one SQLite transaction.

## Retirement Validation

Before retiring the independent source, a multi-hour staging-NMS versus
production-SWIM study joined records by normalized human identity because the
FAA confirmed that the identifier namespaces do not correlate.

```text
NMS unique human identities             5,226
independent-source unique identities    2,982
shared identities                       2,973
independent-source coverage by NMS      99.70%
```

The remaining differences were consistent with environment skew, window
boundaries, and expiration handling rather than a missing broad class of
records. The NMS collector also correctly removed expired records that the old
projection retained. This justified using Initial Load and `lastUpdatedDate` as
one coherent source.

The retired implementation and original comparison machinery are preserved at:

```text
branch: archive/swim-notams-retired-20260724
commit: ec68e4e91ce570bb26d16aea98239b1e44c3c88e
```
