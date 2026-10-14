# NOTAM delivery audit, September 16, 2026

## Production baseline

Read-only inspection around 20:21 UTC, before the changes below:

- Canonical source records: 29,903.
- Published NOTAM records: 27,309.
- Excluded from that feed: 2,594.
- Excluded IDs also present with text in the latest TFR publication: 84.
- Records in neither delivery path: 2,510.

Exclusion categories:

- AIRSPACE: 1,246.
- No keyword: 538.
- NAV: 367.
- OBST: 326.
- COM: 82.
- SVC: 28.
- Other: 7.

These are retained-state counts, including future-effective notices. NOTAM and
TFR publications are independent snapshots. They measure delivery, not upstream
completeness or verified UI reachability.

All 3,836 IAP/ODP/SID/STAR records reached the NOTAM feed. Of those, 75 had no
specific procedure key but retained an airport fallback. The existing
"Procedure NOTAMs without a UI anchor" gauge is narrow, not a general coverage
audit: it does not account for ARTCC and other non-airport notices. ZSE alone had
98 excluded records, eight delivered through TFRs and 90 through neither path.

One fixable airport identity gap: a `4A2 RWY 33 PAPI U/S` notice was excluded,
while another notice with structured `PAAB` reached the client. NASR's existing
authoritative alias table maps `4A2` to `PAAB` (Atmautluak). This needs no fuzzy
name or coordinate matching.

## Implemented follow-through

1. Operational status v4 maintains an ongoing source/client/category audit and
   a distinct-ID overlap against the latest TFR state. It updates on either
   publication, even when a source-only change leaves the client version alone.
   Missing/corrupt evidence gives unknown counts, never a stale success or zero.
2. NAV27's required airport catalog schema 2 carries existing NASR aliases. The
   shared catalog identity includes aliases, so both projection rebuilding and
   daemon-sharing compatibility respond to alias-only changes. Conflicting
   aliases fail explicitly. Non-airport subjects are handled separately below.
3. Pipeline Health graphs the seven exclusion categories over its bounded
   history and shows the current counts as an ordinary HTML table. Separate
   gauges show TFR overlap and the remaining delivery gap.

Regression coverage includes a red/green `4A2` projection test, alias-only
reprojection, catalog identity and conflicts, source-only changes, distinct TFR
IDs despite multiple areas, publication ordering/removal, corrupt TFR state,
unknown-versus-zero monitoring, history compaction, and browser graph plumbing.

Local verification: all 12 non-Python cheap-preflight suites passed. The Python
lane stopped at the expected NAV26 fixture-contract mismatch; its ordinary test
collection was then run separately (783 passed, one skipped). The dashboard
Chrome smoke passed. A separate render check used real Plotly at widths 1280
and 390, with seven visible traces, unclipped legends, and no horizontal overflow.
The screenshots use the audit counts with synthetic history, not a deployed
monitor's newly collected time series. The full preflight is not green until
the fixture handoff below is complete.

## Rollout and remaining scope

The monitor and daemon must both be updated to get the TFR cross-check. The
alias fix additionally requires a NAV27 publication. Before pushing that client
contract bump, rebuild the external compact smoke/release fixtures from the
new publication and update their pinned artifact revision with contract metadata;
do not relabel NAV26 bytes. The permanent logical rollover fixture is migrated
in-tree. Production was inspected, not redeployed, for this work.

## Subject delivery, September 17

The agreed UI homes are implemented:

- Navaids: an N/count badge on FP navaid rows and inspector navaid buttons.
- Airways: the same badge on a selected airway's FP group header.
- Both open the existing, now shared, scrollable NOTAM reader on web and Android.
  Core supplies the subject identity, count, title, advisory, and ordered text.
- General ARTCC and regional-airspace notices deliberately remain server-only;
  they remain in the Pipeline Health exclusion categories/TFR overlap audit.

Classification is by explicit subject, never by a center's filing ID or nearest
airport. Navaids require a NAV notice with a matching location and explicit
facility type (such as HPB-VOR/DME). Airway notices require a route/ARTCC heading
followed by an airway list; incidental airway mentions and radial numbers do not
count. An airway badge includes all notices for that airway, not a guessed
intersection with the flown portion. The reader explicitly asks the pilot to
review the affected segments. No NOTAM is silently suppressed by endpoint
heuristics.

