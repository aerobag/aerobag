// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

fn fix(ident: &str) -> NavRef {
    NavRef::Fix(ident.to_string())
}

pub(crate) fn airway(name: &str, points: &[NavRef]) -> (AirwaySegment, Vec<ResolvedLeg>) {
    (
        AirwaySegment {
            name: name.to_string(),
            branch_key: Some(format!("{name}-branch")),
            entry: points.first().unwrap().clone(),
            exit: points.last().unwrap().clone(),
        },
        points
            .windows(2)
            .enumerate()
            .map(|(index, pair)| ResolvedLeg {
                id: format!("{name}-{index}"),
                from: pair[0].clone(),
                to: pair[1].clone(),
                source: ResolvedLegSource::RouteComponent {
                    component_index: 999,
                },
                procedure_provenance: None,
            })
            .collect(),
    )
}

pub(crate) fn append_airway(plan: &FlightPlan, name: &str, points: &[NavRef]) -> FlightPlan {
    let (airway, legs) = airway(name, points);
    insert_airway_after_waypoint(plan, plan.route_components.len() - 1, airway, legs).unwrap()
}

fn start_at(ident: &str) -> FlightPlan {
    waypoints(&[fix(ident)])
}

pub(crate) fn waypoints(points: &[NavRef]) -> FlightPlan {
    crate::build_flight_plan(FlightPlan {
        route_components: points
            .iter()
            .cloned()
            .map(|waypoint| RouteComponent::Waypoint { waypoint })
            .collect(),
        ..FlightPlan::default()
    })
    .unwrap()
}

#[test]
fn airway_endpoints_are_real_waypoints_and_exit_can_extend_route() {
    let initial = start_at("BANDR");
    let entry_id = project_identity_rows(&initial)[0].uid.clone();
    let plan = append_airway(&initial, "V2", &[fix("BANDR"), fix("MID"), fix("ELN")]);
    let rows = project_identity_rows(&plan);
    assert_eq!(
        rows.iter()
            .map(|row| (row.label.as_str(), row.depth))
            .collect::<Vec<_>>(),
        vec![("BANDR", 0), ("V2", 0), ("MID", 1), ("ELN", 0)]
    );
    assert_eq!(
        rows[0].uid, entry_id,
        "inserting an airway preserves the entry occurrence"
    );
    let exit = rows.last().unwrap();
    assert!(
        exit.can_add_airway_after,
        "the airway exit must support continuing the route"
    );
    let continued = append_airway(&plan, "V187", &[fix("ELN"), fix("NEXT"), fix("PSC")]);
    let continued_rows = project_identity_rows(&continued);
    let junctions = continued_rows
        .iter()
        .filter(|row| row.nav_ref.as_ref() == Some(&fix("ELN")))
        .collect::<Vec<_>>();
    assert_eq!(junctions.len(), 1);
    assert_eq!(junctions[0].uid, exit.uid);
    assert_eq!(junctions[0].depth, 0);
}

#[test]
fn airway_intermediate_waypoints_never_offer_trimming() {
    let plan = append_airway(
        &start_at("BANDR"),
        "V2",
        &[fix("BANDR"), fix("MID"), fix("ELN")],
    );
    let rows = project_identity_rows(&plan);
    for row in rows.iter().filter(|row| row.depth > 0) {
        assert!(
            row.action_matrix
                .iter()
                .flatten()
                .all(|action| action.id != FlightPlanRowActionId::Remove),
            "intermediate row {} must not offer trimming",
            row.label
        );
    }
}

fn is_airway(plan: &FlightPlan, index: usize) -> bool {
    matches!(
        plan.route_components.get(index),
        Some(RouteComponent::Airway { .. })
    )
}

fn pinned(plan: &FlightPlan, index: usize) -> bool {
    index
        .checked_sub(1)
        .is_some_and(|previous| is_airway(plan, previous))
        || is_airway(plan, index + 1)
}

