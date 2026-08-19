# Web / Android Platform Drift Audit

Status: burn-down in progress
Audited revision: `e0858ee5` (`Restore status parity on web charts`)  
Date: 2026-08-19

## Summary

This audit found eight meaningful drift seams. Three already produce visible
platform differences; the others currently agree only because equivalent
policy is duplicated.

The recent status fix restored parity, but it stopped one architectural layer
short: platforms still decide which status controls belong on each surface.

## P0 — Burn Down First

### 1. Status surface composition and status effects are platform-owned

Resolution: implemented. Core now projects ordered Map and Charts status
controls independently of platform-local tray state, and status actions return
a typed `ReloadApplication` platform effect. Both renderers consume those
contracts; neither recognizes the reload action ID.

Web and Android independently assemble the Map and Charts status docks:

- Web Charts: `ui/web-app/src/App.tsx:12202`
- Android Charts:
  `ui/android-app/app/src/main/java/org/aerobag/app/ChartsPage.kt:826`

That is precisely how web Charts lost the global METAR warning.

There is a second boundary violation here: both platforms intercept the magic
`"app:reload"` action before it passes through core:

- Web: `ui/web-app/src/App.tsx:2513`
- Android:
  `ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt:3518`

Core already recognizes `ReloadApplication`, but the session command discards
that intent rather than returning it as an effect:
`ui/core-rust/crates/app-core/src/session.rs:5128`.

Proposed fix:

- Add a pure core query such as `status_controls_for_surface(surface_id)`. The
  platform supplies `"map"` or `"charts"`; core supplies the ordered controls.
  This does not move current-page or tray-open state into core.
- Every action goes through core.
- Return a typed platform effect such as `ReloadApplication`, following the
  existing `CloudPlatformEffect` pattern in
  `ui/core-rust/crates/app-ui-contracts/src/cloud.rs:75`.

### 2. Map-selection and flight-plan-row actions are interpreted by duplicated dispatchers

Resolution: implemented. Core now registers opaque map-selection and
flight-plan-row action UIDs and returns typed effects plus generic
session-mutation decisions. It also owns automatic METAR opening and map action
slot limits. The platform precedence ladders, row action-ID switches, and
web-only offline action-ID cases are gone.

`MapSelectionAction` is a bag of mutually optional effects rather than one
tagged effect: `ui/core-rust/crates/app-core/src/map_overlay.rs:1419`.

Web and Android independently implement effect-precedence ladders:

- Web: `ui/web-app/src/App.tsx:7240`
- Android:
  `ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt:3148`

This is already visibly broken. Core publishes enabled `offline_region_mode`
and `offline_packages` actions without executable effects:
`ui/core-rust/crates/app-core/src/map_overlay.rs:4378`. Web recognizes their
string IDs and invents unavailable dialogs at `ui/web-app/src/App.tsx:7305`.
Android has no matching cases, so the enabled buttons do nothing.

Flight-plan row actions have the same shape and problem:

- Core model: `ui/core-rust/crates/app-core/src/planning.rs:1068`
- Web dispatcher: `ui/web-app/src/App.tsx:9352`
- Android dispatcher:
  `ui/android-app/app/src/main/java/org/aerobag/app/FlightPlanPage.kt:1320`

Proposed fix: send an opaque action UID to one core endpoint and return a
tagged outcome, for example:

- updated session snapshot
- `OpenWeather(detail)`
- `OpenAirportInfo(id)`
- `OpenPicker(context)`
- `Navigate(destination)`
- `ShowDetail(detail)`
- typed platform effect

Platforms should only apply the returned effect. Modal-open state, picker
back-stack, and navigation history remain platform-local.

This should also absorb two duplicated map-selection policies:

- Selecting a METAR item automatically opens its first weather action:
  `ui/web-app/src/App.tsx:11151` and
  `ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt:5165`.
- Both trays silently expose at most six actions and suppress the second row
  when inline detail exists: `ui/web-app/src/App.tsx:10916` and
  `ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt:4353`.

### 3. Android can silently discard new core UI fields

Resolution: implemented for the identified drift and fenced against recurrence.
Waypoint suggestions and navigation symbols now originate in a generated
core-owned contract consumed by both platforms. Android now renders the shared
symbol, core-formatted integer distance text, and the same friendly-name
suppression as web; its permissive JSON decoder can no longer drop new fields
from these DTOs silently.

Core's waypoint suggestion includes `distance_text` and `symbol_feature`:
`ui/core-rust/crates/app-core/src/navdb_types.rs:45`. Web carries both.
Android's domain and wire copies contain neither:

- `ui/android-app/app/src/main/java/org/aerobag/app/domain/Models.kt:47`
- `ui/android-app/app/src/main/java/org/aerobag/app/domain/WireModels.kt:953`

Because Android uses `ignoreUnknownKeys = true`, this fails silently:
`ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt:449`.

The visible result:

- Web uses core's integer distance text and shared symbol.
- Android omits the symbol and locally displays kind plus one-decimal distance:
  `ui/android-app/app/src/main/java/org/aerobag/app/ChartsPage.kt:1613`.
- Friendly-name suppression also differs.

Proposed fix: all UI-facing DTOs should live in `app-ui-contracts` and generate
both Kotlin and TypeScript types. The current generator covers only selected
contracts, leaving a large manually mirrored remainder.

## P1 — Shared Product Policy Currently Duplicated

### 4. Flight-plan global controls require platform switches

Resolution: implemented. The control ID and view are generated UI-contract
types. Both platforms now submit the selected core-projected control through a
single `perform_control` command, and core owns the dispatch to the matching
mutation. The Android wire-ID mapper and both platform control switches are
gone; web undo/redo shortcuts resolve the displayed enabled control and use the
same endpoint as the buttons.

