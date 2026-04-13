use std::path::Path;

use rusqlite::{params, Connection};

use crate::errors::{AppError, AppErrorKind, AppResult};
use crate::geometry::LatLon;
use crate::navdb_types::{
    AirwayAutoSelection, AirwayBranch, AirwayEntryCandidate, AirwayExitCandidate,
    AirwayExitSelection, AirwayFixPoint, AirwayPoint, AirwaySuggestion, MaterializedProcedure,
    ProcedureLegRecord, ProcedureOptions, ProcedureSpecChoice, ProcedureSummary,
    ProcedureVariantKey,
};
use crate::planning::{
    interpret_path_termination, AirwaySegment, ConcretizedNavItem, NavRef,
    ProcedureDiscontinuity, ProcedureKind, ProcedureLegProvenance, ProcedureSegmentRole,
    ResolvedLeg, ResolvedLegSource,
};
#[cfg(test)]
use crate::planning::PathTermination;

const MAX_AIRWAY_BRANCH_HOP_NM: f64 = 500.0;
const AIRWAY_SEARCH_RADII_NM: [f64; 5] = [25.0, 50.0, 100.0, 200.0, 400.0];
const AIRWAY_POINT_QUERY_LIMIT: usize = 256;

pub fn load_airway_points(db_path: &Path, airway_name: &str) -> AppResult<Vec<AirwayPoint>> {
    with_connection(db_path, |connection| {
        let mut stmt = connection.prepare(
            "SELECT trim(name), CAST(sequence AS INTEGER), Latitude, Longitude
             FROM airways
             WHERE trim(name) = trim(?1)
             ORDER BY CAST(sequence AS INTEGER)",
        )?;

        let rows = stmt.query_map(params![airway_name], |row| {
            Ok(AirwayPoint {
                airway_name: row.get::<_, String>(0)?,
                sequence: row.get::<_, i32>(1)?,
                position: LatLon {
                    lat: row.get::<_, f64>(2)?,
                    lon: row.get::<_, f64>(3)?,
                },
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })
}

pub fn load_airway_branches(db_path: &Path, airway_name: &str) -> AppResult<Vec<AirwayBranch>> {
    if let Some(branches) =
        with_connection(db_path, |connection| load_airway_branches_from_branch_table(connection, airway_name))?
    {
        return Ok(branches);
    }

    let points = load_airway_points(db_path, airway_name)?;
    with_connection(db_path, |connection| {
        let named_points = points
            .into_iter()
            .map(|point| resolve_airway_fix_point(connection, point))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(split_airway_branches(named_points))
    })
}

pub fn resolve_nav_ref_position(db_path: &Path, nav_ref: &NavRef) -> AppResult<LatLon> {
    with_connection(db_path, |connection| resolve_nav_ref_position_in_db(connection, nav_ref))
}

pub fn resolve_nav_ref_position_with_procedure_airport(
    db_path: &Path,
    nav_ref: &NavRef,
    procedure_airport_id: Option<&str>,
) -> AppResult<LatLon> {
    with_connection(db_path, |connection| {
        resolve_nav_ref_position_in_db(connection, nav_ref).or_else(|error| {
            match (nav_ref, procedure_airport_id) {
                (NavRef::Fix(code), Some(airport_id)) => {
                    resolve_runway_fix_position_in_db(connection, code, airport_id).or(Err(error))
                }
                _ => Err(error),
            }
        })
    })
}

pub fn suggest_airways_near(
    db_path: &Path,
    anchor: &NavRef,
    limit: usize,
) -> AppResult<Vec<AirwaySuggestion>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    with_connection(db_path, |connection| {
        let anchor_position = resolve_nav_ref_position_in_db(connection, anchor)?;
        let mut suggestions = Vec::<AirwaySuggestion>::new();

        for radius_nm in AIRWAY_SEARCH_RADII_NM {
            suggestions = if table_exists(connection, "airways_branch")? {
                query_airway_suggestions_from_branch_table(connection, anchor_position, radius_nm, limit)?
            } else {
                query_airway_suggestions_from_legacy_table(connection, anchor_position, radius_nm, limit)?
            };

            if suggestions.len() >= limit {
                break;
            }
        }

        suggestions.sort_by(|left, right| {
            left.distance_from_anchor_nm
                .partial_cmp(&right.distance_from_anchor_nm)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.airway_name.cmp(&right.airway_name))
        });
        suggestions.truncate(limit);
        Ok(suggestions)
    })
}

pub fn list_airway_entry_candidates(
    db_path: &Path,
    airway_name: &str,
    anchor: &NavRef,
    limit: usize,
) -> AppResult<Vec<AirwayEntryCandidate>> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let anchor_position = resolve_nav_ref_position(db_path, anchor)?;
    let mut candidates = load_airway_branches(db_path, airway_name)?
        .into_iter()
        .flat_map(|branch| {
            let branch_key = branch.branch_key.clone();
            let airway_name = branch.display_name.clone();
            branch
                .points
                .iter()
                .enumerate()
                .map(|(branch_point_index, point)| AirwayEntryCandidate {
                    airway_name: airway_name.clone(),
                    branch_key: branch_key.clone(),
                    branch_point_index,
                    sequence: point.sequence,
                    nav_ref: point.nav_ref.clone(),
                    distance_from_anchor_nm: distance_nm(anchor_position, point.position),
                    previous_nav_ref: if branch_point_index > 0 {
                        branch
                            .points
                            .get(branch_point_index - 1)
                            .map(|previous| previous.nav_ref.clone())
                    } else {
                        None
                    },
                    next_nav_ref: branch
                        .points
                        .get(branch_point_index + 1)
                        .map(|next| next.nav_ref.clone()),
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        left.distance_from_anchor_nm
            .partial_cmp(&right.distance_from_anchor_nm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.airway_name.cmp(&right.airway_name))
            .then_with(|| left.branch_key.cmp(&right.branch_key))
            .then_with(|| left.sequence.cmp(&right.sequence))
    });
    candidates.truncate(limit);
    Ok(candidates)
}

pub fn list_airway_exit_candidates(
    db_path: &Path,
    airway_name: &str,
    branch_key: &str,
    entry_branch_point_index: usize,
    target: Option<&NavRef>,
) -> AppResult<AirwayExitSelection> {
    let branch = load_specific_airway_branch(db_path, airway_name, branch_key)?;
    if entry_branch_point_index >= branch.points.len() {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "entry index {entry_branch_point_index} is out of bounds for airway {} branch {}",
                airway_name.trim(),
                branch_key.trim()
            ),
        });
    }

    let target_position = target
        .map(|target_ref| resolve_nav_ref_position(db_path, target_ref))
        .transpose()?;

    let mut candidates = branch
        .points
        .iter()
        .enumerate()
        .map(|(branch_point_index, point)| {
            let leg_offset_from_entry = branch_point_index as isize - entry_branch_point_index as isize;
            AirwayExitCandidate {
                airway_name: branch.display_name.clone(),
                branch_key: branch.branch_key.clone(),
                branch_point_index,
                sequence: point.sequence,
                nav_ref: point.nav_ref.clone(),
                leg_offset_from_entry,
                is_entry: branch_point_index == entry_branch_point_index,
                distance_from_target_nm: target_position.map(|position| distance_nm(position, point.position)),
            }
        })
        .collect::<Vec<_>>();

    let recommended_exit_branch_point_index = if target_position.is_some() {
        candidates
            .iter()
            .filter(|candidate| !candidate.is_entry)
            .min_by(|left, right| {
                left.distance_from_target_nm
                    .unwrap_or(f64::MAX)
                    .partial_cmp(&right.distance_from_target_nm.unwrap_or(f64::MAX))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        left.leg_offset_from_entry
                            .abs()
                            .cmp(&right.leg_offset_from_entry.abs())
                    })
            })
            .map(|candidate| candidate.branch_point_index)
    } else {
        None
    };

    candidates.sort_by_key(|candidate| candidate.branch_point_index);

    Ok(AirwayExitSelection {
        airway_name: branch.display_name,
        branch_key: branch.branch_key,
        entry_branch_point_index,
        recommended_exit_branch_point_index,
        candidates,
    })
}