fn expected_actions(
    plan: &FlightPlan,
    row: &FlightPlanDisplayRowUiView,
    first: bool,
) -> Vec<(FlightPlanRowActionId, bool)> {
    use FlightPlanRowActionId::*;
    let already_active = plan.guidance.as_ref().is_some_and(|guidance| {
        guidance.sequencing_mode != SequencingMode::DirectTo
            && row.leg_index == Some(guidance.active_leg_index)
    });
    if row.depth == 1 {
        return vec![(ActivateLeg, !already_active), (DirectTo, true)];
    }
    let index = row.component_index.unwrap();
    let group = row.row_kind == FlightPlanDisplayRowKind::Group;
    let can_move = |neighbor: Option<usize>| {
        !group
            && !pinned(plan, index)
            && neighbor.is_some_and(|neighbor| {
                neighbor < plan.route_components.len()
                    && !is_airway(plan, neighbor)
                    && !pinned(plan, neighbor)
            })
    };
    let mut actions = vec![
        (
            InsertBefore,
            !group
                && !index
                    .checked_sub(1)
                    .is_some_and(|previous| is_airway(plan, previous)),
        ),
        (MoveUp, can_move(index.checked_sub(1))),
        (InsertAfter, !group && !is_airway(plan, index + 1)),
        (MoveDown, can_move(index.checked_add(1))),
        (Remove, group || !pinned(plan, index)),
        (RemoveAllAbove, !is_airway(plan, index + 1)),
    ];
    if !group {
        actions.extend([
            (ActivateLeg, !first && !already_active),
            (DirectTo, true),
            (AddAirway, !is_airway(plan, index + 1)),
            (
                FindRoute,
                !crate::flight_plan_has_direct_to_overlay(plan)
                    && plan
                        .route_components
                        .iter()
                        .skip(index + 1)
                        .take_while(|component| {
                            !matches!(component, RouteComponent::Procedure { .. })
                        })
                        .any(|component| matches!(component, RouteComponent::Waypoint { .. })),
            ),
            (WaypointInfo, false),
            (Weather, false),
            (Plates, false),
            (SelectDeparture, false),
            (SelectArrival, false),
            (SelectApproach, false),
        ]);
    }
    actions
}

fn assert_canonical(plan: &FlightPlan) {
    crate::build_flight_plan(plan.clone()).expect("every successful edit returns a valid plan");
    let rows = project_identity_rows(plan);
    let ids = rows
        .iter()
        .map(|row| &row.uid)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        ids.len(),
        rows.len(),
        "each projected occurrence has exactly one identity"
    );
    for (index, component) in plan.route_components.iter().enumerate() {
        if let RouteComponent::Airway { airway } = component {
            assert_eq!(airway.entry.0, plan.route_component_uids[index - 1]);
            assert_eq!(airway.exit.0, plan.route_component_uids[index + 1]);
        }
    }
    let targets = rows
        .iter()
        .filter_map(|row| row.leg_index)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        targets,
        (0..plan.resolved_legs.len()).collect(),
        "every flown leg ends at a displayed row"
    );
    assert_eq!(
        rows.iter().filter(|row| row.leg_index.is_some()).count(),
        plan.resolved_legs.len(),
        "no distance or guidance leg may be counted twice"
    );
}

