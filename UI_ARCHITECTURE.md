# UI Architecture For Android + Web

## Recommendation

Do not try to share the full UI layer between Android and web.

Share:
- product information architecture,
- domain models and schemas,
- package/catalog metadata,
- chart-selection and geospatial rules,
- visual design tokens,
- acceptance tests and screenshot baselines.

Keep separate:
- native view/component code,
- gesture handling,
- map/tile rendering integration,
- platform navigation shells,
- download/storage plumbing.

That gives you one product, one behavior model, and two platform-native shells.

## Why

The current Android app is not a modern declarative UI codebase. It is a classic Java Android app with `TabActivity`, process-wide singleton state, custom drawing, and hardcoded catalog/boundary logic.

There is also no existing web app in this workspace. So if you force "one shared UI", you are really choosing to:
- rewrite Android,
- invent the web app,
- and add a cross-platform UI abstraction layer

all at the same time.

That is too much risk for the current stage, especially while the backend/preprocessor contract is still being stabilized.

## What Should Be Common

### 1. Shared product contract

Create one machine-readable source of truth for:
- chart families,
- regions,
- package names,
- labels,
- version/cycle semantics,
- feature flags,
- download grouping,
- update availability,
- required local artifacts.

Today some of this lives in Android resources like `arrays.xml` and some in hardcoded code such as `Boundaries`. That should move into generated JSON/TOML/schema-backed metadata.

### 2. Shared geospatial/domain core

Anything that answers questions like these should be shared:
- what chart covers this lat/lon,
- what package is needed for this feature,
- what zoom levels exist,
- what region names and ids mean,
- how airport diagram metadata is interpreted,
- how package manifests are parsed.

The natural place for this is a small Rust core because you are already standardizing the backend in Rust.

Targets:
- Android: Rust -> JNI/FFI
- Web: Rust -> WASM

Keep this core narrow. It should expose deterministic logic, not UI widgets.

### 3. Shared design system specification

Share the design system as artifacts, not as component code:
- color tokens,
- typography scale,
- spacing scale,
- icon rules,
- interaction states,
- layout rules,
- map control patterns,
- screen templates.

This should live as:
- token files,
- a small written component spec,
- platform screenshots/mockups,
- behavior notes for edge cases.

### 4. Shared test suite

Use the same scenarios on both platforms:
- empty state,
- first-run download flow,
- region/package selection,
- offline mode,
- chart switch while panning,
- airport detail,
- plate/diagram view,
- update available / partial download / corrupted package.

The goal is behavioral parity, not identical pixels.

## What Should Stay Native

### Android

Android should own:
- storage permissions and local files,
- background downloads,
- GPS/device integration,
- native gesture performance,
- offline-first local persistence,
- platform notifications/settings.

### Web

Web should own:
- browser navigation,
- responsive layout,
- pointer + keyboard interaction,
- browser cache/service worker behavior,
- shareable URLs,
- desktop/tablet-specific workflows.

## Practical UI Split

Use the same app model on both platforms, but not the same component code.

Recommended shared screen set:
- Map
- Downloads
- Airport
- Plates/Diagrams
- Search
- Plan
- Settings

Recommended shared view-model concepts:
- current chart mode
- selected airport
- active route
- download catalog tree
- package install state
- connectivity state
- cycle/version state
- map camera state

Then implement:
- Android shell with native Android UI
- Web shell with a browser-native UI

## What Not To Share

Do not try to share:
- XML/HTML/CSS/Compose component code,
- map canvas rendering code,
- gesture recognizers,
- tab/nav implementations,
- download-manager implementations.

You will spend more time fighting platform mismatches than building product.

## If You Want More Than "Design Spec Sync"

There is one reasonable deeper-sharing option:

Build a small shared Rust "app core" that owns:
- catalog metadata,
- package/update state machine,
- chart lookup/boundary logic,
- manifest parsing,
- route/search/domain rules.

Then each platform binds that core to its own UI.

That is the highest-value sharing boundary here.

## Suggested Build Order

1. Define the cross-platform product model.
2. Move catalog/boundary/package metadata out of Android resources into generated data files.
3. Build a small Rust domain core for deterministic rules.
4. Design one cross-platform screen spec with tokens and interaction rules.
5. Implement Android and web UIs separately against the same state model.
6. Add parity tests that run the same scenarios on both.

## Bottom Line

You should aim for:
- shared behavior,
- shared metadata,
- shared design language,
- shared tests,

not shared widget code.

For this project, that is the best tradeoff between speed, correctness, and long-term maintainability.

## Concrete Rust Core Layout

Suggested workspace:

```text
ui/
  core-rust/
    Cargo.toml
    crates/
      app-core/
      app-ffi/
      app-wasm/
      app-fixtures/
  android-app/
  web-app/
  shared-assets/
    catalog/
    fixtures/
    design-tokens/
```