pub fn choose_best_airway_plan(
    db_path: &Path,
    airway_name: &str,
    origin_anchor: &NavRef,
    destination_anchor: &NavRef,
) -> AppResult<AirwayAutoSelection> {
    let origin_position = resolve_nav_ref_position(db_path, origin_anchor)?;
    let destination_position = resolve_nav_ref_position(db_path, destination_anchor)?;
    let branches = load_airway_branches(db_path, airway_name)?;

    let mut best_selection = None::<AirwayAutoSelection>;

    for branch in branches {
        if branch.points.len() < 2 {
            continue;
        }

        let origin_distances = branch
            .points
            .iter()
            .map(|point| distance_nm(origin_position, point.position))
            .collect::<Vec<_>>();
        let destination_distances = branch
            .points
            .iter()
            .map(|point| distance_nm(destination_position, point.position))
            .collect::<Vec<_>>();

        let (best_index, second_best_index) = two_best_indexes(&origin_distances)
            .ok_or_else(|| AppError {
                kind: AppErrorKind::InvalidFlightPlan,
                message: format!("airway {} has no selectable entry points", airway_name.trim()),
            })?;

        for exit_index in 0..branch.points.len() {
            let entry_index = if best_index != exit_index {
                best_index
            } else if let Some(second_best_index) = second_best_index {
                second_best_index
            } else {
                continue;
            };

            let selection = AirwayAutoSelection {
                airway_name: branch.display_name.clone(),
                branch_key: branch.branch_key.clone(),
                entry: airway_entry_candidate(&branch, entry_index, origin_position),
                exit: airway_exit_candidate(&branch, entry_index, exit_index, Some(destination_position)),
                origin_distance_nm: origin_distances[entry_index],
                destination_distance_nm: destination_distances[exit_index],
                total_anchor_distance_nm: origin_distances[entry_index] + destination_distances[exit_index],
            };

            let replace = match &best_selection {
                None => true,
                Some(current) => compare_auto_selection(&selection, current).is_lt(),
            };
            if replace {
                best_selection = Some(selection);
            }
        }
    }

    best_selection.ok_or_else(|| AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!(
            "unable to choose airway entry and exit for {} from the provided anchors",
            airway_name.trim()
        ),
    })
}

pub fn select_airway_branch(
    db_path: &Path,
    airway_name: &str,
    entry: &NavRef,
    exit: &NavRef,
) -> AppResult<AirwayBranch> {
    let branches = load_airway_branches(db_path, airway_name)?;
    let matching = branches
        .into_iter()
        .filter(|branch| {
            branch.points.iter().any(|point| &point.nav_ref == entry)
                && branch.points.iter().any(|point| &point.nav_ref == exit)
        })
        .collect::<Vec<_>>();

    match matching.as_slice() {
        [branch] => Ok(branch.clone()),
        [] => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "no airway branch for {} contains both entry and exit",
                airway_name.trim()
            ),
        }),
        _ => Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!(
                "multiple airway branches for {} contain the selected entry and exit",
                airway_name.trim()
            ),
        }),
    }
}

pub fn materialize_airway_selection(
    db_path: &Path,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
    component_index: usize,
) -> AppResult<(AirwaySegment, Vec<ResolvedLeg>)> {
    if entry.airway_name != exit.airway_name {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "entry airway {} does not match exit airway {}",
                entry.airway_name.trim(),
                exit.airway_name.trim()
            ),
        });
    }

    if entry.branch_key != exit.branch_key {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "entry branch {} does not match exit branch {}",
                entry.branch_key.trim(),
                exit.branch_key.trim()
            ),
        });
    }

    resolve_airway_segment_by_index(
        db_path,
        &entry.airway_name,
        &entry.branch_key,
        entry.branch_point_index,
        exit.branch_point_index,
        component_index,
    )
}

pub fn resolve_airway_segment_by_index(
    db_path: &Path,
    airway_name: &str,
    branch_key: &str,
    entry_branch_point_index: usize,
    exit_branch_point_index: usize,
    component_index: usize,
) -> AppResult<(AirwaySegment, Vec<ResolvedLeg>)> {
    let branch = load_specific_airway_branch(db_path, airway_name, branch_key)?;

    let entry = branch
        .points
        .get(entry_branch_point_index)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "entry index {entry_branch_point_index} is out of bounds for airway {} branch {}",
                airway_name.trim(),
                branch_key.trim()
            ),
        })?;
    let exit = branch
        .points
        .get(exit_branch_point_index)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "exit index {exit_branch_point_index} is out of bounds for airway {} branch {}",
                airway_name.trim(),
                branch_key.trim()
            ),
        })?;

    if entry_branch_point_index == exit_branch_point_index {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "airway entry and exit cannot be the same point".to_string(),
        });
    }

    let slice = if entry_branch_point_index < exit_branch_point_index {
        &branch.points[entry_branch_point_index..=exit_branch_point_index]
    } else {
        &branch.points[exit_branch_point_index..=entry_branch_point_index]
    };

    let traversed = if entry_branch_point_index < exit_branch_point_index {
        slice.to_vec()
    } else {
        slice.iter().rev().cloned().collect::<Vec<_>>()
    };

    let legs = traversed
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
            name: branch.display_name,
            branch_key: Some(branch.branch_key),
            entry: entry.nav_ref.clone(),
            exit: exit.nav_ref.clone(),
        },
        legs,
    ))
}

pub fn resolve_airway_segment(
    db_path: &Path,
    airway_name: &str,
    entry: &NavRef,
    exit: &NavRef,
    component_index: usize,
) -> AppResult<(AirwaySegment, Vec<ResolvedLeg>)> {
    let branch = select_airway_branch(db_path, airway_name, entry, exit)?;
    let entry_index = branch
        .points
        .iter()
        .position(|point| &point.nav_ref == entry)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "selected airway entry point is not on branch".to_string(),
        })?;
    let exit_index = branch
        .points
        .iter()
        .position(|point| &point.nav_ref == exit)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "selected airway exit point is not on branch".to_string(),
        })?;

    resolve_airway_segment_by_index(
        db_path,
        &branch.display_name,
        &branch.branch_key,
        entry_index,
        exit_index,
        component_index,
    )
}

pub fn load_procedure_legs(
    db_path: &Path,
    key: &ProcedureVariantKey,
) -> AppResult<Vec<ProcedureLegRecord>> {
    with_connection(db_path, |connection| {
        let mut stmt = connection.prepare(
            "SELECT
                trim(airport_identifier),
                trim(sid_star_approach_identifier),
                trim(route_type),
                trim(transition_identifier),
                CAST(sequence_number AS INTEGER),
                trim(fix_identifier),
                trim(path_and_termination)
             FROM cifp_sid_star_app
             WHERE trim(airport_identifier) = trim(?1)
               AND trim(sid_star_approach_identifier) = trim(?2)
               AND trim(route_type) = trim(?3)
               AND trim(transition_identifier) = trim(?4)
             ORDER BY CAST(sequence_number AS INTEGER)",
        )?;

        let rows = stmt.query_map(
            params![
                key.airport_id,
                key.procedure_id,
                key.route_type,
                key.transition_id
            ],
            |row| {
                let route_type = row.get::<_, String>(2)?;
                let path_termination = row.get::<_, String>(6)?;
                Ok(ProcedureLegRecord {
                    key: ProcedureVariantKey {
                        airport_id: row.get::<_, String>(0)?,
                        procedure_id: row.get::<_, String>(1)?,
                        route_type: route_type.clone(),
                        transition_id: row.get::<_, String>(3)?,
                    },
                    sequence: row.get::<_, i32>(4)?,
                    fix_identifier: row.get::<_, String>(5)?,
                    path_termination_kind: interpret_path_termination(&path_termination),
                    path_termination,
                    inferred_kind: infer_procedure_kind(&route_type),
                })
            },
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })
}

pub fn load_resolved_procedure_legs(
    db_path: &Path,
    key: &ProcedureVariantKey,
    component_index: usize,
) -> AppResult<Vec<ResolvedLeg>> {
    let fixes: Vec<NavRef> = load_procedure_concretized_items(db_path, key)?
        .into_iter()
        .filter_map(|item| match item {
            ConcretizedNavItem::Waypoint { nav_ref } => Some(nav_ref),
            ConcretizedNavItem::Discontinuity { .. } => None,
        })
        .collect();

    Ok(fixes
        .windows(2)
        .enumerate()
        .map(|(index, pair)| ResolvedLeg {
            id: format!(
                "procedure-{}-{}-{}",
                key.procedure_id.trim(),
                key.route_type.trim(),
                index
            ),
            from: pair[0].clone(),
            to: pair[1].clone(),
            source: ResolvedLegSource::RouteComponent { component_index },
            procedure_provenance: None,
        })
        .collect())
}

pub fn list_procedures(
    db_path: &Path,
    airport_id: &str,
    kind: ProcedureKind,
) -> AppResult<Vec<ProcedureSummary>> {
    with_connection(db_path, |connection| {
        let mut stmt = connection.prepare(
            "SELECT DISTINCT
                trim(airport_identifier),
                trim(sid_star_approach_identifier),
                trim(route_type)
             FROM cifp_sid_star_app
             WHERE trim(airport_identifier) = trim(?1)
             ORDER BY trim(sid_star_approach_identifier), trim(route_type)",
        )?;

        let rows = stmt.query_map(params![airport_id], |row| {
            let route_type = row.get::<_, String>(2)?;
            Ok(ProcedureSummary {
                airport_id: row.get::<_, String>(0)?,
                procedure_id: row.get::<_, String>(1)?,
                kind: infer_procedure_kind(&route_type),
            })
        })?;

        let mut procedures = rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|procedure| procedure.kind == kind)
            .collect::<Vec<_>>();
        procedures.sort_by(|left, right| left.procedure_id.cmp(&right.procedure_id));
        procedures.dedup_by(|left, right| left.procedure_id == right.procedure_id && left.kind == right.kind);
        Ok(procedures)
    })
}