fn exercise_every_action(plan: &FlightPlan) -> usize {
    use crate::flight_plan_controller::FlightPlanController;
    use FlightPlanRowActionId::*;
    assert_canonical(plan);
    let controller = FlightPlanController::new(plan.clone(), Vec::new()).unwrap();
    let mut exercised = 0;
    for (row_index, row) in project_identity_rows(plan).iter().enumerate() {
        let expected = expected_actions(plan, row, row_index == 0);
        let actions = flight_plan_row_actions(row).collect::<Vec<_>>();
        assert_eq!(
            actions.len(),
            expected.len(),
            "unexpected menu on {}",
            row.label
        );
        for (id, enabled) in expected {
            let action = actions
                .iter()
                .find(|action| action.id == id)
                .unwrap_or_else(|| panic!("missing {id:?} on {}", row.label));
            assert_eq!(
                action.enabled, enabled,
                "{} {:?}: {id:?}; plan {:?}",
                row.label, row.row_kind, plan.route_components
            );
            if !enabled {
                assert!(action
                    .disabled_reason
                    .as_ref()
                    .is_some_and(|reason| !reason.is_empty()));
            }
            exercised += 1;
            if action.execution == FlightPlanRowActionExecution::CoreSession {
                let result = controller.plan_after_row_action(
                    &row.uid,
                    &action.uid,
                    Some(LatLon {
                        lat: 47.0,
                        lon: -122.0,
                    }),
                );
                assert_eq!(
                    result.is_ok(),
                    enabled,
                    "dispatch {} {id:?}: {result:?}",
                    row.label
                );
                if let Ok((result, _)) = result {
                    assert_canonical(&result);
                }
            }
            // Input dialogs eventually use these same core mutation entry points.
            let index = row.component_index.unwrap();
            let result = match id {
                InsertBefore | InsertAfter => Some(insert_waypoint(
                    plan,
                    index,
                    id == InsertBefore,
                    fix("INSERTED"),
                )),
                AddAirway => {
                    let (airway, legs) =
                        airway("V9", &[row.nav_ref.clone().unwrap(), fix("NEW-EXIT")]);
                    Some(crate::insert_airway_materialized(
                        plan,
                        index,
                        (index + 1 < plan.route_components.len()).then_some(index + 1),
                        airway,
                        legs,
                    ))
                }
                _ => None,
            };
            if let Some(result) = result {
                assert_eq!(
                    result.is_ok(),
                    enabled,
                    "edit {} {id:?}: {result:?}",
                    row.label
                );
                if let Ok(result) = result {
                    assert_canonical(&result);
                }
            }
            assert_eq!(
                controller.active_plan(),
                Some(plan),
                "deciding or rejecting an edit never changes the original"
            );
        }
    }
    exercised
}