Crate responsibilities:

- `app-core`
  - pure domain logic
  - no platform I/O
  - no JNI
  - no browser bindings
- `app-ffi`
  - Android-facing exported API
  - UniFFI definitions or thin JNI-safe wrappers around `app-core`
- `app-wasm`
  - web-facing exported API
  - `wasm-bindgen` wrappers around `app-core`
- `app-fixtures`
  - shared test data builders and golden fixtures for both clients

## Module Layout Inside `app-core`

```text
app-core/src/
  lib.rs
  ids.rs
  catalog.rs
  geometry.rs
  charts.rs
  plates.rs
  manifests.rs
  content.rs
  planning.rs
  sync.rs
  state.rs
  errors.rs
```

Recommended ownership:

- `ids.rs`
  - stable ids and enums
- `catalog.rs`
  - generated metadata loading and indexing
- `geometry.rs`
  - lat/lon, bounds, polygon lookup helpers
- `charts.rs`
  - chart lookup by position or viewport
- `plates.rs`
  - plate and supplement lookup
- `manifests.rs`
  - manifest parsing and validation
- `content.rs`
  - availability, policy, and package requirement logic
- `planning.rs`
  - flight plan domain model and content-requirement queries
- `sync.rs`
  - sync payloads and versioned serialization formats
- `state.rs`
  - deterministic app-domain reducer
- `errors.rs`
  - stable error surface for bindings

## Sample Core Types

These are intentionally plain and FFI-friendly.

```rust
// app-core/src/ids.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChartFamilyId {
    Sectional,
    Tac,
    Wac,
    IfrLow,
    IfrHigh,
    IfrArea,
    Flyway,
    Heli,
    Misc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegionId {
    Ne,
    Nc,
    Nw,
    Se,
    Sc,
    Sw,
    Ec,
    Ak,
    Pac,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AirportId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChartId {
    pub family: ChartFamilyId,
    pub name: String,
    pub cycle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlateId {
    pub airport_id: AirportId,
    pub procedure_code: String,
    pub page: u16,
    pub cycle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageId {
    pub region: RegionId,
    pub family: ChartFamilyId,
    pub cycle: String,
}
```

```rust
// app-core/src/geometry.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoBounds {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MapViewport {
    pub center: LatLon,
    pub zoom: f64,
    pub rotation_deg: f64,
    pub pitch_deg: f64,
}
```

```rust
// app-core/src/content.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentAvailability {
    LocalOnly,
    RemoteOnly,
    LocalAndRemote,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentPolicy {
    OfflineRequired,
    PreferLocal,
    StreamAllowed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvailabilityDetail {
    pub availability: ContentAvailability,
    pub cycle_current: bool,
    pub integrity_ok: bool,
    pub cached: bool,
    pub offline_usable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContentLocator {
    LocalFile { path: String },
    RemoteUrl { url: String, cache_key: String },
    Missing,
}
```

```rust
// app-core/src/planning.rs
use serde::{Deserialize, Serialize};

use crate::geometry::LatLon;
use crate::ids::AirportId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlan {
    pub id: String,
    pub name: String,
    pub legs: Vec<PlanLeg>,
    pub departure: Option<AirportId>,
    pub destination: Option<AirportId>,
    pub alternate: Option<AirportId>,
    pub cruise_altitude_ft: Option<i32>,
    pub notes: Option<String>,
    pub updated_at_epoch_ms: i64,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanLeg {
    pub from: NavRef,
    pub to: NavRef,
    pub airway: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NavRef {
    Airport(String),
    Navaid(String),
    Fix(String),
    LatLon(LatLon),
}
```

## FFI-Friendly Service Boundary

Do not expose a large object graph with many tiny methods.

Prefer:
- plain data in,
- plain data out,
- coarse operations,
- explicit handles only where parsing/indexing is expensive.

A good first service surface:

```rust
pub trait AppCoreApi {
    fn load_catalog(&self, catalog_json: String) -> Result<CatalogHandle, AppError>;
    fn chart_for_position(
        &self,
        catalog: CatalogHandle,
        family: ChartFamilyId,
        lat: f64,
        lon: f64,
    ) -> Result<Option<ChartSummary>, AppError>;
    fn charts_for_viewport(
        &self,
        catalog: CatalogHandle,
        family: ChartFamilyId,
        viewport: MapViewport,
    ) -> Result<Vec<ChartSummary>, AppError>;
    fn plates_for_airport(
        &self,
        catalog: CatalogHandle,
        airport_id: String,
    ) -> Result<Vec<PlateSummary>, AppError>;
    fn build_flight_plan(
        &self,
        request: BuildFlightPlanRequest,
    ) -> Result<FlightPlan, AppError>;
    fn edit_flight_plan(
        &self,
        plan: FlightPlan,
        edit: FlightPlanEdit,
    ) -> Result<FlightPlan, AppError>;
    fn plan_content_requirements(
        &self,
        catalog: CatalogHandle,
        plan: FlightPlan,
    ) -> Result<Vec<ContentRequirement>, AppError>;
    fn resolve_content_status(
        &self,
        requirements: Vec<ContentRequirement>,
        inventory: ContentInventory,
        policy: ContentPolicy,
    ) -> Result<ContentReport, AppError>;
}
```

