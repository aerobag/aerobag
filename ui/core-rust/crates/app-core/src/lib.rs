use serde::{Deserialize, Serialize};

pub mod catalog;
pub mod chart_page;
pub mod content;
pub mod errors;
pub mod geometry;
pub mod ids;
#[cfg(not(target_arch = "wasm32"))]
pub mod navdb;
pub mod navdb_types;
pub mod map_overlay;
pub mod planning;
pub mod situation;
pub mod session;
pub mod state;

pub use catalog::{
    CatalogBundle, CatalogFamily, CatalogHandle, CatalogPackage, CatalogRegion,
    ChartRecord, PlateRecord, SupplementRecord,
};
pub use chart_page::{
    build_chart_catalog, derive_chart_page, derive_chart_page_from_catalog,
    derive_chart_page_state, derive_chart_page_state_from_catalog, DerivedChartAirport,
    DerivedChartAsset, DerivedChartCatalog, DerivedChartPage, DerivedChartPageState,
    ResourceAirportResources, ResourceCsup, ResourceIndexChartPageInput, ResourcePlate,
};
pub use content::{
    AvailabilityDetail, CachedPlate, CachedTileset, ContentAvailability, ContentInventory,
    ContentPolicy, ContentReport, ContentReportItem, ContentRequirement, InstalledPackage,
};
pub use errors::{AppError, AppErrorKind, AppResult};
pub use geometry::{GeoBounds, GeometryBundle, LatLon, MapViewport, PolygonRecord};
pub use ids::{AirportId, ChartFamilyId, ChartId, PackageId, PlateId, RegionId};
pub use map_overlay::{
    query_map_overlay, tile_key, visible_point_tile_window, MapOverlayQueryResult,
    MapOverlayWarning, PointTilePayload, PointVectorRecord, VectorTileRequest,
    VisibleMapFeature, VECTOR_DISPLAY_FEATURE_LIMIT,
};
pub use navdb_types::{
    AirwayAutoSelection, AirwayBranch, AirwayEntryCandidate, AirwayExitCandidate,
    AirwayExitSelection, AirwayFixPoint, AirwayPoint, AirwaySuggestion, MaterializedProcedure,
    ProcedureDistinctRow, ProcedureLegMaterializationRecord, ProcedureLegRecord,
    ProcedureOptions, ProcedureSpecChoice, ProcedureSummary, ProcedureVariantKey,
    AirwayPresentationPlan, AirwayPresentationPoint,
};
#[cfg(not(target_arch = "wasm32"))]
pub use navdb::{
    choose_best_airway_plan, describe_procedure_options, list_airway_entry_candidates,
    list_airway_exit_candidates, list_procedures, load_airway_branches, load_airway_points,
    load_procedure_concretized_items, load_procedure_legs, load_resolved_procedure_legs,
    materialize_airway_selection, materialize_procedure_selection, resolve_airway_segment,
    resolve_airway_segment_by_index, resolve_nav_ref_position, select_airway_branch,
    suggest_airways_near,
};
pub use planning::{
    activate_direct_to, activate_direct_to_leg, activate_leg, activate_next_leg,
    active_guidance_leg, change_airway_entry, change_airway_exit,
    change_procedure_enroute_transition, change_procedure_runway_transition, delete_component,
    delete_waypoint_component,
    flatten_component_to_waypoints, insert_airway_between_waypoints,
    insert_airway_after_waypoint,
    insert_procedure_between_waypoints, interpret_path_termination, project_ui_state,
    move_component, replace_airway_component, replace_procedure_component, sequence_active_leg, suspend_sequencing,
    unsuspend_sequencing, AirwaySegment, ConcretizedNavItem, DirectToState, FlightPlan,
    FlightPlanUiState, GuidanceState, GuidanceUiView, NavRef, PathTermination, PlanLeg,
    ProcedureDiscontinuity, ProcedureKind, ProcedureLegProvenance, ProcedureSegment,
    ProcedureSegmentRole, ResolvedLeg, ResolvedLegSource, ResolvedLegUiView, RouteComponent,
    RouteComponentUiView, RouteComponentViewKind, SequencingMode, DirectToUiView,
};
pub use situation::{Situation, SituationPosition};
pub use session::{
    create_ui_session, destroy_session, get_map_overlay_in_session, get_session_snapshot,
    ingest_point_tiles_in_session, move_waypoint_in_session, remove_leg_in_session,
    replace_flight_plan_in_session,
    restore_chart_page_state_in_session, select_airport_in_session, select_chart_in_session,
    set_situation_in_session,
    UiChartPageState, UiSessionInitResult, UiSessionSnapshot,
};
pub use state::{project_app_ui_state, AppEvent, AppState, AppUiState};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayPlanMutation {
    pub plan: FlightPlan,
    pub component_index: usize,
    pub selection: AirwayAutoSelection,
    pub airway: AirwaySegment,
    pub resolved_legs: Vec<ResolvedLeg>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedurePlanMutation {
    pub plan: FlightPlan,
    pub component_index: usize,
    pub procedure: ProcedureSegment,
    pub concretized_items: Vec<ConcretizedNavItem>,
    pub resolved_legs: Vec<ResolvedLeg>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlanUiMutation {
    pub plan: FlightPlan,
    pub ui_state: FlightPlanUiState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayPlanUiMutation {
    pub mutation: AirwayPlanMutation,
    pub ui_state: FlightPlanUiState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedurePlanUiMutation {
    pub mutation: ProcedurePlanMutation,
    pub ui_state: FlightPlanUiState,
}

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex, OnceLock},
};

pub fn load_catalog(catalog_json: &str) -> AppResult<CatalogHandle> {
    let bundle: CatalogBundle = serde_json::from_str(catalog_json).map_err(|err| AppError {
        kind: AppErrorKind::InvalidCatalog,
        message: format!("failed to parse catalog json: {err}"),
    })?;
    Ok(CatalogHandle { bundle })
}

pub fn load_geometry(geometry_json: &str) -> AppResult<GeometryBundle> {
    serde_json::from_str(geometry_json).map_err(|err| AppError {
        kind: AppErrorKind::InvalidCatalog,
        message: format!("failed to parse geometry json: {err}"),
    })
}

pub fn load_resource_index_chart_page_input(
    resource_index_json: &str,
) -> AppResult<Arc<ResourceIndexChartPageInput>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Arc<ResourceIndexChartPageInput>>>> = OnceLock::new();

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    resource_index_json.hash(&mut hasher);
    let key = hasher.finish();

    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .expect("resource index cache poisoned")
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }

    let parsed: ResourceIndexChartPageInput =
        serde_json::from_str(resource_index_json).map_err(|err| AppError {
            kind: AppErrorKind::InvalidCatalog,
            message: format!("failed to parse resource index json: {err}"),
        })?;
    let parsed = Arc::new(parsed);
    cache
        .lock()
        .expect("resource index cache poisoned")
        .insert(key, parsed.clone());
    Ok(parsed)
}

pub fn build_flight_plan(plan: FlightPlan) -> AppResult<FlightPlan> {
    if plan.route_components.is_empty() && plan.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain structured route data".to_string(),
        });
    }

    let plan = plan.normalized();

    if plan.resolved_legs.is_empty() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "flight plan must contain at least one flyable leg".to_string(),
        });
    }

    Ok(plan)
}

pub fn prepare_airway_presentation(
    airway_name: &str,
    branches: Vec<AirwayBranch>,
    origin_position: LatLon,
    destination_position: Option<LatLon>,
) -> AppResult<AirwayPresentationPlan> {
    let mut best: Option<(f64, AirwayPresentationPlan)> = None;

    for branch in branches
        .into_iter()
        .filter(|branch| branch.display_name.trim() == airway_name.trim())
    {
        if branch.points.is_empty() {
            continue;
        }
        let mut entry_index = 0usize;
        let mut exit_index = branch.points.len().saturating_sub(1);
        let mut entry_distance = f64::MAX;
        let mut exit_distance = 0.0;

        for (index, point) in branch.points.iter().enumerate() {
            let origin_distance = distance_nm(origin_position, point.position);
            if origin_distance < entry_distance {
                entry_distance = origin_distance;
                entry_index = index;
            }
            if let Some(destination_position) = destination_position {
                let destination_distance = distance_nm(destination_position, point.position);
                if index == 0 || destination_distance < exit_distance {
                    exit_distance = destination_distance;
                    exit_index = index;
                }
            }
        }

        let mut points = branch
            .points
            .iter()
            .enumerate()
            .map(|(branch_point_index, point)| AirwayPresentationPoint {
                branch_point_index,
                sequence: point.sequence,
                nav_ref: point.nav_ref.clone(),
            })
            .collect::<Vec<_>>();

        let mut suggested_entry_index = entry_index;
        let mut suggested_exit_index = destination_position.map(|_| exit_index);
        if destination_position.is_some() && entry_index > exit_index {
            points.reverse();
            suggested_entry_index = points.len() - 1 - entry_index;
            suggested_exit_index = Some(points.len() - 1 - exit_index);
        }

        let score = entry_distance + exit_distance;
        let presentation = AirwayPresentationPlan {
            airway_name: branch.display_name,
            branch_key: branch.branch_key,
            points,
            suggested_entry_index,
            suggested_exit_index,
        };

        if best
            .as_ref()
            .is_none_or(|(current_score, _)| score < *current_score)
        {
            best = Some((score, presentation));
        }
    }

    best.map(|(_, presentation)| presentation).ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!("no airway branches found for {}", airway_name.trim()),
    })
}

