# Session Update Dependency Inventory

## Purpose

This inventory is the implementation boundary between the controller extraction
and a revisioned `SessionUpdate` wire contract. It records which core owner
produces each part of `UiSessionSnapshot`, which external inputs affect that
projection, and which current invalidations protect related non-snapshot
queries.

Startup and explicit recovery may continue to send a full snapshot. Ordinary
mutations should eventually return only changed projection groups.

## Residual Coordinator State

`UiSession` composes every domain controller directly. The remaining
`SessionCoordinatorModel` fields are cross-domain inputs rather than hidden
controller state:

| Field | Responsibility | Future update consequence |
|---|---|---|
| `session_revision` | Orders committed session mutations | Envelope revision only |
| `content_policy`, `last_content_report` | Cross-domain content acceptance/reporting | Application UI group |
| `chart_page_state` | Current airport/chart/reference selection | Chart group |
| `platform_capabilities` | Declares available platform effects and URLs | Home, settings, cloud, status groups |
| `persistence_storage` | Platform storage plumbing | No UI projection |
| `debug_state` | Core-owned diagnostic feature policy | Debug, map, application UI groups |
| `cycle_product_freshness` | Schedules product-validity reevaluation | Status scheduling group |
| `wall_clock_epoch_ms` | Input to age, ETA, animation, cloud, and status projections | Input token, not an independently rendered group |
| `altitude_planner_wind_selection`, `time_display_mode` | Cross-domain planner and time-presentation choices | Flight-plan/application UI group |

These accesses are deliberately spelled `session.coordinator.<field>`.
`UiSession` must not implement `Deref` or `DerefMut` to this model.

## Snapshot Projection Groups

| Proposed group | Current snapshot fields | Authoritative owner and revision | Additional projection inputs |
|---|---|---|---|
| Envelope | `ui_contract_version`, `session_revision` | Contract constant and coordinator session revision | None |
| Nav data | `nav_data_epoch`, `active_nav_db`, `next_nav_db_maintenance_epoch_ms` | `NavDataController`; package revision participates in maintenance | Wall clock |
| Flight plan/application | `flight_plan_route_revision`, `app_ui_state`; test-only `app_state` | `FlightPlanController` | Situation projection, weather revision, NAVDB generation, cloud-owned aircraft definitions/digest, content policy/report, debug policy, clock, time zone, planner choices |
| Situation | `playback_ui_state`, `playback_panel_state`, `map_follow_ui_state`, `map_follow_target_viewport` | `SituationController` | None beyond controller model |
| Charts | `chart_page_state` | Coordinator chart selection | Active flight plan and NAVDB reads when selection changes |
| Map | `map_layer_state`, `raster_map` | `MapController` | Debug policy; package/NAVDB changes configure the controller before projection |
| Status | `data_status_state`, `data_status_page_state`, `next_cycle_product_freshness_check_epoch_ms` | `DataStatusController`; coordinator owns the next-check schedule | Typed NAVDB, package, cloud, weather, platform, clock, and build facts |
| Settings | `settings_page_state`, `display_policy`, `disclaimer_state` | `SettingsController` | Display capability, unfiltered flight-data banner, and coordinator debug policy rendered in Settings |
| Cloud | `cloud_page_state` | `CloudController` | Wall clock and QR-scanner capability |
| Packages | `offline_package_preferences_json` | `PackageController` | None beyond controller model |
| Home | `home_page_state` | Platform capability projection | Platform capabilities |
| Debug | `debug_state` | Coordinator debug policy | None |

The current aggregate `app_ui_state` is the largest cross-domain projection. It
should remain one update group initially; splitting flight-plan rows, ownship,
and the flight-data banner is a later measured optimization.

## Existing Query Invalidations

`SessionSnapshot` currently means that some aggregate snapshot field may have
changed. Revisioned updates should replace that broad signal with the actual
changed groups above. The other invalidations remain query-cache dependencies:

| Invalidation | Protected query or platform cache |
|---|---|
| `NavData` | Attached NAVDB-dependent platform caches and leases |
| `RasterTiles` | Raster tile plan/images |
| `MapOverlay` | Vector, METAR, traffic, TFR, and flight-plan map overlay query |
| `NexradOverlay` | Selected NEXRAD frame/tile query |
| `TerrainOverlay` | Terrain image and warning query |
| `FlightPlanRoute` | Chart/plate flight-plan route geometry |
| `DebugPanel` | Debug-only query output |

Core must continue emitting these invalidations. Platforms must not infer them
from product IDs, transport events, or whichever update groups happen to be
present.

## Projection Version Tokens

Core now owns one monotonic token for every group above. Each token observes a
typed dependency stamp containing the authoritative controller revisions and
the exact coordinator inputs used by that group. This covers coordinator-owned
chart, platform-capability, debug, freshness-schedule, content, clock, and
planner state without scattering manual dirty calls across mutation paths.

The version state is checkpointed with aggregate transactions and cloned into a
NAVDB candidate. Failed transactions therefore cannot publish versions for
rolled-back state. Core compares these tokens before and after successful
projection to populate the generated `UiSessionUpdate`; no platform compares
snapshots or serialized values to infer changes.

Each optional update group is a versioned JSON-object patch containing exact
top-level fields from the existing snapshot. This lets adapters merge a patch
into their cached raw snapshot and reuse the existing strict decoder without
duplicating the large legacy snapshot type graph in a second generated schema.
Core owns and tests the non-overlapping field-to-group partition.

## Implementation Order

1. ~~Add core-only projection version tokens for every proposed group and assert
   that unrelated mutations leave them unchanged.~~ Completed.
2. ~~Define a generated `SessionUpdate` with an envelope plus optional projection
   groups. Keep the existing full snapshot as startup/resynchronization data.~~
   Completed.
3. ~~Make each mutation return the update assembled from core-owned changed-group
   decisions while retaining specialized query invalidations.~~ Completed,
   including the former effect-only preferences path.
4. ~~Teach Android and web adapters to merge groups into their local view model.~~
   Completed with one shared conformance fixture and transitional full-snapshot
   equality checks.
5. ~~Remove ordinary full-snapshot serialization only after both platforms have
   conformance and journey coverage.~~ Completed. Revision gaps now recover
   through the explicit paged full-snapshot API, and NAVDB envelopes no longer
   embed snapshots.
