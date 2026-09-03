// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, HashMap};

use crate::planning::{
    display_row_targets_guidance_leg, guidance_detail_ref_by_index, guidance_projects_active_leg,
    project_identity_rows, terminal_hold_start_element_index_for_leg, DirectToTargetRow,
    FlightPlan, FlightPlanRowId, GuidanceLegId, NavRef, SequencingMode,
};
use crate::{
    AppError, AppErrorKind, AppResult, FlightDataCellTone, FlightPlanRouteSegmentStatus,
    GuidanceLegGeometry, LatLon,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MaterializedFlightPlanRowArrow {
    None,
    Leg,
    DirectTo,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MaterializedFlightPlanRow {
    pub id: FlightPlanRowId,
    pub guidance_leg_id: Option<GuidanceLegId>,
    pub location: Option<NavRef>,
    pub leg_index: Option<usize>,
    pub tone: FlightDataCellTone,
    pub arrow: MaterializedFlightPlanRowArrow,
    pub geometry: Option<GuidanceLegGeometry>,
    pub estimate_geometry: Option<GuidanceLegGeometry>,
    pub distance_remaining_nm: Option<f64>,
    pub cumulative_distance_remaining_nm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MaterializedActiveLeg {
    pub row_id: FlightPlanRowId,
    pub summary: String,
    pub geometry: Option<GuidanceLegGeometry>,
    pub distance_remaining_nm: Option<f64>,
}

#[derive(Debug, Clone)]
pub(crate) struct MaterializedFlightPlan {
    pub order: Vec<FlightPlanRowId>,
    pub rows: BTreeMap<FlightPlanRowId, MaterializedFlightPlanRow>,
    pub active: Option<MaterializedActiveLeg>,
    pub total_distance_remaining_nm: Option<f64>,
}

impl MaterializedFlightPlan {
    pub fn build(
        plan: &FlightPlan,
        geometry_by_id: &HashMap<String, GuidanceLegGeometry>,
        ownship_position: Option<LatLon>,
    ) -> AppResult<Self> {
        let estimate_geometry_by_guidance_leg_id = project_identity_rows(plan)
            .into_iter()
            .filter(|row| display_row_targets_guidance_leg(plan, row))
            .filter_map(|row| {
                let leg_index = row.leg_index?;
                let leg = plan.resolved_legs.get(leg_index)?;
                if crate::planning::resolved_leg_ends_in_manual_sequence(leg) {
                    return None;
                }
                geometry_for_resolved_leg(plan, leg_index, geometry_by_id).map(|geometry| {
                    (
                        GuidanceLegId::for_destination_row(&FlightPlanRowId(row.uid)),
                        geometry,
                    )
                })
            })
            .collect();
        Self::build_with_estimate_geometries(
            plan,
            geometry_by_id,
            &estimate_geometry_by_guidance_leg_id,
            ownship_position,
        )
    }

    pub fn build_with_estimate_geometries(
        plan: &FlightPlan,
        geometry_by_id: &HashMap<String, GuidanceLegGeometry>,
        estimate_geometry_by_guidance_leg_id: &HashMap<GuidanceLegId, GuidanceLegGeometry>,
        ownship_position: Option<LatLon>,
    ) -> AppResult<Self> {
        let identity_rows = project_identity_rows(plan);
        let mut order = Vec::new();
        let mut rows = BTreeMap::new();
        let mut row_id_by_leg_index = BTreeMap::new();

        for row in identity_rows
            .into_iter()
            .filter(|row| display_row_targets_guidance_leg(plan, row))
        {
            let id = FlightPlanRowId(row.uid);
            let guidance_leg_id = row
                .leg_index
                .map(|_| GuidanceLegId::for_destination_row(&id));
            if let Some(leg_index) = row.leg_index {
                if row_id_by_leg_index.insert(leg_index, id.clone()).is_some() {
                    return Err(invalid_plan(format!(
                        "resolved leg {leg_index} projects to multiple target rows"
                    )));
                }
            }
            order.push(id.clone());
            rows.insert(
                id.clone(),
                MaterializedFlightPlanRow {
                    id,
                    guidance_leg_id: guidance_leg_id.clone(),
                    location: row.nav_ref,
                    leg_index: row.leg_index,
                    tone: FlightDataCellTone::Planned,
                    arrow: if row.leg_index.is_some() {
                        MaterializedFlightPlanRowArrow::Leg
                    } else {
                        MaterializedFlightPlanRowArrow::None
                    },
                    geometry: row.leg_index.and_then(|leg_index| {
                        geometry_for_resolved_leg(plan, leg_index, geometry_by_id)
                    }),
                    estimate_geometry: guidance_leg_id
                        .as_ref()
                        .and_then(|leg_id| estimate_geometry_by_guidance_leg_id.get(leg_id))
                        .cloned(),
                    distance_remaining_nm: None,
                    cumulative_distance_remaining_nm: None,
                },
            );
        }

        for leg_index in 0..plan.resolved_legs.len() {
            if !row_id_by_leg_index.contains_key(&leg_index) {
                return Err(invalid_plan(format!(
                    "resolved leg {leg_index} has no target row"
                )));
            }
        }

        let mut active_row_id = None;
        let mut active_geometry = None;
        match plan.guidance.as_ref() {
            None => {}
            Some(guidance) if guidance.sequencing_mode == SequencingMode::DirectTo => {
                let direct_to = guidance.direct_to.as_ref().ok_or_else(|| {
                    invalid_plan("direct-to sequencing mode requires direct-to state")
                })?;
                let resume_leg_index = crate::planning::direct_to_resume_leg_index(plan, direct_to);
                if direct_to.resume_row_id.is_some() && resume_leg_index.is_none() {
                    return Err(invalid_plan("direct-to resume row does not exist"));
                }
                for row in rows.values_mut() {
                    if let Some(leg_index) = row.leg_index {
                        row.tone = if resume_leg_index.is_some_and(|resume| leg_index >= resume) {
                            FlightDataCellTone::Planned
                        } else {
                            FlightDataCellTone::Passed
                        };
                    } else {
                        row.tone = FlightDataCellTone::Passed;
                    }
                }

                let target_row_id = direct_to.target_row.row_id().clone();
                let target_row = rows.get_mut(&target_row_id).ok_or_else(|| {
                    invalid_plan(format!(
                        "direct-to target row does not exist: {}",
                        target_row_id.as_str()
                    ))
                })?;
                if matches!(direct_to.target_row, DirectToTargetRow::Planned { .. })
                    && target_row.location.as_ref() != Some(&direct_to.target)
                {
                    return Err(invalid_plan(format!(
                        "direct-to target row {} does not contain its target",
                        target_row_id.as_str()
                    )));
                }
                let direct_geometry = geometry_by_id.get("direct-to").cloned();
                target_row.tone = FlightDataCellTone::Active;
                target_row.arrow = MaterializedFlightPlanRowArrow::DirectTo;
                target_row.geometry = direct_geometry.clone();
                target_row.estimate_geometry = direct_geometry.clone();
                active_row_id = Some(target_row_id);
                active_geometry = direct_geometry;
            }
            Some(guidance) => {
                let projects_active = guidance_projects_active_leg(plan, guidance);
                for row in rows.values_mut() {
                    let Some(leg_index) = row.leg_index else {
                        row.tone = if projects_active {
                            FlightDataCellTone::Passed
                        } else {
                            FlightDataCellTone::Planned
                        };
                        continue;
                    };
                    row.tone = if leg_index < guidance.active_leg_index {
                        FlightDataCellTone::Passed
                    } else if projects_active && leg_index == guidance.active_leg_index {
                        FlightDataCellTone::Active
                    } else {
                        FlightDataCellTone::Planned
                    };
                }
                if projects_active {
                    let row_id = row_id_by_leg_index
                        .get(&guidance.active_leg_index)
                        .cloned()
                        .ok_or_else(|| {
                            invalid_plan(format!(
                                "active leg {} has no target row",
                                guidance.active_leg_index
                            ))
                        })?;
                    active_geometry = active_geometry_for_guidance(plan, guidance, geometry_by_id);
                    active_row_id = Some(row_id);
                }
            }
        }

        let mut cumulative_nm = 0.0;
        let mut cumulative_complete = true;
        let mut has_contributing_row = false;
        for row_id in &order {
            let row = rows.get_mut(row_id).expect("row order and map agree");
            let static_distance_nm = row.estimate_geometry.as_ref().map(geometry_distance_nm);
            row.distance_remaining_nm = if row.tone == FlightDataCellTone::Active {
                match (ownship_position, row.estimate_geometry.as_ref()) {
                    (Some(position), Some(geometry)) => {
                        Some(crate::great_circle_distance_nm(position, geometry.to))
                    }
                    _ => static_distance_nm,
                }
            } else {
                static_distance_nm
            };

            if (row.tone != FlightDataCellTone::Passed && row.leg_index.is_some())
                || row.arrow == MaterializedFlightPlanRowArrow::DirectTo
            {
                has_contributing_row = true;
                if let Some(distance_nm) = row.distance_remaining_nm {
                    cumulative_nm += distance_nm;
                    if cumulative_complete {
                        row.cumulative_distance_remaining_nm = Some(cumulative_nm);
                    }
                } else {
                    cumulative_complete = false;
                }
            }
        }
        let total_distance_remaining_nm =
            (has_contributing_row && cumulative_complete).then_some(cumulative_nm);

        let active = active_row_id.map(|row_id| {
            let row = rows.get(&row_id).expect("active row was validated above");
            MaterializedActiveLeg {
                row_id,
                summary: if active_guidance_detail_is_terminal_hold(plan) {
                    "HOLD".to_string()
                } else if plan.guidance.as_ref().is_some_and(|guidance| {
                    guidance.sequencing_mode != SequencingMode::DirectTo
                        && plan
                            .resolved_legs
                            .get(guidance.active_leg_index)
                            .is_some_and(
                                crate::planning::resolved_leg_targets_vector_discontinuity_row,
                            )
                }) {
                    crate::active_guidance_leg(plan)
                        .map(|leg| format!("{} -> VECTORS", location_label(&leg.from),))
                        .unwrap_or_default()
                } else {
                    crate::active_guidance_leg(plan)
                        .map(|leg| {
                            format!(
                                "{} -> {}",
                                location_label(&leg.from),
                                location_label(&leg.to)
                            )
                        })
                        .unwrap_or_default()
                },
                geometry: active_geometry,
                distance_remaining_nm: row.distance_remaining_nm,
            }
        });

        let materialized = Self {
            order,
            rows,
            active,
            total_distance_remaining_nm,
        };
        debug_assert_eq!(materialized.order.len(), materialized.rows.len());
        Ok(materialized)
    }
}

pub(crate) fn geometry_map_from_route(
    route: &[crate::FlightPlanRouteSegment],
) -> HashMap<String, GuidanceLegGeometry> {
    route
        .iter()
        .map(|segment| {
            (
                segment.id.clone(),
                GuidanceLegGeometry {
                    leg_id: segment.id.clone(),
                    from: segment.from,
                    to: segment.to,
                    path: segment.path.clone(),
                },
            )
        })
        .collect()
}

pub(crate) fn geometry_for_resolved_leg(
    plan: &FlightPlan,
    leg_index: usize,
    geometry_by_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<GuidanceLegGeometry> {
    let leg = plan.resolved_legs.get(leg_index)?;
    let detail_count = crate::guidance_detail_count_for_leg(leg);
    let details = (0..detail_count)
        .map(|element_index| {
            geometry_by_id.get(&crate::guidance_detail_id_for_leg_element(
                leg_index,
                leg,
                element_index,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    let from = details.first()?.from;
    let to = details.last()?.to;
    let mut path = Vec::new();
    for geometry in details {
        for point in geometry_points(geometry) {
            if path.last().copied() != Some(point) {
                path.push(point);
            }
        }
    }
    Some(GuidanceLegGeometry {
        leg_id: leg.id.clone(),
        from,
        to,
        path,
    })
}

pub(crate) fn estimate_geometry_by_guidance_leg_id_with_resolver<E, F>(
    plan: &FlightPlan,
    geometry_by_id: &HashMap<String, GuidanceLegGeometry>,
    mut resolve_position: F,
) -> Result<HashMap<GuidanceLegId, GuidanceLegGeometry>, E>
where
    F: FnMut(&NavRef, Option<&str>) -> Result<LatLon, E>,
{
    let mut estimates = HashMap::new();
    for row in project_identity_rows(plan)
        .into_iter()
        .filter(|row| display_row_targets_guidance_leg(plan, row))
    {
        let Some(leg_index) = row.leg_index else {
            continue;
        };
        let Some(leg) = plan.resolved_legs.get(leg_index) else {
            continue;
        };
        let geometry = if crate::planning::resolved_leg_ends_in_manual_sequence(leg) {
            let procedure_airport_id = leg.procedure_provenance.as_ref().and_then(|provenance| {
                (!provenance.airport_id.is_empty()).then_some(provenance.airport_id.as_str())
            });
            let from = resolve_position(&leg.from, procedure_airport_id)?;
            let to = resolve_position(&leg.to, procedure_airport_id)?;
            Some(GuidanceLegGeometry {
                leg_id: leg.id.clone(),
                from,
                to,
                path: vec![from, to],
            })
        } else {
            geometry_for_resolved_leg(plan, leg_index, geometry_by_id)
        };
        if let Some(geometry) = geometry {
            estimates.insert(
                GuidanceLegId::for_destination_row(&FlightPlanRowId(row.uid)),
                geometry,
            );
        }
    }
    Ok(estimates)
}

pub(crate) fn geometry_points(geometry: &GuidanceLegGeometry) -> Vec<LatLon> {
    if geometry.path.len() >= 2 {
        geometry.path.clone()
    } else {
        vec![geometry.from, geometry.to]
    }
}

pub(crate) fn geometry_distance_nm(geometry: &GuidanceLegGeometry) -> f64 {
    geometry_points(geometry)
        .windows(2)
        .map(|segment| crate::great_circle_distance_nm(segment[0], segment[1]))
        .sum()
}

fn route_statuses_for_guidance(
    plan: &FlightPlan,
    guidance: &crate::GuidanceState,
    projects_active: bool,
) -> Vec<Vec<FlightPlanRouteSegmentStatus>> {
    let active_detail_index = guidance.active_detail_index.or_else(|| {
        crate::planning::first_guidance_detail_index_for_leg(plan, guidance.active_leg_index)
    });
    let active_element_index = active_detail_index
        .and_then(|detail_index| guidance_detail_ref_by_index(plan, detail_index))
        .filter(|detail| detail.leg_index == guidance.active_leg_index)
        .map(|detail| detail.element_index);
    let hold_start = terminal_hold_start_element_index_for_leg(plan, guidance.active_leg_index);
    let mut global_detail_index = 0usize;

    plan.resolved_legs
        .iter()
        .enumerate()
        .map(|(leg_index, leg)| {
            (0..crate::guidance_detail_count_for_leg(leg))
                .map(|element_index| {
                    let detail_index = global_detail_index;
                    global_detail_index += 1;
                    if !projects_active {
                        return if active_detail_index.is_some_and(|active| detail_index <= active) {
                            FlightPlanRouteSegmentStatus::Completed
                        } else {
                            FlightPlanRouteSegmentStatus::Remaining
                        };
                    }
                    if leg_index < guidance.active_leg_index {
                        FlightPlanRouteSegmentStatus::Completed
                    } else if leg_index > guidance.active_leg_index {
                        FlightPlanRouteSegmentStatus::Remaining
                    } else if active_detail_index.is_some_and(|active| detail_index < active) {
                        FlightPlanRouteSegmentStatus::Completed
                    } else if active_detail_index == Some(detail_index) {
                        FlightPlanRouteSegmentStatus::Active
                    } else if hold_start.is_some_and(|hold_start| {
                        active_element_index.is_some_and(|active| active < hold_start)
                            && element_index >= hold_start
                    }) {
                        FlightPlanRouteSegmentStatus::Remaining
                    } else {
                        FlightPlanRouteSegmentStatus::ActiveLegRemaining
                    }
                })
                .collect()
        })
        .collect()
}

pub(crate) fn route_statuses_for_plan(plan: &FlightPlan) -> Vec<Vec<FlightPlanRouteSegmentStatus>> {
    let Some(guidance) = plan.guidance.as_ref() else {
        return plan
            .resolved_legs
            .iter()
            .map(|leg| {
                vec![
                    FlightPlanRouteSegmentStatus::Remaining;
                    crate::guidance_detail_count_for_leg(leg)
                ]
            })
            .collect();
    };
    if guidance.sequencing_mode == SequencingMode::DirectTo {
        let resume_leg_index = guidance
            .direct_to
            .as_ref()
            .and_then(|direct_to| crate::planning::direct_to_resume_leg_index(plan, direct_to));
        return plan
            .resolved_legs
            .iter()
            .enumerate()
            .map(|(leg_index, leg)| {
                let status = if resume_leg_index.is_some_and(|resume| leg_index >= resume) {
                    FlightPlanRouteSegmentStatus::Remaining
                } else {
                    FlightPlanRouteSegmentStatus::Completed
                };
                vec![status; crate::guidance_detail_count_for_leg(leg)]
            })
            .collect();
    }
    route_statuses_for_guidance(plan, guidance, guidance_projects_active_leg(plan, guidance))
}

pub(crate) fn active_geometry_for_guidance(
    plan: &FlightPlan,
    guidance: &crate::GuidanceState,
    geometry_by_id: &HashMap<String, GuidanceLegGeometry>,
) -> Option<GuidanceLegGeometry> {
    if guidance.sequencing_mode == SequencingMode::DirectTo {
        return geometry_by_id.get("direct-to").cloned();
    }
    guidance
        .active_detail_index
        .and_then(|detail_index| guidance_detail_ref_by_index(plan, detail_index))
        .and_then(|detail| {
            let leg = plan.resolved_legs.get(detail.leg_index)?;
            geometry_by_id
                .get(&crate::guidance_detail_id_for_leg_element(
                    detail.leg_index,
                    leg,
                    detail.element_index,
                ))
                .cloned()
        })
        .or_else(|| geometry_for_resolved_leg(plan, guidance.active_leg_index, geometry_by_id))
}

fn active_guidance_detail_is_terminal_hold(plan: &FlightPlan) -> bool {
    plan.guidance.as_ref().is_some_and(|guidance| {
        guidance.active_detail_index.is_some_and(|detail_index| {
            crate::planning::terminal_hold_start_detail_index_for_leg(
                plan,
                guidance.active_leg_index,
            )
            .is_some_and(|hold_start| detail_index >= hold_start)
        })
    })
}

fn location_label(location: &NavRef) -> String {
    match location {
        NavRef::Airport(code)
        | NavRef::Navaid(code)
        | NavRef::Fix(code)
        | NavRef::ArincNavaid {
            identifier: code, ..
        }
        | NavRef::TerminalNavaid {
            identifier: code, ..
        } => code.clone(),
        NavRef::LatLon(_) | NavRef::Spot(_) => "SPOT".to_string(),
    }
}

fn invalid_plan(message: impl Into<String>) -> AppError {
    AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::{
        DirectToState, FlightPlanDisplayRowKind, GuidanceState, ResolvedLeg, ResolvedLegSource,
        RouteComponent,
    };

    const EPSILON_NM: f64 = 1e-6;

    fn point(lat: f64, lon: f64) -> LatLon {
        LatLon { lat, lon }
    }

    fn nav(lat: f64, lon: f64) -> NavRef {
        NavRef::LatLon(point(lat, lon))
    }

    fn three_point_plan() -> FlightPlan {
        let a = nav(40.0, -120.0);
        let b = nav(40.0, -119.0);
        let c = nav(41.0, -119.0);
        crate::build_flight_plan(FlightPlan {
            id: "materialized-plan".to_string(),
            name: "A B C".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: a.clone(),
                },
                RouteComponent::Waypoint {
                    waypoint: b.clone(),
                },
                RouteComponent::Waypoint {
                    waypoint: c.clone(),
                },
            ],
            route_component_uids: vec![
                "row-a".to_string(),
                "row-b".to_string(),
                "row-c".to_string(),
            ],
            route_component_uid_counter: 3,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "leg-a-b".to_string(),
                    from: a,
                    to: b,
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "leg-b-c".to_string(),
                    from: nav(40.0, -119.0),
                    to: c,
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .expect("valid three-point plan")
    }

    fn two_airway_plan() -> FlightPlan {
        let a = nav(40.0, -120.0);
        let b = nav(40.0, -119.8);
        let c = nav(40.0, -119.6);
        let d = nav(40.0, -117.0);
        let e = nav(40.0, -116.0);
        crate::build_flight_plan(FlightPlan {
            id: "two-airway-plan".to_string(),
            name: "A V1 C V2 E".to_string(),
            route_components: vec![
                RouteComponent::Airway {
                    airway: crate::AirwaySegment {
                        name: "V1".to_string(),
                        branch_key: None,
                        entry: a.clone(),
                        exit: c.clone(),
                    },
                },
                RouteComponent::Airway {
                    airway: crate::AirwaySegment {
                        name: "V2".to_string(),
                        branch_key: None,
                        entry: c.clone(),
                        exit: e.clone(),
                    },
                },
            ],
            route_component_uids: vec!["row-v1".to_string(), "row-v2".to_string()],
            route_component_uid_counter: 2,
            resolved_legs: vec![
                ResolvedLeg {
                    id: "airway--0".to_string(),
                    from: a,
                    to: b.clone(),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway--1".to_string(),
                    from: b,
                    to: c.clone(),
                    source: ResolvedLegSource::RouteComponent { component_index: 0 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway--0".to_string(),
                    from: c,
                    to: d.clone(),
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
                ResolvedLeg {
                    id: "airway--1".to_string(),
                    from: d,
                    to: e,
                    source: ResolvedLegSource::RouteComponent { component_index: 1 },
                    procedure_provenance: None,
                },
            ],
            guidance: Some(GuidanceState {
                active_leg_index: 1,
                active_detail_index: Some(1),
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .expect("valid two-airway plan")
    }

    fn geometry_map(
        plan: &FlightPlan,
        direct_to: Option<(LatLon, LatLon)>,
    ) -> HashMap<String, GuidanceLegGeometry> {
        let mut geometry_by_id = HashMap::new();
        for (leg_index, leg) in plan.resolved_legs.iter().enumerate() {
            let NavRef::LatLon(from) = &leg.from else {
                panic!("fixture leg start must be a position");
            };
            let NavRef::LatLon(to) = &leg.to else {
                panic!("fixture leg end must be a position");
            };
            let (from, to) = (*from, *to);
            geometry_by_id.insert(
                crate::guidance_detail_id_for_leg_element(leg_index, leg, 0),
                GuidanceLegGeometry {
                    leg_id: leg.id.clone(),
                    from,
                    to,
                    path: vec![from, to],
                },
            );
        }
        if let Some((from, to)) = direct_to {
            geometry_by_id.insert(
                "direct-to".to_string(),
                GuidanceLegGeometry {
                    leg_id: "direct-to".to_string(),
                    from,
                    to,
                    path: vec![from, to],
                },
            );
        }
        geometry_by_id
    }

    fn assert_near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < EPSILON_NM,
            "got {actual}, expected {expected}"
        );
    }

    fn row_id_for_location(plan: &FlightPlan, location: &NavRef) -> FlightPlanRowId {
        let mut matches = project_identity_rows(plan).into_iter().filter(|row| {
            row.row_kind == FlightPlanDisplayRowKind::Waypoint
                && row.nav_ref.as_ref() == Some(location)
        });
        let row = matches.next().expect("fixture location has a row");
        assert!(
            matches.next().is_none(),
            "fixture location must identify exactly one row"
        );
        FlightPlanRowId(row.uid)
    }

    #[test]
    fn direct_to_keeps_fixed_geometry_while_ownship_changes_remaining_distance() {
        let plan = three_point_plan();
        let direct_start = point(39.5, -120.5);
        let target = point(40.0, -119.0);
        let target_row_id = row_id_for_location(&plan, &NavRef::LatLon(target));
        let plan = crate::planning::activate_direct_to_row(&plan, direct_start, &target_row_id)
            .expect("activate direct-to row");
        let geometry_by_id = geometry_map(&plan, Some((direct_start, target)));
        let first_position = point(39.7, -120.0);
        let second_position = point(39.9, -119.4);

        let first = MaterializedFlightPlan::build(&plan, &geometry_by_id, Some(first_position))
            .expect("first materialization");
        let second = MaterializedFlightPlan::build(&plan, &geometry_by_id, Some(second_position))
            .expect("second materialization");
        let first_active = first.active.as_ref().expect("active direct-to");
        let second_active = second.active.as_ref().expect("active direct-to");

        assert_eq!(first_active.row_id, target_row_id);
        assert_eq!(first_active.geometry, second_active.geometry);
        assert_eq!(
            first_active.geometry.as_ref().map(|geometry| geometry.from),
            Some(direct_start)
        );
        assert_ne!(
            first_active.distance_remaining_nm,
            second_active.distance_remaining_nm
        );
        assert_near(
            first_active.distance_remaining_nm.expect("first distance"),
            crate::great_circle_distance_nm(first_position, target),
        );
        assert_near(
            second_active
                .distance_remaining_nm
                .expect("second distance"),
            crate::great_circle_distance_nm(second_position, target),
        );
    }

    #[test]
    fn active_leg_in_first_of_two_airways_uses_its_own_destination() {
        let plan = two_airway_plan();
        let ownship = point(40.0, -119.8);
        let destination = point(40.0, -119.6);
        let destination_row_id = row_id_for_location(&plan, &NavRef::LatLon(destination));
        let materialized =
            MaterializedFlightPlan::build(&plan, &geometry_map(&plan, None), Some(ownship))
                .expect("materialize two-airway route");

        assert_near(
            materialized.rows[&destination_row_id]
                .distance_remaining_nm
                .expect("active airway distance"),
            crate::great_circle_distance_nm(ownship, destination),
        );
        assert_eq!(
            materialized
                .active
                .as_ref()
                .map(|active| active.row_id.clone()),
            Some(destination_row_id),
        );
    }

    #[test]
    fn vectors_manual_sequence_estimates_direct_to_next_waypoint() {
        let mut plan = three_point_plan();
        let start = point(40.0, -120.0);
        let turn = point(40.0, -119.5);
        let display_end = point(40.5, -119.5);
        let next_waypoint = point(40.0, -119.0);
        let final_waypoint = point(41.0, -119.0);
        plan.resolved_legs[0].procedure_provenance =
            Some(crate::planning::ProcedureLegProvenance {
                airport_id: "KRNT".to_string(),
                procedure_id: "RENTN3".to_string(),
                kind: crate::planning::ProcedureKind::Sid,
                role: crate::planning::ProcedureSegmentRole::RunwayTransition,
                path_termination: crate::planning::PathTermination::HeadingToAltitude,
                leg_sequence: 20,
                discontinuity_after: Some(crate::planning::ProcedureDiscontinuity::Vectors),
                display_path: Some(crate::planning::LegDisplayPath {
                    style: crate::planning::LegDisplayPathStyle::Solid,
                    elements: vec![
                        crate::planning::LegDisplayElement::Segment { start, end: turn },
                        crate::planning::LegDisplayElement::Segment {
                            start: turn,
                            end: display_end,
                        },
                    ],
                    effective_terminal_course_deg: Some(90.0),
                    debug_element_sources: Vec::new(),
                    debug_element_roles: Vec::new(),
                }),
            });
        let route = crate::project_flight_plan_route_with_resolver(&plan, |nav_ref, _| {
            if let NavRef::LatLon(position) = nav_ref {
                Ok(*position)
            } else {
                Err("fixture uses only lat/lon nav refs")
            }
        })
        .expect("project manual-sequence route");
        let geometry_by_id = geometry_map_from_route(&route);
        let estimate_geometry_by_guidance_leg_id =
            estimate_geometry_by_guidance_leg_id_with_resolver(
                &plan,
                &geometry_by_id,
                |nav_ref, _| {
                    if let NavRef::LatLon(position) = nav_ref {
                        Ok(*position)
                    } else {
                        Err("fixture uses only lat/lon nav refs")
                    }
                },
            )
            .expect("estimate manual sequence");
        let materialized = MaterializedFlightPlan::build_with_estimate_geometries(
            &plan,
            &geometry_by_id,
            &estimate_geometry_by_guidance_leg_id,
            None,
        )
        .expect("materialize manual-sequence estimate");
        let manual_sequence_row = row_id_for_location(&plan, &NavRef::LatLon(next_waypoint));
        let estimated_manual_distance = crate::great_circle_distance_nm(start, next_waypoint);
        let displayed_manual_distance = route
            .iter()
            .take(crate::guidance_detail_count_for_leg(&plan.resolved_legs[0]))
            .map(|segment| segment.distance_nm)
            .sum::<f64>();

        assert!(
            (displayed_manual_distance - estimated_manual_distance).abs() > 1.0,
            "fixture must distinguish finite chevrons from the direct estimate"
        );
        assert_eq!(
            materialized.rows[&manual_sequence_row]
                .geometry
                .as_ref()
                .map(|geometry| geometry.to),
            Some(display_end),
            "rendering must retain the finite vectors chevrons"
        );
        assert_eq!(
            materialized.rows[&manual_sequence_row]
                .estimate_geometry
                .as_ref()
                .map(|geometry| geometry.to),
            Some(next_waypoint),
            "estimates must bridge vectors direct to the next waypoint"
        );
        assert_near(
            materialized.rows[&manual_sequence_row]
                .distance_remaining_nm
                .expect("vector estimate"),
            estimated_manual_distance,
        );
        assert_near(
            materialized
                .total_distance_remaining_nm
                .expect("complete estimated total"),
            estimated_manual_distance
                + crate::great_circle_distance_nm(next_waypoint, final_waypoint),
        );
    }

    #[test]
    fn on_plan_direct_to_total_contains_only_override_and_reachable_suffix() {
        let plan = three_point_plan();
        let direct_start = point(39.5, -120.5);
        let ownship = point(39.8, -119.5);
        let target = point(40.0, -119.0);
        let final_position = point(41.0, -119.0);
        let first_row_id = row_id_for_location(&plan, &nav(40.0, -120.0));
        let target_row_id = row_id_for_location(&plan, &NavRef::LatLon(target));
        let final_row_id = row_id_for_location(&plan, &NavRef::LatLon(final_position));
        let plan = crate::planning::activate_direct_to_row(&plan, direct_start, &target_row_id)
            .expect("activate direct-to row");
        let materialized = MaterializedFlightPlan::build(
            &plan,
            &geometry_map(&plan, Some((direct_start, target))),
            Some(ownship),
        )
        .expect("materialize direct-to");

        assert_eq!(
            materialized.rows[&first_row_id].tone,
            FlightDataCellTone::Passed
        );
        assert_eq!(
            materialized.rows[&target_row_id].tone,
            FlightDataCellTone::Active
        );
        assert_eq!(
            materialized.rows[&target_row_id].arrow,
            MaterializedFlightPlanRowArrow::DirectTo
        );
        assert_eq!(
            materialized.rows[&final_row_id].tone,
            FlightDataCellTone::Planned
        );
        assert_near(
            materialized
                .total_distance_remaining_nm
                .expect("complete total"),
            crate::great_circle_distance_nm(ownship, target)
                + crate::great_circle_distance_nm(target, final_position),
        );
    }

    #[test]
    fn temporary_direct_to_excludes_the_suspended_basic_plan() {
        let mut plan = three_point_plan();
        let direct_start = point(39.5, -120.5);
        let ownship = point(39.8, -120.0);
        let target = point(39.0, -117.0);
        let temporary_row_id = FlightPlanRowId("temporary-direct-to".to_string());
        plan.guidance = Some(GuidanceState {
            active_leg_index: 0,
            active_detail_index: None,
            sequencing_mode: SequencingMode::DirectTo,
            direct_to: Some(DirectToState {
                start: NavRef::LatLon(direct_start),
                target: NavRef::Spot(target),
                target_row: DirectToTargetRow::Temporary {
                    row_id: temporary_row_id.clone(),
                },
                resume_row_id: None,
            }),
            suspend_reason: None,
        });
        let materialized = MaterializedFlightPlan::build(
            &plan,
            &geometry_map(&plan, Some((direct_start, target))),
            Some(ownship),
        )
        .expect("materialize temporary direct-to");

        assert!(materialized
            .rows
            .values()
            .filter(|row| row.id != temporary_row_id)
            .all(|row| row.tone == FlightDataCellTone::Passed));
        assert_eq!(
            materialized.rows[&temporary_row_id].tone,
            FlightDataCellTone::Active
        );
        assert_near(
            materialized
                .total_distance_remaining_nm
                .expect("temporary direct-to total"),
            crate::great_circle_distance_nm(ownship, target),
        );
    }

    #[test]
    fn active_row_identity_and_total_share_the_same_materialized_row() {
        let mut plan = three_point_plan();
        plan.guidance = Some(GuidanceState {
            active_leg_index: 1,
            active_detail_index: Some(1),
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        });
        let ownship = point(40.4, -119.0);
        let active_row_id = row_id_for_location(&plan, &nav(41.0, -119.0));
        let materialized =
            MaterializedFlightPlan::build(&plan, &geometry_map(&plan, None), Some(ownship))
                .expect("materialize active plan");
        let active = materialized.active.as_ref().expect("active leg");
        let active_row = &materialized.rows[&active.row_id];

        assert_eq!(active.row_id, active_row_id);
        assert_eq!(active_row.tone, FlightDataCellTone::Active);
        assert_eq!(
            active.distance_remaining_nm,
            active_row.distance_remaining_nm
        );
        assert_eq!(
            materialized.total_distance_remaining_nm,
            active_row.distance_remaining_nm
        );
    }
}