pub fn sort_airway_suggestions_for_ui(mut suggestions: Vec<AirwaySuggestion>) -> Vec<AirwaySuggestion> {
    suggestions.sort_by(|left, right| {
        compare_airway_name_for_ui(&left.airway_name, &right.airway_name)
            .then_with(|| {
                left.distance_from_anchor_nm
                    .partial_cmp(&right.distance_from_anchor_nm)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    suggestions
}

pub fn describe_procedure_options_from_rows(
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    rows: Vec<ProcedureDistinctRow>,
) -> AppResult<ProcedureOptions> {
    if kind == ProcedureKind::Approach {
        let enroute_transitions = rows
            .iter()
            .filter(|row| row.route_type == "A")
            .map(|row| row.transition_id.clone())
            .filter(|transition| !transition.is_empty() && transition != "ALL")
            .collect::<Vec<_>>();
        let has_common_segment = approach_common_route_type(&rows).is_some();
        let valid_choices = if enroute_transitions.is_empty() {
            vec![ProcedureSpecChoice {
                runway_transition: None,
                enroute_transition: None,
            }]
        } else {
            enroute_transitions
                .iter()
                .cloned()
                .map(|enroute_transition| ProcedureSpecChoice {
                    runway_transition: None,
                    enroute_transition: Some(enroute_transition),
                })
                .collect::<Vec<_>>()
        };

        return Ok(ProcedureOptions {
            airport_id: airport_id.trim().to_string(),
            procedure_id: procedure_id.trim().to_string(),
            kind,
            runway_transitions: Vec::new(),
            enroute_transitions,
            has_common_segment,
            valid_choices,
        });
    }

    let layout = procedure_layout(kind.clone());
    let runway_transitions = rows
        .iter()
        .filter(|row| row.route_type == layout.runway_route_type)
        .map(|row| row.transition_id.clone())
        .filter(|transition| !transition.is_empty() && transition != "ALL")
        .collect::<Vec<_>>();
    let enroute_transitions = rows
        .iter()
        .filter(|row| row.route_type == layout.enroute_route_type)
        .map(|row| row.transition_id.clone())
        .filter(|transition| !transition.is_empty() && transition != "ALL")
        .collect::<Vec<_>>();
    let has_common_segment = rows.iter().any(|row| row.route_type == layout.common_route_type);

    let runway_choices = if runway_transitions.is_empty() {
        vec![None]
    } else {
        runway_transitions.iter().cloned().map(Some).collect::<Vec<_>>()
    };
    let enroute_choices = if enroute_transitions.is_empty() {
        vec![None]
    } else {
        enroute_transitions.iter().cloned().map(Some).collect::<Vec<_>>()
    };
    let valid_choices = runway_choices
        .into_iter()
        .flat_map(|runway_transition| {
            enroute_choices
                .iter()
                .cloned()
                .map(move |enroute_transition| ProcedureSpecChoice {
                    runway_transition: runway_transition.clone(),
                    enroute_transition,
                })
        })
        .collect::<Vec<_>>();

    Ok(ProcedureOptions {
        airport_id: airport_id.trim().to_string(),
        procedure_id: procedure_id.trim().to_string(),
        kind,
        runway_transitions,
        enroute_transitions,
        has_common_segment,
        valid_choices,
    })
}

pub fn materialize_procedure_from_records(
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<String>,
    enroute_transition: Option<String>,
    component_index: usize,
    rows: Vec<ProcedureDistinctRow>,
    legs: Vec<ProcedureLegMaterializationRecord>,
) -> AppResult<MaterializedProcedure> {
    let options = describe_procedure_options_from_rows(
        airport_id,
        procedure_id,
        kind.clone(),
        rows.clone(),
    )?;
    let requested = ProcedureSpecChoice {
        runway_transition: runway_transition.as_deref().map(str::trim).filter(|value| !value.is_empty()).map(str::to_string),
        enroute_transition: enroute_transition.as_deref().map(str::trim).filter(|value| !value.is_empty()).map(str::to_string),
    };

    if !options.valid_choices.iter().any(|choice| choice == &requested) {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "invalid procedure selection for {} {}: runway={:?} enroute={:?}",
                airport_id.trim(),
                procedure_id.trim(),
                requested.runway_transition,
                requested.enroute_transition
            ),
        });
    }

    let mut segments = Vec::<(
        MaterializedSegmentRole,
        Vec<ProcedureLegMaterializationRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )>::new();

    if kind == ProcedureKind::Approach {
        if let Some(enroute_transition) = requested.enroute_transition.as_deref() {
            let transition_legs = filter_procedure_records(
                &legs,
                airport_id,
                procedure_id,
                "A",
                enroute_transition,
            );
            let items = concretize_procedure_materialization_legs(&transition_legs, false);
            segments.push((MaterializedSegmentRole::EnrouteTransition, transition_legs, items, false));
        }

        if let Some(common_route_type) = approach_common_route_type(&rows) {
            let common_legs = filter_procedure_records(
                &legs,
                airport_id,
                procedure_id,
                &common_route_type,
                "",
            );
            let items = concretize_procedure_materialization_legs(&common_legs, false);
            segments.push((MaterializedSegmentRole::Common, common_legs, items, false));
        }

        let concretized_items = merge_concretized_segments_from_records(
            segments
                .iter()
                .map(|(_, _, items, _)| items.clone())
                .collect::<Vec<_>>(),
        );
        let terminal_discontinuity = match concretized_items.last() {
            Some(ConcretizedNavItem::Discontinuity { discontinuity, .. }) => Some(discontinuity.clone()),
            _ => None,
        };
        let resolved_legs = resolve_procedure_materialization_legs_with_provenance(
            airport_id,
            procedure_id,
            kind.clone(),
            component_index,
            &segments,
        );

        return Ok(MaterializedProcedure {
            procedure: ProcedureSegment {
                airport_id: AirportId(airport_id.trim().to_string()),
                procedure_id: procedure_id.trim().to_string(),
                kind,
                runway_transition: None,
                enroute_transition: requested.enroute_transition,
                terminal_discontinuity,
            },
            concretized_items,
            resolved_legs,
        });
    }

    let layout = procedure_layout(kind.clone());
    if let Some(enroute_transition) = requested.enroute_transition.as_deref() {
        let segment_legs = filter_procedure_records(
            &legs,
            airport_id,
            procedure_id,
            layout.enroute_route_type,
            enroute_transition,
        );
        let items = concretize_procedure_materialization_legs(&segment_legs, layout.reverse_segment_order);
        segments.push((
            MaterializedSegmentRole::EnrouteTransition,
            segment_legs,
            items,
            layout.reverse_segment_order,
        ));
    }
    if options.has_common_segment {
        let common_legs = filter_procedure_records(
            &legs,
            airport_id,
            procedure_id,
            layout.common_route_type,
            layout.common_transition_id,
        );
        let items = concretize_procedure_materialization_legs(&common_legs, layout.reverse_segment_order);
        segments.push((
            MaterializedSegmentRole::Common,
            common_legs,
            items,
            layout.reverse_segment_order,
        ));
    }
    if let Some(runway_transition) = requested.runway_transition.as_deref() {
        let segment_legs = filter_procedure_records(
            &legs,
            airport_id,
            procedure_id,
            layout.runway_route_type,
            runway_transition,
        );
        let items = concretize_procedure_materialization_legs(&segment_legs, layout.reverse_segment_order);
        segments.push((
            MaterializedSegmentRole::RunwayTransition,
            segment_legs,
            items,
            layout.reverse_segment_order,
        ));
    }

    let concretized_items = merge_concretized_segments_from_records(
        segments
            .iter()
            .map(|(_, _, items, _)| items.clone())
            .collect::<Vec<_>>(),
    );
    let terminal_discontinuity = match concretized_items.last() {
        Some(ConcretizedNavItem::Discontinuity { discontinuity, .. }) => Some(discontinuity.clone()),
        _ => None,
    };
    let resolved_legs = resolve_procedure_materialization_legs_with_provenance(
        airport_id,
        procedure_id,
        kind.clone(),
        component_index,
        &segments,
    );

    Ok(MaterializedProcedure {
        procedure: ProcedureSegment {
            airport_id: AirportId(airport_id.trim().to_string()),
            procedure_id: procedure_id.trim().to_string(),
            kind,
            runway_transition: requested.runway_transition,
            enroute_transition: requested.enroute_transition,
            terminal_discontinuity,
        },
        concretized_items,
        resolved_legs,
    })
}

struct ProcedureLayout {
    runway_route_type: &'static str,
    enroute_route_type: &'static str,
    common_route_type: &'static str,
    common_transition_id: &'static str,
    reverse_segment_order: bool,
}

enum MaterializedSegmentRole {
    EnrouteTransition,
    Common,
    RunwayTransition,
}

fn procedure_layout(kind: ProcedureKind) -> ProcedureLayout {
    match kind {
        ProcedureKind::Sid => ProcedureLayout {
            runway_route_type: "5",
            enroute_route_type: "4",
            common_route_type: "6",
            common_transition_id: "",
            reverse_segment_order: true,
        },
        ProcedureKind::Star => ProcedureLayout {
            runway_route_type: "1",
            enroute_route_type: "3",
            common_route_type: "2",
            common_transition_id: "",
            reverse_segment_order: true,
        },
        ProcedureKind::Approach => ProcedureLayout {
            runway_route_type: "",
            enroute_route_type: "",
            common_route_type: "",
            common_transition_id: "",
            reverse_segment_order: false,
        },
    }
}

fn approach_common_route_type(rows: &[ProcedureDistinctRow]) -> Option<String> {
    rows.iter()
        .find(|row| row.route_type != "A")
        .map(|row| row.route_type.clone())
}

fn filter_procedure_records(
    legs: &[ProcedureLegMaterializationRecord],
    airport_id: &str,
    procedure_id: &str,
    route_type: &str,
    transition_id: &str,
) -> Vec<ProcedureLegMaterializationRecord> {
    let mut filtered = legs
        .iter()
        .filter(|leg| {
            leg.key.airport_id.trim() == airport_id.trim()
                && leg.key.procedure_id.trim() == procedure_id.trim()
                && leg.key.route_type.trim() == route_type.trim()
                && leg.key.transition_id.trim() == transition_id.trim()
        })
        .cloned()
        .collect::<Vec<_>>();
    filtered.sort_by_key(|leg| leg.sequence);
    filtered
}

fn resolve_procedure_materialization_legs_with_provenance(
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    component_index: usize,
    segments: &[(
        MaterializedSegmentRole,
        Vec<ProcedureLegMaterializationRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )],
) -> Vec<ResolvedLeg> {
    let mut resolved = Vec::<ResolvedLeg>::new();

    for (role, leg_records, _, reversed) in segments {
        let mut fix_records = leg_records
            .iter()
            .filter(|leg| leg.nav_ref.is_some())
            .collect::<Vec<_>>();
        if *reversed {
            fix_records.reverse();
        }
        let role = procedure_segment_role(role);

        for pair in fix_records.windows(2) {
            let from = pair[0].nav_ref.clone().expect("filtered non-waypoint leg");
            let to = pair[1].nav_ref.clone().expect("filtered non-waypoint leg");
            let duplicate_of_previous = resolved
                .last()
                .is_some_and(|previous| previous.from == from && previous.to == to);
            if duplicate_of_previous {
                continue;
            }

            resolved.push(ResolvedLeg {
                id: format!(
                    "procedure-{}-{}-{}",
                    procedure_id.trim(),
                    pair[1].key.route_type.trim(),
                    pair[1].sequence
                ),
                from: from.clone(),
                to: to.clone(),
                source: ResolvedLegSource::RouteComponent { component_index },
                procedure_provenance: Some(ProcedureLegProvenance {
                    airport_id: airport_id.trim().to_string(),
                    procedure_id: procedure_id.trim().to_string(),
                    kind: kind.clone(),
                    role: role.clone(),
                    path_termination: pair[1].path_termination_kind.clone(),
                    leg_sequence: pair[1].sequence,
                    }),
            });
        }
    }

    resolved
}

fn procedure_segment_role(role: &MaterializedSegmentRole) -> ProcedureSegmentRole {
    match role {
        MaterializedSegmentRole::EnrouteTransition => ProcedureSegmentRole::EnrouteTransition,
        MaterializedSegmentRole::Common => ProcedureSegmentRole::Common,
        MaterializedSegmentRole::RunwayTransition => ProcedureSegmentRole::RunwayTransition,
    }
}

fn concretize_procedure_materialization_legs(
    legs: &[ProcedureLegMaterializationRecord],
    reverse_segment_order: bool,
) -> Vec<ConcretizedNavItem> {
    let mut waypoints = legs
        .iter()
        .filter_map(|leg| leg.nav_ref.clone())
        .collect::<Vec<_>>();
    waypoints.dedup();

    let terminal_discontinuity = legs.last().and_then(terminal_procedure_discontinuity);
    let initial_discontinuity = legs
        .iter()
        .take_while(|leg| leg.nav_ref.is_none())
        .last()
        .and_then(leading_procedure_discontinuity);

    if reverse_segment_order {
        waypoints.reverse();
    }

    let mut items = waypoints
        .into_iter()
        .map(|nav_ref| ConcretizedNavItem::Waypoint { nav_ref })
        .collect::<Vec<_>>();

    if reverse_segment_order {
        if let Some(discontinuity) = initial_discontinuity {
            items.push(ConcretizedNavItem::Discontinuity {
                label: discontinuity.display_label().to_string(),
                discontinuity,
            });
        }
    } else if let Some(discontinuity) = terminal_discontinuity {
        items.push(ConcretizedNavItem::Discontinuity {
            label: discontinuity.display_label().to_string(),
            discontinuity,
        });
    }

    items
}

fn merge_concretized_segments_from_records(
    segments: Vec<Vec<ConcretizedNavItem>>,
) -> Vec<ConcretizedNavItem> {
    let mut merged = Vec::<ConcretizedNavItem>::new();

    for segment in segments {
        for item in segment {
            let is_duplicate_boundary = matches!(
                (merged.last(), &item),
                (
                    Some(ConcretizedNavItem::Waypoint { nav_ref: left }),
                    ConcretizedNavItem::Waypoint { nav_ref: right }
                ) if left == right
            );
            if !is_duplicate_boundary {
                merged.push(item);
            }
        }
    }

    merged
}

fn terminal_procedure_discontinuity(
    leg: &ProcedureLegMaterializationRecord,
) -> Option<ProcedureDiscontinuity> {
    match leg.path_termination.trim() {
        "FM" => Some(ProcedureDiscontinuity::Vectors),
        "HM" => Some(ProcedureDiscontinuity::Hold),
        "VA" | "VI" if leg.nav_ref.is_none() => Some(ProcedureDiscontinuity::Vectors),
        _ => None,
    }
}

fn leading_procedure_discontinuity(
    leg: &ProcedureLegMaterializationRecord,
) -> Option<ProcedureDiscontinuity> {
    match leg.path_termination.trim() {
        "FM" => Some(ProcedureDiscontinuity::Vectors),
        "HM" => Some(ProcedureDiscontinuity::Hold),
        "VA" | "VI" if leg.nav_ref.is_none() => Some(ProcedureDiscontinuity::Vectors),
        _ => None,
    }
}

pub fn remove_flight_plan_leg(plan: &FlightPlan, index: usize) -> AppResult<FlightPlan> {
    let _ = plan;
    let _ = index;
    Err(AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "legacy leg removal is no longer supported; use structured component mutations"
            .to_string(),
    })
}