Reprocessing the same private production capture with the Rust producer adds
512 previously excluded notices: 290 navaid notices and 222 airway notices.
These are records with addressable subjects, not proof that every subject exists
in the installed geographic NAVDB. The seven-category monitoring follows the
actual publication projection, so excluded center/airspace records remain
visible and newly delivered records leave those exclusion counts naturally.

Reproduce that subject-only comparison (no store mutation):

```sh
gzip -dc /tmp/notam-prod-audit-20260916.json.gz | \
  cargo run --manifest-path product/preprocessor/Cargo.toml \
    -p preprocessor-live-feeds --example audit_notam_subjects
```

NOTAM records contract 8 carries typed subjects; prepared display projection 4
carries their index keys. The source capture schema is unchanged. SQLite derived
projection 14 rejects 13 and uses the existing atomic canonical-source rebuild;
it cannot continue serving a projection built with the old admission rules.
Generated live-feed compatibility inventories advertise records contract 8.

Coverage includes the original red/green HPB/V23 admission regression, typed
subject separation, Postcard preparation, delta retargeting, cancellation,
session FP/inspector projections, retained server-only accounting, and derived
store rebuilding. Browser DOM and Android physical-tap tests exercise the shared
reader with updates/removals. `ui/web-app/scripts/notam-badge-smoke.mjs` checks
physical Chrome clicks, scrolling, cancellation, and viewport bounds at 1280
and 390 pixels. It is a production-widget smoke, not a full application journey.

Rollout still requires the NAV27 publication/fixture handoff above plus a rebuilt
daemon and clients for NOTAM records 8. No production deployment was performed.

### Raw NMS replay expectation audit

The unchanged schema-2 July NMS capture was replayed through the real collector,
full reconciliation store, and incremental journal store. Both stores converged
after all 498 polls and 580 unique source updates. The old expected mutation
count failed (1,443 versus 1,407), as it should after expanding admission.

Comparing every record and timestamped mutation with the September 11 audit:

- All 28,962 previously published baseline records remain. Their existing fields
  are unchanged; the new field is `subjects`.
- The baseline adds 625 subject-addressable records: 408 navaid, 217 airway.
  The final checkpoint retains 599 additional records: 382 navaid, 217 airway.
- All 118 transition timestamps and 1,407 old mutations remain unchanged.
- The 36 additional mutations are navaid notices: five upserts and 31 removals.
- Of those removals, 26 expire and five have explicit `fnse:canceled` evidence
  in the raw polls: DLL `1784821778260463`, BAE `1784747590299402`,
  GDM `1784835851267146`, DGD `1784837070368843`, TXC `1784902106398484`.
- There are no lost old notices/mutations or unrelated field changes.

This raw replay has no installed airport catalog, just like the original audit;
its counts differ from the September production sample above. It verifies
projection and delivery, not the presence of every subject in a particular NAVDB.

The reviewed replacement `notams/nms-api-trace/expected.json` is below. Only the
derived expectation changes; retain all raw input bytes, provenance, and hashes.
It must accompany the external fixture handoff, not a relabeling of old fixtures.

```json
{
  "schema_version": 1,
  "baseline_record_count": 32630,
  "poll_count": 498,
  "update_count": 580,
  "transition_count": 118,
  "mutation_count": 1443,
  "removal_count": 1209,
  "repeated_mutation_id_count": 89,
  "final_state_id": "79aa8b8283453900ef4eeee61a3d65caa3cc0f801725ae79557ec7c286734a5a"
}
```

For future projection audits, setting `AEROBAG_NMS_TRACE_AUDIT_PATH` during the
ignored raw-capture test exports checkpoints, timestamped mutations, and measured
counts before checking pinned expectations. It does not bypass assertions.

The complete replay passed against a separate local copy with those reviewed
expectations: 10 checkpoints, 269 delta spans, and all 114 client recovery paths
converged to the hash above. The unoptimized run took 1,612 seconds. The original
capture and pinned artifact repository were not modified.

Final local checks: all 12 non-Python cheap-preflight lanes passed; the Python
lane still fails the NAV26-versus-NAV27 fixture metadata gate. Its ordinary tests
passed separately (783 passed, one skipped). Desktop/mobile Chrome widget smoke
and Android physical-tap tests passed. A new red/green production FP-row test
ensures the NOTAM badge does not shrink the waypoint button: it overlays the
existing icon area with a separate tap target instead. Full app browser/emulator
release journeys remain unrun pending the new publication and fixture handoff.

