# NOTAM Ingestion

Aerobag obtains current Domestic and FDC NOTAMs from the FAA NOTAM Management
Service (NMS) API. One collector owns both the initial active-state load and all
subsequent updates, so state keys and update semantics stay within one FAA
source.

The incremental state-identity, delta-journal, and checkpoint publication design
in [notam-incremental-publication-plan.md](notam-incremental-publication-plan.md)
is the active client-publication path. NMS supplies canonical source state;
the publication store converts source-state changes into Merkle deltas and
periodic checkpoints.

## Runtime

`aerobag-live-feedsd` runs the NMS collector in-process when started with:

```text
--nms-notams-config /path/to/nms-notams.json
--nms-notams-state-root /path/to/state/nms-notams
```

The credential file has this operator-owned shape:

```json
{
  "sourceEnvironment": "staging",
  "apiBaseUrl": "https://api-staging.cgifederal-aim.com/nmsapi/v1",
  "tokenUrl": "https://api-staging.cgifederal-aim.com/v1/auth/token",
  "clientId": "...",
  "clientSecret": "..."
}
```

Use `sourceEnvironment: "production"` and the production URLs with production
credentials. Credentials must stay outside the repository and publication
trees.

On an empty state directory the collector fetches and validates both Initial
Load classifications, installs them atomically, and sets its poll cursor to the
capture start. It then queries both classifications by `lastUpdatedDate` every
three minutes, looking back ten minutes. Payload hashes make that overlap
idempotent.

The durable NMS source store is `state.sqlite`. Each successful poll applies all
updates, cancellations, and expirations in one transaction and only then
advances the cursor. A failed poll leaves both state and cursor unchanged. A
process lock prevents two collectors from sharing a state directory.

After each successful source transaction, the daemon synchronizes the canonical
`current_notams` rows into the separate `publication/` store. That store owns
the Merkle state, publication journal, deltas, checkpoints, and acknowledgement
cursor. Keeping these stores separate prevents NMS polling concerns from
leaking into the client publication contract.

Startup always synchronizes and queues the persisted state for publication.
Every successful poll queues a refresh: changed state produces a new version,
while unchanged state advances `collected_at` so clients know the source is
still healthy. Failures affect NOTAM source health but do not stop other
live-feed products.

TFR enrichment reads the same current NOTAM state and falls back to the
independent TFR detail cache for FDC IDs absent from that state.

## External Test Fixture

The external `aerobag-test-artifacts` repository contains a bounded raw NMS
Initial Load and API poll trace at `notams/nms-api-trace/`. Generate a
replacement from an immutable copy of collector state with:

```sh
(cd product/preprocessor && cargo run -p nms-notams-fetch -- \
  capture-fixture \
  --initial-load /path/to/nms-initial-load-capture \
  --state-root /path/to/nms-collector-state \
  --output /path/to/aerobag-test-artifacts/notams/nms-api-trace \
  --captured-by-commit "$(git rev-parse HEAD)")
```

The capture excludes credentials, API URLs, local source paths, and redundant
normalized output. The ignored artifact-backed test reparses the raw source,
replays collector transitions at their captured completion times, and verifies
checkpoint/delta convergence in app core.

## Development

The dev stack looks for:

```text
/root/aerobag-credentials/dev-stack/nms-notams-staging.json
```

Override it with `tools/run_dev_stack.py --nms-notams-config PATH`, or disable
NOTAM collection with `--disable-nms-notams`.

Production deployment expects an operator-owned production file at
`/root/aerobag-credentials/nms-notams-production.json` by default and installs
it as `/etc/aerobag/secrets/nms-notams.json`. Set
`"nms_notams_enabled": true` only after production credentials are available.

## Retired Source

The former FAA SWIM/SCDS collector was retired after NMS Initial Load plus
`lastUpdatedDate` proved to be coherent, stateless, and sufficiently complete.
Its final source is preserved at:

```text
branch: archive/swim-notams-retired-20260724
commit: ec68e4e91ce570bb26d16aea98239b1e44c3c88e
```

That branch is the recovery record; the active tree intentionally contains no
compatibility path for the retired collector.