pub fn describe_procedure_options(
    db_path: &Path,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
) -> AppResult<ProcedureOptions> {
    let rows = load_distinct_procedure_rows(db_path, airport_id, procedure_id)?;
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
        runway_transitions
            .iter()
            .cloned()
            .map(Some)
            .collect::<Vec<_>>()
    };
    let enroute_choices = if enroute_transitions.is_empty() {
        vec![None]
    } else {
        enroute_transitions
            .iter()
            .cloned()
            .map(Some)
            .collect::<Vec<_>>()
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

pub fn materialize_procedure_selection(
    db_path: &Path,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
    component_index: usize,
) -> AppResult<MaterializedProcedure> {
    let options = describe_procedure_options(db_path, airport_id, procedure_id, kind.clone())?;
    let connection = Connection::open(db_path).map_err(sqlite_error)?;
    let requested = ProcedureSpecChoice {
        runway_transition: runway_transition.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string),
        enroute_transition: enroute_transition.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string),
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
        Vec<ProcedureLegRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )>::new();
    if kind == ProcedureKind::Approach {
        let rows = load_distinct_procedure_rows(db_path, airport_id, procedure_id)?;
        if let Some(enroute_transition) = requested.enroute_transition.as_deref() {
            let key = ProcedureVariantKey {
                airport_id: airport_id.trim().to_string(),
                procedure_id: procedure_id.trim().to_string(),
                route_type: "A".to_string(),
                transition_id: enroute_transition.to_string(),
            };
            let leg_records = load_procedure_legs(db_path, &key)?;
            let items = concretize_procedure_legs(&connection, &leg_records, false).map_err(sqlite_error)?;
            segments.push((MaterializedSegmentRole::EnrouteTransition, leg_records, items, false));
        }

        if let Some(common_route_type) = approach_common_route_type(&rows) {
            let key = ProcedureVariantKey {
                airport_id: airport_id.trim().to_string(),
                procedure_id: procedure_id.trim().to_string(),
                route_type: common_route_type,
                transition_id: "".to_string(),
            };
            let leg_records = load_procedure_legs(db_path, &key)?;
            let items = concretize_procedure_legs(&connection, &leg_records, false).map_err(sqlite_error)?;
            segments.push((MaterializedSegmentRole::Common, leg_records, items, false));
        }

        let concretized_items = merge_concretized_segments(
            segments
                .iter()
                .map(|(_, _, items, _)| items.clone())
                .collect::<Vec<_>>(),
        );
        let terminal_discontinuity = match concretized_items.last() {
            Some(ConcretizedNavItem::Discontinuity { discontinuity, .. }) => {
                Some(discontinuity.clone())
            }
            _ => None,
        };
        let resolved_legs = resolve_procedure_legs_with_provenance(
            &connection,
            airport_id,
            procedure_id,
            kind.clone(),
            component_index,
            &segments,
        )
        .map_err(sqlite_error)?;

        return Ok(MaterializedProcedure {
            procedure: crate::planning::ProcedureSegment {
                airport_id: crate::ids::AirportId(airport_id.trim().to_string()),
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
        let key = ProcedureVariantKey {
            airport_id: airport_id.trim().to_string(),
            procedure_id: procedure_id.trim().to_string(),
            route_type: layout.enroute_route_type.to_string(),
            transition_id: enroute_transition.to_string(),
        };
        let leg_records = load_procedure_legs(db_path, &key)?;
        let items = concretize_procedure_legs(&connection, &leg_records, layout.reverse_segment_order)
            .map_err(sqlite_error)?;
        segments.push((
            MaterializedSegmentRole::EnrouteTransition,
            leg_records,
            items,
            layout.reverse_segment_order,
        ));
    }

    if options.has_common_segment {
        let key = ProcedureVariantKey {
            airport_id: airport_id.trim().to_string(),
            procedure_id: procedure_id.trim().to_string(),
            route_type: layout.common_route_type.to_string(),
            transition_id: layout.common_transition_id.to_string(),
        };
        let leg_records = load_procedure_legs(db_path, &key)?;
        let items = concretize_procedure_legs(&connection, &leg_records, layout.reverse_segment_order)
            .map_err(sqlite_error)?;
        segments.push((
            MaterializedSegmentRole::Common,
            leg_records,
            items,
            layout.reverse_segment_order,
        ));
    }

    if let Some(runway_transition) = requested.runway_transition.as_deref() {
        let key = ProcedureVariantKey {
            airport_id: airport_id.trim().to_string(),
            procedure_id: procedure_id.trim().to_string(),
            route_type: layout.runway_route_type.to_string(),
            transition_id: runway_transition.to_string(),
        };
        let leg_records = load_procedure_legs(db_path, &key)?;
        let items = concretize_procedure_legs(&connection, &leg_records, layout.reverse_segment_order)
            .map_err(sqlite_error)?;
        segments.push((
            MaterializedSegmentRole::RunwayTransition,
            leg_records,
            items,
            layout.reverse_segment_order,
        ));
    }

    let concretized_items = merge_concretized_segments(
        segments
                .iter()
                .map(|(_, _, items, _)| items.clone())
                .collect::<Vec<_>>(),
        );
    let terminal_discontinuity = match concretized_items.last() {
        Some(ConcretizedNavItem::Discontinuity { discontinuity, .. }) => Some(discontinuity.clone()),
        _ => None,
    };
    let resolved_legs = resolve_procedure_legs_with_provenance(
        &connection,
        airport_id,
        procedure_id,
        kind.clone(),
        component_index,
        &segments,
    )
    .map_err(sqlite_error)?;

    Ok(MaterializedProcedure {
        procedure: crate::planning::ProcedureSegment {
            airport_id: crate::ids::AirportId(airport_id.trim().to_string()),
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

pub fn load_procedure_concretized_items(
    db_path: &Path,
    key: &ProcedureVariantKey,
) -> AppResult<Vec<ConcretizedNavItem>> {
    let legs = load_procedure_legs(db_path, key)?;
    with_connection(db_path, |connection| concretize_procedure_legs(connection, &legs, false))
}

fn resolve_airway_fix_point(
    connection: &Connection,
    point: AirwayPoint,
) -> rusqlite::Result<AirwayFixPoint> {
    let nav_ref = resolve_named_nav_ref(connection, point.position)?
        .unwrap_or(NavRef::LatLon(point.position));

    Ok(AirwayFixPoint {
        airway_name: point.airway_name,
        sequence: point.sequence,
        position: point.position,
        nav_ref,
    })
}

fn load_specific_airway_branch(
    db_path: &Path,
    airway_name: &str,
    branch_key: &str,
) -> AppResult<AirwayBranch> {
    let branch = load_airway_branches(db_path, airway_name)?
        .into_iter()
        .find(|branch| branch.branch_key == branch_key)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "airway {} does not contain branch {}",
                airway_name.trim(),
                branch_key.trim()
            ),
        })?;
    Ok(branch)
}

fn airway_entry_candidate(
    branch: &AirwayBranch,
    branch_point_index: usize,
    anchor_position: LatLon,
) -> AirwayEntryCandidate {
    let point = &branch.points[branch_point_index];
    AirwayEntryCandidate {
        airway_name: branch.display_name.clone(),
        branch_key: branch.branch_key.clone(),
        branch_point_index,
        sequence: point.sequence,
        nav_ref: point.nav_ref.clone(),
        distance_from_anchor_nm: distance_nm(anchor_position, point.position),
        previous_nav_ref: if branch_point_index > 0 {
            branch
                .points
                .get(branch_point_index - 1)
                .map(|previous| previous.nav_ref.clone())
        } else {
            None
        },
        next_nav_ref: branch
            .points
            .get(branch_point_index + 1)
            .map(|next| next.nav_ref.clone()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcedureLayout {
    runway_route_type: &'static str,
    common_route_type: &'static str,
    common_transition_id: &'static str,
    enroute_route_type: &'static str,
    reverse_segment_order: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MaterializedSegmentRole {
    EnrouteTransition,
    Common,
    RunwayTransition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcedureDistinctRow {
    route_type: String,
    transition_id: String,
}

fn procedure_layout(kind: ProcedureKind) -> ProcedureLayout {
    match kind {
        ProcedureKind::Sid => ProcedureLayout {
            runway_route_type: "6",
            common_route_type: "5",
            common_transition_id: "ALL",
            enroute_route_type: "4",
            reverse_segment_order: false,
        },
        ProcedureKind::Star => ProcedureLayout {
            runway_route_type: "1",
            common_route_type: "2",
            common_transition_id: "",
            enroute_route_type: "3",
            reverse_segment_order: true,
        },
        ProcedureKind::Approach => ProcedureLayout {
            runway_route_type: "A",
            common_route_type: "I",
            common_transition_id: "",
            enroute_route_type: "A",
            reverse_segment_order: true,
        },
    }
}

fn load_distinct_procedure_rows(
    db_path: &Path,
    airport_id: &str,
    procedure_id: &str,
) -> AppResult<Vec<ProcedureDistinctRow>> {
    with_connection(db_path, |connection| {
        let mut stmt = connection.prepare(
            "SELECT DISTINCT trim(route_type), trim(transition_identifier)
             FROM cifp_sid_star_app
             WHERE trim(airport_identifier) = trim(?1)
               AND trim(sid_star_approach_identifier) = trim(?2)
             ORDER BY trim(route_type), trim(transition_identifier)",
        )?;
        let rows = stmt.query_map(params![airport_id, procedure_id], |row| {
            Ok(ProcedureDistinctRow {
                route_type: row.get::<_, String>(0)?,
                transition_id: row.get::<_, String>(1)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })
}

fn approach_common_route_type(rows: &[ProcedureDistinctRow]) -> Option<String> {
    rows.iter()
        .find(|row| row.route_type != "A")
        .map(|row| row.route_type.clone())
}

fn resolve_procedure_legs_with_provenance(
    connection: &Connection,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    component_index: usize,
    segments: &[(
        MaterializedSegmentRole,
        Vec<ProcedureLegRecord>,
        Vec<ConcretizedNavItem>,
        bool,
    )],
) -> rusqlite::Result<Vec<ResolvedLeg>> {
    let mut resolved = Vec::<ResolvedLeg>::new();

    for (role, leg_records, _, reversed) in segments {
        let mut fix_records = leg_records
            .iter()
            .filter(|leg| !leg.fix_identifier.is_empty())
            .collect::<Vec<_>>();
        if *reversed {
            fix_records.reverse();
        }
        let role = procedure_segment_role(role);

        for pair in fix_records.windows(2) {
            let from = classify_procedure_identifier_in_db(connection, &pair[0].fix_identifier)?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            let to = classify_procedure_identifier_in_db(connection, &pair[1].fix_identifier)?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            let duplicate_of_previous = resolved
                .last()
                .is_some_and(|previous| previous.from == from && previous.to == to);
            if duplicate_of_previous {
                continue;
            }

            resolved.push(ResolvedLeg {
                id: format!("procedure-{}-{}-{}", procedure_id.trim(), pair[1].key.route_type.trim(), pair[1].sequence),
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

    Ok(resolved)
}

fn procedure_segment_role(role: &MaterializedSegmentRole) -> ProcedureSegmentRole {
    match role {
        MaterializedSegmentRole::EnrouteTransition => ProcedureSegmentRole::EnrouteTransition,
        MaterializedSegmentRole::Common => ProcedureSegmentRole::Common,
        MaterializedSegmentRole::RunwayTransition => ProcedureSegmentRole::RunwayTransition,
    }
}

fn concretize_procedure_legs(
    connection: &Connection,
    legs: &[ProcedureLegRecord],
    reverse_segment_order: bool,
) -> rusqlite::Result<Vec<ConcretizedNavItem>> {
    let mut waypoints = legs
        .iter()
        .filter_map(|leg| classify_procedure_identifier_in_db(connection, &leg.fix_identifier).transpose())
        .collect::<rusqlite::Result<Vec<_>>>()?;
    waypoints.dedup();

    let terminal_discontinuity = legs.last().and_then(terminal_procedure_discontinuity);
    let initial_discontinuity = legs.iter().take_while(|leg| leg.fix_identifier.is_empty()).last().and_then(leading_procedure_discontinuity);

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

    Ok(items)
}

fn merge_concretized_segments(segments: Vec<Vec<ConcretizedNavItem>>) -> Vec<ConcretizedNavItem> {
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

fn airway_exit_candidate(
    branch: &AirwayBranch,
    entry_branch_point_index: usize,
    exit_branch_point_index: usize,
    target_position: Option<LatLon>,
) -> AirwayExitCandidate {
    let point = &branch.points[exit_branch_point_index];
    AirwayExitCandidate {
        airway_name: branch.display_name.clone(),
        branch_key: branch.branch_key.clone(),
        branch_point_index: exit_branch_point_index,
        sequence: point.sequence,
        nav_ref: point.nav_ref.clone(),
        leg_offset_from_entry: exit_branch_point_index as isize - entry_branch_point_index as isize,
        is_entry: exit_branch_point_index == entry_branch_point_index,
        distance_from_target_nm: target_position.map(|position| distance_nm(position, point.position)),
    }
}

fn compare_auto_selection(
    left: &AirwayAutoSelection,
    right: &AirwayAutoSelection,
) -> std::cmp::Ordering {
    left.total_anchor_distance_nm
        .partial_cmp(&right.total_anchor_distance_nm)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| {
            left.destination_distance_nm
                .partial_cmp(&right.destination_distance_nm)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            left.origin_distance_nm
                .partial_cmp(&right.origin_distance_nm)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            left.exit
                .leg_offset_from_entry
                .abs()
                .cmp(&right.exit.leg_offset_from_entry.abs())
        })
}

fn two_best_indexes(values: &[f64]) -> Option<(usize, Option<usize>)> {
    if values.is_empty() {
        return None;
    }

    let mut best = 0usize;
    let mut second = None::<usize>;

    for index in 1..values.len() {
        let value = values[index];
        if value < values[best] {
            second = Some(best);
            best = index;
        } else if second.is_none_or(|second_index| value < values[second_index]) {
            second = Some(index);
        }
    }

    Some((best, second))
}

fn load_airway_branches_from_branch_table(
    connection: &Connection,
    airway_name: &str,
) -> rusqlite::Result<Option<Vec<AirwayBranch>>> {
    if !table_exists(connection, "airways_branch")? {
        return Ok(None);
    }

    let mut stmt = connection.prepare(
        "SELECT
            trim(name),
            trim(branch_key),
            sequence_number,
            trim(point_name),
            Latitude,
            Longitude
         FROM airways_branch
         WHERE trim(name) = trim(?1)
         ORDER BY trim(branch_key), sequence_number",
    )?;

    let rows = stmt
        .query_map(params![airway_name], |row| {
            let position = LatLon {
                lat: row.get::<_, f64>(4)?,
                lon: row.get::<_, f64>(5)?,
            };
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, String>(3)?,
                position,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if rows.is_empty() {
        return Ok(Some(Vec::new()));
    }

    let mut branches = Vec::new();
    let mut current_key = None::<String>;
    let mut current_points = Vec::<AirwayFixPoint>::new();
    let mut display_name = String::new();

    for (name, branch_key, sequence_number, point_name, position) in rows {
        if current_key.as_deref() != Some(branch_key.as_str()) && !current_points.is_empty() {
            branches.push(AirwayBranch {
                display_name: display_name.clone(),
                branch_key: current_key.take().unwrap_or_default(),
                points: std::mem::take(&mut current_points),
            });
        }

        if current_key.is_none() || current_key.as_deref() != Some(branch_key.as_str()) {
            display_name = name.clone();
            current_key = Some(branch_key.clone());
        }

        let nav_ref = resolve_named_nav_ref(connection, position)?
            .unwrap_or_else(|| fallback_nav_ref(&point_name, position));
        current_points.push(AirwayFixPoint {
            airway_name: name,
            sequence: sequence_number,
            position,
            nav_ref,
        });
    }

    if !current_points.is_empty() {
        branches.push(AirwayBranch {
            display_name,
            branch_key: current_key.unwrap_or_default(),
            points: current_points,
        });
    }

    Ok(Some(branches))
}

fn fallback_nav_ref(point_name: &str, position: LatLon) -> NavRef {
    let trimmed = point_name.trim();
    if trimmed.is_empty() {
        NavRef::LatLon(position)
    } else {
        NavRef::Fix(trimmed.to_string())
    }
}

fn query_airway_suggestions_from_branch_table(
    connection: &Connection,
    anchor_position: LatLon,
    radius_nm: f64,
    limit: usize,
) -> rusqlite::Result<Vec<AirwaySuggestion>> {
    let bounds = search_bounds(anchor_position, radius_nm);
    let sql_limit = i64::try_from(limit.saturating_mul(4).max(AIRWAY_POINT_QUERY_LIMIT))
        .unwrap_or(i64::MAX);
    let mut stmt = connection.prepare(
        "SELECT
            trim(name),
            trim(branch_key),
            sequence_number,
            trim(point_name),
            Latitude,
            Longitude
         FROM airways_branch
         WHERE Latitude BETWEEN ?1 AND ?2
           AND Longitude BETWEEN ?3 AND ?4
         ORDER BY ((Latitude - ?5) * (Latitude - ?5)) + ((Longitude - ?6) * (Longitude - ?6))
         LIMIT ?7",
    )?;

    let rows = stmt
        .query_map(
            params![
                bounds.min_lat,
                bounds.max_lat,
                bounds.min_lon,
                bounds.max_lon,
                anchor_position.lat,
                anchor_position.lon,
                sql_limit
            ],
            |row| {
                let position = LatLon {
                    lat: row.get::<_, f64>(4)?,
                    lon: row.get::<_, f64>(5)?,
                };
                let point_name = row.get::<_, String>(3)?;
                let nav_ref = resolve_named_nav_ref(connection, position)?
                    .unwrap_or_else(|| fallback_nav_ref(&point_name, position));
                Ok(AirwaySuggestion {
                    airway_name: row.get::<_, String>(0)?,
                    nearest_branch_key: Some(row.get::<_, String>(1)?),
                    nearest_sequence: row.get::<_, i32>(2)?,
                    nearest_nav_ref: nav_ref,
                    distance_from_anchor_nm: distance_nm(anchor_position, position),
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(dedupe_airway_suggestions(rows, limit))
}

fn query_airway_suggestions_from_legacy_table(
    connection: &Connection,
    anchor_position: LatLon,
    radius_nm: f64,
    limit: usize,
) -> rusqlite::Result<Vec<AirwaySuggestion>> {
    let bounds = search_bounds(anchor_position, radius_nm);
    let sql_limit = i64::try_from(limit.saturating_mul(8).max(AIRWAY_POINT_QUERY_LIMIT))
        .unwrap_or(i64::MAX);
    let mut stmt = connection.prepare(
        "SELECT
            trim(name),
            CAST(sequence AS INTEGER),
            Latitude,
            Longitude
         FROM airways
         WHERE Latitude BETWEEN ?1 AND ?2
           AND Longitude BETWEEN ?3 AND ?4
         ORDER BY ((Latitude - ?5) * (Latitude - ?5)) + ((Longitude - ?6) * (Longitude - ?6))
         LIMIT ?7",
    )?;

    let rows = stmt
        .query_map(
            params![
                bounds.min_lat,
                bounds.max_lat,
                bounds.min_lon,
                bounds.max_lon,
                anchor_position.lat,
                anchor_position.lon,
                sql_limit
            ],
            |row| {
                let position = LatLon {
                    lat: row.get::<_, f64>(2)?,
                    lon: row.get::<_, f64>(3)?,
                };
                let nav_ref = resolve_named_nav_ref(connection, position)?
                    .unwrap_or(NavRef::LatLon(position));
                Ok(AirwaySuggestion {
                    airway_name: row.get::<_, String>(0)?,
                    nearest_branch_key: None,
                    nearest_sequence: row.get::<_, i32>(1)?,
                    nearest_nav_ref: nav_ref,
                    distance_from_anchor_nm: distance_nm(anchor_position, position),
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(dedupe_airway_suggestions(rows, limit))
}

fn dedupe_airway_suggestions(
    mut suggestions: Vec<AirwaySuggestion>,
    limit: usize,
) -> Vec<AirwaySuggestion> {
    suggestions.sort_by(|left, right| {
        left.distance_from_anchor_nm
            .partial_cmp(&right.distance_from_anchor_nm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.airway_name.cmp(&right.airway_name))
            .then_with(|| left.nearest_sequence.cmp(&right.nearest_sequence))
    });

    let mut deduped = Vec::new();
    for suggestion in suggestions {
        if deduped
            .iter()
            .any(|existing: &AirwaySuggestion| existing.airway_name == suggestion.airway_name)
        {
            continue;
        }
        deduped.push(suggestion);
        if deduped.len() >= limit {
            break;
        }
    }
    deduped
}

fn search_bounds(anchor: LatLon, radius_nm: f64) -> SearchBounds {
    let lat_delta = radius_nm / 60.0;
    let cos_lat = anchor.lat.to_radians().cos().abs().max(0.1);
    let lon_delta = radius_nm / (60.0 * cos_lat);
    SearchBounds {
        min_lat: anchor.lat - lat_delta,
        max_lat: anchor.lat + lat_delta,
        min_lon: anchor.lon - lon_delta,
        max_lon: anchor.lon + lon_delta,
    }
}

#[derive(Debug, Clone, Copy)]
struct SearchBounds {
    min_lat: f64,
    max_lat: f64,
    min_lon: f64,
    max_lon: f64,
}

fn table_exists(connection: &Connection, table_name: &str) -> rusqlite::Result<bool> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
            params![table_name],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
}

fn resolve_named_nav_ref(
    connection: &Connection,
    position: LatLon,
) -> rusqlite::Result<Option<NavRef>> {
    for (table, variant) in [
        ("fix", 0usize),
        ("nav", 1usize),
        ("airports", 2usize),
    ] {
        let mut stmt = connection.prepare(&format!(
            "SELECT trim(LocationID) FROM {table}
             WHERE abs(ARPLatitude - ?1) < 1e-6
               AND abs(ARPLongitude - ?2) < 1e-6
             LIMIT 1"
        ))?;

        if let Some(id) = stmt
            .query_row(params![position.lat, position.lon], |row| row.get::<_, String>(0))
            .optional()?
        {
            return Ok(Some(match variant {
                0 => NavRef::Fix(id),
                1 => NavRef::Navaid(id),
                _ => NavRef::Airport(id),
            }));
        }
    }

    Ok(None)
}

fn resolve_nav_ref_position_in_db(
    connection: &Connection,
    nav_ref: &NavRef,
) -> rusqlite::Result<LatLon> {
    match nav_ref {
        NavRef::LatLon(position) => Ok(*position),
        NavRef::Airport(code) => lookup_nav_ref_position(connection, "airports", code),
        NavRef::Navaid(code) => lookup_nav_ref_position(connection, "nav", code),
        NavRef::Fix(code) => lookup_nav_ref_position(connection, "fix", code),
    }
}

fn classify_procedure_identifier_in_db(
    connection: &Connection,
    identifier: &str,
) -> rusqlite::Result<Option<NavRef>> {
    let trimmed = identifier.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.starts_with("RW") || trimmed.starts_with("rw") {
        return Ok(Some(NavRef::Fix(trimmed.to_string())));
    }
    if connection
        .query_row(
            "SELECT LocationID FROM airports WHERE trim(LocationID) = trim(?1) LIMIT 1",
            params![trimmed],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some()
    {
        return Ok(Some(NavRef::Airport(trimmed.to_string())));
    }
    if connection
        .query_row(
            "SELECT LocationID FROM nav WHERE trim(LocationID) = trim(?1) LIMIT 1",
            params![trimmed],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some()
    {
        return Ok(Some(NavRef::Navaid(trimmed.to_string())));
    }
    if connection
        .query_row(
            "SELECT LocationID FROM fix WHERE trim(LocationID) = trim(?1) LIMIT 1",
            params![trimmed],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some()
    {
        return Ok(Some(NavRef::Fix(trimmed.to_string())));
    }
    Ok(None)
}

fn lookup_nav_ref_position(
    connection: &Connection,
    table: &str,
    code: &str,
) -> rusqlite::Result<LatLon> {
    connection.query_row(
        &format!(
            "SELECT ARPLatitude, ARPLongitude FROM {table}
             WHERE trim(LocationID) = trim(?1)
             LIMIT 1"
        ),
        params![code],
        |row| {
            Ok(LatLon {
                lat: row.get::<_, f64>(0)?,
                lon: row.get::<_, f64>(1)?,
            })
        },
    )
}

fn resolve_runway_fix_position_in_db(
    connection: &Connection,
    runway_fix: &str,
    airport_id: &str,
) -> rusqlite::Result<LatLon> {
    let runway_ident = runway_fix.trim().trim_start_matches("RW").trim_start_matches("rw");
    connection.query_row(
        "
        SELECT
          CASE
            WHEN trim(LEIdent) = trim(?2) THEN CAST(LELatitude AS REAL)
            ELSE CAST(HELatitude AS REAL)
          END AS lat,
          CASE
            WHEN trim(LEIdent) = trim(?2) THEN CAST(LELongitude AS REAL)
            ELSE CAST(HELongitude AS REAL)
          END AS lon
        FROM airportrunways
        WHERE trim(LocationID) = trim(?1)
          AND (trim(LEIdent) = trim(?2) OR trim(HEIdent) = trim(?2))
        LIMIT 1
        ",
        params![airport_id, runway_ident],
        |row| {
            Ok(LatLon {
                lat: row.get::<_, f64>(0)?,
                lon: row.get::<_, f64>(1)?,
            })
        },
    )
}

fn split_airway_branches(points: Vec<AirwayFixPoint>) -> Vec<AirwayBranch> {
    let mut branches: Vec<Vec<AirwayFixPoint>> = Vec::new();

    for point in points {
        let mut assigned_branch = None;
        let mut best_distance = f64::MAX;

        for (branch_index, branch) in branches.iter().enumerate() {
            let Some(last) = branch.last() else {
                continue;
            };

            if point.sequence < last.sequence {
                continue;
            }

            let distance = distance_nm(last.position, point.position);
            if distance <= MAX_AIRWAY_BRANCH_HOP_NM && distance < best_distance {
                assigned_branch = Some(branch_index);
                best_distance = distance;
            }
        }

        if let Some(branch_index) = assigned_branch {
            branches[branch_index].push(point);
        } else {
            branches.push(vec![point]);
        }
    }

    branches
        .into_iter()
        .enumerate()
        .map(|(index, points)| AirwayBranch {
            display_name: points
                .first()
                .map(|point| point.airway_name.clone())
                .unwrap_or_default(),
            branch_key: format!(
                "{}-{}",
                points
                    .first()
                    .map(|point| point.airway_name.as_str())
                    .unwrap_or("AWY"),
                branch_suffix(index)
            ),
            points,
        })
        .collect()
}

fn distance_nm(a: LatLon, b: LatLon) -> f64 {
    let earth_radius_nm = 3440.065_f64;
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * earth_radius_nm * h.sqrt().asin()
}

fn branch_suffix(index: usize) -> char {
    (b'A' + (index as u8)) as char
}

fn with_connection<T>(
    db_path: &Path,
    f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
) -> AppResult<T> {
    let connection = Connection::open(db_path).map_err(sqlite_error)?;
    f(&connection).map_err(sqlite_error)
}

fn sqlite_error(err: rusqlite::Error) -> AppError {
    AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!("nav database query failed: {err}"),
    }
}

fn infer_procedure_kind(route_type: &str) -> ProcedureKind {
    match route_type.trim() {
        "1" | "2" | "3" => ProcedureKind::Star,
        "4" | "5" | "6" => ProcedureKind::Sid,
        _ => ProcedureKind::Approach,
    }
}

fn terminal_procedure_discontinuity(leg: &ProcedureLegRecord) -> Option<ProcedureDiscontinuity> {
    match leg.path_termination.trim() {
        "FM" => Some(ProcedureDiscontinuity::Vectors),
        "HM" => Some(ProcedureDiscontinuity::Hold),
        "VA" | "VI" if leg.fix_identifier.is_empty() => Some(ProcedureDiscontinuity::Vectors),
        _ => None,
    }
}

fn leading_procedure_discontinuity(leg: &ProcedureLegRecord) -> Option<ProcedureDiscontinuity> {
    match leg.path_termination.trim() {
        "FM" => Some(ProcedureDiscontinuity::Vectors),
        "HM" => Some(ProcedureDiscontinuity::Hold),
        "VA" | "VI" if leg.fix_identifier.is_empty() => Some(ProcedureDiscontinuity::Vectors),
        _ => None,
    }
}

trait OptionalRow<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalRow<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::OnceLock;

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

    #[test]
    fn loads_airway_points_from_real_nav_db() {
        let points = load_airway_points(fixture_db_path(), "V16").unwrap();

        assert!(points.len() > 50);
        assert_eq!(points[0].airway_name, "V16");
        assert!(points.windows(2).all(|pair| pair[0].sequence <= pair[1].sequence));
        assert!(points.windows(2).any(|pair| pair[0].sequence == pair[1].sequence));
    }

    #[test]
    fn splits_v16_into_multiple_internal_branches() {
        let branches = load_airway_branches(fixture_db_path(), "V16").unwrap();

        assert!(branches.len() >= 2);
        assert_eq!(branches[0].display_name, "V16");
        assert_eq!(branches[0].branch_key, "V16-A");
        assert!(branches.iter().all(|branch| branch.points.len() > 10));
    }

    #[test]
    fn resolves_named_airway_branch_from_entry_and_exit() {
        let (segment, legs) = resolve_airway_segment(
            fixture_db_path(),
            "V16",
            &NavRef::Navaid("LAX".to_string()),
            &NavRef::Navaid("PDZ".to_string()),
            1,
        )
        .unwrap();

        assert_eq!(segment.name, "V16");
        assert_eq!(segment.branch_key.as_deref(), Some("V16-A"));
        assert!(!legs.is_empty());
        assert_eq!(legs.first().unwrap().from, NavRef::Navaid("LAX".to_string()));
        assert_eq!(legs.last().unwrap().to, NavRef::Navaid("PDZ".to_string()));
    }

    #[test]
    fn suggests_nearby_airways_for_krnt_in_distance_order() {
        let suggestions =
            suggest_airways_near(fixture_db_path(), &NavRef::Airport("KRNT".to_string()), 5)
                .unwrap();

        assert_eq!(suggestions.len(), 5);
        assert!(suggestions.windows(2).all(|pair| {
            pair[0].distance_from_anchor_nm <= pair[1].distance_from_anchor_nm
        }));
        assert!(suggestions
            .iter()
            .all(|suggestion| matches!(suggestion.nearest_nav_ref, NavRef::Airport(_) | NavRef::Navaid(_) | NavRef::Fix(_))));
    }

    #[test]
    fn ranks_v2_entry_candidates_by_proximity_to_krnt() {
        let entries = list_airway_entry_candidates(
            fixture_db_path(),
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            5,
        )
        .unwrap();

        assert!(!entries.is_empty());
        assert_eq!(entries[0].airway_name, "V2");
        assert_eq!(entries[0].nav_ref, NavRef::Navaid("SEA".to_string()));
        assert_eq!(entries[0].branch_key, "V2-A");
    }

    #[test]
    fn orders_exit_candidates_along_branch_and_recommends_near_kuao() {
        let entry = list_airway_entry_candidates(
            fixture_db_path(),
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            1,
        )
        .unwrap()
        .remove(0);

        let exits = list_airway_exit_candidates(
            fixture_db_path(),
            "V2",
            &entry.branch_key,
            entry.branch_point_index,
            Some(&NavRef::Airport("KUAO".to_string())),
        )
        .unwrap();

        assert_eq!(exits.airway_name, "V2");
        assert_eq!(exits.branch_key, "V2-A");
        assert_eq!(exits.entry_branch_point_index, entry.branch_point_index);
        assert_eq!(
            exits.candidates[exits.entry_branch_point_index].nav_ref,
            NavRef::Navaid("SEA".to_string())
        );
        let recommended = exits
            .recommended_exit_branch_point_index
            .and_then(|index| exits.candidates.get(index))
            .expect("recommended exit");
        assert_eq!(recommended.nav_ref, NavRef::Fix("VAMPS".to_string()));
    }

    #[test]
    fn materializes_selected_airway_candidates_into_component_and_legs() {
        let entry = list_airway_entry_candidates(
            fixture_db_path(),
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            1,
        )
        .unwrap()
        .remove(0);
        let exits = list_airway_exit_candidates(
            fixture_db_path(),
            "V2",
            &entry.branch_key,
            entry.branch_point_index,
            Some(&NavRef::Airport("KUAO".to_string())),
        )
        .unwrap();
        let exit = exits.candidates[exits.recommended_exit_branch_point_index.unwrap()].clone();

        let (segment, legs) = materialize_airway_selection(fixture_db_path(), &entry, &exit, 2).unwrap();

        assert_eq!(segment.name, "V2");
        assert_eq!(segment.branch_key.as_deref(), Some("V2-A"));
        assert_eq!(segment.entry, entry.nav_ref);
        assert_eq!(segment.exit, exit.nav_ref);
        assert!(!legs.is_empty());
        assert_eq!(legs.first().unwrap().from, segment.entry);
        assert_eq!(legs.last().unwrap().to, segment.exit);
    }

    #[test]
    fn chooses_best_airway_plan_for_krnt_v2_kuao() {
        let selection = choose_best_airway_plan(
            fixture_db_path(),
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            &NavRef::Airport("KUAO".to_string()),
        )
        .unwrap();

        assert_eq!(selection.airway_name, "V2");
        assert_eq!(selection.branch_key, "V2-A");
        assert_eq!(selection.entry.nav_ref, NavRef::Navaid("SEA".to_string()));
        assert_eq!(selection.exit.nav_ref, NavRef::Fix("VAMPS".to_string()));
        assert!(!selection.exit.is_entry);
    }

    #[test]
    fn chooses_reverse_airway_plan_when_destination_is_near_origin_side() {
        let selection = choose_best_airway_plan(
            fixture_db_path(),
            "V2",
            &NavRef::Airport("KUAO".to_string()),
            &NavRef::Airport("KRNT".to_string()),
        )
        .unwrap();

        assert_eq!(selection.entry.nav_ref, NavRef::Fix("VAMPS".to_string()));
        assert_eq!(selection.exit.nav_ref, NavRef::Navaid("SEA".to_string()));
        assert!(selection.exit.leg_offset_from_entry < 0);
    }

    #[test]
    fn choose_best_airway_plan_rejects_unknown_airway() {
        let err = choose_best_airway_plan(
            fixture_db_path(),
            "ZZ999",
            &NavRef::Airport("KRNT".to_string()),
            &NavRef::Airport("KUAO".to_string()),
        )
        .unwrap_err();

        assert_eq!(err.kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn materialize_airway_selection_rejects_branch_mismatch() {
        let mut entry = list_airway_entry_candidates(
            fixture_db_path(),
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            1,
        )
        .unwrap()
        .remove(0);
        let exit = AirwayExitCandidate {
            airway_name: "V2".to_string(),
            branch_key: "V2-B".to_string(),
            branch_point_index: 10,
            sequence: 110,
            nav_ref: NavRef::Fix("FAKE".to_string()),
            leg_offset_from_entry: 10,
            is_entry: false,
            distance_from_target_nm: Some(10.0),
        };

        let err = materialize_airway_selection(fixture_db_path(), &entry, &exit, 2).unwrap_err();
        assert_eq!(err.kind, AppErrorKind::InvalidFlightPlan);

        entry.branch_key = "V2-A".to_string();
    }

    #[test]
    fn resolve_airway_segment_by_index_rejects_same_point() {
        let entry = list_airway_entry_candidates(
            fixture_db_path(),
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            1,
        )
        .unwrap()
        .remove(0);

        let err = resolve_airway_segment_by_index(
            fixture_db_path(),
            "V2",
            &entry.branch_key,
            entry.branch_point_index,
            entry.branch_point_index,
            0,
        )
        .unwrap_err();
        assert_eq!(err.kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn resolve_airway_segment_by_index_supports_reverse_traversal() {
        let branch = load_specific_airway_branch(fixture_db_path(), "V2", "V2-A").unwrap();
        let exit_index = branch
            .points
            .iter()
            .position(|point| point.nav_ref == NavRef::Navaid("SEA".to_string()))
            .unwrap();
        let entry_index = branch
            .points
            .iter()
            .position(|point| point.nav_ref == NavRef::Fix("VAMPS".to_string()))
            .unwrap();

        let (segment, legs) = resolve_airway_segment_by_index(
            fixture_db_path(),
            "V2",
            "V2-A",
            entry_index,
            exit_index,
            4,
        )
        .unwrap();

        assert_eq!(segment.entry, NavRef::Fix("VAMPS".to_string()));
        assert_eq!(segment.exit, NavRef::Navaid("SEA".to_string()));
        assert_eq!(legs.first().unwrap().from, segment.entry);
        assert_eq!(legs.last().unwrap().to, segment.exit);
    }

    #[test]
    fn resolve_airway_segment_by_index_rejects_out_of_bounds_index() {
        let err = resolve_airway_segment_by_index(
            fixture_db_path(),
            "V2",
            "V2-A",
            999,
            1000,
            0,
        )
        .unwrap_err();
        assert_eq!(err.kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn loads_sid_like_procedure_variant_from_real_nav_db() {
        let legs = load_procedure_legs(
            fixture_db_path(),
            &ProcedureVariantKey {
                airport_id: "KBOS".to_string(),
                procedure_id: "BLZZR6".to_string(),
                route_type: "4".to_string(),
                transition_id: "RW04R".to_string(),
            },
        )
        .unwrap();

        assert!(!legs.is_empty());
        assert_eq!(legs[0].inferred_kind, ProcedureKind::Sid);
        assert_eq!(legs[0].path_termination_kind, PathTermination::HeadingToAltitude);
        assert_eq!(legs[1].fix_identifier, "NHANT");
    }

    #[test]
    fn loads_approach_like_procedure_variant_from_real_nav_db() {
        let legs = load_procedure_legs(
            fixture_db_path(),
            &ProcedureVariantKey {
                airport_id: "KBOS".to_string(),
                procedure_id: "I04R".to_string(),
                route_type: "I".to_string(),
                transition_id: "".to_string(),
            },
        )
        .unwrap();

        assert!(!legs.is_empty());
        assert_eq!(legs[0].inferred_kind, ProcedureKind::Approach);
        assert_eq!(legs[0].fix_identifier, "WINNI");
        assert_eq!(legs[0].path_termination_kind, PathTermination::InitialFix);
    }

    #[test]
    fn resolves_procedure_rows_into_fix_to_fix_legs() {
        let legs = load_resolved_procedure_legs(
            fixture_db_path(),
            &ProcedureVariantKey {
                airport_id: "KBOS".to_string(),
                procedure_id: "I04R".to_string(),
                route_type: "I".to_string(),
                transition_id: "".to_string(),
            },
            3,
        )
        .unwrap();

        assert!(!legs.is_empty());
        assert_eq!(legs[0].from, NavRef::Fix("WINNI".to_string()));
        assert_eq!(legs[0].to, NavRef::Fix("NABBO".to_string()));
        assert_eq!(
            legs[0].source,
            ResolvedLegSource::RouteComponent { component_index: 3 }
        );
    }

    #[test]
    fn concretizes_terminal_manual_procedure_with_explicit_vectors_gap() {
        let items = load_procedure_concretized_items(
            fixture_db_path(),
            &ProcedureVariantKey {
                airport_id: "CYQG".to_string(),
                procedure_id: "AUTTO1".to_string(),
                route_type: "5".to_string(),
                transition_id: "ALL".to_string(),
            },
        )
        .unwrap();

        assert_eq!(
            items,
            vec![
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("AXXIS".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("AUTTO".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("GIGGY".to_string())
                },
                ConcretizedNavItem::Discontinuity {
                    discontinuity: ProcedureDiscontinuity::Vectors,
                    label: "VECTORS".to_string()
                },
            ]
        );
    }

    #[test]
    fn heading_based_start_that_resolves_to_fixes_does_not_emit_terminal_gap() {
        let items = load_procedure_concretized_items(
            fixture_db_path(),
            &ProcedureVariantKey {
                airport_id: "KBOS".to_string(),
                procedure_id: "BLZZR6".to_string(),
                route_type: "4".to_string(),
                transition_id: "RW04R".to_string(),
            },
        )
        .unwrap();

        assert_eq!(
            items.first(),
            Some(&ConcretizedNavItem::Waypoint {
                nav_ref: NavRef::Fix("NHANT".to_string())
            })
        );
        assert_eq!(
            items.last(),
            Some(&ConcretizedNavItem::Waypoint {
                nav_ref: NavRef::Fix("BLZZR".to_string())
            })
        );
        assert!(items.iter().all(|item| matches!(item, ConcretizedNavItem::Waypoint { .. })));
    }

    #[test]
    fn resolved_procedure_legs_do_not_invent_a_leg_past_terminal_vectors_gap() {
        let legs = load_resolved_procedure_legs(
            fixture_db_path(),
            &ProcedureVariantKey {
                airport_id: "CYQG".to_string(),
                procedure_id: "AUTTO1".to_string(),
                route_type: "5".to_string(),
                transition_id: "ALL".to_string(),
            },
            7,
        )
        .unwrap();

        assert_eq!(legs.len(), 2);
        assert_eq!(legs[0].from, NavRef::Fix("AXXIS".to_string()));
        assert_eq!(legs[0].to, NavRef::Fix("AUTTO".to_string()));
        assert_eq!(legs[1].from, NavRef::Fix("AUTTO".to_string()));
        assert_eq!(legs[1].to, NavRef::Fix("GIGGY".to_string()));
    }

    #[test]
    fn lists_sid_procedures_for_airport() {
        let procedures = list_procedures(fixture_db_path(), "CYQG", ProcedureKind::Sid).unwrap();

        assert!(procedures.iter().any(|procedure| procedure.procedure_id == "AUTTO1"));
        assert!(procedures.iter().all(|procedure| procedure.kind == ProcedureKind::Sid));
    }

    #[test]
    fn describes_sid_transition_options_from_real_nav_db() {
        let options =
            describe_procedure_options(fixture_db_path(), "CYQG", "AUTTO1", ProcedureKind::Sid)
                .unwrap();

        assert_eq!(options.enroute_transitions, vec!["COLTS".to_string(), "PICUP".to_string()]);
        assert!(options.runway_transitions.is_empty());
        assert!(options.has_common_segment);
        assert_eq!(
            options.valid_choices,
            vec![
                ProcedureSpecChoice {
                    runway_transition: None,
                    enroute_transition: Some("COLTS".to_string())
                },
                ProcedureSpecChoice {
                    runway_transition: None,
                    enroute_transition: Some("PICUP".to_string())
                },
            ]
        );
    }

    #[test]
    fn materializes_sid_selection_into_atomic_procedure_spec_and_concretized_route() {
        let built = materialize_procedure_selection(
            fixture_db_path(),
            "CYQG",
            "AUTTO1",
            ProcedureKind::Sid,
            None,
            Some("COLTS"),
            4,
        )
        .unwrap();

        assert_eq!(built.procedure.procedure_id, "AUTTO1");
        assert_eq!(built.procedure.enroute_transition.as_deref(), Some("COLTS"));
        assert_eq!(
            built.procedure.terminal_discontinuity,
            Some(ProcedureDiscontinuity::Vectors)
        );
        assert_eq!(
            built.concretized_items,
            vec![
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("COLTS".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("BOREK".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("LOPVO".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("ITPEG".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("PICUP".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("AXXIS".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("AUTTO".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("GIGGY".to_string())
                },
                ConcretizedNavItem::Discontinuity {
                    discontinuity: ProcedureDiscontinuity::Vectors,
                    label: "VECTORS".to_string()
                },
            ]
        );
        assert_eq!(built.resolved_legs.first().unwrap().from, NavRef::Fix("COLTS".to_string()));
        assert_eq!(built.resolved_legs.last().unwrap().to, NavRef::Fix("GIGGY".to_string()));
        let provenance = built.resolved_legs[0].procedure_provenance.as_ref().unwrap();
        assert_eq!(provenance.airport_id, "CYQG");
        assert_eq!(provenance.procedure_id, "AUTTO1");
        assert_eq!(provenance.kind, ProcedureKind::Sid);
        assert_eq!(provenance.role, ProcedureSegmentRole::EnrouteTransition);
        assert_eq!(provenance.path_termination, PathTermination::TrackToFix);
        assert_eq!(provenance.leg_sequence, 20);
    }

    #[test]
    fn rejects_invalid_sid_transition_combination() {
        let err = materialize_procedure_selection(
            fixture_db_path(),
            "CYQG",
            "AUTTO1",
            ProcedureKind::Sid,
            Some("RW04R"),
            Some("COLTS"),
            4,
        )
        .unwrap_err();

        assert_eq!(err.kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn describes_star_transition_options_from_real_nav_db() {
        let options =
            describe_procedure_options(fixture_db_path(), "47N", "CENTR1", ProcedureKind::Star)
                .unwrap();

        assert_eq!(options.runway_transitions, vec!["RW07".to_string(), "RW25".to_string()]);
        assert!(options.enroute_transitions.is_empty());
        assert!(options.has_common_segment);
    }

    #[test]
    fn describes_approach_transition_options_without_fake_runway_dimension() {
        let options =
            describe_procedure_options(fixture_db_path(), "KBOS", "I04R", ProcedureKind::Approach)
                .unwrap();

        assert!(options.runway_transitions.is_empty());
        assert_eq!(options.enroute_transitions, vec!["GOSHI".to_string()]);
        assert!(options.has_common_segment);
        assert_eq!(
            options.valid_choices,
            vec![ProcedureSpecChoice {
                runway_transition: None,
                enroute_transition: Some("GOSHI".to_string())
            }]
        );
    }

    #[test]
    fn materializes_star_selection_in_arrival_order_with_terminal_vectors_gap() {
        let built = materialize_procedure_selection(
            fixture_db_path(),
            "47N",
            "CENTR1",
            ProcedureKind::Star,
            Some("RW07"),
            None,
            5,
        )
        .unwrap();

        assert_eq!(built.procedure.runway_transition.as_deref(), Some("RW07"));
        assert_eq!(
            built.procedure.terminal_discontinuity,
            Some(ProcedureDiscontinuity::Vectors)
        );
        assert_eq!(
            built.concretized_items,
            vec![
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Navaid("ARD".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("DYLIN".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("METRO".to_string())
                },
                ConcretizedNavItem::Discontinuity {
                    discontinuity: ProcedureDiscontinuity::Vectors,
                    label: "VECTORS".to_string()
                },
            ]
        );
        assert_eq!(built.resolved_legs.len(), 2);
        assert_eq!(built.resolved_legs[0].from, NavRef::Navaid("ARD".to_string()));
        assert_eq!(built.resolved_legs[1].to, NavRef::Fix("METRO".to_string()));
        let provenance = built.resolved_legs[0].procedure_provenance.as_ref().unwrap();
        assert_eq!(provenance.role, ProcedureSegmentRole::Common);
        assert_eq!(provenance.path_termination, PathTermination::TrackToFix);
    }

    #[test]
    fn materializes_ils_approach_in_flying_order_with_hold_discontinuity() {
        let built = materialize_procedure_selection(
            fixture_db_path(),
            "KBOS",
            "I04R",
            ProcedureKind::Approach,
            None,
            Some("GOSHI"),
            6,
        )
        .unwrap();

        assert!(built.procedure.runway_transition.is_none());
        assert_eq!(built.procedure.enroute_transition.as_deref(), Some("GOSHI"));
        assert_eq!(
            built.procedure.terminal_discontinuity,
            Some(ProcedureDiscontinuity::Hold)
        );
        assert_eq!(
            built.concretized_items,
            vec![
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("GOSHI".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("WINNI".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("NABBO".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("MILTT".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("RW04R".to_string())
                },
                ConcretizedNavItem::Waypoint {
                    nav_ref: NavRef::Fix("WAXEN".to_string())
                },
                ConcretizedNavItem::Discontinuity {
                    discontinuity: ProcedureDiscontinuity::Hold,
                    label: "HOLD".to_string()
                },
            ]
        );
        assert_eq!(built.resolved_legs.first().unwrap().from, NavRef::Fix("GOSHI".to_string()));
        assert_eq!(built.resolved_legs.last().unwrap().to, NavRef::Fix("WAXEN".to_string()));
    }

    #[test]
    fn materializes_rnav_approach_using_dynamic_common_route_type() {
        let built = materialize_procedure_selection(
            fixture_db_path(),
            "KBOS",
            "R04R",
            ProcedureKind::Approach,
            None,
            Some("GOSHI"),
            6,
        )
        .unwrap();

        assert_eq!(
            built.procedure.terminal_discontinuity,
            Some(ProcedureDiscontinuity::Hold)
        );
        assert_eq!(
            built.concretized_items.first(),
            Some(&ConcretizedNavItem::Waypoint {
                nav_ref: NavRef::Fix("GOSHI".to_string())
            })
        );
        assert_eq!(
            built.concretized_items.last(),
            Some(&ConcretizedNavItem::Discontinuity {
                discontinuity: ProcedureDiscontinuity::Hold,
                label: "HOLD".to_string()
            })
        );
        assert!(built
            .concretized_items
            .contains(&ConcretizedNavItem::Waypoint { nav_ref: NavRef::Fix("OMVOZ".to_string()) }));
    }

    #[test]
    fn materializes_tayto_approach_with_navaid_rdd() {
        let built = materialize_procedure_selection(
            fixture_db_path(),
            "KRDD",
            "I34",
            ProcedureKind::Approach,
            None,
            Some("TAYTO"),
            0,
        )
        .unwrap();

        assert!(built.resolved_legs.iter().any(|leg| {
            leg.from == NavRef::Fix("TAYTO".to_string())
                && leg.to == NavRef::Navaid("RDD".to_string())
        }));
        assert!(built.resolved_legs.iter().any(|leg| {
            leg.from == NavRef::Navaid("RDD".to_string())
                && leg.to == NavRef::Fix("LASSN".to_string())
        }));
    }
}