## September 17: Cross-Cycle Airport Alias Repair

The first NAV27 publication failed daemon startup because `FLT` was canonical
in cycle 2609 but an alias for `PAFT` in cycle 2610. Both individual catalogs
were valid. Strictly validating their unnormalized union invented a conflict.
This was the only such collision in that publication.

The fix follows explicit alias chains and carries airport aliases in each
client notice, preserving lookup under either cycle's identity. Merely deleting
the alias or choosing a cycle would have started the server while losing notice
visibility on the other cycle. See [NOTAM records v9](../contracts/notam-records-v9.md).
New regressions were observed failing for the catalog union, the prepared client
lookup, and propagation of a malformed publication error before the fixes.

The rebuilt daemon successfully described publication
`main-e71021b4ba9c/20260917T044230Z`, with 19,451 canonical airports and catalog
identity `b1f8beff52c75ec58fbeb890f337821f6e9f19a28b011897b662a9a918fc3be6`.
Neither production nor the shared dev-stack was restarted during validation.

The raw trace has no publication catalog, so its records gain empty alias sets;
the admission, updates, expiry, and removal decisions must not change. An
independent hash calculation first reproduced the reviewed v8 final hash above,
then calculated the v9 expected final hash below from those same records:

```json
{
  "schema_version": 1,
  "baseline_record_count": 32630,
  "poll_count": 498,
  "update_count": 580,
  "transition_count": 118,
  "mutation_count": 1443,
  "removal_count": 1209,
  "repeated_mutation_id_count": 89,
  "final_state_id": "cebde5724a38fa74986965525002cef0a55bdaf80a3ee9968bc9414063a8cc6d"
}
```

The external fixture handoff is still pending: publish genuinely rebuilt
NAV27/NOTAM9 compact fixtures and the reviewed raw-trace expectation, then update
the artifact pin. Do not relabel the older NAV26/NOTAM7 fixture bytes.

Validation completed against a separate local copy of the raw fixture with that
reviewed expectation: all 114 recovery paths, 10 checkpoints, and 269 delta spans
converged in 1,630 seconds. Comparing both exported traces confirmed that all
29,587 initial records, 28,611 final records, and every timestamped mutation were
identical after removing the new empty `airport_aliases` field. Counts and
admission/removal decisions were unchanged.

All 12 non-Python cheap-preflight suites passed. The Python lane stopped at the
known NAV26-versus-NAV27 fixture metadata gate; its ordinary tests were run
separately with 795 passing and one skipped. Telemetry contract validation also
passed. This is not a green external-fixture gate or a browser/emulator release
journey run. The shared dev stack and production remain untouched.

### Partial Delivery Instead Of Metadata-Driven Outages

Follow-up policy: an isolated catalog defect must not disable the NOTAM feed.
The loader now returns a usable catalog plus explicit diagnostics. Invalid IDs,
alias conflicts/cycles, and dependent ambiguous aliases are omitted; surviving
associations are fingerprinted and used normally. An unreadable catalog/bundle
can be omitted when other catalogs remain usable. Canonical source records are
retained, so omissions can be repaired through the normal catalog re-projection.

[Operational status v5](../contracts/live-feed-status-v5.md) adds a persistent
`metadata_error_count` gauge and detailed errors. Pipeline Health immediately
alarms critical above zero. Successful or unchanged publications do not clear
the gauge; a restart with repaired inputs does. Offline qualification still
rejects a damaged candidate. Only genuinely unrecoverable inputs (no usable
catalog, unreadable entire publication, corrupt storage) stop NOTAM publication.

The bad-alias availability test was observed failing on the old startup path,
and the critical-metric test failed because that measurement did not exist.
Both pass with the new behavior. Additional tests exercise partial publication
and database reopening, retained unbound notices, HTTP availability, diagnostic
persistence, repaired restart, conflict ordering, dependent aliases, cycles,
and missing/invalid/older telemetry without inventing zero values.

Follow-up verification: all 12 non-Python cheap-preflight suites passed; the
Python gate still reports the existing NAV26-versus-NAV27 smoke-fixture pin.
Running that lane's Python tests separately passed 798 tests with one skipped.
Telemetry validation passed. Strict inspection of the actual two-cycle
publication still yields 19,451 airports and the same catalog identity above,
with no metadata omissions. Shared services were not restarted.

