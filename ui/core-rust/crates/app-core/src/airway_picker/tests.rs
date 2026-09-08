// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::navdb_types::AirwayFixPoint;

fn anchor() -> NavRef {
    NavRef::Navaid("ELN".into())
}
fn position() -> LatLon {
    LatLon {
        lat: 47.1,
        lon: -120.5,
    }
}

fn point(name: &str, nav_ref: NavRef, position: LatLon) -> AirwaySpatialPoint {
    AirwaySpatialPoint {
        airway_name: name.into(),
        branch_key: name.into(),
        sequence: 0,
        position,
        nav_ref,
    }
}

pub(crate) fn store() -> NavKvStore {
    let mut records = BTreeMap::new();
    let mut spatial = Vec::new();
    for (name, refs) in [
        (
            "V2",
            vec![
                NavRef::Fix("BANDR".into()),
                anchor(),
                NavRef::Fix("END".into()),
            ],
        ),
        ("V187", vec![anchor(), NavRef::Navaid("PSC".into())]),
        (
            "V9",
            vec![NavRef::Fix("NEAR".into()), NavRef::Fix("OTHER".into())],
        ),
    ] {
        let points = refs
            .into_iter()
            .enumerate()
            .map(|(index, nav_ref)| {
                // Keep the fixture's shared waypoint coordinates identical in every airway.
                let offset = match crate::nav_ref_picker_label(&nav_ref).as_str() {
                    "ELN" => 0.0,
                    "BANDR" => -0.1,
                    "NEAR" => 0.02,
                    _ => 0.1,
                };
                let pos = LatLon {
                    lon: position().lon + offset,
                    ..position()
                };
                let kind = if matches!(nav_ref, NavRef::Navaid(_)) {
                    "navaid"
                } else {
                    "fix"
                };
                records.insert(
                    format!(
                        "navref/position/{kind}/{}",
                        crate::nav_ref_picker_label(&nav_ref)
                    ),
                    serde_json::to_vec(&pos).unwrap(),
                );
                spatial.push(point(name, nav_ref.clone(), pos));
                AirwayFixPoint {
                    airway_name: name.into(),
                    sequence: index as i32,
                    position: pos,
                    nav_ref,
                }
            })
            .collect();
        records.insert(
            format!("airway/{name}"),
            serde_json::to_vec(&vec![AirwayBranch {
                display_name: name.into(),
                branch_key: name.into(),
                points,
            }])
            .unwrap(),
        );
    }
    records.insert(
        "airway/spatial/47/-121".into(),
        serde_json::to_vec(&spatial).unwrap(),
    );
    crate::navkv::nav_kv_store_for_test(
        &records
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_slice()))
            .collect::<Vec<_>>(),
        4096,
    )
}

fn open(store: &NavKvStore) -> AirwayPickerController {
    AirwayPickerController::default()
        .open(
            store,
            "eln-row".into(),
            anchor(),
            Some(NavRef::Navaid("PSC".into())),
            1,
        )
        .unwrap()
}

fn button(picker: &AirwayPickerController, label: &str) -> UiAirwayPickerButton {
    let view = picker.view(1).unwrap();
    view.sections
        .into_iter()
        .flat_map(|s| s.buttons)
        .chain(view.footer)
        .find(|b| b.label == label)
        .unwrap()
}

fn click(
    picker: &AirwayPickerController,
    store: &NavKvStore,
    label: &str,
) -> AirwayPickerController {
    match picker
        .advance(store, &button(picker, label).action_id, 1)
        .unwrap()
    {
        Transition::Picker(next) => next,
        Transition::Insert { .. } => panic!("expected a menu, not a mutation"),
    }
}

#[test]
fn nearby_airways_keep_the_entire_sea_tie_and_show_low_routes_first() {
    let renton = NavRef::Airport("KRNT".into());
    let renton_position = LatLon {
        lat: 47.4931389,
        lon: -122.21575,
    };
    let sea_position = LatLon {
        lat: 47.4353731,
        lon: -122.3096161,
    };
    let mut points = [
        "J1", "J12", "J189", "J20", "J5", "J503", "J505", "J523", "J65", "J70", "J90", "Q902",
        "T487", "V120", "V2", "V23", "V27", "V298", "V4", "V495",
    ]
    .map(|name| point(name, NavRef::Navaid("SEA".into()), sea_position))
    .to_vec();
    points.push(points[0].clone());
    points.push(point(
        "T1",
        NavRef::Fix("FARTHER".into()),
        LatLon {
            lat: 48.0,
            ..renton_position
        },
    ));
    let result = classify_points(&renton, renton_position, &points);
    assert!(result.exact.is_empty());
    assert_eq!(
        result.nearby,
        [
            "T487", "V2", "V4", "V23", "V27", "V120", "V298", "V495", "J1", "J5", "J12", "J20",
            "J65", "J70", "J90", "J189", "J503", "J505", "J523", "Q902",
        ]
    );
    points.reverse();
    assert_eq!(result, classify_points(&renton, renton_position, &points));
}

