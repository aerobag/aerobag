# Permanent NAVDB rollover source

This test-only crate builds two controlled NAVDB generations from `source.json`.
It is **not for navigation** and does not model two historical FAA publications.
The synthetic cycle IDs `9901` and `9902` identify the scenarios, not source dates.

The permanent input is a curated set of decoded logical records captured from
one NAV25 publication: KRNT, SEA, KPAE, the KPAE VOR-A procedure and its shared
arc/hold geometry, plate metadata, local magnetic variation, aircraft definitions,
and the two NW chart catalogs with their package metadata. It contains no encoded
NAVKV roots/pages, chart images, downloaded archives, or dependency on the FAA
publication calendar. Provenance records the original package checksum. The
NOTAM airport list is reduced to the scenario airports and the unused airway
routing graph is explicitly empty, with its canonical schema.

Captured ancillary package IDs and magnetic-variation provenance are historical
labels, not live inputs. The generator clears ancillary package validity dates;
chart freshness is not part of this transaction test. NAVDB validity windows
are supplied by the lab's controlled clock, independently of the payload bytes.

`build()` uses `had-nav-kv`, the same root/page encoder as the producer. The web
lab wraps those pages with the production Stored-ZIP/XZ writer. Both the Rust
session test and the browser lab consume this same source and generation logic.

- Initial: the captured records.
- Candidate: the airport-info name of KRNT changes to `RENTON MUNI - ROLLOVER B`.
- Rejected: the candidate also lacks SEA's required navaid position.

The successful transaction must preserve the rich flight plan, active guidance,
procedure arc/hold and raster family, while exposing the candidate's changed
airport record. The rejected transaction must preserve the old database and
old airport record, warn visibly, offer reload, and block further attempts.

## Contract changes

When the storage encoder changes, roots and pages are rebuilt from these logical
values. If a NAVDB contract changes, explicitly review/migrate the relevant
logical records and their `nav_db_contract` descriptor in the same commit.
The generator and cheap fixture preflight fail closed on a descriptor mismatch
or missing required schema; neither relabels old packages nor silently fabricates
missing contract data. Ordinary FAA cycle turnover requires **no fixture update**.

This is transaction coverage, not validation of the current FAA producer inputs.
The independent smoke/release fixtures still exercise a real, currently available
publication and must be rebuilt when the client contract changes. They need only
one publication, never a historical predecessor.

## Optional recapture

Recapture is an explicit maintenance operation, not a CI setup step. From `crates/`:

```sh
cargo run --locked -p nav-db-fixture --bin capture -- \
  /path/to/current-contract-nav-db.zip /tmp/review-rollover-source.json
```

The output must not already exist. The tool uses the shared NAVKV package reader,
preserves geometry/package dependencies, and records the input hash. Review the
source diff and run both transaction scenarios before replacing `source.json`.
No external artifact repository needs publishing for rollover-source changes.

## Checks

```sh
cargo test --locked --manifest-path crates/Cargo.toml -p nav-db-fixture
cargo test --locked --manifest-path ui/core-rust/Cargo.toml -p app-core \
  --lib generated_nav_db_advance_preserves_rich_session
npm --prefix ui/web-app run e2e:nav-db-rollover -- --no-record
```

The browser lab creates temporary publication validity windows after encoding
the packages and triggers maintenance at the controlled boundary. It never
fetches historical fixtures, consults a sibling checkout, or reads a bare repo's
unpinned HEAD. Browser result directories include the scenario and source hash.