Integration note: upstream subsequently advanced the client to NAV28 for glide
performance data. After rebasing, the pending compact-fixture handoff must target
NAV28/NOTAM9, not NAV27/NOTAM9. The earlier NAV27 publication/replay evidence
above remains the actual tested input, not relabeled fixture metadata.

### Completed Raw-Replay Fixture Handoff

Heavy Fixture CI run `35269957905` used artifact commit `d7c244ef` with rebuilt
NAV28/NOTAM9 compact fixtures, but the raw NMS replay expectation still described
the older projection. Its measured counts and final hash exactly matched the
reviewed v9 result above. It stopped at the stale 1,407-mutation assertion, before
the client recovery checks.

Artifact commit `4a0aa71284d6b57021ccba38f24b4215331db52f` completes that handoff:
only the raw replay's derived expectation and adjacent audit notes change. It
sets 1,443 mutations, 1,209 removals, and the reviewed `cebde572...cc6d` final hash.
All capture bytes, capture schema, compact fixtures, and replay assertions remain
unchanged. The application fixture lock now pins this revision.

## TFR attribution audit, October 14, 2026

The July-detail/October-geometry cache failure prompted an ownership and scope
audit. Production was inspected read-only, not deployed or restarted.

The current production daemon's NMS SQLite snapshot contained 28,294 records
(production environment, poll cursor 2026-10-14T17:06:59Z). Use the state root
from the running daemon's arguments: the older top-level `state/nms-notams`
directory is a retired August store, not the current production input.

The original enrichment helper produced 32 index entries owned by a different
notice and 21 associations that changed when input order was reversed. It
scanned all prose for number-shaped strings, including replacements and cross
references, then ranked conflicting owners. The captured published TFR state
`5ca2f863f8194ff8` actually attached domestic airport references to 15 of its 65
TFR notices (31 of 92 polygons). For example, `9/2934` displayed Addison's
`05/025`, which merely says to see FDC `9/2934`.

The corrected join accepts only the record's own structured FDC year/number,
including NMS's `6/0543` spelling, and never treats prose references as aliases.
Conflicting owners remain unresolved for FAA detail backfill, not arbitrary
first/last-wins selection. The same capture now yields 4,840 entries, zero
wrong owners, zero order-dependent results, and zero missing unambiguous owners.

The FAA detail parser also now verifies the response's own XML year/number
against the requested identity. Core now prefers area-specific effective dates
over the enclosing notice's dates. Both had failing regressions before fixes.

### Repeatable checks

The production join audit reads canonical source JSON objects, one per line;
capture them with a read-only `SELECT record_json FROM current_notams ORDER BY id`
against the running daemon's store. No credentials or source captures are checked
into the repository.

```sh
cargo run --manifest-path product/preprocessor/Cargo.toml \
  -p preprocessor-live-feeds --example audit_tfr_joins < records.jsonl

python3 tools/audit_tfr_attribution.py \
  --state published-tfrs.json.xz --details detail-records.json
```

The second command takes a JSON array of `{tfr_id, raw_xml}` records, exported
read-only from a current FAA detail cache. Identity contradictions fail the
command. Area-scope and coarse geographic inconsistencies are review candidates,
not inferred polygon joins or automatic reasons to discard valid restrictions.

Fixture-free tests now exercise own identity versus references, inconsistent
year/number pairs, conflicting-owner permutations (including duplicates),
foreign detail responses, area-date precedence, and warm/reopened versus cold
cache equivalence. The audit checks positive coverage too: dropping everything
cannot pass. The live sweeps are explicit tools, not scheduled CI or new
pipeline-health metrics. Wire-format/delta round trips remain separate tests;
they cannot establish that a well-formed record belongs to the displayed entity.

### Area-scope defects and resolution

Comparing the published state with 77 freshly fetched FAA XML details found no
disjoint notice-level geographic bounds among the 65 comparable notices. That
coarse check does not establish exact polygon equality or validate attached text.

- Four notices have distinct FAA area altitude bands collapsed to one published
  band: `6/3002`, `6/4143`, `6/4348`, `6/8902`. For example, `6/4348` has SFC-2400
  and 2400-FL180 areas, but both published polygons say SFC-2400. The current
  text enrichment takes the first range in the entire notice.
- Six notices have different area schedules but no published area schedule
  fragments: `6/3002`, `6/8923`, `6/8927`, `6/8928`, `6/8992`, `6/9367`. Kalispell
  and Missoula do not start simultaneously, despite sharing one notice.