Core projects control IDs, labels, availability, and disabled reasons, but each
platform maps every ID to a particular core method:

- Web: `ui/web-app/src/App.tsx:4065`
- Android:
  `ui/android-app/app/src/main/java/org/aerobag/app/FlightPlanPage.kt:394`

Android also maintains a second ID-to-string mapping at
`ui/android-app/app/src/main/java/org/aerobag/app/FlightPlanPage.kt:1513`.

Proposed fix: `perform_flight_plan_control(control_uid)`. Undo/redo keystrokes
should find the displayed core control and invoke the same endpoint.

### 5. Procedure and airway picker presentation is reconstructed twice

Resolution: implemented. Procedure-picker headings, empty states, and every
transition label now come from core. Airway point labels (including coordinate
spacing), suggested choices, and the same-point exit disabled explanation are
also core-projected and keyed by opaque point UIDs. Platforms retain only
loading state, the current picker stage, and Back navigation.

The procedure titles, empty messages, and transition labels are duplicated
verbatim:

- Web: `ui/web-app/src/App.tsx:13832`
- Android:
  `ui/android-app/app/src/main/java/org/aerobag/app/FlightPlanPage.kt:1475`

Core already contains nearly the same transition-label formatter for plate
loading: `ui/core-rust/crates/app-core/src/lib.rs:1470`.

Airway presentation similarly returns raw `NavRef`s:
`ui/core-rust/crates/app-core/src/navdb_types.rs:80`. Platforms locally decide
labels, suggested highlighting, entry eligibility, and the disabled
explanation.

There is current visible drift in `NavRef` formatting: web puts spaces after
coordinate commas; Android does not:

- Web: `ui/web-app/src/App.tsx:13868`
- Android:
  `ui/android-app/app/src/main/java/org/aerobag/app/PlanDisplayWidgets.kt:423`

Proposed fix: core picker queries should return render-ready stages and options
containing label, active/suggested, enabled, disabled reason, and opaque action
UID. Loading state and Back navigation remain local.

### 6. Detail views stop short of semantic presentation

Core supplies weather and airport values, but platforms independently
construct section order, headings, fact labels, and empty-state messages:

- Weather model: `ui/core-rust/crates/app-core/src/map_overlay.rs:848`
- Airport model: `ui/core-rust/crates/app-core/src/airport_info.rs:23`
- Web rendering: `ui/web-app/src/App.tsx:11170`
- Android rendering:
  `ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt:4601`

They agree today, but labels such as `Airport elevation`, METAR/TAF ordering,
and `No procedure NOTAMs available` are shared product policy.

Proposed fix: project semantic detail sections and rows from core; platforms
choose typography and layout.

### 7. Chart selector and procedure-load controls contain local inference

Both platforms derive the airport/reference launcher label by inspecting the
first chart:

- Web: `ui/web-app/src/App.tsx:12364`
- Android:
  `ui/android-app/app/src/main/java/org/aerobag/app/ChartsPage.kt:1662`

`ProcedureLoadMenu` supplies labels and options but not control availability or
its disabled reason, so both platforms hardcode those:

- Core model: `ui/core-rust/crates/app-core/src/lib.rs:383`
- Web: `ui/web-app/src/App.tsx:12438`
- Android:
  `ui/android-app/app/src/main/java/org/aerobag/app/ChartsPage.kt:1714`

Proposed fix: core should project complete tray controls: launcher, header,
enabled, disabled reason, and ordered options.

## P2 — Drift Fences and Asymmetric Debt

### 8. Several pointer-rate geometry mirrors lack shared conformance vectors

The situation-ring selection, predictor projection, ticks, and cardinal
placement are independently implemented:

- Web: `ui/web-app/src/App.tsx:13683`
- Android:
  `ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt:1606`

Route-chevron and distance-pill layout are likewise duplicated:

- Web: `ui/web-app/src/domain/flightPlanRouteRender.ts:36`
- Android:
  `ui/android-app/app/src/main/java/org/aerobag/app/RouteRendering.kt:39`

The existing shared geometry golden covers map transforms, antimeridian
behavior, image clamping, and plate affine geometry, but not situation rings or
route annotation layout:
`ui/core-rust/crates/app-ui-contracts/tests/goldens/ui-geometry-conformance.json`.

For pointer-rate code, mirroring can be justified. It needs core-generated
shared vectors, like the existing viewport fence.

### Android-only watch item: offline package orchestration

Offline-package policy is much improved, but Compose still orchestrates the
controller, clamps core-requested parallelism, and owns retry classification
and backoff:

- `ui/android-app/app/src/main/java/org/aerobag/app/HomePage.kt:481`
- `ui/android-app/app/src/main/java/org/aerobag/app/OfflinePackagesPage.kt:1224`

It is not current web/Android drift because web lacks that capability, but it
has the same future-risk shape.

## Recommended Burn-Down Order

1. Introduce the common typed action-outcome/effect contract and use it for
   status, map-selection, and flight-plan-row actions.
2. Add core's pure per-surface status projection.
3. Finish generated contracts for every UI-facing DTO; fix waypoint
   suggestions as the first proof.
4. Replace flight-plan control switches and picker presentation reconstruction.
5. Move detail/chart presentation into core.
6. Extend geometry conformance vectors.

## Explicit Non-Findings

The following remain properly platform-owned:

- current page and navigation history
- tray/modal open state and picker back-stack
- focus and gesture bookkeeping
- asset decoding and render caches
- filesystem and network execution
- Android OS display and GPS actuation

The known raster/plate package-member transport decision is also not classified
as observed policy drift in this audit.