#[test]
fn every_action_on_zero_one_and_two_airway_routes_obeys_the_pilot_model() {
    let mut cases = 0;
    let mut actions = 0;
    for count in 0..=2usize {
        for prefix in 0..=1 {
            for suffix in 0..=1 {
                for gap in 0..=usize::from(count == 2) {
                    for first_interiors in 0..=2 {
                        for second_interiors in 0..=if count == 2 { 2 } else { 0 } {
                            for reverse in [false, true] {
                                let mut expected_rows = Vec::new();
                                let start = if reverse { "END" } else { "START" };
                                let mut plan = if prefix == 1 {
                                    expected_rows.push(("PREFIX".to_string(), 0));
                                    waypoints(&[fix("PREFIX"), fix(start)])
                                } else {
                                    start_at(start)
                                };
                                expected_rows.push((start.to_string(), 0));
                                let mut current = fix(start);
                                for n in 0..count {
                                    if n == 1 && gap == 1 {
                                        current = fix("GAP");
                                        plan = insert_waypoint(
                                            &plan,
                                            plan.route_components.len() - 1,
                                            false,
                                            current.clone(),
                                        )
                                        .unwrap();
                                        expected_rows.push(("GAP".into(), 0));
                                    }
                                    let name = format!("V{}", n + 1);
                                    let end = if n == 0 {
                                        "JUNCTION"
                                    } else if reverse {
                                        "START"
                                    } else {
                                        "END"
                                    };
                                    let mut points = vec![current];
                                    expected_rows.push((name.clone(), 0));
                                    for interior in 0..if n == 0 {
                                        first_interiors
                                    } else {
                                        second_interiors
                                    } {
                                        let ident = format!("MID-{n}-{interior}");
                                        points.push(fix(&ident));
                                        expected_rows.push((ident, 1));
                                    }
                                    points.push(fix(end));
                                    plan = append_airway(&plan, &name, &points);
                                    current = fix(end);
                                    expected_rows.push((end.to_string(), 0));
                                }
                                if suffix == 1 {
                                    plan = insert_waypoint(
                                        &plan,
                                        plan.route_components.len() - 1,
                                        false,
                                        fix("SUFFIX"),
                                    )
                                    .unwrap();
                                    expected_rows.push(("SUFFIX".into(), 0));
                                }
                                assert_eq!(
                                    project_identity_rows(&plan)
                                        .iter()
                                        .map(|row| (row.label.clone(), row.depth))
                                        .collect::<Vec<_>>(),
                                    expected_rows
                                );
                                actions += exercise_every_action(&plan);
                                if !plan.resolved_legs.is_empty() {
                                    let index = plan.resolved_legs.len() / 2;
                                    actions +=
                                        exercise_every_action(&activate_leg(&plan, index).unwrap());
                                    let target = project_identity_rows(&plan)
                                        .into_iter()
                                        .find(|row| row.leg_index == Some(index))
                                        .unwrap();
                                    let direct = activate_direct_to_row(
                                        &plan,
                                        LatLon {
                                            lat: 47.0,
                                            lon: -122.0,
                                        },
                                        &FlightPlanRowId(target.uid),
                                    )
                                    .unwrap();
                                    actions += exercise_every_action(&direct);
                                }
                                cases += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 192);
    assert!(
        actions > 30_000,
        "exercise all projected controls, not just one deletion"
    );
}

#[test]
fn removing_airways_unpins_only_endpoints_without_remaining_dependencies() {
    let first = append_airway(
        &start_at("BANDR"),
        "V2",
        &[fix("BANDR"), fix("MID"), fix("ELN")],
    );
    let plan = append_airway(&first, "V187", &[fix("ELN"), fix("PSC")]);
    let without_v2 = delete_component(&plan, 1).unwrap();
    assert_eq!(without_v2.resolved_legs[0].from, fix("BANDR"));
    assert_eq!(without_v2.resolved_legs[0].to, fix("ELN"));
    assert!(delete_component(&without_v2, 0).is_ok());
    assert!(
        delete_component(&without_v2, 1).is_err(),
        "V187 still pins ELN"
    );
    let no_airways = delete_component(&without_v2, 2).unwrap();
    assert!(delete_component(&no_airways, 1).is_ok());
    assert_eq!(
        no_airways.route_component_uids,
        vec![
            plan.route_component_uids[0].clone(),
            plan.route_component_uids[2].clone(),
            plan.route_component_uids[4].clone()
        ]
    );
}

#[test]
fn airway_references_and_geometry_cannot_disagree_or_resolve_to_another_occurrence() {
    let plan = append_airway(
        &start_at("BANDR"),
        "V2",
        &[fix("BANDR"), fix("MID"), fix("ELN")],
    );
    let mut wrong_id = plan.clone();
    let RouteComponent::Airway { airway } = &mut wrong_id.route_components[1] else {
        unreachable!()
    };
    airway.exit = airway.entry.clone();
    assert!(crate::build_flight_plan(wrong_id).is_err());
    let mut wrong_geometry = plan.clone();
    wrong_geometry.resolved_legs[1].to = fix("PSC");
    assert!(crate::build_flight_plan(wrong_geometry).is_err());
    let mut missing_geometry = plan.clone();
    missing_geometry.resolved_legs.clear();
    assert!(crate::build_flight_plan(missing_geometry).is_err());
    let encoded = serde_json::to_vec(&plan).unwrap();
    let restored = crate::build_flight_plan(serde_json::from_slice(&encoded).unwrap()).unwrap();
    assert_eq!(restored, plan);
    exercise_every_action(&restored);
}

#[test]
fn repeated_identifiers_are_distinct_occurrences_not_implicitly_joined() {
    let first = append_airway(&start_at("BANDR"), "V2", &[fix("BANDR"), fix("ELN")]);
    let plan = append_airway(&first, "V187", &[fix("ELN"), fix("BANDR")]);
    let rows = project_identity_rows(&plan);
    let repeated = rows
        .iter()
        .filter(|row| row.nav_ref.as_ref() == Some(&fix("BANDR")))
        .collect::<Vec<_>>();
    assert_eq!(repeated.len(), 2);
    assert_ne!(repeated[0].uid, repeated[1].uid);
    let mut broken = plan.clone();
    let first_id = broken.route_component_uids[0].clone();
    let RouteComponent::Airway { airway } = &mut broken.route_components[3] else {
        panic!("V187")
    };
    airway.exit.0 = first_id;
    assert!(
        crate::build_flight_plan(broken).is_err(),
        "matching NavRefs cannot substitute for the bound occurrence"
    );
    exercise_every_action(&plan);
}

fn spot(lon: f64) -> NavRef {
    NavRef::Spot(LatLon { lat: 47.0, lon })
}

fn position(nav_ref: &NavRef) -> LatLon {
    match nav_ref {
        NavRef::Spot(position) | NavRef::LatLon(position) => *position,
        _ => panic!("test uses coordinates"),
    }
}

fn assert_active_geometry_and_totals(plan: &FlightPlan, ownship: LatLon, destination: &NavRef) {
    use crate::flight_plan_materialization::{geometry_map_from_route, MaterializedFlightPlan};
    use crate::{FlightDataCellTone, FlightPlanRouteSegmentStatus};
    let route = crate::project_flight_plan_route_with_resolver(plan, |nav_ref, _| {
        Ok::<_, AppError>(position(nav_ref))
    })
    .unwrap();
    let materialized =
        MaterializedFlightPlan::build(plan, &geometry_map_from_route(&route), Some(ownship))
            .unwrap();
    let active = materialized.active.as_ref().expect("active navigation");
    let row = &materialized.rows[&active.row_id];
    assert_eq!(row.location.as_ref(), Some(destination));
    assert_eq!(row.tone, FlightDataCellTone::Active);
    assert_eq!(
        materialized
            .rows
            .values()
            .filter(|row| row.tone == FlightDataCellTone::Active)
            .count(),
        1
    );
    let magenta = route
        .iter()
        .filter(|segment| segment.status == FlightPlanRouteSegmentStatus::Active)
        .collect::<Vec<_>>();
    assert_eq!(magenta.len(), 1);
    assert_eq!(magenta[0].to, position(destination));
    assert_eq!(active.geometry.as_ref().unwrap().to, magenta[0].to);
    let expected = crate::great_circle_distance_nm(ownship, position(destination))
        + route
            .iter()
            .filter(|segment| segment.status == FlightPlanRouteSegmentStatus::Remaining)
            .map(|segment| segment.distance_nm)
            .sum::<f64>();
    assert!(
        (materialized.total_distance_remaining_nm.unwrap() - expected).abs() < 0.001,
        "each reachable leg contributes exactly once, including shared endpoint rows"
    );
}

#[test]
fn sequencing_through_airway_junctions_keeps_map_rows_and_remaining_distance_together() {
    for first_interiors in 0..=2 {
        for second_interiors in 0..=2 {
            for reverse in [false, true] {
                let longitude = |n: f64| -123.0 + if reverse { -n } else { n };
                let a = spot(longitude(0.0));
                let junction = spot(longitude(1.0));
                let end = spot(longitude(2.0));
                let first = std::iter::once(a.clone())
                    .chain(
                        (1..=first_interiors)
                            .map(|n| spot(longitude(n as f64 / (first_interiors + 1) as f64))),
                    )
                    .chain([junction.clone()])
                    .collect::<Vec<_>>();
                let second =
                    std::iter::once(junction.clone())
                        .chain((1..=second_interiors).map(|n| {
                            spot(longitude(1.0 + n as f64 / (second_interiors + 1) as f64))
                        }))
                        .chain([end])
                        .collect::<Vec<_>>();
                let plan = append_airway(
                    &append_airway(&waypoints(&[a]), "V2", &first),
                    "V187",
                    &second,
                );
                let mut flying = activate_leg(&plan, 0).unwrap();
                for leg in &plan.resolved_legs {
                    let from = position(&leg.from);
                    let to = position(&leg.to);
                    let ownship = LatLon {
                        lat: 47.0,
                        lon: (from.lon + to.lon) / 2.0,
                    };
                    assert_active_geometry_and_totals(&flying, ownship, &leg.to);
                    flying = sequence_active_leg(&flying).unwrap();
                }
                let junction_row = project_identity_rows(&plan)
                    .into_iter()
                    .find(|row| row.nav_ref.as_ref() == Some(&junction))
                    .unwrap();
                let direct = activate_direct_to_row(
                    &plan,
                    position(&spot(longitude(0.4))),
                    &FlightPlanRowId(junction_row.uid.clone()),
                )
                .unwrap();
                assert_active_geometry_and_totals(
                    &direct,
                    position(&spot(longitude(0.6))),
                    &junction,
                );
                let resumed = sequence_active_leg(&direct).unwrap();
                assert_active_geometry_and_totals(
                    &resumed,
                    position(&spot(longitude(1.1))),
                    &second[1],
                );
                assert_canonical(&resumed);
                // An edit before the airway must preserve the direct-to occurrence and its continuation.
                let edited = insert_waypoint(&direct, 0, true, spot(longitude(-1.0))).unwrap();
                assert_eq!(
                    edited.guidance.as_ref().unwrap().direct_to,
                    direct.guidance.as_ref().unwrap().direct_to
                );
                assert_active_geometry_and_totals(
                    &edited,
                    position(&spot(longitude(0.6))),
                    &junction,
                );
                assert_active_geometry_and_totals(
                    &sequence_active_leg(&edited).unwrap(),
                    position(&spot(longitude(1.1))),
                    &second[1],
                );
            }
        }
    }
}

#[test]
fn airway_route_replacement_preserves_two_intervals_and_occurrence_boundaries() {
    let base = waypoints(&[
        fix("PREFIX"),
        fix("START"),
        fix("END"),
        fix("FINAL"),
        fix("START"),
    ]);
    let first = replace_airway_route_span(
        &base,
        1,
        2,
        vec![
            airway("V2", &[fix("ENTRY"), fix("JOIN")]),
            airway("V298", &[fix("JOIN"), fix("EXIT")]),
        ],
    )
    .unwrap();
    assert_canonical(&first);
    let end_uid = base.route_component_uids[2].clone();
    let end = first
        .route_component_uids
        .iter()
        .position(|uid| uid == &end_uid)
        .unwrap();
    let second = replace_airway_route_span(
        &first,
        end,
        end + 1,
        vec![airway("V9", &[fix("END"), fix("FINAL")])],
    )
    .unwrap();
    assert_eq!(
        &second.route_component_uids[..=end],
        &first.route_component_uids[..=end]
    );
    let reopened = replace_airway_route_span(
        &second,
        1,
        end,
        vec![airway("V4", &[fix("START"), fix("MID"), fix("END")])],
    )
    .unwrap();
    assert_canonical(&reopened);
    assert_eq!(
        &reopened.route_component_uids[..2],
        &base.route_component_uids[..2]
    );
    assert_eq!(
        &reopened.route_component_uids[3..],
        &second.route_component_uids[end..]
    );
    assert_eq!(
        reopened.route_component_uids.last(),
        base.route_component_uids.last(),
        "repeated START remains its own occurrence"
    );
    assert_eq!(reopened.airway_segment(4).unwrap().name, "V9");
    assert!(
        replace_airway_route_span(
            &second,
            3,
            end,
            vec![airway("V4", &[fix("START"), fix("END")])]
        )
        .is_err(),
        "airway group cannot be an interval boundary"
    );
}

#[test]
fn airway_route_replacement_rejects_interior_directs() {
    let base = waypoints(&[fix("START"), fix("END")]);
    assert!(replace_airway_route_span(
        &base,
        0,
        1,
        vec![
            airway("V2", &[fix("A"), fix("B")]),
            airway("V4", &[fix("C"), fix("D")])
        ]
    )
    .is_err());
}