#[test]
fn nearby_cutoff_keeps_a_boundary_tie_after_closer_routes_without_admitting_farther_routes() {
    let mut points = (0..8)
        .map(|i| {
            point(
                &format!("J{}", i + 100),
                NavRef::Fix("CLOSER".into()),
                LatLon {
                    lat: position().lat + 0.01,
                    ..position()
                },
            )
        })
        .collect::<Vec<_>>();
    points.extend(["T1", "V1", "V2", "J1", "Q1"].map(|name| {
        point(
            name,
            NavRef::Fix("BOUNDARY".into()),
            LatLon {
                lat: position().lat + 0.02,
                ..position()
            },
        )
    }));
    points.push(point(
        "T2",
        NavRef::Fix("FARTHER".into()),
        LatLon {
            lat: position().lat + 0.03,
            ..position()
        },
    ));
    let result = classify_points(&anchor(), position(), &points);
    assert_eq!(result.nearby.len(), 13);
    for name in ["T1", "V1", "V2", "J1", "Q1"] {
        assert!(result.nearby.iter().any(|found| found == name));
    }
    assert!(!result.nearby.iter().any(|name| name == "T2"));
}

#[test]
fn through_airways_use_altitude_groups_then_prefix_and_numeric_route_number() {
    let points =
        ["V25", "Q7", "T10", "J12", "V2", "T2", "J2"].map(|name| point(name, anchor(), position()));
    assert_eq!(
        classify_points(&anchor(), position(), &points).exact,
        ["T2", "T10", "V2", "V25", "J2", "J12", "Q7"],
    );
}

#[test]
fn exact_membership_is_unlimited_natural_and_nearby_is_unique_distance_limited_and_disjoint() {
    let mut points = (0..35)
        .map(|i| point(&format!("V{i}"), anchor(), position()))
        .collect::<Vec<_>>();
    for i in (0..20).rev() {
        let nearby = point(
            &format!("N{i:02}"),
            NavRef::Fix("OTHER".into()),
            LatLon {
                lon: position().lon + i as f64 * 0.001,
                ..position()
            },
        );
        points.extend([nearby.clone(), nearby]);
    }
    // Coincident coordinates, or merely the same identifier in a different domain,
    // do not make an airway contain the selected waypoint.
    points.push(point("COLLISION", NavRef::Fix("ELN".into()), position()));
    points.push(point("V2", NavRef::Fix("OTHER".into()), position()));
    let result = classify_points(&anchor(), position(), &points);
    assert_eq!(result.exact.len(), 35);
    assert_eq!(&result.exact[..4], &["V0", "V1", "V2", "V3"]);
    assert_eq!(result.nearby.len(), 10);
    assert_eq!(&result.nearby[..3], &["COLLISION", "N00", "N01"]);
    assert!(result
        .nearby
        .iter()
        .all(|name| !result.exact.contains(name)));
    points.reverse();
    assert_eq!(result, classify_points(&anchor(), position(), &points));
}