This avoids chatty crossings and makes both Android and web bindings straightforward.

## Handle Strategy

Use handles only for parsed, reusable, immutable data:

- `CatalogHandle`
- maybe later `BoundaryIndexHandle`

Do not use handles for transient UI concepts like:
- current bottom sheet
- selected tab
- drag state
- animation state

If handle management becomes annoying on web, expose a stateless JSON-in/JSON-out mode in `app-wasm` for simpler early integration.

## Android Binding Plan

Recommended:
- `app-core` as the source of truth
- `app-ffi` using UniFFI where possible
- Kotlin wrapper layer that converts generated bindings into app-specific models

Android boundary shape:

```text
Compose screen/viewmodel
  -> Kotlin domain adapter
  -> generated UniFFI bindings
  -> Rust app-core
```

Platform-owned Android services:
- GPS and sensors
- storage/file APIs
- WorkManager/background downloads
- notifications
- SQLite/Room if needed
- map rendering and gestures

Rust-owned Android decisions:
- which chart applies
- what content is required
- whether installed content satisfies a plan
- how manifests/catalogs are interpreted

## Web Binding Plan

Recommended:
- `app-wasm` with `wasm-bindgen`
- thin TypeScript facade over generated WASM bindings

Web boundary shape:

```text
React components
  -> TypeScript domain adapter
  -> WASM wrapper
  -> Rust app-core
```

Platform-owned web services:
- HTTP fetch
- service worker and cache storage
- URL routing
- pointer/keyboard handling
- canvas/WebGL/DOM map rendering

Rust-owned web decisions:
- same catalog interpretation as Android
- same chart and plate lookup
- same flight-plan validation
- same content requirement logic

## Example Content Inventory Model

This is the key seam that lets Android and web differ without drifting.

```rust
use serde::{Deserialize, Serialize};

use crate::ids::{ChartId, PackageId, PlateId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentInventory {
    pub installed_packages: Vec<InstalledPackage>,
    pub cached_tilesets: Vec<CachedTileset>,
    pub cached_plates: Vec<CachedPlate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub package_id: PackageId,
    pub integrity_ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedTileset {
    pub chart_id: ChartId,
    pub fully_cached: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedPlate {
    pub plate_id: PlateId,
    pub cached_pages: Vec<u16>,
}
```

Interpretation:

- Android mostly fills `installed_packages`
- Web mostly fills `cached_tilesets` and `cached_plates`

The same Rust logic can then report:
- fully offline-ready
- partially available
- stream-only
- unavailable

## Example Output Types

```rust
use serde::{Deserialize, Serialize};

use crate::content::AvailabilityDetail;
use crate::ids::{ChartId, PackageId, PlateId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartSummary {
    pub chart_id: ChartId,
    pub display_name: String,
    pub max_zoom: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlateSummary {
    pub plate_id: PlateId,
    pub display_name: String,
    pub georeferenced: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentRequirement {
    pub package_ids: Vec<PackageId>,
    pub chart_ids: Vec<ChartId>,
    pub plate_ids: Vec<PlateId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentReportItem {
    pub label: String,
    pub availability: AvailabilityDetail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentReport {
    pub fully_satisfied: bool,
    pub items: Vec<ContentReportItem>,
}
```

## Error Surface

Keep the error model boring and explicit.

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppErrorKind {
    InvalidCatalog,
    InvalidManifest,
    InvalidFlightPlan,
    UnknownAirport,
    UnknownChart,
    UnsupportedOperation,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppError {
    pub kind: AppErrorKind,
    pub message: String,
}
```

Do not leak Rust-internal error chains over FFI as the primary contract.

## Serialization Rule

For all shared durable models:
- derive `Serialize` and `Deserialize`
- keep fields explicit and versionable
- prefer additive evolution

For sync payloads, include an explicit schema version:

```rust
pub struct UserSyncBundle {
    pub schema_version: u32,
    pub flight_plans: Vec<FlightPlan>,
    pub recents: Vec<String>,
}
```

That makes Android-web sync safer over time.

## First Vertical Slice

Build this before touching map rendering:

1. generated `catalog.json`
2. `load_catalog`
3. `build_flight_plan`
4. `plan_content_requirements`
5. `resolve_content_status`
6. one Android `Content` screen
7. one web `Content` screen

If that slice feels natural, the architecture is probably right.

If it feels like the UI needs dozens of tiny Rust calls just to render one screen, the FFI boundary is wrong and should be made coarser.
