# HAD Query Keyspaces

HAD keys are query contracts. They should be shaped around the question core
needs to answer, not around legacy JSON bundles or UI view models.

Key components are UTF-8 percent-encoded. Identifiers whose source domain is
case-insensitive, such as airport ids, CIFP ids, airway names, and procedure
kinds, are trimmed and uppercased before encoding. Opaque published ids, such as
plate ids, are trimmed and encoded without case changes.

## Implemented Client Key Builders

The web client key builders live in `ui/web-app/src/domain/navHad.ts`. They are
the current aspirational consumer contract for the preprocessor HAD writer.

## Raster Chart Startup

`chart/catalog`

Current consumer: web chart/map startup.

Current producer: preprocessor HAD writer.

Value: `MapViewOptionJson[]`.

This is already the right shape: one small app-ready list describing selectable
raster chart views and their tile roots. Do not rename this to `map/catalog`;
the product is a chart.

## Plate Folder Data

`plate/airport-index`

Current consumer: PLT airport selector/folder entry.

Current old source: bulk `resource_index.airport_resources`.

Value: compact list of airports that have plate-page material:

```json
[
  { "id": "KRDD", "label": "KRDD" }
]
```

This index is for discovery and ordering only. It must not include the full
chart asset arrays.

`plate/airport/{airport_id}`

Current consumer: PLT/FLDR selected airport and recent-plan airports.

Current old source: `derive_chart_page` over bulk `resource_index`.

Value: one `DerivedChartAirport` / `ChartPageData["airports"][number]`.

This includes that airport's `charts` array, sorted into folder order. It is the
replacement for the single huge `chart/page/catalog` value.

`plate/by-id/{plate_id}`

Current consumer: selected-chart restoration and direct “Show Plate” navigation.

Current old source: scanning the bulk chart catalog by id.

Value: one `DerivedChartAsset`.

This lets core/UI restore or navigate to an exact plate without fetching the
airport folder first. The UI may still fetch the airport folder afterward to
paint the folder context.

## CIFP to TPP Plate Matching

`plate/cifp/{airport_id}/{cifp_id}`

Current consumer: PLN procedure row “Show Plate” enabledness and target.

Current old source: web SQLite query:

```sql
SELECT ...
FROM cifp_tpp_matches
WHERE trim(airport_id) = trim(?1)
  AND trim(cifp_id) = trim(?2)
ORDER BY CAST(is_primary AS INTEGER) DESC, CAST(priority AS INTEGER), trim(plate_label)
```

Value: `CifpTppMatchRow[]` in the same order as the SQL query. Core still
chooses the preferred row with `describe_show_plate_for_procedure`.

`plate/procedure-candidates/{plate_id}`

Current consumer: PLT `LOAD APPCH` enabledness and transition options.

Current old source: web SQLite query:

```sql
SELECT ...
FROM cifp_tpp_matches
WHERE trim(plate_id) = trim(?1)
ORDER BY trim(cifp_id), CAST(is_primary AS INTEGER) DESC, CAST(priority AS INTEGER), trim(plate_label)
```

Value: `CifpTppMatchRow[]` in SQL-equivalent order. Core groups by
`airport_id:cifp_id`, chooses the preferred match per group, then combines with
procedure transition rows.

## Procedures

`procedure/list/{airport_id}/{kind}`

Current consumer: PLN “Add Procedure” tray.

Current old source:

- For approaches, `cifp_tpp_matches` by airport, interpreted as approach
  procedures.
- For SID/STAR, `cifp_sid_star_app` distinct procedure rows, with kind coming
  from published metadata rather than UI route-type inference.

Value: `ProcedureSummary[]`, already filtered to the requested kind and sorted
by `procedure_id`.

`procedure/distinct-rows/{airport_id}/{procedure_id}`

Current consumer: procedure transition selector and PLT load-procedure options.

Current old source:

```sql
SELECT DISTINCT trim(route_type), trim(transition_identifier)
FROM cifp_sid_star_app
WHERE trim(airport_identifier) = trim(?1)
  AND trim(sid_star_approach_identifier) = trim(?2)
ORDER BY trim(route_type), trim(transition_identifier)
```

Value: `ProcedureDistinctRow[]`.

This is still row-shaped because core owns interpretation. Preproc should emit
the same normalized fields, but it should not force UI-facing transition policy
into the value.

`procedure/materialization-rows/{airport_id}/{procedure_id}`

Current consumer: materialize selected procedure into flight-plan components.

Current old source: `cifp_sid_star_app` rows plus the nav/runway lookups needed
to populate `ProcedureLegMaterializationRecord`.

Value: `ProcedureLegMaterializationRecord[]`, already enriched with resolved
fix/nav/runway positions and magnetic variation fields that core needs to build
the procedure.

This key should absorb the current per-leg SQLite subqueries for navaid
variation, airport magnetic variation, runway threshold position, and fix/nav
classification. Core still owns the path-termination and transition-selection
interpretation.

## Airway Data

`airway/{airway_name}`

Current consumer: airway selection and materialization.

Current old source: `airways_branch`, with the legacy `airways` table as an old
producer shape we should not carry forward.

Value: `AirwayBranch[]`.

Preproc should emit branch-normalized data only. Do not publish the legacy table
shape into HAD.

`airway/spatial/{tile_key}`

Current consumer: `suggestAirwaysNear`.

Current old source: radius SQL over `airways_branch`.

Value: spatial tile containing nearby airway points sufficient for core to sort
and de-duplicate `AirwaySuggestion`s by distance.

This should be a tile product, not a prefix-scan over all airways.

## Waypoint and Symbol Data

`waypoint/ident/{identifier}`

Current consumer: insert waypoint validation and exact identifier resolution.

Current old source: exact lookup across `airports`, `nav`, and `fix`.

Value: candidates for that identifier in core priority order, including
`NavRef`, position, kind, and friendly display name.

`waypoint/prefix/{prefix_shard}`

Current consumer: completion suggestions.

Current old source: prefix SQL across `airports`, `nav`, and `fix`.

Value: sorted waypoint records for that prefix shard. Core filters the requested
prefix, computes distance from the insertion anchor, and truncates.

This can be a trie-like shard layout later. The important contract is that the
UI asks core for suggestions; core asks HAD for a prefix shard and does the
ranking.

`navref/position/{kind}/{identifier}`

Current consumer: route projection, leg metrics, and procedure geometry.

Current old source: exact position lookup in the corresponding SQLite table,
plus runway threshold lookup for procedure runway fixes.

Value: one `LatLon`, or a richer object where runway/procedure context is
required.

`navref/symbol/{kind}/{identifier}`

Current consumer: PLN pill symbols.

Current old source: exact point record lookup plus airport runway/fuel/tower
decoration queries.

Value: `NavSymbolFeature`.

Airport records must use the same shared airport-symbol derivation as vector
tile preprocessing so plan pills and chart symbols cannot diverge.

## Current Refactor Blockers

`create_ui_session` still requires a complete `DerivedChartCatalog` JSON at
session creation. That blocks truly lazy `plate/airport/{id}` loading because
core has no API to ingest a newly fetched airport folder into an existing
session.

The required core boundary change is:

- create sessions with an initially known set of airport folder records, not a
  whole catalog;
- add a core API to ingest or replace one `DerivedChartAirport` by id;
- have `select_airport`, selected-chart restoration, and flight-plan changes
  request missing HAD keys before asking core to recompute PLT state.

The UI should only fetch the key core asks for or the key needed to satisfy an
explicit user navigation. It should not reimplement folder membership,
procedure-load enabledness, or preferred CIFP/TPP selection.