#[test]
fn exact_choice_locks_entry_and_back_returns_to_both_sections() {
    let store = store();
    let initial = open(&store);
    let view = initial.view(1).unwrap();
    assert_eq!(
        view.sections
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>(),
        ["Through ELN", "Nearby"]
    );
    assert_eq!(
        view.sections[0]
            .buttons
            .iter()
            .map(|b| b.label.as_str())
            .collect::<Vec<_>>(),
        ["V2", "V187"]
    );
    assert_eq!(view.sections[1].buttons[0].label, "V9");
    assert!(view.sections.iter().all(|s| s.dense));
    let exit = click(&initial, &store, "V2");
    assert_eq!(exit.view(1).unwrap().title, "V2: Select exit");
    let same = button(&exit, "ELN");
    assert!(!same.enabled);
    assert!(same.disabled_reason.is_some());
    assert!(exit.advance(&store, &same.action_id, 1).is_err());
    match exit
        .advance(&store, &button(&exit, "BANDR").action_id, 1)
        .unwrap()
    {
        Transition::Insert { row_uid, selection } => {
            assert_eq!(row_uid, "eln-row");
            let materialized =
                crate::had_ops::materialize_airway_presentation_selection(&store, 0, selection)
                    .unwrap();
            assert_eq!(materialized.airway.entry, anchor());
            assert_eq!(materialized.airway.exit, NavRef::Fix("BANDR".into()));
        }
        _ => panic!("exit must produce one core-owned insertion"),
    }
    let back = click(&exit, &store, "Back");
    assert_eq!(
        back.view(1)
            .unwrap()
            .sections
            .iter()
            .map(|s| (
                &s.title,
                s.buttons.iter().map(|b| &b.label).collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        view.sections
            .iter()
            .map(|s| (
                &s.title,
                s.buttons.iter().map(|b| &b.label).collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>()
    );
}

#[test]
fn nearby_flow_owns_entry_exit_back_and_stale_clicks() {
    let store = store();
    let initial = open(&store);
    let entry = click(&initial, &store, "V9");
    assert_eq!(entry.view(1).unwrap().title, "V9: Select entry");
    assert!(entry
        .advance(&store, &button(&initial, "V9").action_id, 1)
        .is_err());
    let exit = click(&entry, &store, "NEAR");
    assert_eq!(exit.view(1).unwrap().title, "V9: Select exit");
    assert!(!button(&exit, "NEAR").enabled);
    let entry_again = click(&exit, &store, "Back");
    assert_eq!(
        entry_again.view(1).unwrap().title,
        entry.view(1).unwrap().title
    );
    assert_eq!(
        click(&entry_again, &store, "Back").view(1).unwrap().title,
        "AIRWAY ELN"
    );
    assert!(exit.view(2).is_none());
    assert!(exit
        .advance(&store, &button(&exit, "OTHER").action_id, 2)
        .is_err());
    let Transition::Picker(closed) = exit
        .advance(&store, &exit.view(1).unwrap().dismiss_action_id, 1)
        .unwrap()
    else {
        panic!("dismiss")
    };
    assert!(closed.view(1).is_none());
    let reopened = closed
        .open(&store, "eln-row".into(), anchor(), None, 1)
        .unwrap();
    assert!(reopened
        .advance(&store, &button(&initial, "V9").action_id, 1)
        .is_err());
}

#[test]
fn paging_does_not_commit_partial_picker_transitions() {
    let full = store();
    let mut unloaded = full.clone();
    unloaded.clear_pages();
    let initial = AirwayPickerController::default();
    assert!(matches!(
        initial.open(&unloaded, "row".into(), anchor(), None, 1),
        Err(HadReadError::NeedPages(_))
    ));
    assert!(initial.view(1).is_none());
    let opened = open(&full);
    let before = opened.clone();
    assert!(matches!(
        opened.advance(&unloaded, &button(&opened, "V2").action_id, 1),
        Err(HadReadError::NeedPages(_))
    ));
    assert_eq!(opened, before);
    assert_eq!(
        click(&opened, &full, "V2").view(1).unwrap().title,
        "V2: Select exit"
    );
}

#[test]
fn navigation_keeps_picker_but_definition_edit_closes_it_and_rollback_restores_it() {
    let store = store();
    let plan = crate::build_flight_plan(crate::FlightPlan {
        route_components: vec![
            crate::RouteComponent::Waypoint { waypoint: anchor() },
            crate::RouteComponent::Waypoint {
                waypoint: NavRef::Navaid("PSC".into()),
            },
        ],
        ..crate::FlightPlan::empty()
    })
    .unwrap();
    let mut controller =
        crate::flight_plan_controller::FlightPlanController::new(plan.clone(), Vec::new()).unwrap();
    controller.set_airway_picker(open(&store));
    controller
        .apply_navigation_update(crate::activate_leg(&plan, 0).unwrap())
        .unwrap();
    assert!(controller.airway_picker().view(1).is_some());
    let checkpoint = controller.checkpoint_model();
    let mut edited = plan;
    edited.name = "Edited".into();
    controller.apply_definition_edit(edited).unwrap();
    assert!(controller.airway_picker().view(1).is_none());
    controller.rollback_model(checkpoint);
    assert!(controller.airway_picker().view(1).is_some());
}