- Altitude reference (AGL versus MSL) is present in XML `codeDistVer*`, but the
  text-derived FT unit does not preserve it. Per-area attribution must preserve
  this distinction as well as value and unit.

These producer defects are now fixed in the working tree:

- Request revision-matched FAA XML for every current TFR, even when production
  NMS already supplies its text. Cached current details require no network fetch.
- Publish each XML `TFRAreaGroup` as one coupled geometry/altitude/schedule unit.
  Validate its boundary's area identity. Do not spatially match it to WFS.
- Preserve AGL/MSL and flight levels; zero MSL is not surface. Remove the
  notice-wide text altitude parser, including synthesized first-area altitudes.
- Compile daily, overnight, and weekday schedules into ordered UTC interval
  pairs. Area dates override notice defaults; missing dates use those defaults.
  Core selects the current or next interval, merges overlaps, and does not
  report an ended first interval when another window remains.
- While details are pending or rejected, retain WFS geometry and readable NMS
  notice text where available, but show unknown area altitudes. One invalid
  detail must not remove other restrictions or abort the entire publication.
- Area IDs incorporate the XML area identity as well as canonical geometry,
  allowing identical footprints with different altitude bands. Exact duplicate
  groups coalesce; conflicting definitions are rejected at notice scope.

There is no new required wire field or encoding: `area_id` remains opaque,
limits retain their value/unit strings, and `schedule_fragments` retains its
effective/expires UTC values. No nav-db rebuild is needed. Both the live-feed
daemon and app-core need these changes to publish and display the corrected
facts. Production has not been deployed or restarted for this audit.

Replaying all 77 captured details through the production parser/publisher yields
65 drawable notices and 97 areas (the other 12 notices have no merged boundary).
An independent exact-geometry-and-altitude audit agrees on all 97 areas, with
zero missing drawable notices, collapsed altitude/schedule findings, or identity
mismatches. This checks the captured inputs, not all possible future FAA syntax.

```sh
cargo run --manifest-path product/preprocessor/Cargo.toml \
  -p preprocessor-live-feeds --example replay_tfr_details \
  < detail-records.json > rebuilt-tfrs.json
python3 tools/audit_tfr_attribution.py --state rebuilt-tfrs.json \
  --details detail-records.json --require-xml-areas
```

### Six automated invariants

The tests are fixture-free, included in ordinary Rust/Python suites and cheap
preflight, and require no FAA connection. Producer tests live in
`preprocessor-live-feeds/src/{lib,tfr_area_tests,tfr_detail_backfill,products}.rs`;
core timing/label tests live in `app-core/src/map_overlay.rs`.

1. **References cannot change ownership.** Own structured FDC identity wins;
   replacements, SEE references, domestic notices, and foreign environments
   cannot acquire another notice's polygon.
2. **Order and duplication cannot change associations.** Reverse conflicting
   owners, duplicate owners, and reorder/duplicate area groups. Compare complete
   published output, including identities and counts; ambiguous owners stay
   unresolved rather than choosing the first or last record.
3. **Warm and clean caches agree.** Install an old revision, change revisions,
   inject late old responses, reopen SQLite, and compare the whole publication
   against a fresh cache holding the same current response. Production does not
   run a cold rebuild on cache hits; this cost is confined to tests/audits.
4. **Responses identify themselves.** Reject missing/wrong XML notice identity,
   multiple notices, mismatched boundary ownership, and invalid coordinates.
   Retain unrelated valid geometry when a response is rejected.
5. **Area facts override broader defaults.** Identical footprints with different
   AGL/MSL bands and windows stay distinct. Test overnight clipping, weekdays,
   multiple windows, notice defaults, and core's visible status/altitude labels.
6. **Valid records remain present.** Assert exact positive counts and preserved
   unrelated WFS areas. Audit tools report missing drawable notices; dropping
   all records cannot turn the audit green.

Before implementation, the three new area-publication regressions failed (old
code ignored XML and published two WFS polygons instead of three areas). The
core multiple-window regression also failed: it claimed the interval had ended
during the gap before a later window. Both failure modes are now covered.

A narrow cross-product spot check found no repeated station IDs in current NOAA
snapshots (5,272 METARs and 2,978 TAFs). Weather station/airport alias resolution
already checks geographic distance; this was not a complete weather audit.
