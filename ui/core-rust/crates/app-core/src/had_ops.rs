use std::collections::{HashMap, HashSet};

use procedure_geometry_types as pgt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::planning::FlightPlanRowActionId;
use crate::{
    chart_page::{airport_ids_from_plan, derive_chart_page_state_from_airports},
    describe_plate_procedure_load_options, describe_show_plate_for_procedure,
    flight_leg_distance_nm, flight_plan_contains_nav_ref, flight_plan_has_direct_to_overlay,
    insert_airway_after_waypoint, insert_waypoint, prepare_airway_presentation,
    project_flight_plan_route_with_resolver, AirportId, AirwayAutoSelection, AirwayBranch,
    AirwayEntryCandidate, AirwayExitCandidate, AirwayPresentationPlan, AirwaySegment,
    AirwaySpatialPoint, AirwaySuggestion, AppError, AppErrorKind, AppResult, CifpTppMatchRow,
    ConcretizedNavItem, FlightPlan, FlightPlanRouteSegment, FlightPlanUiMutation,
    FlightPlanUiState, LatLon, LegDisplayElement, LegDisplayPath, LegDisplayPathStyle,
    MaterializedProcedure, NavKvLookup, NavKvQuery, NavKvStore, NavRef, NavSymbolFeature,
    PathTermination, PlateProcedureLoadCandidateInput, PolygonRecord, ProcedureDiscontinuity,
    ProcedureKind, ProcedureLegProvenance, ProcedureLoadOption, ProcedureOptions, ProcedureSegment,
    ProcedureSegmentRole, ProcedureSummary, ResolvedLeg, ResolvedLegSource, RouteComponent,
    WaypointIdentifierRecord, WaypointIdentifierSuggestion,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HadOperationOutcome {
    Complete { result: Value },
    NeedResources { resources: Vec<CoreResourceRequest> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreResourceRequest {
    pub id: String,
    pub address: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HadOperation {
    VectorManifest,
    ChartPageState {
        plan: FlightPlan,
        recent_airport_ids: Vec<String>,
        selected_airport_id: Option<String>,
        selected_chart_id: Option<String>,
    },
    PlateAirport {
        airport_id: String,
    },
    PlateById {
        plate_id: String,
    },
    FlightPlanUiState {
        plan: FlightPlan,
        current_ui_state: FlightPlanUiState,
    },
    FlightPlanUiMutation {
        mutation: FlightPlanUiMutation,
    },
    PreviewFlightPlanEntry {
        plan: FlightPlan,
        input: String,
    },
    AppendFlightPlanEntry {
        plan: FlightPlan,
        input: String,
    },
    ProjectFlightPlanRoute {
        plan: FlightPlan,
    },
    ResolveWaypointIdentifier {
        identifier: String,
    },
    ResolveNavRefPosition {
        nav_ref: NavRef,
    },
    ResolveNavSymbolFeature {
        nav_ref: NavRef,
    },
    SuggestWaypointIdentifiers {
        plan: FlightPlan,
        component_index: usize,
        before: bool,
        prefix: String,
        limit: usize,
    },
    SuggestWaypointIdentifiersNear {
        anchor: LatLon,
        prefix: String,
        limit: usize,
    },
    SuggestAirwaysNearAnchor {
        anchor: NavRef,
        limit: usize,
    },
    AirwayBranches {
        airway_name: String,
    },
    PrepareAirwayPresentationForAnchors {
        airway_name: String,
        origin_anchor: NavRef,
        destination_anchor: Option<NavRef>,
    },
    MaterializeAirwaySelection {
        start_component_index: usize,
        entry: AirwayEntryCandidate,
        exit: AirwayExitCandidate,
        origin_anchor: NavRef,
        destination_anchor: Option<NavRef>,
    },
    MaterializeAirwayPresentationSelection {
        start_component_index: usize,
        presentation: AirwayPresentationPlan,
        entry_index: usize,
        exit_index: usize,
        origin_anchor: NavRef,
        destination_anchor: Option<NavRef>,
    },
    ListProcedures {
        airport_id: String,
        procedure_kind: ProcedureKind,
    },
    DescribeProcedureOptions {
        airport_id: String,
        procedure_id: String,
        procedure_kind: ProcedureKind,
    },
    MaterializeProcedure {
        airport_id: String,
        procedure_id: String,
        procedure_kind: ProcedureKind,
        runway_transition: Option<String>,
        enroute_transition: Option<String>,
        component_index: usize,
    },
    FindProcedurePlateMatch {
        airport_id: String,
        cifp_id: String,
    },
    DescribePlateProcedureLoads {
        plan: FlightPlan,
        plate_id: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HadReadError {
    NeedPages(Vec<u32>),
    Fatal(String),
}

impl From<AppError> for HadReadError {
    fn from(err: AppError) -> Self {
        Self::Fatal(err.to_string())
    }
}

pub fn run_had_operation(store: &NavKvStore, op: HadOperation) -> AppResult<HadOperationOutcome> {
    match run_had_operation_value(store, op) {
        Ok(result) => Ok(HadOperationOutcome::Complete { result }),
        Err(HadReadError::NeedPages(pages)) => Ok(HadOperationOutcome::NeedResources {
            resources: nav_kv_page_resources(pages),
        }),
        Err(HadReadError::Fatal(message)) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message,
        }),
    }
}

pub(crate) fn nav_kv_page_resources(mut pages: Vec<u32>) -> Vec<CoreResourceRequest> {
    pages.sort_unstable();
    pages.dedup();
    pages.into_iter().map(nav_kv_page_resource).collect()
}

pub(crate) fn nav_kv_page_resource(page: u32) -> CoreResourceRequest {
    CoreResourceRequest {
        id: format!("nav_kv/page/{page:04}"),
        address: format!("/nav-kv/page/{page:04}"),
        optional: false,
    }
}

pub fn nav_kv_page_index_from_resource_id(resource_id: &str) -> Option<u32> {
    resource_id
        .strip_prefix("nav_kv/page/")
        .and_then(|value| value.parse::<u32>().ok())
}

fn run_had_operation_value(store: &NavKvStore, op: HadOperation) -> Result<Value, HadReadError> {
    let value = match op {
        HadOperation::VectorManifest => {
            read_required::<Value>(store, NavKvQuery::VectorManifest, "vector manifest")?
        }
        HadOperation::ChartPageState {
            plan,
            recent_airport_ids,
            selected_airport_id,
            selected_chart_id,
        } => serde_json::to_value(chart_page_state(
            store,
            &plan,
            &recent_airport_ids,
            selected_airport_id.as_deref(),
            selected_chart_id.as_deref(),
        )?)?,
        HadOperation::PlateAirport { airport_id } => serde_json::to_value(read_optional::<Value>(
            store,
            NavKvQuery::PlateAirport { airport_id },
        )?)?,
        HadOperation::PlateById { plate_id } => serde_json::to_value(read_optional::<Value>(
            store,
            NavKvQuery::PlateById { plate_id },
        )?)?,
        HadOperation::FlightPlanUiState {
            plan,
            current_ui_state,
        } => serde_json::to_value(flight_plan_ui_state(store, plan, current_ui_state)?)?,
        HadOperation::FlightPlanUiMutation { mutation } => {
            serde_json::to_value(FlightPlanUiMutation {
                ui_state: flight_plan_ui_state(store, mutation.plan.clone(), mutation.ui_state)?,
                ..mutation
            })?
        }
        HadOperation::PreviewFlightPlanEntry { plan, input } => {
            serde_json::to_value(preview_flight_plan_entry(store, &plan, &input)?)?
        }
        HadOperation::AppendFlightPlanEntry { plan, input } => {
            serde_json::to_value(append_flight_plan_entry(store, &plan, &input)?)?
        }
        HadOperation::ProjectFlightPlanRoute { plan } => {
            serde_json::to_value(project_flight_plan_route(store, &plan)?)?
        }
        HadOperation::ResolveWaypointIdentifier { identifier } => {
            serde_json::to_value(resolve_waypoint_identifier_for_ui(store, &identifier)?)?
        }
        HadOperation::ResolveNavRefPosition { nav_ref } => {
            serde_json::to_value(nav_ref_position(store, &nav_ref, None)?)?
        }
        HadOperation::ResolveNavSymbolFeature { nav_ref } => {
            serde_json::to_value(nav_symbol_feature(store, &nav_ref)?)?
        }
        HadOperation::SuggestWaypointIdentifiers {
            plan,
            component_index,
            before,
            prefix,
            limit,
        } => serde_json::to_value(suggest_waypoint_identifiers(
            store,
            &plan,
            component_index,
            before,
            &prefix,
            limit,
        )?)?,
        HadOperation::SuggestWaypointIdentifiersNear {
            anchor,
            prefix,
            limit,
        } => serde_json::to_value(suggest_waypoint_identifiers_near(
            store, anchor, &prefix, limit,
        )?)?,
        HadOperation::SuggestAirwaysNearAnchor { anchor, limit } => {
            serde_json::to_value(suggest_airways_near_anchor(store, &anchor, limit)?)?
        }
        HadOperation::AirwayBranches { airway_name } => {
            serde_json::to_value(read_required::<Vec<AirwayBranch>>(
                store,
                NavKvQuery::AirwayBranches { airway_name },
                "airway branches",
            )?)?
        }
        HadOperation::PrepareAirwayPresentationForAnchors {
            airway_name,
            origin_anchor,
            destination_anchor,
        } => serde_json::to_value(prepare_airway_presentation_for_anchors(
            store,
            &airway_name,
            &origin_anchor,
            destination_anchor.as_ref(),
        )?)?,
        HadOperation::MaterializeAirwaySelection {
            start_component_index,
            entry,
            exit,
            origin_anchor,
            destination_anchor,
        } => serde_json::to_value(materialize_airway_selection(
            store,
            start_component_index,
            entry,
            exit,
            &origin_anchor,
            destination_anchor.as_ref(),
        )?)?,
        HadOperation::MaterializeAirwayPresentationSelection {
            start_component_index,
            presentation,
            entry_index,
            exit_index,
            origin_anchor,
            destination_anchor,
        } => serde_json::to_value(materialize_airway_presentation_selection(
            store,
            start_component_index,
            presentation,
            entry_index,
            exit_index,
            &origin_anchor,
            destination_anchor.as_ref(),
        )?)?,
        HadOperation::ListProcedures {
            airport_id,
            procedure_kind,
        } => serde_json::to_value(list_procedures_from_geometry(
            store,
            &airport_id,
            procedure_kind,
        )?)?,
        HadOperation::DescribeProcedureOptions {
            airport_id,
            procedure_id,
            procedure_kind,
        } => serde_json::to_value(describe_procedure_options(
            store,
            &airport_id,
            &procedure_id,
            procedure_kind,
        )?)?,
        HadOperation::MaterializeProcedure {
            airport_id,
            procedure_id,
            procedure_kind,
            runway_transition,
            enroute_transition,
            component_index,
        } => serde_json::to_value(materialize_procedure(
            store,
            &airport_id,
            &procedure_id,
            procedure_kind,
            runway_transition.as_deref(),
            enroute_transition.as_deref(),
            component_index,
        )?)?,
        HadOperation::FindProcedurePlateMatch {
            airport_id,
            cifp_id,
        } => {
            let rows = read_optional::<Vec<CifpTppMatchRow>>(
                store,
                NavKvQuery::PlateCifpMatch {
                    airport_id,
                    cifp_id,
                },
            )?;
            serde_json::to_value(rows.and_then(describe_show_plate_for_procedure))?
        }
        HadOperation::DescribePlateProcedureLoads { plan, plate_id } => {
            serde_json::to_value(describe_plate_loads(store, &plan, &plate_id)?)?
        }
    };
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapSelectorState {
    selected_map_id: String,
    selected_map: Option<MapViewOptionRecord>,
    available_maps: Vec<MapViewOptionRecord>,
    displayed_maps: Vec<MapViewOptionRecord>,
    geometry: DisplayGeometryRecord,
    family_options: Vec<MapFamilyOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapFamilyOption {
    id: String,
    label: String,
    launcher_label: String,
    enabled: bool,
    active: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapViewOptionRecord {
    id: String,
    label: String,
    region_id: String,
    map_view: MapViewRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DisplayGeometryRecord {
    schema_version: u32,
    polygons: Vec<PolygonRecord>,
    polygon_sets: Vec<DisplayPolygonSetRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DisplayPolygonSetRecord {
    id: String,
    polygon_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapViewRecord {
    chart_family: String,
    chart_name: String,
    chart_index: i64,
    tile_root: String,
    tile_url_root: String,
    tile_path_template: String,
    tile_size: i64,
    min_zoom: f64,
    max_zoom: f64,
    #[serde(default)]
    max_source_zoom: Option<i64>,
    #[serde(default)]
    max_display_zoom: Option<f64>,
    storage_kind: String,
    package_name: Option<String>,
    #[serde(default)]
    package_relative_path: Option<String>,
    full_coverage_zoom: Option<f64>,
    #[serde(default)]
    wide_angle: Option<WideAngleMapViewRecord>,
    initial_viewport: MapInitialViewportRecord,
    levels: Vec<MapViewLevelRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct WideAngleMapViewRecord {
    region_id: String,
    max_zoom: f64,
    package_name: String,
    #[serde(default)]
    package_relative_path: Option<String>,
    tile_url_root: String,
    tile_path_template: String,
    levels: Vec<MapViewLevelRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PackageRecord {
    id: String,
    family_id: String,
    region_id: Option<String>,
    #[serde(default)]
    relative_path: Option<String>,
    #[serde(default)]
    metadata: Option<PackageMetadataRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PackageMetadataRecord {
    full_coverage_zoom: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapInitialViewportRecord {
    lat: f64,
    lon: f64,
    zoom: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapViewLevelRecord {
    zoom: i64,
    boxes: Vec<MapViewTileBoundsRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapViewTileBoundsRecord {
    x_min: i64,
    x_max: i64,
    y_tms_min: i64,
    y_tms_max: i64,
}

fn read_required<T: DeserializeOwned>(
    store: &NavKvStore,
    query: NavKvQuery,
    family: &str,
) -> Result<T, HadReadError> {
    read_optional(store, query.clone())?.ok_or_else(|| {
        let key = crate::nav_kv_key_for_query(&query).unwrap_or_else(|| "<no-key>".to_string());
        HadReadError::Fatal(format!("HAD missing required {family} key: {key}"))
    })
}

fn read_optional<T: DeserializeOwned>(
    store: &NavKvStore,
    query: NavKvQuery,
) -> Result<Option<T>, HadReadError> {
    let Some(key) = crate::nav_kv_key_for_query(&query) else {
        return Ok(None);
    };
    match store.get_bytes(&key).map_err(HadReadError::Fatal)? {
        NavKvLookup::Hit(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|err| HadReadError::Fatal(format!("HAD JSON decode failed for {key}: {err}"))),
        NavKvLookup::MissingKey => Ok(None),
        NavKvLookup::MissingPages(pages) => Err(HadReadError::NeedPages(pages)),
    }
}

fn nav_ref_position(
    store: &NavKvStore,
    nav_ref: &NavRef,
    procedure_airport_id: Option<&str>,
) -> Result<LatLon, HadReadError> {
    if let NavRef::LatLon(position) | NavRef::Spot(position) = nav_ref {
        return Ok(*position);
    }
    read_required(
        store,
        NavKvQuery::NavRefPosition {
            nav_ref: nav_ref.clone(),
            procedure_airport_id: procedure_airport_id.map(str::to_string),
        },
        "navref position",
    )
}

fn nav_symbol_feature(
    store: &NavKvStore,
    nav_ref: &NavRef,
) -> Result<Option<NavSymbolFeature>, HadReadError> {
    read_optional(
        store,
        NavKvQuery::NavRefSymbol {
            nav_ref: nav_ref.clone(),
        },
    )
}

pub(crate) fn flight_plan_ui_state(
    store: &NavKvStore,
    plan: FlightPlan,
    current_ui_state: FlightPlanUiState,
) -> Result<FlightPlanUiState, HadReadError> {
    let plan = crate::build_flight_plan(plan)?;
    let mut ui_state = current_ui_state;
    let route = project_flight_plan_route(store, &plan)?;
    for row in &mut ui_state.display_rows {
        row.symbol_feature = match &row.nav_ref {
            Some(nav_ref) => nav_symbol_feature(store, nav_ref)?,
            None => None,
        };
        if let Some(leg_index) = row.leg_index {
            if let Some(leg) = plan.resolved_legs.get(leg_index) {
                let mut matching_segments = route.iter().filter(|segment| segment.leg_id == leg.id);
                if let Some(first_segment) = matching_segments.next() {
                    row.distance_nm = Some(
                        first_segment.distance_nm
                            + matching_segments
                                .map(|segment| segment.distance_nm)
                                .sum::<f64>(),
                    );
                    row.course_deg = Some(first_segment.course_deg);
                }
            }
        }
        if row
            .actions
            .iter()
            .any(|action| action.id == FlightPlanRowActionId::ShowPlate)
        {
            let match_rows = match (&row.chart_airport_id, &row.procedure_id) {
                (Some(airport_id), Some(procedure_id)) => read_optional::<Vec<CifpTppMatchRow>>(
                    store,
                    NavKvQuery::PlateCifpMatch {
                        airport_id: airport_id.clone(),
                        cifp_id: procedure_id.clone(),
                    },
                )?,
                _ => None,
            };
            let plate_match = match_rows.and_then(describe_show_plate_for_procedure);
            row.show_plate_target_id = plate_match.as_ref().map(|matched| matched.plate_id.clone());
            for action in &mut row.actions {
                if action.id == FlightPlanRowActionId::ShowPlate {
                    action.enabled = plate_match.is_some();
                }
            }
        }
    }
    Ok(ui_state)
}

fn chart_page_state(
    store: &NavKvStore,
    plan: &FlightPlan,
    stored_recent_airport_ids: &[String],
    candidate_airport_id: Option<&str>,
    candidate_chart_id: Option<&str>,
) -> Result<crate::DerivedChartPageState, HadReadError> {
    let mut airports = Vec::new();
    for airport_id in
        chart_page_airport_candidates(plan, stored_recent_airport_ids, candidate_airport_id)
    {
        if airports
            .iter()
            .any(|airport: &crate::DerivedChartAirport| airport.id == airport_id)
        {
            continue;
        }
        if let Some(airport) = read_optional(
            store,
            NavKvQuery::PlateAirport {
                airport_id: airport_id.clone(),
            },
        )? {
            airports.push(airport);
        }
    }
    Ok(derive_chart_page_state_from_airports(
        airports,
        stored_recent_airport_ids,
        candidate_airport_id,
        candidate_chart_id,
    ))
}

fn chart_page_airport_candidates(
    plan: &FlightPlan,
    stored_recent_airport_ids: &[String],
    candidate_airport_id: Option<&str>,
) -> Vec<String> {
    let mut airport_ids = Vec::new();
    if let Some(candidate_airport_id) = candidate_airport_id
        .map(str::trim)
        .filter(|airport_id| !airport_id.is_empty())
    {
        airport_ids.push(candidate_airport_id.to_ascii_uppercase());
    }
    for airport_id in stored_recent_airport_ids {
        let airport_id = airport_id.trim();
        if !airport_id.is_empty() {
            airport_ids.push(airport_id.to_ascii_uppercase());
        }
    }
    airport_ids.extend(airport_ids_from_plan(plan));

    let mut unique_airport_ids = Vec::new();
    for airport_id in airport_ids {
        if !unique_airport_ids
            .iter()
            .any(|existing| existing == &airport_id)
        {
            unique_airport_ids.push(airport_id);
        }
    }
    unique_airport_ids
}

fn map_selector_state(
    store: &NavKvStore,
    selected_map_id: Option<&str>,
    selected_family_id: Option<&str>,
) -> Result<MapSelectorState, HadReadError> {
    let mut map_views = chart_catalog(store)?;
    normalize_map_views(&mut map_views);
    let seed_map = map_views
        .iter()
        .find(|view| Some(view.id.as_str()) == selected_map_id);
    let seed_region_id = seed_map.map(|view| view.region_id.as_str()).or(Some("nw"));
    let selected_map = selected_family_id
        .and_then(|family_id| preferred_family_map(&map_views, family_id, seed_region_id))
        .or(seed_map)
        .or_else(|| preferred_family_map(&map_views, "tac", Some("nw")))
        .or_else(|| map_views.first())
        .cloned();
    let selected_family_id = selected_map
        .as_ref()
        .map(|view| view.map_view.chart_family.as_str())
        .unwrap_or("sec");
    let mut displayed_maps: Vec<MapViewOptionRecord> =
        displayed_family_maps(&map_views, selected_family_id)
            .into_iter()
            .cloned()
            .collect();
    let mut displayed_map_ids = displayed_maps
        .iter()
        .map(|view| view.id.clone())
        .collect::<HashSet<_>>();
    displayed_maps.extend(
        background_maps(&map_views)
            .into_iter()
            .filter(|view| displayed_map_ids.insert(view.id.clone()))
            .cloned(),
    );
    let geometry = displayed_geometry();
    let family_options = supported_chart_families()
        .into_iter()
        .map(|(id, label, launcher_label)| MapFamilyOption {
            id: id.to_string(),
            label: label.to_string(),
            launcher_label: launcher_label.to_string(),
            enabled: map_views
                .iter()
                .any(|view| view.map_view.chart_family == id),
            active: selected_family_id == id,
        })
        .collect();
    Ok(MapSelectorState {
        selected_map_id: selected_map
            .as_ref()
            .map(|view| view.id.clone())
            .unwrap_or_default(),
        selected_map,
        available_maps: map_views,
        displayed_maps,
        geometry,
        family_options,
    })
}

fn normalize_map_views(map_views: &mut [MapViewOptionRecord]) {
    for view in map_views {
        if view.map_view.max_source_zoom.is_none() {
            view.map_view.max_source_zoom =
                view.map_view.levels.iter().map(|level| level.zoom).max();
        }
        if view.map_view.max_display_zoom.is_none() {
            view.map_view.max_display_zoom = Some(view.map_view.max_zoom);
        }
    }
}

pub(crate) fn raster_map_catalog_from_nav_kv(
    store: &NavKvStore,
    selected_map_id: Option<&str>,
    selected_family_id: Option<&str>,
) -> Result<crate::RasterMapCatalog, HadReadError> {
    let state = map_selector_state(store, selected_map_id, selected_family_id)?;
    serde_json::from_value(serde_json::to_value(state)?)
        .map_err(|err| HadReadError::Fatal(format!("failed to decode raster map catalog: {err}")))
}

fn chart_catalog(store: &NavKvStore) -> Result<Vec<MapViewOptionRecord>, HadReadError> {
    let mut map_views = read_required::<Vec<MapViewOptionRecord>>(
        store,
        NavKvQuery::ChartCatalog,
        "chart catalog",
    )?;
    enrich_map_views_with_package_metadata(store, &mut map_views)?;
    Ok(map_views)
}

fn enrich_map_views_with_package_metadata(
    store: &NavKvStore,
    map_views: &mut [MapViewOptionRecord],
) -> Result<(), HadReadError> {
    for view in map_views {
        enrich_map_view_with_package_metadata(store, &mut view.map_view, &view.id)?;
        if let Some(wide_angle) = view.map_view.wide_angle.as_mut() {
            enrich_wide_angle_map_view_with_package_metadata(store, wide_angle, &view.id)?;
        }
    }
    Ok(())
}

fn enrich_map_view_with_package_metadata(
    store: &NavKvStore,
    map_view: &mut MapViewRecord,
    map_view_id: &str,
) -> Result<(), HadReadError> {
    let Some(package_id) = map_view.package_name.as_ref() else {
        return Ok(());
    };
    let package = package_record_for_raster_source(store, package_id, map_view_id)?;
    map_view.package_relative_path = Some(package_zip_relative_path(&package)?.to_string());
    map_view.full_coverage_zoom = package
        .metadata
        .and_then(|metadata| metadata.full_coverage_zoom);
    Ok(())
}

fn enrich_wide_angle_map_view_with_package_metadata(
    store: &NavKvStore,
    wide_angle: &mut WideAngleMapViewRecord,
    map_view_id: &str,
) -> Result<(), HadReadError> {
    let package = package_record_for_raster_source(store, &wide_angle.package_name, map_view_id)?;
    wide_angle.package_relative_path = Some(package_zip_relative_path(&package)?.to_string());
    Ok(())
}

fn package_record_for_raster_source(
    store: &NavKvStore,
    package_id: &str,
    map_view_id: &str,
) -> Result<PackageRecord, HadReadError> {
    read_optional::<PackageRecord>(
        store,
        NavKvQuery::PackageById {
            package_id: package_id.to_string(),
        },
    )?
    .ok_or_else(|| {
        HadReadError::Fatal(format!(
            "raster map view {map_view_id} references package {package_id}, but package/by-id/{package_id} is missing"
        ))
    })
}

fn package_zip_relative_path(package: &PackageRecord) -> Result<&str, HadReadError> {
    let relative_path = package.relative_path.as_ref().ok_or_else(|| {
        HadReadError::Fatal(format!(
            "package {} missing relative_path required for raster resources",
            package.id
        ))
    })?;
    if !relative_path.ends_with(".zip") {
        return Err(HadReadError::Fatal(format!(
            "package {} relative_path is not a zip: {}",
            package.id, relative_path
        )));
    }
    Ok(relative_path)
}

fn displayed_geometry() -> DisplayGeometryRecord {
    DisplayGeometryRecord {
        schema_version: 1,
        polygons: Vec::new(),
        polygon_sets: Vec::new(),
    }
}

fn supported_chart_families() -> [(&'static str, &'static str, &'static str); 5] {
    [
        ("sec", "SECTIONAL", "SEC"),
        ("tac", "TAC", "TAC"),
        ("enr-l", "IFR-LOW", "IFR L"),
        ("enr-h", "IFR-HIGH", "IFR H"),
        ("shaded-relief", "SHADED RELIEF", "RELIEF"),
    ]
}

fn displayed_family_maps<'a>(
    map_views: &'a [MapViewOptionRecord],
    family_id: &str,
) -> Vec<&'a MapViewOptionRecord> {
    if family_id == "tac" {
        return map_views
            .iter()
            .filter(|view| {
                let chart_family = view.map_view.chart_family.as_str();
                chart_family == "sec" || chart_family == "tac"
            })
            .collect();
    }
    map_views
        .iter()
        .filter(|view| view.map_view.chart_family == family_id)
        .collect()
}

fn background_maps(map_views: &[MapViewOptionRecord]) -> Vec<&MapViewOptionRecord> {
    map_views
        .iter()
        .filter(|view| view.map_view.chart_family == "world-basemap")
        .collect()
}

fn preferred_family_map<'a>(
    map_views: &'a [MapViewOptionRecord],
    family_id: &str,
    fallback_region_id: Option<&str>,
) -> Option<&'a MapViewOptionRecord> {
    let family_maps = map_views
        .iter()
        .filter(|view| view.map_view.chart_family == family_id)
        .collect::<Vec<_>>();
    family_maps
        .iter()
        .find(|view| Some(view.region_id.as_str()) == fallback_region_id)
        .copied()
        .or_else(|| family_maps.first().copied())
}

pub(crate) fn project_flight_plan_route(
    store: &NavKvStore,
    plan: &FlightPlan,
) -> Result<Vec<FlightPlanRouteSegment>, HadReadError> {
    let plan = crate::build_flight_plan(plan.clone())?;
    project_flight_plan_route_with_resolver(&plan, |nav_ref, procedure_airport_id| {
        nav_ref_position(store, nav_ref, procedure_airport_id)
    })
}

fn component_insert_anchor(
    plan: &FlightPlan,
    component_index: usize,
    before: bool,
) -> Result<NavRef, HadReadError> {
    let plan = plan.clone().normalized();
    let component = plan.route_components.get(component_index).ok_or_else(|| {
        HadReadError::Fatal(format!("component index out of bounds: {component_index}"))
    })?;
    let waypoint = match component {
        RouteComponent::Waypoint { waypoint } => Some(waypoint.clone()),
        RouteComponent::Airway { airway } => {
            if before {
                Some(airway.entry.clone())
            } else {
                Some(airway.exit.clone())
            }
        }
        RouteComponent::Procedure { .. } => {
            let mut legs = plan.resolved_legs.iter().filter(|leg| {
                matches!(
                    leg.source,
                    ResolvedLegSource::RouteComponent { component_index: index } if index == component_index
                )
            });
            if before {
                legs.next().map(|leg| leg.from.clone())
            } else {
                legs.last().map(|leg| leg.to.clone())
            }
        }
    };
    waypoint
        .ok_or_else(|| HadReadError::Fatal("selected component has no waypoint anchor".to_string()))
}

pub(crate) fn suggest_waypoint_identifiers(
    store: &NavKvStore,
    plan: &FlightPlan,
    component_index: usize,
    before: bool,
    prefix: &str,
    limit: usize,
) -> Result<Vec<WaypointIdentifierSuggestion>, HadReadError> {
    let anchor = component_insert_anchor(plan, component_index, before)?;
    let anchor_position = nav_ref_position(store, &anchor, None)?;
    suggest_waypoint_identifier_candidates(store, &prefix, limit, anchor_position)
}

fn suggest_waypoint_identifiers_near(
    store: &NavKvStore,
    anchor: LatLon,
    prefix: &str,
    limit: usize,
) -> Result<Vec<WaypointIdentifierSuggestion>, HadReadError> {
    suggest_waypoint_identifier_candidates(store, prefix, limit, anchor)
}

fn suggest_waypoint_identifier_candidates(
    store: &NavKvStore,
    prefix: &str,
    limit: usize,
    anchor_position: LatLon,
) -> Result<Vec<WaypointIdentifierSuggestion>, HadReadError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let prefix = prefix.trim().to_ascii_uppercase();
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    let prefix_chars = prefix.chars().collect::<Vec<_>>();
    let mut candidates = None;
    for length in (1..=prefix_chars.len()).rev() {
        let lookup_prefix = prefix_chars.iter().take(length).collect::<String>();
        if let Some(records) = read_optional::<Vec<WaypointIdentifierRecord>>(
            store,
            NavKvQuery::WaypointPrefix {
                prefix: lookup_prefix,
            },
        )? {
            candidates = Some(records);
            break;
        }
    }
    let Some(candidates) = candidates else {
        return Ok(Vec::new());
    };
    let mut suggestions = candidates
        .into_iter()
        .filter_map(|candidate| {
            canonical_waypoint_identifier_record_for_ui(store, candidate).transpose()
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|candidate| {
            candidate
                .identifier
                .trim()
                .to_ascii_uppercase()
                .starts_with(&prefix)
        })
        .map(|candidate| WaypointIdentifierSuggestion {
            identifier: candidate.identifier,
            nav_ref: candidate.nav_ref,
            kind: candidate.kind.clone(),
            display_name: waypoint_identifier_display_name(
                &candidate.kind,
                &candidate.city,
                &candidate.state,
                &candidate.facility_name,
            ),
            distance_from_anchor_nm: flight_leg_distance_nm(anchor_position, candidate.position),
        })
        .collect::<Vec<_>>();
    suggestions.sort_by(|left, right| {
        left.distance_from_anchor_nm
            .partial_cmp(&right.distance_from_anchor_nm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.identifier.cmp(&right.identifier))
            .then_with(|| {
                nav_ref_kind_order(&left.nav_ref).cmp(&nav_ref_kind_order(&right.nav_ref))
            })
    });
    let mut seen = Vec::<(String, NavRef)>::new();
    suggestions.retain(|suggestion| {
        let key = (suggestion.identifier.clone(), suggestion.nav_ref.clone());
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
    suggestions.truncate(limit);
    Ok(suggestions)
}

fn resolve_waypoint_identifier_for_ui(
    store: &NavKvStore,
    identifier: &str,
) -> Result<Option<NavRef>, HadReadError> {
    let normalized_identifier = identifier.trim().to_ascii_uppercase();
    let nav_ref = read_optional::<NavRef>(
        store,
        NavKvQuery::WaypointIdentifier {
            identifier: normalized_identifier.clone(),
        },
    )?;
    let Some(nav_ref) = nav_ref else {
        return Ok(None);
    };
    if !waypoint_identifier_is_canonical_for_ui(&normalized_identifier, &nav_ref) {
        return Ok(None);
    }
    if !waypoint_identifier_nav_ref_is_acceptable_for_ui(store, &nav_ref)? {
        return Ok(None);
    }
    Ok(Some(nav_ref))
}

fn canonical_waypoint_identifier_record_for_ui(
    store: &NavKvStore,
    mut record: WaypointIdentifierRecord,
) -> Result<Option<WaypointIdentifierRecord>, HadReadError> {
    if !waypoint_identifier_nav_ref_is_acceptable_for_ui(store, &record.nav_ref)? {
        return Ok(None);
    }
    match &record.nav_ref {
        NavRef::Airport(code) => record.identifier = code.trim().to_ascii_uppercase(),
        NavRef::Navaid(_)
        | NavRef::ArincNavaid { .. }
        | NavRef::TerminalNavaid { .. }
        | NavRef::Fix(_)
        | NavRef::LatLon(_)
        | NavRef::Spot(_) => record.identifier = record.identifier.trim().to_ascii_uppercase(),
    }
    if record.identifier.is_empty() {
        Ok(None)
    } else {
        Ok(Some(record))
    }
}

fn waypoint_identifier_nav_ref_is_acceptable_for_ui(
    store: &NavKvStore,
    nav_ref: &NavRef,
) -> Result<bool, HadReadError> {
    match nav_ref {
        NavRef::Navaid(_) | NavRef::ArincNavaid { .. } | NavRef::TerminalNavaid { .. } => {
            Ok(nav_symbol_feature(store, nav_ref)?.is_some())
        }
        NavRef::Airport(_) | NavRef::Fix(_) | NavRef::LatLon(_) | NavRef::Spot(_) => Ok(true),
    }
}

fn waypoint_identifier_is_canonical_for_ui(identifier: &str, nav_ref: &NavRef) -> bool {
    match nav_ref {
        NavRef::Airport(code) => identifier.trim().eq_ignore_ascii_case(code.trim()),
        NavRef::Navaid(_)
        | NavRef::ArincNavaid { .. }
        | NavRef::TerminalNavaid { .. }
        | NavRef::Fix(_)
        | NavRef::LatLon(_)
        | NavRef::Spot(_) => true,
    }
}

fn waypoint_identifier_display_name(
    kind: &str,
    city: &str,
    state: &str,
    facility_name: &str,
) -> String {
    let facility_name = facility_name.trim();
    let location = [city.trim(), state.trim()]
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if !facility_name.is_empty() && !location.is_empty() {
        format!("{facility_name}\n{location}")
    } else if !facility_name.is_empty() {
        facility_name.to_string()
    } else if !location.is_empty() {
        location
    } else {
        kind.to_string()
    }
}

fn nav_ref_kind_order(nav_ref: &NavRef) -> usize {
    match nav_ref {
        NavRef::Airport(_) => 0,
        NavRef::Navaid(_) => 1,
        NavRef::ArincNavaid { .. } => 1,
        NavRef::TerminalNavaid { .. } => 1,
        NavRef::Fix(_) => 2,
        NavRef::LatLon(_) | NavRef::Spot(_) => 3,
    }
}

fn suggest_airways_near_anchor(
    store: &NavKvStore,
    anchor: &NavRef,
    limit: usize,
) -> Result<Vec<AirwaySuggestion>, HadReadError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let anchor_position = nav_ref_position(store, anchor, None)?;
    let mut points = Vec::new();
    for radius_nm in [25.0, 50.0, 100.0, 200.0, 400.0] {
        for (lat_tile, lon_tile) in airway_spatial_tiles(anchor_position, radius_nm) {
            if let Some(tile_points) = read_optional::<Vec<AirwaySpatialPoint>>(
                store,
                NavKvQuery::AirwaySpatial { lat_tile, lon_tile },
            )? {
                points.extend(tile_points);
            }
        }
        let mut suggestions = suggestions_from_airway_points(anchor_position, &points, limit);
        if suggestions.len() >= limit || radius_nm == 400.0 {
            suggestions.truncate(limit);
            return Ok(suggestions);
        }
    }
    Ok(Vec::new())
}

fn airway_spatial_tiles(anchor: LatLon, radius_nm: f64) -> Vec<(i32, i32)> {
    let lat_delta = radius_nm / 60.0;
    let lon_delta = radius_nm / (60.0 * anchor.lat.to_radians().cos().abs().max(0.1));
    let min_lat = (anchor.lat - lat_delta).floor() as i32;
    let max_lat = (anchor.lat + lat_delta).floor() as i32;
    let min_lon = (anchor.lon - lon_delta).floor() as i32;
    let max_lon = (anchor.lon + lon_delta).floor() as i32;
    let mut tiles = Vec::new();
    for lat_tile in min_lat..=max_lat {
        for lon_tile in min_lon..=max_lon {
            tiles.push((lat_tile, lon_tile));
        }
    }
    tiles
}

fn suggestions_from_airway_points(
    anchor_position: LatLon,
    points: &[AirwaySpatialPoint],
    limit: usize,
) -> Vec<AirwaySuggestion> {
    let mut seen = HashMap::<String, AirwaySuggestion>::new();
    for point in points {
        let distance_from_anchor_nm = flight_leg_distance_nm(anchor_position, point.position);
        let suggestion = AirwaySuggestion {
            airway_name: point.airway_name.clone(),
            nearest_branch_key: Some(point.branch_key.clone()),
            nearest_nav_ref: point.nav_ref.clone(),
            nearest_sequence: point.sequence,
            distance_from_anchor_nm,
        };
        match seen.get(&point.airway_name) {
            Some(existing) if existing.distance_from_anchor_nm <= distance_from_anchor_nm => {}
            _ => {
                seen.insert(point.airway_name.clone(), suggestion);
            }
        }
    }
    let mut suggestions = seen.into_values().collect::<Vec<_>>();
    suggestions.sort_by(|left, right| {
        left.distance_from_anchor_nm
            .partial_cmp(&right.distance_from_anchor_nm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.airway_name.cmp(&right.airway_name))
    });
    suggestions.truncate(limit);
    suggestions
}

fn prepare_airway_presentation_for_anchors(
    store: &NavKvStore,
    airway_name: &str,
    origin_anchor: &NavRef,
    destination_anchor: Option<&NavRef>,
) -> Result<AirwayPresentationPlan, HadReadError> {
    let branches = read_required::<Vec<AirwayBranch>>(
        store,
        NavKvQuery::AirwayBranches {
            airway_name: airway_name.to_string(),
        },
        "airway branches",
    )?;
    let origin_position = nav_ref_position(store, origin_anchor, None)?;
    let destination_position = destination_anchor
        .map(|anchor| nav_ref_position(store, anchor, None))
        .transpose()?;
    prepare_airway_presentation(airway_name, branches, origin_position, destination_position)
        .map_err(Into::into)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MaterializedAirwayResponse {
    pub(crate) selection: AirwayAutoSelection,
    pub(crate) airway: AirwaySegment,
    #[serde(rename = "resolvedLegs")]
    pub(crate) resolved_legs: Vec<ResolvedLeg>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FlightPlanEntryTokenState {
    Neutral,
    Recognized,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FlightPlanEntryToken {
    start: usize,
    end: usize,
    state: FlightPlanEntryTokenState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FlightPlanEntryIssue {
    start: usize,
    end: usize,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct FlightPlanEntryPreview {
    can_commit: bool,
    tokens: Vec<FlightPlanEntryToken>,
    issues: Vec<FlightPlanEntryIssue>,
}

#[derive(Debug, Clone, PartialEq)]
struct ParsedInputToken {
    text: String,
    start: usize,
    end: usize,
    terminated: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum RecognizedInputToken {
    Waypoint(NavRef),
    Airway {
        airway_name: String,
        branches: Vec<AirwayBranch>,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct EvaluatedInputToken {
    parsed: ParsedInputToken,
    token_state: FlightPlanEntryTokenState,
    recognized: Option<RecognizedInputToken>,
}

#[derive(Debug, Clone, PartialEq)]
struct ExactAirwayMaterialization {
    airway: AirwaySegment,
    resolved_legs: Vec<ResolvedLeg>,
}

fn materialize_airway_selection(
    store: &NavKvStore,
    start_component_index: usize,
    entry: AirwayEntryCandidate,
    exit: AirwayExitCandidate,
    origin_anchor: &NavRef,
    destination_anchor: Option<&NavRef>,
) -> Result<MaterializedAirwayResponse, HadReadError> {
    let branches = read_required::<Vec<AirwayBranch>>(
        store,
        NavKvQuery::AirwayBranches {
            airway_name: entry.airway_name.clone(),
        },
        "airway branches",
    )?;
    let origin_position = nav_ref_position(store, origin_anchor, None)?;
    let destination_position = destination_anchor
        .map(|anchor| nav_ref_position(store, anchor, None))
        .transpose()?;
    let (airway, resolved_legs) =
        materialize_airway_from_branches(start_component_index, &entry, &exit, &branches)?;
    let entry_position = branches
        .iter()
        .find(|branch| branch.branch_key == entry.branch_key)
        .and_then(|branch| branch.points.get(entry.branch_point_index))
        .map(|point| point.position)
        .ok_or_else(|| {
            HadReadError::Fatal("selected airway entry point is not on branch".to_string())
        })?;
    let exit_position = branches
        .iter()
        .find(|branch| branch.branch_key == exit.branch_key)
        .and_then(|branch| branch.points.get(exit.branch_point_index))
        .map(|point| point.position)
        .ok_or_else(|| {
            HadReadError::Fatal("selected airway exit point is not on branch".to_string())
        })?;
    let origin_distance_nm = flight_leg_distance_nm(origin_position, entry_position);
    let destination_distance_nm = destination_position
        .map(|position| flight_leg_distance_nm(position, exit_position))
        .unwrap_or(0.0);
    Ok(MaterializedAirwayResponse {
        selection: AirwayAutoSelection {
            airway_name: entry.airway_name.clone(),
            branch_key: entry.branch_key.clone(),
            entry,
            exit,
            origin_distance_nm,
            destination_distance_nm,
            total_anchor_distance_nm: origin_distance_nm + destination_distance_nm,
        },
        airway,
        resolved_legs,
    })
}

pub(crate) fn materialize_airway_presentation_selection(
    store: &NavKvStore,
    start_component_index: usize,
    presentation: AirwayPresentationPlan,
    entry_index: usize,
    exit_index: usize,
    origin_anchor: &NavRef,
    destination_anchor: Option<&NavRef>,
) -> Result<MaterializedAirwayResponse, HadReadError> {
    if entry_index >= presentation.points.len() {
        return Err(HadReadError::Fatal(format!(
            "airway presentation entry index {entry_index} is out of bounds"
        )));
    }
    if exit_index >= presentation.points.len() {
        return Err(HadReadError::Fatal(format!(
            "airway presentation exit index {exit_index} is out of bounds"
        )));
    }
    if entry_index == exit_index {
        return Err(HadReadError::Fatal(
            "airway presentation exit cannot be the entry point".to_string(),
        ));
    }
    let entry = airway_entry_candidate_from_presentation(&presentation, entry_index);
    let exit = airway_exit_candidate_from_presentation(&presentation, entry_index, exit_index);
    materialize_airway_selection(
        store,
        start_component_index,
        entry,
        exit,
        origin_anchor,
        destination_anchor,
    )
}

fn airway_entry_candidate_from_presentation(
    presentation: &AirwayPresentationPlan,
    point_index: usize,
) -> AirwayEntryCandidate {
    let point = &presentation.points[point_index];
    AirwayEntryCandidate {
        airway_name: presentation.airway_name.clone(),
        branch_key: presentation.branch_key.clone(),
        branch_point_index: point.branch_point_index,
        sequence: point.sequence,
        nav_ref: point.nav_ref.clone(),
        distance_from_anchor_nm: 0.0,
        previous_nav_ref: point_index
            .checked_sub(1)
            .and_then(|index| presentation.points.get(index))
            .map(|point| point.nav_ref.clone()),
        next_nav_ref: presentation
            .points
            .get(point_index + 1)
            .map(|point| point.nav_ref.clone()),
    }
}

fn airway_exit_candidate_from_presentation(
    presentation: &AirwayPresentationPlan,
    entry_index: usize,
    point_index: usize,
) -> AirwayExitCandidate {
    let point = &presentation.points[point_index];
    AirwayExitCandidate {
        airway_name: presentation.airway_name.clone(),
        branch_key: presentation.branch_key.clone(),
        branch_point_index: point.branch_point_index,
        sequence: point.sequence,
        nav_ref: point.nav_ref.clone(),
        leg_offset_from_entry: point_index as isize - entry_index as isize,
        is_entry: point_index == entry_index,
        distance_from_target_nm: None,
    }
}

fn materialize_airway_from_branches(
    component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
    branches: &[AirwayBranch],
) -> Result<(AirwaySegment, Vec<ResolvedLeg>), HadReadError> {
    if entry.airway_name != exit.airway_name || entry.branch_key != exit.branch_key {
        return Err(HadReadError::Fatal(format!(
            "entry airway {} branch {} does not match exit airway {} branch {}",
            entry.airway_name, entry.branch_key, exit.airway_name, exit.branch_key
        )));
    }
    let branch = branches
        .iter()
        .find(|branch| branch.branch_key == entry.branch_key)
        .ok_or_else(|| {
            HadReadError::Fatal(format!(
                "unknown airway branch {} {}",
                entry.airway_name, entry.branch_key
            ))
        })?;
    let entry_point = branch.points.get(entry.branch_point_index).ok_or_else(|| {
        HadReadError::Fatal(format!(
            "entry index {} is out of bounds for airway {} branch {}",
            entry.branch_point_index, entry.airway_name, entry.branch_key
        ))
    })?;
    let exit_point = branch.points.get(exit.branch_point_index).ok_or_else(|| {
        HadReadError::Fatal(format!(
            "exit index {} is out of bounds for airway {} branch {}",
            exit.branch_point_index, entry.airway_name, entry.branch_key
        ))
    })?;
    if entry.branch_point_index == exit.branch_point_index {
        return Err(HadReadError::Fatal(
            "airway entry and exit cannot be the same point".to_string(),
        ));
    }
    let slice = if entry.branch_point_index < exit.branch_point_index {
        &branch.points[entry.branch_point_index..=exit.branch_point_index]
    } else {
        &branch.points[exit.branch_point_index..=entry.branch_point_index]
    };
    let traversed = if entry.branch_point_index < exit.branch_point_index {
        slice.to_vec()
    } else {
        slice.iter().rev().cloned().collect::<Vec<_>>()
    };
    let resolved_legs = traversed
        .windows(2)
        .enumerate()
        .map(|(index, pair)| ResolvedLeg {
            id: format!("airway-{}-{index}", branch.branch_key),
            from: pair[0].nav_ref.clone(),
            to: pair[1].nav_ref.clone(),
            source: ResolvedLegSource::RouteComponent { component_index },
            procedure_provenance: None,
        })
        .collect::<Vec<_>>();
    Ok((
        AirwaySegment {
            name: branch.display_name.clone(),
            branch_key: Some(branch.branch_key.clone()),
            entry: entry_point.nav_ref.clone(),
            exit: exit_point.nav_ref.clone(),
        },
        resolved_legs,
    ))
}

fn describe_procedure_options(
    store: &NavKvStore,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
) -> Result<ProcedureOptions, HadReadError> {
    describe_procedure_options_from_geometry_keys(store, airport_id, procedure_id, kind)
}

pub(crate) fn materialize_procedure(
    store: &NavKvStore,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
    component_index: usize,
) -> Result<MaterializedProcedure, HadReadError> {
    let record = read_required::<pgt::ProcedureGeometryRecord>(
        store,
        NavKvQuery::ProcedureGeometry {
            airport_id: airport_id.to_string(),
            procedure_kind: kind,
            procedure_id: procedure_id.to_string(),
            runway_transition: runway_transition.map(str::to_string),
            enroute_transition: enroute_transition.map(str::to_string),
        },
        "procedure geometry",
    )?;
    materialized_procedure_from_geometry_record(record, component_index).map_err(Into::into)
}

fn list_procedures_from_geometry(
    store: &NavKvStore,
    airport_id: &str,
    kind: ProcedureKind,
) -> Result<Vec<ProcedureSummary>, HadReadError> {
    let prefix = crate::navkv::procedure_geometry_kind_prefix(airport_id, &kind);
    let mut procedure_ids = nav_kv_prefix_keys(store, &prefix)?
        .filter_map(|key| {
            let suffix = key.strip_prefix(&prefix)?;
            let procedure_id = suffix.split('/').next()?;
            decode_key_component(procedure_id).ok()
        })
        .collect::<Vec<_>>();
    procedure_ids.sort();
    procedure_ids.dedup();
    Ok(procedure_ids
        .into_iter()
        .map(|procedure_id| ProcedureSummary {
            airport_id: airport_id.trim().to_string(),
            procedure_id,
            kind: kind.clone(),
        })
        .collect())
}

fn describe_procedure_options_from_geometry_keys(
    store: &NavKvStore,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
) -> Result<ProcedureOptions, HadReadError> {
    let prefix = crate::navkv::procedure_geometry_prefix(airport_id, &kind, procedure_id);
    let mut valid_choices = nav_kv_prefix_keys(store, &prefix)?
        .filter_map(|key| {
            let suffix = key.strip_prefix(&prefix)?;
            let mut parts = suffix.split('/');
            let runway_transition = decode_optional_transition_component(parts.next()?)?;
            let enroute_transition = decode_optional_transition_component(parts.next()?)?;
            if parts.next().is_some() {
                return None;
            }
            Some(crate::ProcedureSpecChoice {
                runway_transition,
                enroute_transition,
            })
        })
        .collect::<Vec<_>>();
    valid_choices.sort_by(|left, right| {
        left.runway_transition
            .cmp(&right.runway_transition)
            .then_with(|| left.enroute_transition.cmp(&right.enroute_transition))
    });
    valid_choices.dedup();

    let mut runway_transitions = valid_choices
        .iter()
        .filter_map(|choice| choice.runway_transition.clone())
        .collect::<Vec<_>>();
    runway_transitions.sort();
    runway_transitions.dedup();

    let mut enroute_transitions = valid_choices
        .iter()
        .filter_map(|choice| choice.enroute_transition.clone())
        .collect::<Vec<_>>();
    enroute_transitions.sort();
    enroute_transitions.dedup();

    Ok(ProcedureOptions {
        airport_id: airport_id.trim().to_string(),
        procedure_id: procedure_id.trim().to_string(),
        kind,
        runway_transitions,
        enroute_transitions,
        has_common_segment: valid_choices.len() > 1,
        valid_choices,
    })
}

fn nav_kv_prefix_keys(
    store: &NavKvStore,
    prefix: &str,
) -> Result<std::vec::IntoIter<String>, HadReadError> {
    match store
        .keys_with_prefix_lookup(prefix)
        .map_err(HadReadError::Fatal)?
    {
        NavKvLookup::Hit(bytes) => {
            let text =
                String::from_utf8(bytes).map_err(|err| HadReadError::Fatal(err.to_string()))?;
            Ok(text
                .lines()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
                .into_iter())
        }
        NavKvLookup::MissingPages(pages) => Err(HadReadError::NeedPages(pages)),
        NavKvLookup::MissingKey => Ok(Vec::new().into_iter()),
    }
}

fn decode_optional_transition_component(component: &str) -> Option<Option<String>> {
    if component == "_" {
        Some(None)
    } else {
        decode_key_component(component).ok().map(Some)
    }
}

fn decode_key_component(component: &str) -> Result<String, String> {
    let bytes = component.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().ok_or("truncated escape")?;
            let low = bytes.get(index + 2).copied().ok_or("truncated escape")?;
            out.push((hex_value(high)? << 4) | hex_value(low)?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|err| err.to_string())
}

fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex digit {}", byte as char)),
    }
}

fn materialized_procedure_from_geometry_record(
    record: pgt::ProcedureGeometryRecord,
    component_index: usize,
) -> AppResult<MaterializedProcedure> {
    let kind = procedure_kind_from_geometry(record.key.kind.clone());
    let terminal_discontinuity = record
        .terminal_discontinuity
        .clone()
        .map(procedure_discontinuity_from_geometry);
    let concretized_items = concretized_items_from_geometry_record(&record);
    let resolved_legs = record
        .leg_bundles
        .iter()
        .map(|bundle| resolved_leg_from_geometry_bundle(&record, bundle, component_index))
        .collect::<AppResult<Vec<_>>>()?;

    Ok(MaterializedProcedure {
        procedure: ProcedureSegment {
            airport_id: AirportId(record.key.airport_id.trim().to_string()),
            procedure_id: record.key.procedure_id.trim().to_string(),
            kind,
            runway_transition: record.key.runway_transition,
            enroute_transition: record.key.enroute_transition,
            terminal_discontinuity,
        },
        concretized_items,
        resolved_legs,
        data_quality: record
            .data_quality
            .into_iter()
            .map(|annotation| annotation.message)
            .collect(),
    })
}

fn concretized_items_from_geometry_record(
    record: &pgt::ProcedureGeometryRecord,
) -> Vec<ConcretizedNavItem> {
    let mut items = Vec::new();
    for bundle in &record.leg_bundles {
        for waypoint in &bundle.waypoints {
            let item = ConcretizedNavItem::Waypoint {
                nav_ref: nav_ref_from_geometry(waypoint.nav_ref.clone()),
            };
            if !matches!(
                (items.last(), &item),
                (
                    Some(ConcretizedNavItem::Waypoint { nav_ref: left }),
                    ConcretizedNavItem::Waypoint { nav_ref: right }
                ) if left == right
            ) {
                items.push(item);
            }
        }
    }
    if let Some(discontinuity) = record
        .terminal_discontinuity
        .clone()
        .map(procedure_discontinuity_from_geometry)
    {
        items.push(ConcretizedNavItem::Discontinuity {
            label: discontinuity.display_label().to_string(),
            discontinuity,
        });
    }
    items
}

fn resolved_leg_from_geometry_bundle(
    record: &pgt::ProcedureGeometryRecord,
    bundle: &pgt::ProcedureGeometryLegBundle,
    component_index: usize,
) -> AppResult<ResolvedLeg> {
    Ok(ResolvedLeg {
        id: bundle.id.clone(),
        from: nav_ref_from_geometry(bundle.from.clone()),
        to: nav_ref_from_geometry(bundle.to.clone()),
        source: ResolvedLegSource::RouteComponent { component_index },
        procedure_provenance: Some(ProcedureLegProvenance {
            airport_id: record.key.airport_id.clone(),
            procedure_id: record.key.procedure_id.clone(),
            kind: procedure_kind_from_geometry(record.key.kind.clone()),
            role: procedure_segment_role_from_geometry(bundle.role.clone()),
            path_termination: path_termination_from_geometry(bundle.path_termination.clone()),
            leg_sequence: bundle.leg_sequence,
            display_path: Some(display_path_from_geometry(bundle.path.clone())),
        }),
    })
}

fn display_path_from_geometry(path: pgt::ProcedureGeometryPath) -> LegDisplayPath {
    LegDisplayPath {
        style: match path.style {
            pgt::ProcedureGeometryPathStyle::Solid => LegDisplayPathStyle::Solid,
            pgt::ProcedureGeometryPathStyle::Dashed => LegDisplayPathStyle::Dashed,
        },
        elements: path
            .elements
            .into_iter()
            .map(|element| match element {
                pgt::ProcedureGeometryElement::Segment { start, end } => {
                    LegDisplayElement::Segment {
                        start: lat_lon_from_geometry(start),
                        end: lat_lon_from_geometry(end),
                    }
                }
                pgt::ProcedureGeometryElement::Arc {
                    center,
                    radius_nm,
                    start,
                    end,
                    clockwise,
                    sweep_degrees,
                } => LegDisplayElement::Arc {
                    center: lat_lon_from_geometry(center),
                    radius_nm,
                    start: lat_lon_from_geometry(start),
                    end: lat_lon_from_geometry(end),
                    clockwise,
                    sweep_degrees,
                },
            })
            .collect(),
        effective_terminal_course_deg: path.effective_terminal_course_deg,
        debug_element_sources: Vec::new(),
        debug_element_roles: Vec::new(),
    }
}

fn lat_lon_from_geometry(value: pgt::ProcedureLatLon) -> LatLon {
    LatLon {
        lat: value.lat,
        lon: value.lon,
    }
}

fn nav_ref_from_geometry(value: pgt::ProcedureNavRef) -> NavRef {
    match value {
        pgt::ProcedureNavRef::Airport(id) => NavRef::Airport(id),
        pgt::ProcedureNavRef::Navaid(id) => NavRef::Navaid(id),
        pgt::ProcedureNavRef::Fix(id) => NavRef::Fix(id),
        pgt::ProcedureNavRef::ArincNavaid {
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => NavRef::ArincNavaid {
            identifier,
            icao_code,
            section_code,
            subsection_code,
        },
        pgt::ProcedureNavRef::TerminalNavaid {
            airport_id,
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => NavRef::TerminalNavaid {
            airport_id,
            identifier,
            icao_code,
            section_code,
            subsection_code,
        },
        pgt::ProcedureNavRef::LatLon(value) => NavRef::LatLon(lat_lon_from_geometry(value)),
    }
}

fn procedure_kind_from_geometry(kind: pgt::ProcedureKind) -> ProcedureKind {
    match kind {
        pgt::ProcedureKind::Sid => ProcedureKind::Sid,
        pgt::ProcedureKind::Star => ProcedureKind::Star,
        pgt::ProcedureKind::Approach => ProcedureKind::Approach,
    }
}

fn procedure_discontinuity_from_geometry(
    discontinuity: pgt::ProcedureDiscontinuity,
) -> ProcedureDiscontinuity {
    match discontinuity {
        pgt::ProcedureDiscontinuity::Vectors => ProcedureDiscontinuity::Vectors,
        pgt::ProcedureDiscontinuity::Hold => ProcedureDiscontinuity::Hold,
        pgt::ProcedureDiscontinuity::Other(label) => ProcedureDiscontinuity::Other(label),
    }
}

fn procedure_segment_role_from_geometry(role: pgt::ProcedureSegmentRole) -> ProcedureSegmentRole {
    match role {
        pgt::ProcedureSegmentRole::EnrouteTransition => ProcedureSegmentRole::EnrouteTransition,
        pgt::ProcedureSegmentRole::Common => ProcedureSegmentRole::Common,
        pgt::ProcedureSegmentRole::RunwayTransition => ProcedureSegmentRole::RunwayTransition,
    }
}

fn path_termination_from_geometry(path: pgt::ProcedurePathTermination) -> PathTermination {
    match path {
        pgt::ProcedurePathTermination::InitialFix => PathTermination::InitialFix,
        pgt::ProcedurePathTermination::TrackToFix => PathTermination::TrackToFix,
        pgt::ProcedurePathTermination::CourseToFix => PathTermination::CourseToFix,
        pgt::ProcedurePathTermination::DirectToFix => PathTermination::DirectToFix,
        pgt::ProcedurePathTermination::HeadingToManual => PathTermination::HeadingToManual,
        pgt::ProcedurePathTermination::HeadingToAltitude => PathTermination::HeadingToAltitude,
        pgt::ProcedurePathTermination::Other(value) => PathTermination::Other(value),
    }
}

fn preview_flight_plan_entry(
    store: &NavKvStore,
    plan: &FlightPlan,
    input: &str,
) -> Result<FlightPlanEntryPreview, HadReadError> {
    let tokens = tokenize_flight_plan_entry(input);
    let evaluated = evaluate_flight_plan_entry_tokens(store, &tokens)?;
    let issues = validate_flight_plan_entry(plan, &evaluated)?;
    let can_commit = !input.trim().is_empty()
        && issues.is_empty()
        && evaluated.iter().all(|token| {
            token.recognized.is_some()
                && (token.parsed.terminated
                    || token.token_state == FlightPlanEntryTokenState::Recognized)
        })
        && append_flight_plan_tokens(plan, &evaluated).is_ok();
    Ok(FlightPlanEntryPreview {
        can_commit,
        tokens: evaluated
            .iter()
            .map(|token| FlightPlanEntryToken {
                start: token.parsed.start,
                end: token.parsed.end,
                state: token.token_state.clone(),
            })
            .collect(),
        issues,
    })
}

fn append_flight_plan_entry(
    store: &NavKvStore,
    plan: &FlightPlan,
    input: &str,
) -> Result<FlightPlanUiMutation, HadReadError> {
    let tokens = tokenize_flight_plan_entry(input);
    let evaluated = evaluate_flight_plan_entry_tokens(store, &tokens)?;
    let issues = validate_flight_plan_entry(plan, &evaluated)?;
    if !issues.is_empty() {
        return Err(HadReadError::Fatal(
            issues
                .first()
                .map(|issue| issue.message.clone())
                .unwrap_or_else(|| "invalid route entry".to_string()),
        ));
    }
    let appended = append_flight_plan_tokens(plan, &evaluated)?;
    Ok(FlightPlanUiMutation {
        ui_state: flight_plan_ui_state(
            store,
            appended.clone(),
            crate::project_ui_state(&appended),
        )?,
        plan: appended,
    })
}

pub(crate) fn insert_waypoint_best_position(
    store: &NavKvStore,
    plan: &FlightPlan,
    waypoint: NavRef,
) -> Result<FlightPlanUiMutation, HadReadError> {
    if let Some(message) = insert_waypoint_best_position_rejection(plan, &waypoint) {
        return Err(HadReadError::Fatal(message));
    }

    let insertion_index = best_top_level_insertion_index(store, plan, &waypoint)?;
    let inserted = insert_waypoint_at_top_level(plan, insertion_index, waypoint)?;
    Ok(FlightPlanUiMutation {
        ui_state: flight_plan_ui_state(
            store,
            inserted.clone(),
            crate::project_ui_state(&inserted),
        )?,
        plan: inserted,
    })
}

pub(crate) fn insert_waypoint_best_position_rejection(
    plan: &FlightPlan,
    waypoint: &NavRef,
) -> Option<String> {
    if flight_plan_has_direct_to_overlay(plan) {
        return Some("cannot insert while direct-to is off the flight plan".to_string());
    }
    if !matches!(
        waypoint,
        NavRef::Airport(_)
            | NavRef::Navaid(_)
            | NavRef::Fix(_)
            | NavRef::LatLon(_)
            | NavRef::Spot(_)
    ) {
        return Some("only positioned waypoints can be inserted from map selection".to_string());
    }
    if flight_plan_contains_nav_ref(plan, waypoint) {
        return Some(format!(
            "{} is already in the flight plan",
            nav_ref_display_label(waypoint)
        ));
    }
    None
}

fn best_top_level_insertion_index(
    store: &NavKvStore,
    plan: &FlightPlan,
    waypoint: &NavRef,
) -> Result<usize, HadReadError> {
    let plan = plan.clone().normalized();
    if plan.route_components.len() <= 1 {
        return Ok(plan.route_components.len());
    }
    let waypoint_position = nav_ref_position(store, waypoint, None)?;
    let mut best: Option<(usize, f64)> = None;
    for insertion_index in 0..=plan.route_components.len() {
        let prev = plan.route_components[..insertion_index]
            .iter()
            .rev()
            .find_map(route_component_last_nav_ref);
        let next = plan.route_components[insertion_index..]
            .iter()
            .find_map(route_component_first_nav_ref);
        let cost = insertion_added_length_nm(store, prev, next, waypoint_position)?;
        match best {
            Some((_, best_cost)) if cost >= best_cost => {}
            _ => best = Some((insertion_index, cost)),
        }
    }
    best.map(|(index, _)| index)
        .ok_or_else(|| HadReadError::Fatal("could not place waypoint in flight plan".to_string()))
}

fn insertion_added_length_nm(
    store: &NavKvStore,
    prev: Option<&NavRef>,
    next: Option<&NavRef>,
    waypoint_position: LatLon,
) -> Result<f64, HadReadError> {
    let prev_position = match prev {
        Some(nav_ref) => Some(nav_ref_position(store, nav_ref, None)?),
        None => None,
    };
    let next_position = match next {
        Some(nav_ref) => Some(nav_ref_position(store, nav_ref, None)?),
        None => None,
    };
    Ok(match (prev_position, next_position) {
        (Some(prev), Some(next)) => {
            flight_leg_distance_nm(prev, waypoint_position)
                + flight_leg_distance_nm(waypoint_position, next)
                - flight_leg_distance_nm(prev, next)
        }
        (Some(prev), None) => flight_leg_distance_nm(prev, waypoint_position),
        (None, Some(next)) => flight_leg_distance_nm(waypoint_position, next),
        (None, None) => 0.0,
    })
}

fn insert_waypoint_at_top_level(
    plan: &FlightPlan,
    insertion_index: usize,
    waypoint: NavRef,
) -> Result<FlightPlan, HadReadError> {
    let plan = plan.clone().normalized();
    if plan.route_components.is_empty() {
        let mut next_plan = plan;
        next_plan
            .route_components
            .push(RouteComponent::Waypoint { waypoint });
        next_plan.resolved_legs.clear();
        return Ok(next_plan.normalized());
    }
    if insertion_index >= plan.route_components.len() {
        insert_waypoint(&plan, plan.route_components.len() - 1, false, waypoint).map_err(Into::into)
    } else {
        insert_waypoint(&plan, insertion_index, true, waypoint).map_err(Into::into)
    }
}

fn route_component_first_nav_ref(component: &RouteComponent) -> Option<&NavRef> {
    match component {
        RouteComponent::Waypoint { waypoint } => Some(waypoint),
        RouteComponent::Airway { airway } => Some(&airway.entry),
        RouteComponent::Procedure { .. } => None,
    }
}

fn route_component_last_nav_ref(component: &RouteComponent) -> Option<&NavRef> {
    match component {
        RouteComponent::Waypoint { waypoint } => Some(waypoint),
        RouteComponent::Airway { airway } => Some(&airway.exit),
        RouteComponent::Procedure { .. } => None,
    }
}

fn tokenize_flight_plan_entry(input: &str) -> Vec<ParsedInputToken> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, ch) in input.char_indices() {
        if ch.is_whitespace() {
            if let Some(token_start) = start.take() {
                tokens.push(ParsedInputToken {
                    text: input[token_start..index].to_ascii_uppercase(),
                    start: token_start,
                    end: index,
                    terminated: true,
                });
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(token_start) = start {
        tokens.push(ParsedInputToken {
            text: input[token_start..].to_ascii_uppercase(),
            start: token_start,
            end: input.len(),
            terminated: false,
        });
    }
    tokens
}

fn evaluate_flight_plan_entry_tokens(
    store: &NavKvStore,
    tokens: &[ParsedInputToken],
) -> Result<Vec<EvaluatedInputToken>, HadReadError> {
    let mut evaluated = Vec::with_capacity(tokens.len());
    for token in tokens {
        let recognized = recognize_flight_plan_entry_token(store, &token.text)?;
        let token_state = match (&recognized, token.terminated) {
            (Some(_), _) => FlightPlanEntryTokenState::Recognized,
            (None, true) => FlightPlanEntryTokenState::Invalid,
            (None, false) => FlightPlanEntryTokenState::Neutral,
        };
        evaluated.push(EvaluatedInputToken {
            parsed: token.clone(),
            token_state,
            recognized,
        });
    }
    Ok(evaluated)
}

fn recognize_flight_plan_entry_token(
    store: &NavKvStore,
    token: &str,
) -> Result<Option<RecognizedInputToken>, HadReadError> {
    if token.trim().is_empty() {
        return Ok(None);
    }
    if let Some(nav_ref) = resolve_waypoint_identifier_for_ui(store, token)? {
        return Ok(Some(RecognizedInputToken::Waypoint(nav_ref)));
    }
    if let Some(branches) = read_optional::<Vec<AirwayBranch>>(
        store,
        NavKvQuery::AirwayBranches {
            airway_name: token.to_string(),
        },
    )? {
        if !branches.is_empty() {
            return Ok(Some(RecognizedInputToken::Airway {
                airway_name: token.to_string(),
                branches,
            }));
        }
    }
    Ok(None)
}

fn validate_flight_plan_entry(
    plan: &FlightPlan,
    tokens: &[EvaluatedInputToken],
) -> Result<Vec<FlightPlanEntryIssue>, HadReadError> {
    let mut issues = Vec::new();
    let mut current_anchor = trailing_component_anchor(plan);
    let mut current_anchor_token_index = None;
    let mut pending_airway: Option<(usize, NavRef, String, Vec<AirwayBranch>)> = None;

    for (index, token) in tokens.iter().enumerate() {
        let Some(recognized) = &token.recognized else {
            if token.parsed.terminated {
                issues.push(FlightPlanEntryIssue {
                    start: token.parsed.start,
                    end: token.parsed.end,
                    message: format!("unknown route element {}", token.parsed.text),
                });
            }
            break;
        };
        match recognized {
            RecognizedInputToken::Waypoint(nav_ref) => {
                if let Some((airway_index, origin_anchor, airway_name, branches)) =
                    pending_airway.take()
                {
                    if let Err(err) = exact_airway_materialization(
                        &airway_name,
                        &branches,
                        &origin_anchor,
                        nav_ref,
                        plan.route_components.len(),
                    ) {
                        let message = match err {
                            HadReadError::Fatal(message) => message,
                            HadReadError::NeedPages(_) => {
                                "airway lookup needs resources".to_string()
                            }
                        };
                        issues.push(FlightPlanEntryIssue {
                            start: tokens[airway_index].parsed.start,
                            end: token.parsed.end,
                            message,
                        });
                        break;
                    }
                }
                current_anchor = Some(nav_ref.clone());
                current_anchor_token_index = Some(index);
            }
            RecognizedInputToken::Airway {
                airway_name,
                branches,
            } => {
                let Some(origin_anchor) = current_anchor.clone() else {
                    issues.push(FlightPlanEntryIssue {
                        start: token.parsed.start,
                        end: token.parsed.end,
                        message: format!("{airway_name} requires a preceding waypoint"),
                    });
                    break;
                };
                if !airway_contains_nav_ref(airway_name, branches, &origin_anchor) {
                    let start = current_anchor_token_index
                        .and_then(|anchor_index| tokens.get(anchor_index))
                        .map(|anchor_token| anchor_token.parsed.start)
                        .unwrap_or(token.parsed.start);
                    issues.push(FlightPlanEntryIssue {
                        start,
                        end: token.parsed.end,
                        message: format!(
                            "{} not on {}",
                            nav_ref_display_label(&origin_anchor),
                            airway_name
                        ),
                    });
                    break;
                }
                if pending_airway.is_some() {
                    issues.push(FlightPlanEntryIssue {
                        start: token.parsed.start,
                        end: token.parsed.end,
                        message: "airway requires an exit waypoint".to_string(),
                    });
                    break;
                }
                pending_airway =
                    Some((index, origin_anchor, airway_name.clone(), branches.clone()));
            }
        }
    }

    if issues.is_empty() && tokens.iter().all(|token| token.recognized.is_some()) {
        if let Some((airway_index, _, airway_name, _)) = pending_airway {
            if tokens[airway_index].parsed.terminated {
                issues.push(FlightPlanEntryIssue {
                    start: tokens[airway_index].parsed.start,
                    end: tokens[airway_index].parsed.end,
                    message: format!("{airway_name} requires an exit waypoint"),
                });
            }
        }
    }
    Ok(issues)
}

fn append_flight_plan_tokens(
    plan: &FlightPlan,
    tokens: &[EvaluatedInputToken],
) -> Result<FlightPlan, HadReadError> {
    let mut next_plan = plan.clone();
    let mut index = 0usize;
    while index < tokens.len() {
        let Some(recognized) = &tokens[index].recognized else {
            return Err(HadReadError::Fatal(format!(
                "unknown route element {}",
                tokens[index].parsed.text
            )));
        };
        match recognized {
            RecognizedInputToken::Waypoint(nav_ref) => {
                next_plan = append_waypoint_tail(&next_plan, nav_ref.clone())?;
                index += 1;
            }
            RecognizedInputToken::Airway {
                airway_name,
                branches,
            } => {
                let origin_anchor = trailing_component_anchor(&next_plan).ok_or_else(|| {
                    HadReadError::Fatal(format!("{airway_name} requires a preceding waypoint"))
                })?;
                let next_token = tokens.get(index + 1).ok_or_else(|| {
                    HadReadError::Fatal(format!("{airway_name} requires an exit waypoint"))
                })?;
                let destination = match &next_token.recognized {
                    Some(RecognizedInputToken::Waypoint(nav_ref)) => nav_ref.clone(),
                    Some(RecognizedInputToken::Airway { .. }) => {
                        return Err(HadReadError::Fatal(format!(
                            "{airway_name} requires an exit waypoint"
                        )))
                    }
                    None => {
                        return Err(HadReadError::Fatal(format!(
                            "unknown route element {}",
                            next_token.parsed.text
                        )))
                    }
                };
                let materialized = exact_airway_materialization(
                    airway_name,
                    branches,
                    &origin_anchor,
                    &destination,
                    next_plan.route_components.len(),
                )?;
                next_plan = append_airway_tail(&next_plan, materialized)?;
                index += 2;
            }
        }
    }
    let normalized = next_plan.normalized();
    if normalized.resolved_legs.is_empty() && normalized.route_components.len() > 1 {
        return Err(HadReadError::Fatal(
            "flight plan append requires at least one flyable leg".to_string(),
        ));
    }
    Ok(normalized)
}

fn append_waypoint_tail(plan: &FlightPlan, waypoint: NavRef) -> Result<FlightPlan, HadReadError> {
    if plan.route_components.is_empty() {
        let mut next_plan = plan.clone();
        next_plan
            .route_components
            .push(RouteComponent::Waypoint { waypoint });
        next_plan.resolved_legs.clear();
        return Ok(next_plan.normalized());
    }
    if let Some(RouteComponent::Airway { airway }) = plan.route_components.last() {
        let with_exit = insert_waypoint(
            plan,
            plan.route_components.len() - 1,
            false,
            airway.exit.clone(),
        )
        .map_err(HadReadError::from)?;
        return append_waypoint_tail(&with_exit, waypoint);
    }
    insert_waypoint(plan, plan.route_components.len() - 1, false, waypoint).map_err(Into::into)
}

fn append_airway_tail(
    plan: &FlightPlan,
    materialized: ExactAirwayMaterialization,
) -> Result<FlightPlan, HadReadError> {
    let mut plan = plan.clone();
    let mut start_component_index = plan
        .route_components
        .len()
        .checked_sub(1)
        .ok_or_else(|| HadReadError::Fatal("cannot append airway to empty plan".to_string()))?;
    if let Some(RouteComponent::Airway { airway }) =
        plan.route_components.get(start_component_index)
    {
        if airway.exit != materialized.airway.entry {
            return Err(HadReadError::Fatal(format!(
                "{} cannot start after {}",
                materialized.airway.name,
                nav_ref_display_label(&airway.exit)
            )));
        }
        plan = append_waypoint_tail(&plan, airway.exit.clone())?;
        start_component_index = plan.route_components.len() - 1;
    }
    insert_airway_after_waypoint(
        &plan,
        start_component_index,
        materialized.airway,
        materialized.resolved_legs,
    )
    .map_err(Into::into)
}

fn exact_airway_materialization(
    airway_name: &str,
    branches: &[AirwayBranch],
    origin_anchor: &NavRef,
    destination_anchor: &NavRef,
    component_index: usize,
) -> Result<ExactAirwayMaterialization, HadReadError> {
    for branch in branches
        .iter()
        .filter(|branch| branch.display_name.trim() == airway_name.trim())
    {
        let Some(entry_index) = branch
            .points
            .iter()
            .position(|point| point.nav_ref == *origin_anchor)
        else {
            continue;
        };
        let Some(exit_index) = branch
            .points
            .iter()
            .position(|point| point.nav_ref == *destination_anchor)
        else {
            continue;
        };
        let entry = AirwayEntryCandidate {
            airway_name: airway_name.to_string(),
            branch_key: branch.branch_key.clone(),
            branch_point_index: entry_index,
            sequence: branch.points[entry_index].sequence,
            nav_ref: branch.points[entry_index].nav_ref.clone(),
            distance_from_anchor_nm: 0.0,
            previous_nav_ref: entry_index
                .checked_sub(1)
                .and_then(|idx| branch.points.get(idx))
                .map(|point| point.nav_ref.clone()),
            next_nav_ref: branch
                .points
                .get(entry_index + 1)
                .map(|point| point.nav_ref.clone()),
        };
        let exit = AirwayExitCandidate {
            airway_name: airway_name.to_string(),
            branch_key: branch.branch_key.clone(),
            branch_point_index: exit_index,
            sequence: branch.points[exit_index].sequence,
            nav_ref: branch.points[exit_index].nav_ref.clone(),
            leg_offset_from_entry: exit_index as isize - entry_index as isize,
            is_entry: exit_index == entry_index,
            distance_from_target_nm: Some(0.0),
        };
        let (airway, resolved_legs) =
            materialize_airway_from_branches(component_index, &entry, &exit, branches)?;
        return Ok(ExactAirwayMaterialization {
            airway,
            resolved_legs,
        });
    }
    let matching_branches = branches
        .iter()
        .filter(|branch| branch.display_name.trim() == airway_name.trim())
        .collect::<Vec<_>>();
    let origin_on_airway = matching_branches.iter().any(|branch| {
        branch
            .points
            .iter()
            .any(|point| point.nav_ref == *origin_anchor)
    });
    let destination_on_airway = matching_branches.iter().any(|branch| {
        branch
            .points
            .iter()
            .any(|point| point.nav_ref == *destination_anchor)
    });
    if !origin_on_airway {
        return Err(HadReadError::Fatal(format!(
            "{} not on {}",
            nav_ref_display_label(origin_anchor),
            airway_name
        )));
    }
    if !destination_on_airway {
        return Err(HadReadError::Fatal(format!(
            "{} not on {}",
            nav_ref_display_label(destination_anchor),
            airway_name
        )));
    }
    Err(HadReadError::Fatal(format!(
        "{} and {} are not on the same {} branch",
        nav_ref_display_label(origin_anchor),
        nav_ref_display_label(destination_anchor),
        airway_name
    )))
}

fn airway_contains_nav_ref(airway_name: &str, branches: &[AirwayBranch], nav_ref: &NavRef) -> bool {
    branches
        .iter()
        .filter(|branch| branch.display_name.trim() == airway_name.trim())
        .any(|branch| branch.points.iter().any(|point| point.nav_ref == *nav_ref))
}

fn trailing_component_anchor(plan: &FlightPlan) -> Option<NavRef> {
    match plan.route_components.last()? {
        RouteComponent::Waypoint { waypoint } => Some(waypoint.clone()),
        RouteComponent::Airway { airway } => Some(airway.exit.clone()),
        RouteComponent::Procedure { .. } => None,
    }
}

fn nav_ref_display_label(nav_ref: &NavRef) -> String {
    match nav_ref {
        NavRef::Airport(value) | NavRef::Navaid(value) | NavRef::Fix(value) => value.clone(),
        NavRef::ArincNavaid { identifier, .. } | NavRef::TerminalNavaid { identifier, .. } => {
            identifier.clone()
        }
        NavRef::LatLon(value) => format!("{:.3},{:.3}", value.lat, value.lon),
        NavRef::Spot(value) => format!("SPOT {:.3},{:.3}", value.lat, value.lon),
    }
}

fn describe_plate_loads(
    store: &NavKvStore,
    plan: &FlightPlan,
    plate_id: &str,
) -> Result<Vec<ProcedureLoadOption>, HadReadError> {
    let Some(rows) = read_optional::<Vec<CifpTppMatchRow>>(
        store,
        NavKvQuery::PlateProcedureCandidates {
            plate_id: plate_id.to_string(),
        },
    )?
    else {
        return Ok(Vec::new());
    };
    let mut grouped = HashMap::<String, Vec<CifpTppMatchRow>>::new();
    for row in rows {
        grouped
            .entry(format!("{}:{}", row.airport_id, row.cifp_id))
            .or_default()
            .push(row);
    }
    let mut candidates = Vec::new();
    for match_rows in grouped.into_values() {
        let Some(preferred) = crate::select_preferred_cifp_tpp_match(match_rows.clone()) else {
            continue;
        };
        let options = describe_procedure_options_from_geometry_keys(
            store,
            &preferred.airport_id,
            &preferred.cifp_id,
            ProcedureKind::Approach,
        )?;
        if options.valid_choices.is_empty() {
            continue;
        }
        candidates.push(PlateProcedureLoadCandidateInput {
            airport_id: preferred.airport_id,
            cifp_id: preferred.cifp_id,
            match_rows,
            options,
        });
    }
    describe_plate_procedure_load_options(plan, candidates).map_err(Into::into)
}

impl From<serde_json::Error> for HadReadError {
    fn from(err: serde_json::Error) -> Self {
        Self::Fatal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{planning::NavElementUiView, AirportId, NavKvRoot, ProcedureKind, SequencingMode};
    use app_fixtures::load_fixture_nav_kv_pages;
    use std::{fs, path::PathBuf};

    fn resource_page_indexes(resources: &[CoreResourceRequest]) -> Vec<u32> {
        resources
            .iter()
            .map(|resource| {
                nav_kv_page_index_from_resource_id(&resource.id)
                    .unwrap_or_else(|| panic!("unexpected resource id: {}", resource.id))
            })
            .collect()
    }

    #[test]
    fn operation_reports_page_faults_instead_of_exposing_query_keys() {
        let (root, _pages) = fixture(&[("vector/manifest", br#"{"layers":[]}"#.as_slice())], 64);
        let store = NavKvStore::new(root);

        match run_had_operation(&store, HadOperation::VectorManifest).unwrap() {
            HadOperationOutcome::NeedResources { resources } => {
                assert_eq!(resource_page_indexes(&resources), vec![0]);
            }
            other => panic!("expected resource fault, got {other:?}"),
        }
    }

    #[test]
    fn operation_decodes_values_after_platform_supplies_pages() {
        let (root, pages) = fixture(&[("vector/manifest", br#"{"layers":[]}"#.as_slice())], 64);
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        assert_eq!(
            run_had_operation(&store, HadOperation::VectorManifest).unwrap(),
            HadOperationOutcome::Complete {
                result: serde_json::json!({"layers":[]})
            }
        );
    }

    #[test]
    fn vector_manifest_operation_reads_core_owned_key() {
        let manifest = br#"{"point_layers":{},"airspace":{"reference_tile_min_zoom":0,"reference_tile_max_zoom":12,"label_tile_min_zoom":0,"label_tile_max_zoom":12}}"#;
        let (root, pages) = fixture(&[("vector/manifest", manifest.as_slice())], 256);
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        let outcome = run_had_operation(&store, HadOperation::VectorManifest).unwrap();
        match outcome {
            HadOperationOutcome::Complete { result } => {
                assert_eq!(result["airspace"]["reference_tile_max_zoom"], 12);
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("unexpected resources: {resources:?}")
            }
        }
    }

    #[test]
    fn empty_plan_direct_to_projects_active_route_segment() {
        let (root, _pages) = fixture(&[("dummy", b"1".as_slice())], 64);
        let store = NavKvStore::new(root);
        let start = LatLon {
            lat: 47.600,
            lon: -122.300,
        };
        let target = LatLon {
            lat: 47.700,
            lon: -122.100,
        };
        let plan = crate::activate_direct_to(&FlightPlan::empty(), start, NavRef::LatLon(target))
            .expect("activate direct-to");

        let route = project_flight_plan_route(&store, &plan).expect("project direct-to route");

        assert_eq!(route.len(), 1);
        assert_eq!(route[0].id, "direct-to");
        assert_eq!(route[0].status, crate::FlightPlanRouteSegmentStatus::Active);
        assert_eq!(route[0].from, start);
        assert_eq!(route[0].to, target);
    }

    #[test]
    fn map_selector_state_enriches_full_coverage_zoom_from_package_metadata() {
        let chart_catalog = br#"[{
          "id":"sec:nw",
          "label":"Northwest Sectional",
          "region_id":"nw",
          "map_view":{
            "chart_family":"sec",
            "chart_name":"Northwest Sectional",
            "chart_index":0,
            "tile_root":"tiles",
            "tile_url_root":"/sectional-packages/NW_SEC_2604/tiles",
            "tile_path_template":"0/{z}/{x}/{y}.webp",
            "tile_size":512,
            "min_zoom":4.2,
            "max_zoom":12.5,
            "storage_kind":"sectional_package",
            "package_name":"NW_SEC_2604",
            "initial_viewport":{"lat":45.0,"lon":-122.0,"zoom":8.0},
            "levels":[{"zoom":10,"boxes":[{"x_min":1,"x_max":2,"y_tms_min":3,"y_tms_max":4}]}]
          }
        }]"#;
        let package_by_id = br#"{
          "id":"NW_SEC_2604",
          "family_id":"sec",
          "region_id":"nw",
          "relative_path":"nw_sec_2604_hash.zip",
          "metadata":{"full_coverage_zoom":7}
        }"#;
        let (root, pages) = fixture(
            &[
                ("chart/catalog", chart_catalog.as_slice()),
                ("package/by-id/NW_SEC_2604", package_by_id.as_slice()),
            ],
            8192,
        );
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        let state = map_selector_state(&store, Some("sec:nw"), None).expect("map selector state");
        assert_eq!(
            state
                .selected_map
                .as_ref()
                .and_then(|view| view.map_view.full_coverage_zoom),
            Some(7.0)
        );
        assert_eq!(
            state.displayed_maps[0].map_view.full_coverage_zoom,
            Some(7.0)
        );
        assert_eq!(
            state.displayed_maps[0]
                .map_view
                .package_relative_path
                .as_deref(),
            Some("nw_sec_2604_hash.zip")
        );
        assert_eq!(
            state.displayed_maps[0].map_view.tile_url_root,
            "/sectional-packages/NW_SEC_2604/tiles"
        );
    }

    fn fixture(entries: &[(&str, &[u8])], page_size: u32) -> (NavKvRoot, Vec<Vec<u8>>) {
        let (root, pages) = build_root(entries, page_size);
        (NavKvRoot::parse(&root).unwrap(), pages)
    }

    fn build_root(entries: &[(&str, &[u8])], page_size: u32) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut value_bytes = Vec::new();
        let mut leaf = Vec::new();
        leaf.extend_from_slice(&1u32.to_le_bytes());
        leaf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        leaf.extend_from_slice(&u32::MAX.to_le_bytes());
        for (key, value) in entries {
            leaf.extend_from_slice(&(key.len() as u32).to_le_bytes());
            if value.len() > 4096 {
                let offset = value_bytes.len() as u32;
                leaf.extend_from_slice(&0u32.to_le_bytes());
                leaf.extend_from_slice(&offset.to_le_bytes());
                leaf.extend_from_slice(&(value.len() as u32).to_le_bytes());
                value_bytes.extend_from_slice(value);
            } else {
                leaf.extend_from_slice(&1u32.to_le_bytes());
                leaf.extend_from_slice(&(value.len() as u32).to_le_bytes());
                leaf.extend_from_slice(&0u32.to_le_bytes());
            }
            leaf.extend_from_slice(key.as_bytes());
            if value.len() <= 4096 {
                leaf.extend_from_slice(value);
            }
        }
        assert!(leaf.len() <= page_size as usize);
        let mut pages = vec![leaf];
        let value_page_start = pages.len() as u32;
        pages.extend(
            value_bytes
                .chunks(page_size as usize)
                .map(|chunk| chunk.to_vec()),
        );
        let page_count = pages.len() as u32;
        let mut root = vec![0; 64];
        root[..16].copy_from_slice(b"AEROBAGNAVKV0001");
        write_u32(&mut root, 16, 4);
        write_u32(&mut root, 20, entries.len() as u32);
        write_u32(&mut root, 24, page_size);
        write_u32(&mut root, 28, 0);
        write_u32(&mut root, 32, page_count);
        write_u32(&mut root, 36, value_page_start);
        write_u32(&mut root, 40, value_bytes.len() as u32);
        (root, pages)
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn artifact_read_root() -> PathBuf {
        if let Ok(path) = std::env::var("AEROBAG_ARTIFACT_READ_PATH") {
            return PathBuf::from(path);
        }
        let repo_root = repo_root();
        let configured = fs::read_to_string(repo_root.join(".aerobag-artifact-read-path"))
            .expect("read .aerobag-artifact-read-path");
        let configured = PathBuf::from(configured.trim());
        if configured.is_absolute() {
            configured
        } else {
            repo_root.join(configured)
        }
    }

    fn repo_root() -> PathBuf {
        let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        loop {
            if dir.join(".aerobag-artifact-read-path").is_file() {
                return dir;
            }
            if !dir.pop() {
                panic!(
                    "could not find repo root above {}",
                    env!("CARGO_MANIFEST_DIR")
                );
            }
        }
    }

    fn current_nav_db_dir() -> PathBuf {
        let artifact_root = artifact_read_root();
        let current: serde_json::Value = serde_json::from_slice(
            &fs::read(artifact_root.join("current_artifacts.json"))
                .expect("read current_artifacts.json"),
        )
        .expect("decode current_artifacts.json");
        let unpacked_root = current["artifact_roots"]["unpacked"]
            .as_str()
            .expect("current_artifacts artifact_roots.unpacked");
        let cycle_bundle = current["bundles"]
            .as_array()
            .expect("current_artifacts bundles")
            .iter()
            .find(|bundle| bundle["bundle_type"].as_str() == Some("cycle"))
            .expect("cycle bundle in current_artifacts");
        let bundle_relative_path = cycle_bundle["relative_path"]
            .as_str()
            .or_else(|| cycle_bundle["filename"].as_str())
            .expect("cycle bundle relative_path");
        let bundle: serde_json::Value = serde_json::from_slice(
            &fs::read(artifact_root.join(unpacked_root).join(bundle_relative_path))
                .expect("read cycle bundle manifest"),
        )
        .expect("decode cycle bundle manifest");
        let nav_db = bundle["packages"]
            .as_array()
            .expect("cycle bundle packages")
            .iter()
            .find(|package| package["family_id"].as_str() == Some("nav-db"))
            .expect("nav-db package in cycle bundle");
        let relative_path = nav_db["relative_path"]
            .as_str()
            .expect("nav-db relative_path");
        let unpacked_dir = relative_path
            .strip_suffix(".zip")
            .expect("nav-db relative_path is zip");
        artifact_root.join(unpacked_root).join(unpacked_dir)
    }

    fn load_current_nav_kv_store() -> NavKvStore {
        let nav_kv_dir = current_nav_db_dir();
        let root_bytes = fs::read(nav_kv_dir.join("root")).expect("read current nav_db root");
        let root = NavKvRoot::parse(&root_bytes).expect("parse current nav_db root");
        let mut page_paths = fs::read_dir(&nav_kv_dir)
            .expect("read current nav_db dir")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("page_"))
            })
            .collect::<Vec<_>>();
        page_paths.sort();
        assert!(
            !page_paths.is_empty(),
            "current nav_db is missing pages under {}",
            nav_kv_dir.display()
        );
        let mut store = NavKvStore::new(root);
        for (page_index, page_path) in page_paths.into_iter().enumerate() {
            let page_bytes = fs::read(&page_path)
                .unwrap_or_else(|err| panic!("read nav_kv page {}: {err}", page_path.display()));
            store.insert_page(page_index as u32, page_bytes);
        }
        store
    }

    fn load_fixture_nav_kv_store() -> NavKvStore {
        let (root_bytes, pages) = load_fixture_nav_kv_pages();
        let root = NavKvRoot::parse(&root_bytes).expect("parse fixture nav_kv root");
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }
        store
    }

    fn test_nav_kv_store(entries: &[(&str, serde_json::Value)]) -> NavKvStore {
        let encoded = entries
            .iter()
            .map(|(key, value)| {
                (
                    *key,
                    serde_json::to_vec(value).expect("encode test nav_kv value"),
                )
            })
            .collect::<Vec<_>>();
        let entry_refs = encoded
            .iter()
            .map(|(key, value)| (*key, value.as_slice()))
            .collect::<Vec<_>>();
        let (root, pages) = fixture(&entry_refs, 65536);
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }
        store
    }

    #[test]
    fn generated_nav_kv_projects_kpae_vor_a_inserted_plan_ui_state() {
        let store = load_current_nav_kv_store();
        let plan = FlightPlan {
            id: "krnt-sea-pae".to_string(),
            name: "KRNT SEA PAE".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("SEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: Some(crate::GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KPAE".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let built = match run_had_operation(
            &store,
            HadOperation::MaterializeProcedure {
                airport_id: "KPAE".to_string(),
                procedure_id: "VOR-A".to_string(),
                procedure_kind: ProcedureKind::Approach,
                runway_transition: None,
                enroute_transition: Some("ECEPO".to_string()),
                component_index: 2,
            },
        )
        .expect("materialize KPAE VOR-A ECEPO through generated nav_kv")
        {
            HadOperationOutcome::Complete { result } => {
                serde_json::from_value::<MaterializedProcedure>(result)
                    .expect("decode materialized procedure")
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
        };
        let mutation = crate::insert_procedure_materialized_ui(&plan, 1, 2, built)
            .expect("insert KPAE VOR-A ECEPO");
        let route =
            project_flight_plan_route(&store, &mutation.mutation.plan).expect("project route");
        assert!(
            route.iter().any(|segment| matches!(
                segment.geometry,
                crate::GuidanceRouteGeometry::Arc { .. }
            )),
            "procedure route should preserve fine-grained arc geometry"
        );
        assert!(
            route.len() > mutation.mutation.plan.resolved_legs.len(),
            "procedure route should split coarse legs into drawable fine-grained segments"
        );

        let outcome = run_had_operation(
            &store,
            HadOperation::FlightPlanUiState {
                plan: mutation.mutation.plan.clone(),
                current_ui_state: mutation.ui_state.clone(),
            },
        )
        .expect("project flight plan ui state");

        match outcome {
            HadOperationOutcome::Complete { result } => {
                let ui_state: FlightPlanUiState =
                    serde_json::from_value(result).expect("decode ui state");
                assert!(
                    ui_state
                        .components
                        .iter()
                        .any(|component| component.procedure_id.as_deref() == Some("VOR-A")),
                    "expected inserted procedure component in ui state"
                );
                for (label, expected_leg_index) in [
                    ("ECEPO", 1),
                    ("YAVUR", 2),
                    ("ZELIG", 3),
                    ("XUKRE", 4),
                    ("ECEPO", 5),
                ] {
                    let row = ui_state
                        .display_rows
                        .iter()
                        .filter(|row| row.depth == 1 && row.label == label)
                        .find(|row| row.leg_index == Some(expected_leg_index))
                        .unwrap_or_else(|| panic!("expected procedure row for {label}"));
                    assert_eq!(
                        row.leg_index,
                        Some(expected_leg_index),
                        "procedure waypoint row {label} should activate the guidance leg ending there"
                    );
                    assert!(
                        row.actions.iter().any(|action| {
                            action.id == crate::FlightPlanRowActionId::ActivateLeg && action.enabled
                        }),
                        "procedure waypoint row {label} should expose enabled Activate Leg"
                    );
                    assert!(
                        row.actions.iter().all(|action| {
                            action.id != crate::FlightPlanRowActionId::WaypointInfo
                                && action.id != crate::FlightPlanRowActionId::Plates
                        }),
                        "procedure waypoint row {label} should not expose generic waypoint info or plates actions"
                    );
                }
                let hold_row = ui_state
                    .display_rows
                    .iter()
                    .find(|row| row.depth == 1 && row.label == "HOLD")
                    .expect("expected procedure hold row");
                assert_eq!(
                    hold_row.leg_index,
                    Some(5),
                    "hold row should activate the guidance leg carrying the hold geometry"
                );
                assert!(
                    hold_row.actions.iter().any(|action| {
                        action.id == crate::FlightPlanRowActionId::ActivateLeg && action.enabled
                    }),
                    "hold row should expose enabled Activate Leg"
                );

                let activated = crate::activate_leg(&mutation.mutation.plan, 5)
                    .expect("activate XUKRE -> ECEPO guidance leg");
                let activated_ui = crate::project_ui_state(&activated);
                let active_ecepo_row = activated_ui
                    .display_rows
                    .iter()
                    .filter(|row| row.depth == 1 && row.label == "ECEPO")
                    .find(|row| row.leg_index == Some(5))
                    .expect("active ECEPO row");
                assert!(
                    active_ecepo_row.actions.iter().any(|action| {
                        action.id == crate::FlightPlanRowActionId::ActivateLeg && !action.enabled
                    }),
                    "already-active guidance leg row should disable Activate Leg"
                );
                let active_hold_row = activated_ui
                    .display_rows
                    .iter()
                    .find(|row| row.depth == 1 && row.label == "HOLD")
                    .expect("active plan hold row");
                assert!(
                    active_hold_row.actions.iter().any(|action| {
                        action.id == crate::FlightPlanRowActionId::ActivateLeg && action.enabled
                    }),
                    "hold row should still activate the hold detail when the inbound leg is active"
                );
                let airport_row = activated_ui
                    .display_rows
                    .iter()
                    .find(|row| row.depth == 0 && row.label == "KPAE")
                    .expect("destination airport row");
                assert_eq!(
                    airport_row.leg_index,
                    Some(5),
                    "destination airport after a procedure should activate the procedure's last guidance leg"
                );

                let activated_route =
                    project_flight_plan_route(&store, &activated).expect("project active route");
                let active_leg_segments = activated_route
                    .iter()
                    .filter(|segment| segment.leg_id == "procedure-VOR-A-S-70")
                    .collect::<Vec<_>>();
                assert!(
                    active_leg_segments.iter().any(
                        |segment| segment.status == crate::FlightPlanRouteSegmentStatus::Active
                    ),
                    "active guidance leg should have a CDI-active path element"
                );
                assert!(
                    active_leg_segments
                        .iter()
                        .take(4)
                        .filter(|segment| {
                            segment.status == crate::FlightPlanRouteSegmentStatus::Active
                        })
                        .count()
                        == 1,
                    "XUKRE -> ECEPO should have exactly one CDI-active path element"
                );
                assert!(
                    active_leg_segments
                        .iter()
                        .take(4)
                        .filter(|segment| {
                            segment.status
                                == crate::FlightPlanRouteSegmentStatus::ActiveLegRemaining
                        })
                        .count()
                        == 3,
                    "XUKRE -> ECEPO should light the remaining non-hold path elements"
                );
                assert!(
                    active_leg_segments.iter().skip(4).all(|segment| {
                        segment.status == crate::FlightPlanRouteSegmentStatus::Remaining
                    }),
                    "hold path elements should not light up until the hold row is activated"
                );

                let hold_detail = crate::terminal_hold_start_detail_index_for_leg(&activated, 5)
                    .expect("hold detail start");
                let hold_activated =
                    crate::activate_leg_at_detail_index(&activated, 5, hold_detail)
                        .expect("activate hold detail");
                let hold_activated_ui = crate::project_ui_state(&hold_activated);
                let guidance = hold_activated_ui.guidance.as_ref().expect("guidance");
                let active_from_row = hold_activated_ui
                    .display_rows
                    .iter()
                    .find(|row| guidance.active_from_row_uid.as_ref() == Some(&row.uid))
                    .expect("hold active from row");
                let active_to_row = hold_activated_ui
                    .display_rows
                    .iter()
                    .find(|row| guidance.active_to_row_uid.as_ref() == Some(&row.uid))
                    .expect("hold active to row");
                assert_eq!(active_from_row.label, "ECEPO");
                assert_eq!(active_to_row.label, "HOLD");

                let hold_activated_route =
                    project_flight_plan_route(&store, &hold_activated).expect("project hold route");
                let hold_segments = hold_activated_route
                    .iter()
                    .filter(|segment| segment.leg_id == "procedure-VOR-A-S-70")
                    .collect::<Vec<_>>();
                assert!(
                    hold_segments
                        .iter()
                        .take(4)
                        .all(|segment| segment.status
                            == crate::FlightPlanRouteSegmentStatus::Completed),
                    "activating HOLD should mark the inbound elements complete"
                );
                assert!(
                    hold_segments.iter().skip(4).any(|segment| {
                        segment.status == crate::FlightPlanRouteSegmentStatus::Active
                    }),
                    "activating HOLD should light the racetrack"
                );

                let zelig_activated = crate::activate_leg(&mutation.mutation.plan, 4)
                    .expect("activate ZELIG -> XUKRE guidance leg");
                let zelig_route = project_flight_plan_route(&store, &zelig_activated)
                    .expect("project ZELIG route");
                let zelig_segments = zelig_route
                    .iter()
                    .filter(|segment| segment.leg_id == "procedure-VOR-A-S-30")
                    .collect::<Vec<_>>();
                assert_eq!(
                    zelig_segments.len(),
                    1,
                    "ZELIG -> XUKRE should have one path element"
                );
                assert_eq!(
                    zelig_segments[0].status,
                    crate::FlightPlanRouteSegmentStatus::Active,
                    "ZELIG -> XUKRE should highlight only its single path element"
                );
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
        }
    }

    #[test]
    fn flight_plan_ui_state_enrichment_preserves_live_guidance_nav_element() {
        let store = load_current_nav_kv_store();
        let plan = FlightPlan {
            id: "kpao-vpdub-vcb-wlw".to_string(),
            name: "KPAO VPDUB KVCB KWLW".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAO".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("VPDUB".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KVCB".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KWLW".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: Some(crate::GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KPAO".to_string())),
            destination: Some(AirportId("KWLW".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let mut current_ui_state =
            crate::project_ui_state(&crate::build_flight_plan(plan.clone()).expect("build plan"));
        current_ui_state
            .guidance
            .as_mut()
            .expect("guidance")
            .nav_element = NavElementUiView {
            active_leg_summary: "LIVE".to_string(),
            cdi_indicator_dots: Some(2.5),
            cdi_offscale_readout: Some("R".to_string()),
        };

        let outcome = run_had_operation(
            &store,
            HadOperation::FlightPlanUiState {
                plan,
                current_ui_state,
            },
        )
        .expect("project enriched flight plan ui state");

        match outcome {
            HadOperationOutcome::Complete { result } => {
                let ui_state: FlightPlanUiState =
                    serde_json::from_value(result).expect("decode ui state");
                let nav_element = &ui_state.guidance.as_ref().expect("guidance").nav_element;
                assert_eq!(nav_element.active_leg_summary, "LIVE");
                assert_eq!(nav_element.cdi_indicator_dots, Some(2.5));
                assert_eq!(nav_element.cdi_offscale_readout.as_deref(), Some("R"));
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
        }
    }

    #[test]
    fn generated_nav_kv_session_select_chart_keeps_kpae_vor_a() {
        let store = load_current_nav_kv_store();
        let kpae = match run_had_operation(
            &store,
            HadOperation::PlateAirport {
                airport_id: "KPAE".to_string(),
            },
        )
        .expect("load generated KPAE plate airport")
        {
            HadOperationOutcome::Complete { result } => {
                serde_json::from_value::<crate::DerivedChartAirport>(result)
                    .expect("decode generated KPAE plate airport")
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!(
                    "expected complete KPAE plate airport, got missing resources: {resources:?}"
                );
            }
        };
        let vor_a = kpae
            .charts
            .iter()
            .find(|chart| chart.label == "VOR-A")
            .unwrap_or_else(|| {
                panic!(
                    "expected KPAE VOR-A chart in generated catalog: {:?}",
                    kpae.charts
                )
            })
            .clone();
        assert!(
            vor_a.asset_path.to_uppercase().contains("VOR-A"),
            "expected VOR-A asset path, got {:?}",
            vor_a
        );
        let _chart_catalog = crate::DerivedChartCatalog {
            airports: vec![kpae],
        };
        let plan = FlightPlan {
            id: "krnt-sea-pae".to_string(),
            name: "KRNT SEA KPAE".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("SEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: Some(crate::GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KPAE".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let init = crate::create_ui_session(
            r#"{"airspace":{"reference_tile_min_zoom":0,"reference_tile_max_zoom":12,"label_tile_min_zoom":0,"label_tile_max_zoom":12}}"#,
            plan,
            &["KPAE".to_string()],
            Some("KPAE"),
            None,
        )
        .expect("create ui session");

        let snapshot = crate::select_chart_in_session(init.handle, &vor_a.id)
            .expect("select KPAE VOR-A chart in session");

        assert_eq!(snapshot.chart_page_state.selected_airport_id, "KPAE");
        assert_eq!(snapshot.chart_page_state.selected_chart_id, vor_a.id);
    }

    #[test]
    fn generated_nav_kv_chart_catalog_lists_multiple_regions() {
        let store = load_current_nav_kv_store();
        let catalog = chart_catalog(&store).expect("load generated chart catalog");

        let regions = catalog
            .iter()
            .map(|view| view.region_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let ids = catalog
            .iter()
            .map(|view| view.id.as_str())
            .collect::<Vec<_>>();

        println!("generated chart catalog regions: {regions:?}");
        println!("generated chart catalog ids: {ids:?}");

        assert!(
            regions.len() > 1,
            "expected multi-region chart catalog, got {regions:?}"
        );
    }

    #[test]
    fn generated_nav_kv_default_map_selector_state_displays_all_tac_and_sec_regions() {
        let store = load_current_nav_kv_store();
        let state =
            map_selector_state(&store, None, None).expect("load generated map selector state");

        let displayed_ids = state
            .displayed_maps
            .iter()
            .map(|view| view.id.as_str())
            .collect::<Vec<_>>();
        let displayed_regions = state
            .displayed_maps
            .iter()
            .map(|view| view.region_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();

        println!("default selected map id: {}", state.selected_map_id);
        println!("default displayed map ids: {displayed_ids:?}");
        println!("default displayed regions: {displayed_regions:?}");

        assert_eq!(state.selected_map_id, "tac:nw");
        assert!(displayed_ids.iter().any(|id| *id == "sec:sc"));
        assert!(displayed_ids.iter().any(|id| *id == "tac:sc"));
        let sec_sc = state
            .displayed_maps
            .iter()
            .find(|view| view.id == "sec:sc")
            .expect("missing sec:sc map");
        assert!(!sec_sc.map_view.tile_url_root.starts_with("/packages/"));
        assert!(sec_sc
            .map_view
            .package_relative_path
            .as_deref()
            .is_some_and(|path| path.starts_with("sec_sc_2603_") && path.ends_with(".zip")));
        let tac_sc = state
            .displayed_maps
            .iter()
            .find(|view| view.id == "tac:sc")
            .expect("missing tac:sc map");
        assert!(!tac_sc.map_view.tile_url_root.starts_with("/packages/"));
        assert!(state.geometry.polygon_sets.is_empty());
        assert!(
            displayed_regions.len() > 1,
            "expected multi-region displayed maps, got {displayed_regions:?}"
        );
    }

    #[test]
    fn generated_nav_kv_south_central_maps_cover_kmsy_tile() {
        let store = load_current_nav_kv_store();
        let state =
            map_selector_state(&store, None, None).expect("load generated map selector state");

        fn lat_lon_to_tile_tms(lat: f64, lon: f64, zoom: f64) -> (i64, i64) {
            let world_size = 256.0f64;
            let max_latitude = 85.05112878f64;
            let clamped_lat = lat.max(-max_latitude).min(max_latitude);
            let world_x = ((lon + 180.0) / 360.0) * world_size;
            let world_y = ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI)
                / 2.0)
                * world_size;
            let scale = 2.0f64.powf(zoom);
            let x_xyz = (world_x * scale / world_size).floor() as i64;
            let y_xyz = (world_y * scale / world_size).floor() as i64;
            let y_tms = ((2.0f64.powf(zoom) as i64) - 1) - y_xyz;
            (x_xyz, y_tms)
        }

        let kmsy_lat = 29.993389f64;
        let kmsy_lon = -90.258028f64;
        for map_id in ["sec:sc", "tac:sc"] {
            let view = state
                .displayed_maps
                .iter()
                .find(|view| view.id == map_id)
                .unwrap_or_else(|| panic!("missing displayed map {map_id}"));
            let level = view
                .map_view
                .levels
                .iter()
                .find(|level| level.zoom == 10)
                .unwrap_or_else(|| {
                    panic!(
                        "missing zoom-10 level for {map_id}: {:?}",
                        view.map_view.levels
                    )
                });
            let (x, y_tms) = lat_lon_to_tile_tms(kmsy_lat, kmsy_lon, level.zoom as f64);
            println!(
                "{map_id} zoom {} tile x={} y_tms={} boxes {:?}",
                level.zoom, x, y_tms, level.boxes
            );
            assert!(
                level.boxes.iter().any(|bbox| {
                    x >= bbox.x_min
                        && x <= bbox.x_max
                        && y_tms >= bbox.y_tms_min
                        && y_tms <= bbox.y_tms_max
                }),
                "{map_id} does not cover KMSY at zoom {}",
                level.zoom
            );
        }
    }

    #[test]
    fn generated_nav_kv_materialize_procedure_kpae_vor_a_from_ecepo_succeeds() {
        let store = load_current_nav_kv_store();

        let outcome = run_had_operation(
            &store,
            HadOperation::MaterializeProcedure {
                airport_id: "KPAE".to_string(),
                procedure_id: "VOR-A".to_string(),
                procedure_kind: ProcedureKind::Approach,
                runway_transition: None,
                enroute_transition: Some("ECEPO".to_string()),
                component_index: 2,
            },
        )
        .expect("materialize KPAE VOR-A ECEPO through generated nav_kv");

        match outcome {
            HadOperationOutcome::Complete { result } => {
                let materialized = serde_json::from_value::<MaterializedProcedure>(result)
                    .expect("decode materialized procedure");
                assert_eq!(materialized.procedure.procedure_id, "VOR-A");
                assert_eq!(
                    materialized.procedure.enroute_transition.as_deref(),
                    Some("ECEPO")
                );
                assert!(!materialized.resolved_legs.is_empty());
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
        }
    }

    #[test]
    fn fixture_nav_kv_resolves_waypoint_identifier() {
        let store = load_fixture_nav_kv_store();
        let outcome = run_had_operation(
            &store,
            HadOperation::ResolveWaypointIdentifier {
                identifier: "OLM".to_string(),
            },
        )
        .expect("resolve OLM through fixture nav_kv");
        match outcome {
            HadOperationOutcome::Complete { result } => {
                let nav_ref =
                    serde_json::from_value::<Option<NavRef>>(result).expect("decode nav ref");
                assert_eq!(nav_ref, Some(NavRef::Navaid("OLM".to_string())));
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
        }
    }

    #[test]
    fn fixture_nav_kv_suggests_waypoint_identifiers_near_view_center() {
        let store = load_fixture_nav_kv_store();
        let outcome = run_had_operation(
            &store,
            HadOperation::SuggestWaypointIdentifiersNear {
                anchor: LatLon {
                    lat: 47.493,
                    lon: -122.216,
                },
                prefix: "KR".to_string(),
                limit: 5,
            },
        )
        .expect("suggest waypoint identifiers near view center");
        match outcome {
            HadOperationOutcome::Complete { result } => {
                let suggestions =
                    serde_json::from_value::<Vec<WaypointIdentifierSuggestion>>(result)
                        .expect("decode waypoint suggestions");
                assert!(!suggestions.is_empty());
                assert!(suggestions.len() <= 5);
                assert!(suggestions
                    .iter()
                    .all(|suggestion| suggestion.identifier.starts_with("KR")));
                assert!(suggestions.windows(2).all(|pair| {
                    pair[0].distance_from_anchor_nm <= pair[1].distance_from_anchor_nm
                }));
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
        }
    }

    #[test]
    fn missing_waypoint_prefix_suggestions_are_empty() {
        let store = test_nav_kv_store(&[]);
        let suggestions =
            suggest_waypoint_identifier_candidates(&store, "K", 5, LatLon { lat: 0.0, lon: 0.0 })
                .expect("missing prefix should not be fatal");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn waypoint_prefix_suggestions_fall_back_to_shortest_emitted_bucket() {
        let kr_bucket = vec![
            WaypointIdentifierRecord {
                identifier: "KRNT".to_string(),
                nav_ref: NavRef::Airport("KRNT".to_string()),
                kind: "airport".to_string(),
                city: "Renton".to_string(),
                state: "WA".to_string(),
                facility_name: "Renton Municipal".to_string(),
                position: LatLon {
                    lat: 47.493,
                    lon: -122.216,
                },
            },
            WaypointIdentifierRecord {
                identifier: "KRDD".to_string(),
                nav_ref: NavRef::Airport("KRDD".to_string()),
                kind: "airport".to_string(),
                city: "Redding".to_string(),
                state: "CA".to_string(),
                facility_name: "Redding Regional".to_string(),
                position: LatLon {
                    lat: 40.509,
                    lon: -122.293,
                },
            },
        ];
        let store = test_nav_kv_store(&[(
            "waypoint/prefix/KR",
            serde_json::to_value(kr_bucket).expect("encode waypoint bucket"),
        )]);
        let suggestions = suggest_waypoint_identifier_candidates(
            &store,
            "KRNT",
            5,
            LatLon {
                lat: 47.493,
                lon: -122.216,
            },
        )
        .expect("suggest from ancestor prefix bucket");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].identifier, "KRNT");
    }

    #[test]
    fn fixture_nav_kv_suggests_airways_near_krnt() {
        let store = load_fixture_nav_kv_store();
        let outcome = run_had_operation(
            &store,
            HadOperation::SuggestAirwaysNearAnchor {
                anchor: NavRef::Airport("KRNT".to_string()),
                limit: 5,
            },
        )
        .expect("suggest nearby airways through fixture nav_kv");
        match outcome {
            HadOperationOutcome::Complete { result } => {
                let suggestions = serde_json::from_value::<Vec<AirwaySuggestion>>(result)
                    .expect("decode airway suggestions");
                assert!(!suggestions.is_empty());
                assert!(suggestions.windows(2).all(|pair| {
                    pair[0].distance_from_anchor_nm <= pair[1].distance_from_anchor_nm
                }));
                assert!(suggestions.iter().all(|suggestion| {
                    suggestion.distance_from_anchor_nm.is_finite()
                        && !suggestion.airway_name.trim().is_empty()
                }));
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
        }
    }

    #[test]
    fn fixture_nav_kv_prepares_and_materializes_v2_between_krnt_and_kuao() {
        let store = load_fixture_nav_kv_store();
        let presentation = match run_had_operation(
            &store,
            HadOperation::PrepareAirwayPresentationForAnchors {
                airway_name: "V2".to_string(),
                origin_anchor: NavRef::Airport("KRNT".to_string()),
                destination_anchor: Some(NavRef::Airport("KUAO".to_string())),
            },
        )
        .expect("prepare airway presentation through fixture nav_kv")
        {
            HadOperationOutcome::Complete { result } => {
                serde_json::from_value::<AirwayPresentationPlan>(result)
                    .expect("decode airway presentation")
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
        };

        let entry_index = presentation
            .points
            .iter()
            .position(|point| point.nav_ref == NavRef::Navaid("SEA".to_string()))
            .expect("presentation should include SEA");
        let exit_index = presentation
            .points
            .iter()
            .position(|point| point.nav_ref == NavRef::Fix("VAMPS".to_string()))
            .expect("presentation should include VAMPS");

        let materialized = match run_had_operation(
            &store,
            HadOperation::MaterializeAirwayPresentationSelection {
                start_component_index: 0,
                presentation,
                entry_index,
                exit_index,
                origin_anchor: NavRef::Airport("KRNT".to_string()),
                destination_anchor: Some(NavRef::Airport("KUAO".to_string())),
            },
        )
        .expect("materialize airway presentation selection through fixture nav_kv")
        {
            HadOperationOutcome::Complete { result } => {
                serde_json::from_value::<MaterializedAirwayResponse>(result)
                    .expect("decode materialized airway response")
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
        };

        assert_eq!(materialized.airway.name, "V2");
        assert_eq!(
            materialized.selection.entry.nav_ref,
            NavRef::Navaid("SEA".to_string())
        );
        assert_eq!(
            materialized.selection.exit.nav_ref,
            NavRef::Fix("VAMPS".to_string())
        );
        assert_eq!(
            materialized.airway.entry,
            materialized.selection.entry.nav_ref
        );
        assert_eq!(
            materialized.airway.exit,
            materialized.selection.exit.nav_ref
        );
        assert!(!materialized.resolved_legs.is_empty());
        assert_eq!(
            materialized.resolved_legs.first().unwrap().from,
            materialized.airway.entry
        );
        assert_eq!(
            materialized.resolved_legs.last().unwrap().to,
            materialized.airway.exit
        );
    }

    #[test]
    fn fixture_nav_kv_previews_append_entry_with_relationship_issue() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "krnt".to_string(),
            name: "KRNT".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KRNT".to_string()),
            }],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KRNT".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let preview = match run_had_operation(
            &store,
            HadOperation::PreviewFlightPlanEntry {
                plan,
                input: "SEA V112 VAMPS".to_string(),
            },
        )
        .expect("preview route entry")
        {
            HadOperationOutcome::Complete { result } => {
                serde_json::from_value::<FlightPlanEntryPreview>(result).expect("decode preview")
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete preview, got missing resources: {resources:?}");
            }
        };

        assert!(!preview.can_commit);
        assert_eq!(preview.tokens.len(), 3);
        assert_eq!(
            preview.tokens[0].state,
            FlightPlanEntryTokenState::Recognized
        );
        assert_eq!(
            preview.tokens[1].state,
            FlightPlanEntryTokenState::Recognized
        );
        assert_eq!(
            preview.tokens[2].state,
            FlightPlanEntryTokenState::Recognized
        );
        assert_eq!(preview.issues.len(), 1);
        assert!(preview.issues[0].message.contains("V112"));
        assert_eq!(preview.issues[0].start, 4);
        assert_eq!(preview.issues[0].end, 14);
    }

    #[test]
    fn fixture_nav_kv_appends_waypoint_airway_waypoint_sequence() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "krnt".to_string(),
            name: "KRNT".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KRNT".to_string()),
            }],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KRNT".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation = match run_had_operation(
            &store,
            HadOperation::AppendFlightPlanEntry {
                plan,
                input: "SEA V2 VAMPS KUAO".to_string(),
            },
        )
        .expect("append route entry")
        {
            HadOperationOutcome::Complete { result } => {
                serde_json::from_value::<FlightPlanUiMutation>(result)
                    .expect("decode append mutation")
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete append, got missing resources: {resources:?}");
            }
        };

        assert_eq!(mutation.plan.route_components.len(), 5);
        assert!(matches!(
            mutation.plan.route_components[0],
            RouteComponent::Waypoint { .. }
        ));
        assert!(matches!(
            mutation.plan.route_components[1],
            RouteComponent::Waypoint { waypoint: NavRef::Navaid(ref id) } if id == "SEA"
        ));
        assert!(matches!(
            mutation.plan.route_components[2],
            RouteComponent::Airway { ref airway } if airway.name == "V2"
        ));
        assert!(matches!(
            mutation.plan.route_components[3],
            RouteComponent::Waypoint { waypoint: NavRef::Fix(ref id) } if id == "VAMPS"
        ));
        assert!(matches!(
            mutation.plan.route_components[4],
            RouteComponent::Waypoint { waypoint: NavRef::Airport(ref id) } if id == "KUAO"
        ));
    }

    #[test]
    fn exact_airway_materialization_reports_missing_entry_waypoint() {
        let branches = vec![AirwayBranch {
            display_name: "V495".to_string(),
            branch_key: String::new(),
            points: vec![
                crate::AirwayFixPoint {
                    airway_name: "V495".to_string(),
                    sequence: 10,
                    position: LatLon { lat: 0.0, lon: 0.0 },
                    nav_ref: NavRef::Navaid("SEA".to_string()),
                },
                crate::AirwayFixPoint {
                    airway_name: "V495".to_string(),
                    sequence: 20,
                    position: LatLon { lat: 1.0, lon: 1.0 },
                    nav_ref: NavRef::Fix("NEXT".to_string()),
                },
            ],
        }];

        let err = exact_airway_materialization(
            "V495",
            &branches,
            &NavRef::Navaid("PAE".to_string()),
            &NavRef::Navaid("SEA".to_string()),
            0,
        )
        .expect_err("PAE is not on V495");

        assert!(matches!(err, HadReadError::Fatal(message) if message == "PAE not on V495"));
    }

    #[test]
    fn route_entry_validation_reports_missing_airway_entry_before_missing_exit() {
        let tokens = vec![
            EvaluatedInputToken {
                parsed: ParsedInputToken {
                    text: "PAE".to_string(),
                    start: 5,
                    end: 8,
                    terminated: true,
                },
                token_state: FlightPlanEntryTokenState::Recognized,
                recognized: Some(RecognizedInputToken::Waypoint(NavRef::Navaid(
                    "PAE".to_string(),
                ))),
            },
            EvaluatedInputToken {
                parsed: ParsedInputToken {
                    text: "V495".to_string(),
                    start: 9,
                    end: 13,
                    terminated: true,
                },
                token_state: FlightPlanEntryTokenState::Recognized,
                recognized: Some(RecognizedInputToken::Airway {
                    airway_name: "V495".to_string(),
                    branches: vec![AirwayBranch {
                        display_name: "V495".to_string(),
                        branch_key: String::new(),
                        points: vec![crate::AirwayFixPoint {
                            airway_name: "V495".to_string(),
                            sequence: 10,
                            position: LatLon { lat: 0.0, lon: 0.0 },
                            nav_ref: NavRef::Navaid("SEA".to_string()),
                        }],
                    }],
                }),
            },
        ];

        let plan = FlightPlan {
            id: "route".to_string(),
            name: "Route".to_string(),
            legs: Vec::new(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let issues = validate_flight_plan_entry(&plan, &tokens).expect("validate route");

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].start, 5);
        assert_eq!(issues[0].end, 13);
        assert_eq!(issues[0].message, "PAE not on V495");
    }

    #[test]
    fn route_entry_appends_chained_airways_with_shared_waypoint() {
        let store = load_current_nav_kv_store();
        let plan = FlightPlan {
            id: "route".to_string(),
            name: "Route".to_string(),
            legs: Vec::new(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let input = "KPAE PAE V23 SEA V495 VAUGN EUG";
        let preview = preview_flight_plan_entry(&store, &plan, input).expect("preview");
        assert!(preview.can_commit, "{preview:#?}");
        let mutation = append_flight_plan_entry(&store, &plan, input).expect("append route");

        assert!(mutation.plan.route_components.iter().any(|component| {
            matches!(component, RouteComponent::Waypoint { waypoint: NavRef::Navaid(id) } if id == "SEA")
        }));
        assert!(mutation.plan.route_components.iter().any(|component| {
            matches!(component, RouteComponent::Airway { airway } if airway.name == "V23")
        }));
        assert!(mutation.plan.route_components.iter().any(|component| {
            matches!(component, RouteComponent::Airway { airway } if airway.name == "V495")
        }));
    }

    #[test]
    fn route_entry_materializes_airway_exit_before_final_airport() {
        let store = load_current_nav_kv_store();
        let plan = FlightPlan {
            id: "route".to_string(),
            name: "Route".to_string(),
            legs: Vec::new(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let input = "KPAE PAE V23 SEA V495 VAUGN KEUG";
        let preview = preview_flight_plan_entry(&store, &plan, input).expect("preview");
        assert!(preview.can_commit, "{preview:#?}");
        let mutation = append_flight_plan_entry(&store, &plan, input).expect("append route");

        let route = &mutation.plan.route_components;
        assert!(route.windows(2).any(|window| {
            matches!(
                (&window[0], &window[1]),
                (
                    RouteComponent::Waypoint {
                        waypoint: NavRef::Fix(exit)
                    },
                    RouteComponent::Waypoint {
                        waypoint: NavRef::Airport(airport)
                    },
                ) if exit == "VAUGN" && airport == "KEUG"
            )
        }));

        let keug_row = mutation
            .ui_state
            .display_rows
            .iter()
            .find(|row| row.depth == 0 && row.label == "KEUG")
            .expect("KEUG row");
        assert!(
            keug_row.symbol_feature.is_some(),
            "airport row should keep its symbol"
        );
        assert!(
            keug_row
                .actions
                .iter()
                .any(|action| action.id == FlightPlanRowActionId::SelectProcedure && action.enabled),
            "final airport after airway exit should offer Select Procedure"
        );
    }

    #[test]
    fn append_flight_plan_entry_allows_empty_plan_to_gain_single_waypoint() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "empty".to_string(),
            name: "Empty".to_string(),
            legs: Vec::new(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation = match run_had_operation(
            &store,
            HadOperation::AppendFlightPlanEntry {
                plan,
                input: "KPAE".to_string(),
            },
        )
        .expect("append route entry to empty plan")
        {
            HadOperationOutcome::Complete { result } => {
                serde_json::from_value::<FlightPlanUiMutation>(result)
                    .expect("decode append mutation")
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete append, got missing resources: {resources:?}");
            }
        };

        assert_eq!(mutation.plan.route_components.len(), 1);
        assert!(mutation.plan.resolved_legs.is_empty());
        assert!(matches!(
            mutation.plan.route_components[0],
            RouteComponent::Waypoint { waypoint: NavRef::Airport(ref id) } if id == "KPAE"
        ));
    }

    #[test]
    fn waypoint_identifier_ui_rule_rejects_short_airport_aliases() {
        let store = test_nav_kv_store(&[(
            "navref/symbol/navaid/SEA",
            serde_json::json!({
                "kind": "vortac",
                "label": "SEA 116.80",
                "style_class": "nav",
                "towered": false,
                "fuel_available": false,
                "runway_length_ratio": 0.0,
                "longest_runway_heading_true_deg": null
            }),
        )]);
        let short_alias = WaypointIdentifierRecord {
            identifier: "RNT".to_string(),
            nav_ref: NavRef::Airport("KRNT".to_string()),
            kind: "airport".to_string(),
            city: "Renton".to_string(),
            state: "WA".to_string(),
            facility_name: "Renton Municipal".to_string(),
            position: LatLon {
                lat: 47.493,
                lon: -122.216,
            },
        };
        let canonical = WaypointIdentifierRecord {
            identifier: "KRNT".to_string(),
            ..short_alias.clone()
        };
        let navaid = WaypointIdentifierRecord {
            identifier: "SEA".to_string(),
            nav_ref: NavRef::Navaid("SEA".to_string()),
            ..short_alias.clone()
        };
        let ndb = WaypointIdentifierRecord {
            identifier: "RNT".to_string(),
            nav_ref: NavRef::Navaid("RNT".to_string()),
            ..short_alias.clone()
        };

        assert_eq!(
            canonical_waypoint_identifier_record_for_ui(&store, short_alias)
                .expect("canonicalize short airport alias")
                .expect("short airport alias canonicalizes for suggestions")
                .identifier,
            "KRNT"
        );
        assert_eq!(
            canonical_waypoint_identifier_record_for_ui(&store, canonical)
                .expect("canonicalize airport")
                .expect("canonical airport")
                .identifier,
            "KRNT"
        );
        assert_eq!(
            canonical_waypoint_identifier_record_for_ui(&store, navaid)
                .expect("canonicalize VOR-family navaid")
                .expect("navaid identifier")
                .identifier,
            "SEA"
        );
        assert!(canonical_waypoint_identifier_record_for_ui(&store, ndb)
            .expect("canonicalize NDB")
            .is_none());
        assert!(!waypoint_identifier_is_canonical_for_ui(
            "RNT",
            &NavRef::Airport("KRNT".to_string())
        ));
        assert!(waypoint_identifier_is_canonical_for_ui(
            "SEA",
            &NavRef::Navaid("SEA".to_string())
        ));
    }

    #[test]
    fn insert_waypoint_best_position_uses_minimum_added_route_length() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "west-coast".to_string(),
            name: "West Coast".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KPAE".to_string())),
            destination: Some(AirportId("KUAO".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation =
            insert_waypoint_best_position(&store, &plan, NavRef::Navaid("SEA".to_string()))
                .expect("insert waypoint");

        assert_eq!(mutation.plan.route_components.len(), 3);
        assert!(matches!(
            mutation.plan.route_components[1],
            RouteComponent::Waypoint { waypoint: NavRef::Navaid(ref id) } if id == "SEA"
        ));
    }

    #[test]
    fn insert_waypoint_best_position_accepts_spot_waypoint() {
        let store = load_fixture_nav_kv_store();
        let spot = NavRef::Spot(LatLon {
            lat: 47.5,
            lon: -122.2,
        });
        let plan = FlightPlan {
            id: "spot".to_string(),
            name: "Spot".to_string(),
            legs: Vec::new(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation =
            insert_waypoint_best_position(&store, &plan, spot.clone()).expect("insert spot");

        assert_eq!(
            mutation.plan.route_components,
            vec![RouteComponent::Waypoint { waypoint: spot }]
        );
    }

    #[test]
    fn insert_waypoint_best_position_rejects_existing_waypoint() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "duplicate".to_string(),
            name: "Duplicate".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Navaid("SEA".to_string()),
            }],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let err = insert_waypoint_best_position(&store, &plan, NavRef::Navaid("SEA".to_string()))
            .expect_err("duplicate insert should fail");

        assert!(
            matches!(err, HadReadError::Fatal(message) if message.contains("already in the flight plan"))
        );
    }
}