pub fn move_flight_plan_waypoint(
    plan: &FlightPlan,
    waypoint_index: usize,
    delta: isize,
) -> AppResult<FlightPlan> {
    let _ = plan;
    let _ = waypoint_index;
    let _ = delta;
    Err(AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: "legacy waypoint reordering is no longer supported; use structured component reordering"
            .to_string(),
    })
}

fn distance_nm(first: LatLon, second: LatLon) -> f64 {
    let lat_nm = (second.lat - first.lat) * 60.0;
    let lon_nm = (second.lon - first.lon) * 60.0 * ((first.lat + second.lat).to_radians() / 2.0).cos();
    (lat_nm.powi(2) + lon_nm.powi(2)).sqrt()
}

fn compare_airway_name_for_ui(left: &str, right: &str) -> std::cmp::Ordering {
    let left_parsed = parse_airway_name_for_ui(left);
    let right_parsed = parse_airway_name_for_ui(right);
    left_parsed
        .0
        .cmp(&right_parsed.0)
        .then_with(|| left_parsed.1.cmp(&right_parsed.1))
        .then_with(|| left.cmp(right))
}

fn parse_airway_name_for_ui(name: &str) -> (String, i32) {
    let trimmed = name.trim();
    let split_at = trimmed
        .find(|ch: char| ch.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let prefix = trimmed[..split_at].to_ascii_uppercase();
    let number = trimmed[split_at..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse::<i32>()
        .unwrap_or(i32::MAX);
    (prefix, number)
}

pub fn build_flight_plan_ui(plan: FlightPlan) -> AppResult<FlightPlanUiState> {
    let plan = build_flight_plan(plan)?;
    Ok(project_ui_state(&plan))
}

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_airway_from_anchors(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    airway_name: &str,
    origin_anchor: &NavRef,
    destination_anchor: &NavRef,
) -> AppResult<AirwayPlanMutation> {
    let selection = choose_best_airway_plan(db_path, airway_name, origin_anchor, destination_anchor)?;
    let entry = selection.entry.clone();
    let exit = selection.exit.clone();
    insert_airway_from_selection(
        db_path,
        plan,
        start_component_index,
        end_component_index,
        &entry,
        &exit,
        Some(selection),
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_airway_from_anchors_ui(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    airway_name: &str,
    origin_anchor: &NavRef,
    destination_anchor: &NavRef,
) -> AppResult<AirwayPlanUiMutation> {
    let mutation = insert_airway_from_anchors(
        db_path,
        plan,
        start_component_index,
        end_component_index,
        airway_name,
        origin_anchor,
        destination_anchor,
    )?;
    Ok(project_airway_mutation(mutation))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_airway_from_selection(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
    selection: Option<AirwayAutoSelection>,
) -> AppResult<AirwayPlanMutation> {
    let (airway, legs) = materialize_airway_selection(db_path, entry, exit, 0)?;
    let mutation_legs = with_component_index_source(&legs, start_component_index + 1);
    let inserted = insert_airway_between_waypoints(
        plan,
        start_component_index,
        end_component_index,
        airway,
        legs,
    )?;
    let component_index = start_component_index + 1;
    Ok(AirwayPlanMutation {
        airway: component_airway(&inserted, component_index)?,
        resolved_legs: mutation_legs,
        plan: inserted,
        component_index,
        selection: selection.unwrap_or_else(|| AirwayAutoSelection {
            airway_name: entry.airway_name.clone(),
            branch_key: entry.branch_key.clone(),
            entry: entry.clone(),
            exit: exit.clone(),
            origin_distance_nm: entry.distance_from_anchor_nm,
            destination_distance_nm: exit.distance_from_target_nm.unwrap_or(0.0),
            total_anchor_distance_nm: entry.distance_from_anchor_nm
                + exit.distance_from_target_nm.unwrap_or(0.0),
        }),
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_airway_from_selection_ui(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
    selection: Option<AirwayAutoSelection>,
) -> AppResult<AirwayPlanUiMutation> {
    let mutation = insert_airway_from_selection(
        db_path,
        plan,
        start_component_index,
        end_component_index,
        entry,
        exit,
        selection,
    )?;
    Ok(project_airway_mutation(mutation))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn replace_airway_from_selection(
    db_path: &Path,
    plan: &FlightPlan,
    component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
) -> AppResult<AirwayPlanMutation> {
    let (airway, legs) = materialize_airway_selection(db_path, entry, exit, component_index)?;
    let mutation_legs = with_component_index_source(&legs, component_index);
    let replaced = replace_airway_component(plan, component_index, airway, legs)?;
    Ok(AirwayPlanMutation {
        airway: component_airway(&replaced, component_index)?,
        resolved_legs: mutation_legs,
        plan: replaced,
        component_index,
        selection: AirwayAutoSelection {
            airway_name: entry.airway_name.clone(),
            branch_key: entry.branch_key.clone(),
            entry: entry.clone(),
            exit: exit.clone(),
            origin_distance_nm: entry.distance_from_anchor_nm,
            destination_distance_nm: exit.distance_from_target_nm.unwrap_or(0.0),
            total_anchor_distance_nm: entry.distance_from_anchor_nm
                + exit.distance_from_target_nm.unwrap_or(0.0),
        },
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn replace_airway_from_selection_ui(
    db_path: &Path,
    plan: &FlightPlan,
    component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
) -> AppResult<AirwayPlanUiMutation> {
    let mutation = replace_airway_from_selection(db_path, plan, component_index, entry, exit)?;
    Ok(project_airway_mutation(mutation))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_procedure_from_selection(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
) -> AppResult<ProcedurePlanMutation> {
    let built = materialize_procedure_selection(
        db_path,
        airport_id,
        procedure_id,
        kind,
        runway_transition,
        enroute_transition,
        0,
    )?;
    let inserted = insert_procedure_between_waypoints(
        plan,
        start_component_index,
        end_component_index,
        built.procedure.clone(),
        built.resolved_legs.clone(),
    )?;
    let component_index = start_component_index + 1;

    Ok(ProcedurePlanMutation {
        procedure: component_procedure(&inserted, component_index)?,
        resolved_legs: with_component_index_source(&built.resolved_legs, component_index),
        concretized_items: built.concretized_items,
        plan: inserted,
        component_index,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn insert_procedure_from_selection_ui(
    db_path: &Path,
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
) -> AppResult<ProcedurePlanUiMutation> {
    let mutation = insert_procedure_from_selection(
        db_path,
        plan,
        start_component_index,
        end_component_index,
        airport_id,
        procedure_id,
        kind,
        runway_transition,
        enroute_transition,
    )?;
    Ok(project_procedure_mutation(mutation))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn replace_procedure_from_selection(
    db_path: &Path,
    plan: &FlightPlan,
    component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
) -> AppResult<ProcedurePlanMutation> {
    let built = materialize_procedure_selection(
        db_path,
        airport_id,
        procedure_id,
        kind,
        runway_transition,
        enroute_transition,
        component_index,
    )?;
    let replaced = replace_procedure_component(
        plan,
        component_index,
        built.procedure.clone(),
        built.resolved_legs.clone(),
    )?;

    Ok(ProcedurePlanMutation {
        procedure: component_procedure(&replaced, component_index)?,
        resolved_legs: with_component_index_source(&built.resolved_legs, component_index),
        concretized_items: built.concretized_items,
        plan: replaced,
        component_index,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn replace_procedure_from_selection_ui(
    db_path: &Path,
    plan: &FlightPlan,
    component_index: usize,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
) -> AppResult<ProcedurePlanUiMutation> {
    let mutation = replace_procedure_from_selection(
        db_path,
        plan,
        component_index,
        airport_id,
        procedure_id,
        kind,
        runway_transition,
        enroute_transition,
    )?;
    Ok(project_procedure_mutation(mutation))
}

pub fn insert_airway_materialized_ui(
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: Option<usize>,
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    resolved_legs: Vec<ResolvedLeg>,
) -> AppResult<AirwayPlanUiMutation> {
    let (inserted, component_index) = match end_component_index {
        Some(end_component_index) => (
            insert_airway_between_waypoints(
                plan,
                start_component_index,
                end_component_index,
                airway,
                resolved_legs.clone(),
            )?,
            start_component_index + 1,
        ),
        None => {
            let inserted =
                insert_airway_after_waypoint(plan, start_component_index, airway, resolved_legs.clone())?;
            let component_index = inserted.route_components.len() - 1;
            (inserted, component_index)
        }
    };
    let mutation_legs = with_component_index_source(&resolved_legs, component_index);
    Ok(project_airway_mutation(AirwayPlanMutation {
        airway: component_airway(&inserted, component_index)?,
        resolved_legs: mutation_legs,
        plan: inserted,
        component_index,
        selection,
    }))
}

pub fn replace_airway_materialized_ui(
    plan: &FlightPlan,
    component_index: usize,
    selection: AirwayAutoSelection,
    airway: AirwaySegment,
    resolved_legs: Vec<ResolvedLeg>,
) -> AppResult<AirwayPlanUiMutation> {
    let mutation_legs = with_component_index_source(&resolved_legs, component_index);
    let replaced = replace_airway_component(plan, component_index, airway, resolved_legs)?;
    Ok(project_airway_mutation(AirwayPlanMutation {
        airway: component_airway(&replaced, component_index)?,
        resolved_legs: mutation_legs,
        plan: replaced,
        component_index,
        selection,
    }))
}

pub fn insert_procedure_materialized_ui(
    plan: &FlightPlan,
    start_component_index: usize,
    end_component_index: usize,
    built: MaterializedProcedure,
) -> AppResult<ProcedurePlanUiMutation> {
    let inserted = insert_procedure_between_waypoints(
        plan,
        start_component_index,
        end_component_index,
        built.procedure.clone(),
        built.resolved_legs.clone(),
    )?;
    let component_index = start_component_index + 1;
    Ok(project_procedure_mutation(ProcedurePlanMutation {
        procedure: component_procedure(&inserted, component_index)?,
        resolved_legs: with_component_index_source(&built.resolved_legs, component_index),
        concretized_items: built.concretized_items,
        plan: inserted,
        component_index,
    }))
}

pub fn replace_procedure_materialized_ui(
    plan: &FlightPlan,
    component_index: usize,
    built: MaterializedProcedure,
) -> AppResult<ProcedurePlanUiMutation> {
    let replaced = replace_procedure_component(
        plan,
        component_index,
        built.procedure.clone(),
        built.resolved_legs.clone(),
    )?;
    Ok(project_procedure_mutation(ProcedurePlanMutation {
        procedure: component_procedure(&replaced, component_index)?,
        resolved_legs: with_component_index_source(&built.resolved_legs, component_index),
        concretized_items: built.concretized_items,
        plan: replaced,
        component_index,
    }))
}

pub fn sequence_active_leg_ui(plan: &FlightPlan) -> AppResult<FlightPlanUiMutation> {
    let plan = sequence_active_leg(plan)?;
    Ok(project_plan_mutation(plan))
}

pub fn activate_leg_ui(plan: &FlightPlan, leg_index: usize) -> AppResult<FlightPlanUiMutation> {
    let plan = activate_leg(plan, leg_index)?;
    Ok(project_plan_mutation(plan))
}

pub fn activate_next_leg_ui(plan: &FlightPlan) -> AppResult<FlightPlanUiMutation> {
    let plan = activate_next_leg(plan)?;
    Ok(project_plan_mutation(plan))
}

pub fn suspend_sequencing_ui(plan: &FlightPlan) -> AppResult<FlightPlanUiMutation> {
    let plan = suspend_sequencing(plan)?;
    Ok(project_plan_mutation(plan))
}

pub fn unsuspend_sequencing_ui(plan: &FlightPlan) -> AppResult<FlightPlanUiMutation> {
    let plan = unsuspend_sequencing(plan)?;
    Ok(project_plan_mutation(plan))
}

pub fn delete_component_ui(plan: &FlightPlan, component_index: usize) -> AppResult<FlightPlanUiMutation> {
    let plan = delete_component(plan, component_index)?;
    Ok(project_plan_mutation(plan))
}

pub fn move_component_ui(
    plan: &FlightPlan,
    component_index: usize,
    delta: isize,
) -> AppResult<FlightPlanUiMutation> {
    let plan = move_component(plan, component_index, delta)?;
    Ok(project_plan_mutation(plan))
}

pub fn activate_direct_to_ui(
    plan: &FlightPlan,
    from_position: LatLon,
    target: NavRef,
) -> AppResult<FlightPlanUiMutation> {
    let plan = activate_direct_to(plan, from_position, target)?;
    Ok(project_plan_mutation(plan))
}

pub fn activate_direct_to_leg_ui(
    plan: &FlightPlan,
    from_position: LatLon,
    target_leg_id: &str,
) -> AppResult<FlightPlanUiMutation> {
    let plan = activate_direct_to_leg(plan, from_position, target_leg_id)?;
    Ok(project_plan_mutation(plan))
}

fn with_component_index_source(legs: &[ResolvedLeg], component_index: usize) -> Vec<ResolvedLeg> {
    legs.iter()
        .cloned()
        .map(|leg| ResolvedLeg {
            source: ResolvedLegSource::RouteComponent { component_index },
            ..leg
        })
        .collect()
}

fn project_plan_mutation(plan: FlightPlan) -> FlightPlanUiMutation {
    let ui_state = project_ui_state(&plan);
    FlightPlanUiMutation { plan, ui_state }
}

fn project_airway_mutation(mutation: AirwayPlanMutation) -> AirwayPlanUiMutation {
    let ui_state = project_ui_state(&mutation.plan);
    AirwayPlanUiMutation { mutation, ui_state }
}

fn project_procedure_mutation(mutation: ProcedurePlanMutation) -> ProcedurePlanUiMutation {
    let ui_state = project_ui_state(&mutation.plan);
    ProcedurePlanUiMutation { mutation, ui_state }
}

fn component_airway(plan: &FlightPlan, component_index: usize) -> AppResult<AirwaySegment> {
    match plan.route_components.get(component_index) {
        Some(RouteComponent::Airway { airway }) => Ok(airway.clone()),
        Some(_) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("component at index {component_index} is not an airway"),
        }),
        None => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("component index out of bounds: {component_index}"),
        }),
    }
}

fn component_procedure(plan: &FlightPlan, component_index: usize) -> AppResult<ProcedureSegment> {
    match plan.route_components.get(component_index) {
        Some(RouteComponent::Procedure { procedure }) => Ok(procedure.clone()),
        Some(_) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("component at index {component_index} is not a procedure"),
        }),
        None => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!("component index out of bounds: {component_index}"),
        }),
    }
}

pub fn plan_content_requirements(
    catalog: &CatalogHandle,
    plan: &FlightPlan,
) -> AppResult<Vec<ContentRequirement>> {
    let mut package_ids = Vec::new();

    for airport_code in plan_airport_codes(plan) {
        for plate in &catalog.bundle.plates {
            if plate.airport_id.0.eq_ignore_ascii_case(airport_code) {
                if let Some(pkg) = catalog
                    .bundle
                    .packages
                    .iter()
                    .find(|pkg| pkg.region_id == plate.region_id)
                {
                    package_ids.push(pkg.id.clone());
                }
            }
        }
    }

    package_ids.sort();
    package_ids.dedup();

    Ok(vec![ContentRequirement {
        package_ids,
        chart_ids: Vec::new(),
        plate_ids: Vec::new(),
    }])
}

fn plan_airport_codes(plan: &FlightPlan) -> Vec<&str> {
    let mut codes = Vec::new();

    for component in &plan.route_components {
        match component {
            RouteComponent::Waypoint { waypoint } => {
                if let Some(code) = waypoint.airport_code() {
                    codes.push(code);
                }
            }
            RouteComponent::Airway { airway } => {
                if let Some(code) = airway.entry.airport_code() {
                    codes.push(code);
                }
                if let Some(code) = airway.exit.airport_code() {
                    codes.push(code);
                }
            }
            RouteComponent::Procedure { procedure } => {
                codes.push(procedure.airport_id.0.as_str());
            }
        }
    }

    codes.sort();
    codes.dedup();
    codes
}

pub fn resolve_content_status(
    requirements: &[ContentRequirement],
    inventory: &ContentInventory,
    policy: ContentPolicy,
) -> AppResult<ContentReport> {
    let mut items = Vec::new();

    for requirement in requirements {
        for package_id in &requirement.package_ids {
            let installed = inventory
                .installed_packages
                .iter()
                .any(|pkg| &pkg.package_id == package_id && pkg.integrity_ok);

            let availability = match (installed, policy) {
                (true, ContentPolicy::StreamAllowed) => ContentAvailability::LocalAndRemote,
                (true, _) => ContentAvailability::LocalOnly,
                (false, ContentPolicy::StreamAllowed) => ContentAvailability::RemoteOnly,
                (false, _) => ContentAvailability::Unavailable,
            };

            items.push(ContentReportItem {
                label: package_id.package_name(),
                availability: AvailabilityDetail {
                    availability,
                    cycle_current: true,
                    integrity_ok: installed,
                    cached: installed,
                    offline_usable: installed,
                },
            });
        }
    }

    let fully_satisfied = items.iter().all(|item| match policy {
        ContentPolicy::StreamAllowed => !matches!(
            item.availability.availability,
            ContentAvailability::Unavailable
        ),
        _ => matches!(
            item.availability.availability,
            ContentAvailability::LocalOnly | ContentAvailability::LocalAndRemote
        ),
    });

    Ok(ContentReport {
        fully_satisfied,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};
    use std::path::PathBuf;

    fn sample_catalog_json() -> String {
        serde_json::json!({
            "schema_version": 1,
            "cycle": "2026-04-16",
            "catalog_revision": "2026-04-05T22:00:00Z",
            "families": [
                {
                    "id": "sectional",
                    "display_name": "VFR Sectional Charts",
                    "kind": "tiled_raster",
                    "max_zoom": 10,
                    "tile_size": 512
                }
            ],
            "regions": [
                {
                    "id": "ne",
                    "display_name": "Northeast",
                    "sort_order": 0
                }
            ],
            "packages": [
                {
                    "id": {
                        "region": "ne",
                        "family": "sectional",
                        "cycle": "2026-04-16"
                    },
                    "package_name": "NE_SEC",
                    "family_id": "sectional",
                    "region_id": "ne",
                    "cycle": "2026-04-16",
                    "artifact_kind": "zip",
                    "relative_url": "/2026-04-16/NE_SEC.zip",
                    "manifest_name": "NE_SEC",
                    "size_bytes": null,
                    "checksum_sha256": null
                }
            ],
            "charts": [
                {
                    "id": {
                        "family": "sectional",
                        "name": "Boston",
                        "cycle": "2026-04-16"
                    },
                    "family_id": "sectional",
                    "name": "Boston",
                    "display_name": "Boston",
                    "cycle": "2026-04-16",
                    "region_ids": ["ne"],
                    "max_zoom": 10,
                    "tile_path_template": "tiles/{chart_index}/{z}/{x}/{y}"
                }
            ],
            "plates": [
                {
                    "id": {
                        "airport_id": "KBOS",
                        "procedure_code": "IAP-ILS-RWY-04R",
                        "page": 1,
                        "cycle": "2026-04-16"
                    },
                    "airport_id": "KBOS",
                    "region_id": "ne",
                    "cycle": "2026-04-16",
                    "procedure_code": "IAP-ILS-RWY-04R",
                    "display_name": "ILS OR LOC RWY 04R",
                    "kind": "approach",
                    "georeferenced": true,
                    "page_count": 1,
                    "asset_base_path": "plates/KBOS/IAP-ILS-RWY-04R"
                }
            ],
            "supplements": []
        })
        .to_string()
    }

    fn sample_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-1".to_string(),
            name: "KBOS to KJFK".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOS".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KJFK".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KBOS".to_string())),
            destination: Some(AirportId("KJFK".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    fn fixture_db_path() -> &'static Path {
        static DB_PATH: OnceLock<PathBuf> = OnceLock::new();
        DB_PATH.get_or_init(|| {
            if let Some(value) = std::env::var_os("AEROBAG_FIXTURE_NAV_DB") {
                let path = PathBuf::from(value);
                if path.is_file() {
                    return path;
                }
            }
            for candidate in [
                "/root/aerobag-three/ui-target-flightplan/android/assets/nav-db/main.db",
                "/root/aerobag-three/ui-target/android/assets/nav-db/main.db",
            ] {
                let path = PathBuf::from(candidate);
                if path.is_file() {
                    return path;
                }
            }
            for root in [
                "/root/aerobag-artifacts/published-unpacked",
                "/root/aerobag-artifacts/cache/nodes",
                "/root/aerobag-artifacts/private-work",
            ] {
                if let Some(path) = find_fixture_nav_db(Path::new(root)) {
                    return path;
                }
            }
            panic!("unable to locate nav database fixture");
        })
        .as_path()
    }

    fn find_fixture_nav_db(root: &Path) -> Option<PathBuf> {
        let entries = std::fs::read_dir(root).ok()?;
        for entry in entries {
            let path = entry.ok()?.path();
            if path.is_dir() {
                if let Some(found) = find_fixture_nav_db(&path) {
                    return Some(found);
                }
                continue;
            }
            if path.file_name().is_some_and(|name| name == "main.db")
                && path
                    .parent()
                    .and_then(|parent| parent.file_name())
                    .is_some_and(|name| name == "output" || name == "data_2604")
            {
                return Some(path);
            }
        }
        None
    }

    fn load_browser_style_procedure_distinct_rows(
        db_path: &Path,
        airport_id: &str,
        procedure_id: &str,
    ) -> Vec<ProcedureDistinctRow> {
        let connection = Connection::open(db_path).expect("open fixture nav db");
        let mut stmt = connection
            .prepare(
                "
                SELECT DISTINCT
                  trim(route_type) AS route_type,
                  trim(transition_identifier) AS transition_id
                FROM cifp_sid_star_app
                WHERE trim(airport_identifier) = trim(?1)
                  AND trim(sid_star_approach_identifier) = trim(?2)
                ORDER BY trim(route_type), trim(transition_identifier)
                ",
            )
            .expect("prepare distinct row query");
        let rows = stmt
            .query_map(params![airport_id, procedure_id], |row| {
                Ok(ProcedureDistinctRow {
                    route_type: row.get::<_, String>(0)?,
                    transition_id: row.get::<_, String>(1)?,
                })
            })
            .expect("query distinct rows");
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect distinct rows")
    }

    fn browser_style_nav_ref_for_identifier(
        connection: &Connection,
        identifier: &str,
    ) -> Option<NavRef> {
        let trimmed = identifier.trim();
        if trimmed.is_empty() {
            return None;
        }
        if trimmed.starts_with("RW") {
            return Some(NavRef::Fix(trimmed.to_string()));
        }
        if connection
            .query_row(
                "SELECT LocationID FROM airports WHERE trim(LocationID) = trim(?1) LIMIT 1",
                params![trimmed],
                |row| row.get::<_, String>(0),
            )
            .is_ok()
        {
            return Some(NavRef::Airport(trimmed.to_string()));
        }
        if connection
            .query_row(
                "SELECT LocationID FROM nav WHERE trim(LocationID) = trim(?1) LIMIT 1",
                params![trimmed],
                |row| row.get::<_, String>(0),
            )
            .is_ok()
        {
            return Some(NavRef::Navaid(trimmed.to_string()));
        }
        if connection
            .query_row(
                "SELECT LocationID FROM fix WHERE trim(LocationID) = trim(?1) LIMIT 1",
                params![trimmed],
                |row| row.get::<_, String>(0),
            )
            .is_ok()
        {
            return Some(NavRef::Fix(trimmed.to_string()));
        }
        None
    }

    fn load_browser_style_procedure_materialization_records(
        db_path: &Path,
        airport_id: &str,
        procedure_id: &str,
    ) -> Vec<ProcedureLegMaterializationRecord> {
        let connection = Connection::open(db_path).expect("open fixture nav db");
        let mut stmt = connection
            .prepare(
                "
                SELECT
                  trim(airport_identifier) AS airport_id,
                  trim(sid_star_approach_identifier) AS procedure_id,
                  trim(route_type) AS route_type,
                  trim(transition_identifier) AS transition_id,
                  CAST(sequence_number AS INTEGER) AS sequence,
                  trim(fix_identifier) AS fix_identifier,
                  trim(path_and_termination) AS path_termination
                FROM cifp_sid_star_app
                WHERE trim(airport_identifier) = trim(?1)
                  AND trim(sid_star_approach_identifier) = trim(?2)
                ORDER BY trim(route_type), trim(transition_identifier), CAST(sequence_number AS INTEGER)
                ",
            )
            .expect("prepare materialization records query");
        let rows = stmt
            .query_map(params![airport_id, procedure_id], |row| {
                let airport_id = row.get::<_, String>(0)?;
                let procedure_id = row.get::<_, String>(1)?;
                let route_type = row.get::<_, String>(2)?;
                let transition_id = row.get::<_, String>(3)?;
                let sequence = row.get::<_, i32>(4)?;
                let fix_identifier = row.get::<_, String>(5)?;
                let path_termination = row.get::<_, String>(6)?;
                Ok(ProcedureLegMaterializationRecord {
                    key: ProcedureVariantKey {
                        airport_id,
                        procedure_id,
                        route_type,
                        transition_id,
                    },
                    sequence,
                    nav_ref: browser_style_nav_ref_for_identifier(&connection, &fix_identifier),
                    path_termination_kind: interpret_path_termination(&path_termination),
                    path_termination,
                })
            })
            .expect("query materialization records");
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect materialization records")
    }

    fn nav_ref_label_for_test(nav_ref: &NavRef) -> String {
        match nav_ref {
            NavRef::Airport(code) => code.clone(),
            NavRef::Navaid(code) => code.clone(),
            NavRef::Fix(code) => code.clone(),
            NavRef::LatLon(position) => format!("L:{:.6},{:.6}", position.lat, position.lon),
        }
    }

    fn concretized_item_label_for_test(item: &ConcretizedNavItem) -> String {
        match item {
            ConcretizedNavItem::Waypoint { nav_ref } => nav_ref_label_for_test(nav_ref),
            ConcretizedNavItem::Discontinuity { label, .. } => format!("D:{label}"),
        }
    }

    #[test]
    fn loads_catalog_with_structured_ids() {
        let handle = load_catalog(&sample_catalog_json()).unwrap();
        assert_eq!(handle.bundle.schema_version, 1);
        assert_eq!(handle.bundle.families[0].id, ChartFamilyId::Sectional);
        assert_eq!(handle.bundle.regions[0].id, RegionId::Ne);
    }

    #[test]
    fn rejects_empty_flight_plan() {
        let result = build_flight_plan(FlightPlan {
            id: "plan-1".to_string(),
            name: "Empty".to_string(),
            legs: Vec::new(),
            route_components: Vec::new(),
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        });
        assert_eq!(result.unwrap_err().kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn deduplicates_required_packages_across_matching_legs() {
        let catalog = load_catalog(&sample_catalog_json()).unwrap();
        let requirements = plan_content_requirements(&catalog, &sample_plan()).unwrap();
        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].package_ids.len(), 1);
        assert_eq!(requirements[0].package_ids[0].region, RegionId::Ne);
    }

    #[test]
    fn component_only_plan_builds_resolved_legs() {
        let plan = build_flight_plan(sample_plan()).unwrap();

        assert_eq!(plan.route_components.len(), 2);
        assert_eq!(plan.resolved_legs.len(), 1);
    }

    #[test]
    fn rejects_legacy_leg_only_plan() {
        let err = build_flight_plan(FlightPlan {
            id: "legacy-only".to_string(),
            name: "Legacy only".to_string(),
            legs: vec![PlanLeg {
                from: NavRef::Airport("KBOS".to_string()),
                to: NavRef::Airport("KJFK".to_string()),
                airway: None,
            }],
            route_components: Vec::new(),
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KBOS".to_string())),
            destination: Some(AirportId("KJFK".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .unwrap_err();

        assert_eq!(err.kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn inserts_airway_from_origin_and_destination_anchors() {
        let plan = FlightPlan {
            id: "airway-insert".to_string(),
            name: "Airway insert".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KHIO".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KHIO".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation = insert_airway_from_anchors(
            fixture_db_path(),
            &plan,
            0,
            1,
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            &NavRef::Airport("KUAO".to_string()),
        )
        .unwrap();

        assert_eq!(mutation.component_index, 1);
        assert_eq!(mutation.selection.branch_key, "V2-A");
        assert_eq!(mutation.selection.entry.nav_ref, NavRef::Navaid("SEA".to_string()));
        assert_eq!(mutation.selection.exit.nav_ref, NavRef::Fix("VAMPS".to_string()));
        assert!(matches!(mutation.plan.route_components[1], RouteComponent::Airway { .. }));
        assert!(mutation.plan.guidance.is_none());
        assert_eq!(mutation.resolved_legs.first().unwrap().from, NavRef::Navaid("SEA".to_string()));
        assert_eq!(mutation.resolved_legs.last().unwrap().to, NavRef::Fix("VAMPS".to_string()));
    }

    #[test]
    fn inserts_procedure_from_selection_returns_mutated_plan_and_component_payload() {
        let plan = FlightPlan {
            id: "procedure-insert".to_string(),
            name: "Procedure insert".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("ETX".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOS".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: None,
            destination: Some(AirportId("KBOS".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation = insert_procedure_from_selection(
            fixture_db_path(),
            &plan,
            0,
            1,
            "KBOS",
            "I04R",
            ProcedureKind::Approach,
            None,
            Some("GOSHI"),
        )
        .unwrap();

        assert_eq!(mutation.component_index, 1);
        assert!(matches!(
            mutation.plan.route_components[1],
            RouteComponent::Procedure { .. }
        ));
        assert_eq!(mutation.procedure.procedure_id, "I04R");
        assert!(!mutation.concretized_items.is_empty());
        assert!(!mutation.resolved_legs.is_empty());
    }

    #[test]
    fn inserts_airway_from_materialized_selection_returns_projected_mutation() {
        let plan = FlightPlan {
            id: "airway-materialized".to_string(),
            name: "Airway materialized".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KUAO".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let selection = choose_best_airway_plan(
            fixture_db_path(),
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            &NavRef::Airport("KUAO".to_string()),
        )
        .unwrap();
        let (airway, resolved_legs) = materialize_airway_selection(
            fixture_db_path(),
            &selection.entry,
            &selection.exit,
            0,
        )
        .unwrap();

        let mutation = insert_airway_materialized_ui(
            &plan,
            0,
            Some(1),
            selection,
            airway,
            resolved_legs,
        )
        .unwrap();

        assert_eq!(mutation.mutation.component_index, 1);
        assert!(matches!(
            mutation.mutation.plan.route_components[1],
            RouteComponent::Airway { .. }
        ));
        assert_eq!(mutation.ui_state.components[1].kind, RouteComponentViewKind::Airway);
    }

    #[test]
    fn sorts_airway_suggestions_for_ui_by_prefix_then_number() {
        let suggestions = vec![
            AirwaySuggestion {
                airway_name: "V120".to_string(),
                nearest_branch_key: Some("A".to_string()),
                nearest_nav_ref: NavRef::Navaid("SEA".to_string()),
                nearest_sequence: 10,
                distance_from_anchor_nm: 1.0,
            },
            AirwaySuggestion {
                airway_name: "J1".to_string(),
                nearest_branch_key: Some("A".to_string()),
                nearest_nav_ref: NavRef::Navaid("SEA".to_string()),
                nearest_sequence: 10,
                distance_from_anchor_nm: 1.0,
            },
            AirwaySuggestion {
                airway_name: "V2".to_string(),
                nearest_branch_key: Some("A".to_string()),
                nearest_nav_ref: NavRef::Navaid("SEA".to_string()),
                nearest_sequence: 10,
                distance_from_anchor_nm: 1.0,
            },
            AirwaySuggestion {
                airway_name: "V17".to_string(),
                nearest_branch_key: Some("A".to_string()),
                nearest_nav_ref: NavRef::Navaid("SEA".to_string()),
                nearest_sequence: 10,
                distance_from_anchor_nm: 1.0,
            },
        ];

        let sorted = sort_airway_suggestions_for_ui(suggestions);
        let names = sorted
            .into_iter()
            .map(|suggestion| suggestion.airway_name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["J1", "V2", "V17", "V120"]);
    }

    #[test]
    fn prepares_airway_presentation_oriented_between_origin_and_destination() {
        let branches = vec![AirwayBranch {
            display_name: "V2".to_string(),
            branch_key: "A".to_string(),
            points: vec![
                AirwayFixPoint {
                    airway_name: "V2".to_string(),
                    sequence: 10,
                    position: LatLon { lat: 0.0, lon: 0.0 },
                    nav_ref: NavRef::Fix("A".to_string()),
                },
                AirwayFixPoint {
                    airway_name: "V2".to_string(),
                    sequence: 20,
                    position: LatLon { lat: 0.0, lon: 1.0 },
                    nav_ref: NavRef::Fix("B".to_string()),
                },
                AirwayFixPoint {
                    airway_name: "V2".to_string(),
                    sequence: 30,
                    position: LatLon { lat: 0.0, lon: 2.0 },
                    nav_ref: NavRef::Fix("C".to_string()),
                },
            ],
        }];

        let presentation = prepare_airway_presentation(
            "V2",
            branches,
            LatLon { lat: 0.0, lon: 1.8 },
            Some(LatLon { lat: 0.0, lon: 0.1 }),
        )
        .unwrap();

        let labels = presentation
            .points
            .iter()
            .map(|point| match &point.nav_ref {
                NavRef::Fix(id) => id.clone(),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();

        assert_eq!(labels, vec!["C", "B", "A"]);
        assert_eq!(presentation.suggested_entry_index, 0);
        assert_eq!(presentation.suggested_exit_index, Some(2));
    }

    #[test]
    fn prepares_airway_presentation_without_destination_keeps_forward_order_and_no_exit_hint() {
        let branches = vec![AirwayBranch {
            display_name: "V2".to_string(),
            branch_key: "A".to_string(),
            points: vec![
                AirwayFixPoint {
                    airway_name: "V2".to_string(),
                    sequence: 10,
                    position: LatLon { lat: 0.0, lon: 0.0 },
                    nav_ref: NavRef::Fix("A".to_string()),
                },
                AirwayFixPoint {
                    airway_name: "V2".to_string(),
                    sequence: 20,
                    position: LatLon { lat: 0.0, lon: 1.0 },
                    nav_ref: NavRef::Fix("B".to_string()),
                },
            ],
        }];

        let presentation = prepare_airway_presentation(
            "V2",
            branches,
            LatLon { lat: 0.0, lon: 0.2 },
            None,
        )
        .unwrap();

        assert_eq!(presentation.suggested_entry_index, 0);
        assert_eq!(presentation.suggested_exit_index, None);
    }

    #[test]
    fn inserts_procedure_from_materialized_selection_returns_projected_mutation() {
        let plan = FlightPlan {
            id: "procedure-materialized".to_string(),
            name: "Procedure materialized".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("ETX".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOS".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: None,
            destination: Some(AirportId("KBOS".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let built = materialize_procedure_selection(
            fixture_db_path(),
            "KBOS",
            "I04R",
            ProcedureKind::Approach,
            None,
            Some("GOSHI"),
            0,
        )
        .unwrap();

        let mutation = insert_procedure_materialized_ui(&plan, 0, 1, built).unwrap();

        assert_eq!(mutation.mutation.component_index, 1);
        assert!(matches!(
            mutation.mutation.plan.route_components[1],
            RouteComponent::Procedure { .. }
        ));
        assert_eq!(mutation.ui_state.components[1].kind, RouteComponentViewKind::Procedure);
    }

    #[test]
    fn browser_style_i34_materialization_matches_native_core_and_stays_local() {
        let base_plan = FlightPlan {
            id: "krnt-v23-kuao-krdd".to_string(),
            name: "Seeded KRNT V23 KUAO KRDD".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRDD".to_string()),
                },
            ],
            resolved_legs: Vec::new(),
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KRDD".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let airway = insert_airway_from_anchors(
            fixture_db_path(),
            &base_plan,
            0,
            1,
            "V23",
            &NavRef::Airport("KRNT".to_string()),
            &NavRef::Airport("KUAO".to_string()),
        )
        .unwrap();

        let native = materialize_procedure_selection(
            fixture_db_path(),
            "KRDD",
            "I34",
            ProcedureKind::Approach,
            None,
            Some("RBL"),
            0,
        )
        .unwrap();

        let distinct_rows =
            load_browser_style_procedure_distinct_rows(fixture_db_path(), "KRDD", "I34");
        let records =
            load_browser_style_procedure_materialization_records(fixture_db_path(), "KRDD", "I34");
        let browser_style = materialize_procedure_from_records(
            "KRDD",
            "I34",
            ProcedureKind::Approach,
            None,
            Some("RBL".to_string()),
            0,
            distinct_rows,
            records,
        )
        .unwrap();

        let browser_labels = browser_style
            .concretized_items
            .iter()
            .map(concretized_item_label_for_test)
            .collect::<Vec<_>>();
        let native_labels = native
            .concretized_items
            .iter()
            .map(concretized_item_label_for_test)
            .collect::<Vec<_>>();
        assert_eq!(browser_labels, native_labels);

        let inserted = insert_procedure_materialized_ui(&airway.plan, 2, 3, browser_style).unwrap();
        let route_pairs = inserted
            .mutation
            .plan
            .resolved_legs
            .iter()
            .map(|leg| (leg.from.clone(), leg.to.clone()))
            .collect::<Vec<_>>();

        assert!(route_pairs.contains(&(
            NavRef::Airport("KUAO".to_string()),
            NavRef::Navaid("RBL".to_string()),
        )));
        assert!(route_pairs.contains(&(
            NavRef::Navaid("RBL".to_string()),
            NavRef::Fix("DIBLE".to_string()),
        )));
        assert!(route_pairs.contains(&(
            NavRef::Fix("DIBLE".to_string()),
            NavRef::Fix("LASSN".to_string()),
        )));
        assert!(route_pairs.contains(&(
            NavRef::Fix("LASSN".to_string()),
            NavRef::Fix("RW34".to_string()),
        )));
    }

    #[test]
    fn replaces_procedure_from_selection_returns_updated_atomic_component_payload() {
        let inserted = insert_procedure_from_selection(
            fixture_db_path(),
            &FlightPlan {
                id: "procedure-replace".to_string(),
                name: "Procedure replace".to_string(),
                legs: Vec::new(),
                route_components: vec![
                    RouteComponent::Waypoint {
                        waypoint: NavRef::Fix("ETX".to_string()),
                    },
                    RouteComponent::Waypoint {
                        waypoint: NavRef::Airport("KBOS".to_string()),
                    },
                ],
                resolved_legs: Vec::new(),
                guidance: None,
                departure: None,
                destination: Some(AirportId("KBOS".to_string())),
                alternate: None,
                cruise_altitude_ft: None,
                notes: None,
                updated_at_epoch_ms: 0,
                version: 1,
            },
            0,
            1,
            "KBOS",
            "I04R",
            ProcedureKind::Approach,
            None,
            Some("GOSHI"),
        )
        .unwrap();

        let replaced = replace_procedure_from_selection(
            fixture_db_path(),
            &inserted.plan,
            inserted.component_index,
            "KBOS",
            "R04R",
            ProcedureKind::Approach,
            None,
            Some("GOSHI"),
        )
        .unwrap();

        assert_eq!(replaced.component_index, inserted.component_index);
        assert_eq!(replaced.procedure.procedure_id, "R04R");
        assert!(matches!(
            replaced.plan.route_components[replaced.component_index],
            RouteComponent::Procedure { .. }
        ));
        assert!(!replaced.resolved_legs.is_empty());
    }

    #[test]
    fn content_requirements_consider_airports_from_procedure_components() {
        let catalog = load_catalog(&sample_catalog_json()).unwrap();
        let plan = build_flight_plan(FlightPlan {
            id: "proc".to_string(),
            name: "Procedure".to_string(),
            legs: Vec::new(),
            route_components: vec![RouteComponent::Procedure {
                procedure: ProcedureSegment {
                    airport_id: AirportId("KBOS".to_string()),
                    procedure_id: "IAP-ILS-RWY-04R".to_string(),
                    kind: ProcedureKind::Approach,
                    runway_transition: Some("04R".to_string()),
                    enroute_transition: None,
                    terminal_discontinuity: None,
                },
            }],
            resolved_legs: vec![ResolvedLeg {
                id: "proc-0".to_string(),
                from: NavRef::Fix("NOONY".to_string()),
                to: NavRef::Airport("KBOS".to_string()),
                source: ResolvedLegSource::RouteComponent { component_index: 0 },
                procedure_provenance: None,
            }],
            guidance: None,
            departure: None,
            destination: Some(AirportId("KBOS".to_string())),
            alternate: None,
            cruise_altitude_ft: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .unwrap();

        let requirements = plan_content_requirements(&catalog, &plan).unwrap();

        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].package_ids.len(), 1);
        assert_eq!(requirements[0].package_ids[0].package_name(), "NE_SEC");
    }

    #[test]
    fn stream_allowed_reports_remote_content_as_satisfied() {
        let requirements = vec![ContentRequirement {
            package_ids: vec![PackageId {
                region: RegionId::Ne,
                family: ChartFamilyId::Sectional,
                cycle: "2026-04-16".to_string(),
            }],
            chart_ids: Vec::new(),
            plate_ids: Vec::new(),
        }];

        let report = resolve_content_status(
            &requirements,
            &ContentInventory {
                installed_packages: Vec::new(),
                cached_tilesets: Vec::new(),
                cached_plates: Vec::new(),
            },
            ContentPolicy::StreamAllowed,
        )
        .unwrap();

        assert!(report.fully_satisfied);
        assert_eq!(
            report.items[0].availability.availability,
            ContentAvailability::RemoteOnly
        );
    }

    #[test]
    fn offline_required_marks_missing_content_unsatisfied() {
        let requirements = vec![ContentRequirement {
            package_ids: vec![PackageId {
                region: RegionId::Ne,
                family: ChartFamilyId::Sectional,
                cycle: "2026-04-16".to_string(),
            }],
            chart_ids: Vec::new(),
            plate_ids: Vec::new(),
        }];

        let report = resolve_content_status(
            &requirements,
            &ContentInventory {
                installed_packages: Vec::new(),
                cached_tilesets: Vec::new(),
                cached_plates: Vec::new(),
            },
            ContentPolicy::OfflineRequired,
        )
        .unwrap();

        assert!(!report.fully_satisfied);
        assert_eq!(
            report.items[0].availability.availability,
            ContentAvailability::Unavailable
        );
    }

}
