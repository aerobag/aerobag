// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::airway_routing::tests::{graph, store};
use crate::flight_plan_controller::FlightPlanController;

fn plan() -> FlightPlan {
    crate::build_flight_plan(FlightPlan {
        route_components: ["START", "END"]
            .into_iter()
            .map(|id| RouteComponent::Waypoint {
                waypoint: NavRef::Fix(id.into()),
            })
            .collect(),
        ..FlightPlan::default()
    })
    .unwrap()
}
fn open(plan: &FlightPlan, store: &NavKvStore) -> RoutingEditor {
    let row = crate::project_ui_state(plan).display_rows[0].uid.clone();
    let editor = RoutingEditor::default()
        .open(store, plan, &row, 7, AirwayNavigationMode::Gnss)
        .unwrap();
    if editor.view(7).unwrap().map_open {
        return editor;
    }
    let button = editor
        .view(7)
        .unwrap()
        .endpoints
        .last()
        .unwrap()
        .action_id
        .clone();
    let Transition::Editor(editor) = editor.advance(store, plan, &button, 7).unwrap() else {
        panic!("draft")
    };
    editor
}
fn click(editor: &RoutingEditor, plan: &FlightPlan, label: &str) -> Transition {
    let action = editor
        .view(7)
        .unwrap()
        .controls
        .iter()
        .map(|control| &control.button)
        .find(|button| button.label == label)
        .unwrap()
        .action_id
        .clone();
    editor.advance(&store(), plan, &action, 7).unwrap()
}

fn choose_navigation(editor: &RoutingEditor, plan: &FlightPlan, label: &str) -> RoutingEditor {
    let action = editor
        .view(7)
        .unwrap()
        .controls
        .into_iter()
        .find(|choice| choice.button.label == label)
        .unwrap()
        .button
        .action_id;
    let Transition::Editor(next) = editor.advance(&store(), plan, &action, 7).unwrap() else {
        panic!("editor")
    };
    next
}

#[test]
fn airway_editor_distance_summary_compares_the_full_route_with_direct_distance() {
    assert_eq!(
        route_distance_summary(1213.0, 1255.0),
        "Direct: 1213 nm Airway: 1255 nm +3%"
    );
    assert_eq!(
        route_distance_summary(100.0, 100.4),
        "Direct: 100 nm Airway: 100 nm +0%"
    );
    assert_eq!(
        route_distance_summary(0.0, 20.0),
        "Direct: 0 nm Airway: 20 nm"
    );
}

#[test]
fn airway_editor_exports_compact_controls_in_display_order_with_selected_navigation() {
    let original = plan();
    let editor = open(&original, &store());
    let view = editor.view(7).unwrap();
    assert!(view.message.is_empty());
    assert_eq!(
        view.controls
            .iter()
            .map(|control| (
                control.button.label.as_str(),
                control.symbol_id.as_str(),
                control.selected,
                control.button.enabled
            ))
            .collect::<Vec<_>>(),
        vec![
            ("VOR", "vor", false, true),
            ("GNSS", "gnss", true, true),
            ("Undo", "undo", false, false),
            ("Redo", "redo", false, false),
            ("Apply", "apply_route", false, true),
            ("Cancel", "remove", false, true),
        ]
    );
    let draft = editor.editor.as_ref().unwrap().draft.as_ref().unwrap();
    assert_eq!(
        view.route.unwrap().summary,
        route_distance_summary(
            crate::flight_leg_distance_nm(draft.origin_position, draft.destination_position),
            draft.paths[0].distance_nm,
        )
    );
    let vor = choose_navigation(&editor, &original, "VOR");
    let view = vor.view(7).unwrap();
    assert!(view.controls[0].selected);
    assert!(!view.controls[1].selected);
    assert!(view.controls[2].button.enabled);
}

#[test]
fn airway_editor_single_endpoint_opens_atomically_after_navigation_pages_arrive() {
    let original = plan();
    let row = crate::project_ui_state(&original).display_rows[0]
        .uid
        .clone();
    let records = crate::airway_routing::tests::records(&graph());
    let entries = records
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_slice()))
        .collect::<Vec<_>>();
    let (mut store, pages) =
        crate::navkv::nav_kv_store_without_pages_and_pages_for_test(&entries, 4096);
    let current = RoutingEditor::default();
    let mut rounds = 0;
    let opened = loop {
        match current.open(&store, &original, &row, 7, AirwayNavigationMode::Vor) {
            Ok(opened) => break opened,
            Err(HadReadError::NeedPages(missing)) => {
                assert_eq!(
                    current,
                    RoutingEditor::default(),
                    "no intermediate picker or draft"
                );
                rounds += 1;
                assert!(rounds <= pages.len() + 1);
                for id in missing {
                    store.insert_page(id, pages[id as usize].clone());
                }
            }
            Err(error) => panic!("{error:?}"),
        }
    };
    assert!(rounds > 0);
    let view = opened.view(7).unwrap();
    assert!(view.map_open);
    assert!(view.route.is_some());
    assert!(view.endpoints.is_empty());
    assert_eq!(opened.navigation_mode(), Some(AirwayNavigationMode::Vor));
    assert_eq!(original.route_components, plan().route_components);
}

#[test]
fn airway_editor_multiple_endpoints_keep_picker_and_scrim_dismissal_without_cancel_button() {
    let mut original = plan();
    original.route_components.push(RouteComponent::Waypoint {
        waypoint: NavRef::Fix("BEEZR".into()),
    });
    let original = crate::build_flight_plan(original).unwrap();
    let row = crate::project_ui_state(&original).display_rows[0]
        .uid
        .clone();
    let editor = RoutingEditor::default()
        .open(&store(), &original, &row, 7, AirwayNavigationMode::Gnss)
        .unwrap();
    let view = editor.view(7).unwrap();
    assert!(!view.map_open);
    assert_eq!(
        view.endpoints
            .iter()
            .map(|end| end.label.as_str())
            .collect::<Vec<_>>(),
        vec!["END", "BEEZR"]
    );
    assert!(view.controls.is_empty());
    assert!(!view.dismiss_action_id.is_empty());
    let Transition::Editor(closed) = editor
        .advance(&store(), &original, &view.dismiss_action_id, 7)
        .unwrap()
    else {
        panic!("dismiss")
    };
    assert!(closed.view(7).is_none());
    let Transition::Editor(draft) = editor
        .advance(&store(), &original, &view.endpoints[1].action_id, 7)
        .unwrap()
    else {
        panic!("endpoint")
    };
    assert!(draft.view(7).unwrap().map_open);
    assert_eq!(
        draft
            .editor
            .as_ref()
            .unwrap()
            .draft
            .as_ref()
            .unwrap()
            .destination,
        NavRef::Fix("BEEZR".into())
    );
}

#[test]
fn airway_editor_navigation_mode_recomputes_altitudes_and_undo_restores_mode_and_pins() {
    let original = plan();
    let mut graph = graph();
    graph.nodes[0].edges.retain(|edge| edge.to == 1);
    graph.nodes[0].edges[0].mea_ft = Some(16000);
    graph.nodes[0].edges[0].gnss_mea_ft = Some(11700);
    graph.nodes[1]
        .edges
        .iter_mut()
        .find(|edge| edge.to == 4)
        .unwrap()
        .mea_ft = Some(12000);
    let editor = open(&original, &custom_store(&graph));
    let route = editor.view(7).unwrap().route.unwrap();
    assert_eq!(route.legs[0].label, "V4 · GNSS MEA 11,700 ft");
    assert!(!route.legs[0].highest_mea);
    assert!(route.legs[1].highest_mea);
    assert!(route
        .legs
        .iter()
        .any(|leg| leg.highest_mea && leg.label.contains("12,000 ft")));
    let pinned = editor
        .drag(
            7,
            &editor.view(7).unwrap().edit_id,
            UiAirwayRouteDragPhase::Commit,
            position(&graph.nodes[1]),
            1.0,
            0,
            None,
        )
        .unwrap();
    let vor = choose_navigation(&pinned, &original, "VOR");
    let view = vor.view(7).unwrap();
    assert_eq!(view.via_points[0].label, "YKM");
    let route = view.route.unwrap();
    assert_eq!(route.legs[0].label, "V4 · MEA 16,000 ft");
    assert!(route.legs[0].highest_mea);
    assert!(!route.legs[1].highest_mea);
    assert!(route
        .legs
        .iter()
        .any(|leg| leg.highest_mea && leg.label.contains("16,000 ft")));
    assert!(!route.summary.contains("GNSS"));
    let Transition::Editor(undo_mode) = click(&vor, &original, "Undo") else {
        panic!("undo")
    };
    assert_eq!(
        undo_mode.navigation_mode(),
        Some(AirwayNavigationMode::Gnss)
    );
    assert_eq!(undo_mode.view(7).unwrap().via_points[0].label, "YKM");
    let Transition::Editor(undo_pin) = click(&undo_mode, &original, "Undo") else {
        panic!("undo")
    };
    assert!(undo_pin.view(7).unwrap().via_points.is_empty());
    let Transition::Editor(redo_pin) = click(&undo_pin, &original, "Redo") else {
        panic!("redo")
    };
    let Transition::Editor(redo_mode) = click(&redo_pin, &original, "Redo") else {
        panic!("redo")
    };
    assert_eq!(redo_mode.navigation_mode(), Some(AirwayNavigationMode::Vor));
    assert_eq!(redo_mode.view(7).unwrap().via_points[0].label, "YKM");
    assert_eq!(original.route_components, plan().route_components);
}

#[test]
fn airway_editor_keeps_incompatible_pins_and_disables_apply_until_a_route_exists() {
    let original = plan();
    let mut graph = graph();
    for node in &mut graph.nodes {
        for edge in &mut node.edges {
            if edge.airway_name == "V4" {
                edge.airway_name = "T261".into();
            }
            if edge.airway_name == "V298" {
                edge.mea_ft = Some(7000);
            }
        }
    }
    let editor = open(&original, &custom_store(&graph));
    let pinned = editor
        .drag(
            7,
            &editor.view(7).unwrap().edit_id,
            UiAirwayRouteDragPhase::Commit,
            position(&graph.nodes[1]),
            1.0,
            0,
            None,
        )
        .unwrap();
    let vor = choose_navigation(&pinned, &original, "VOR");
    let view = vor.view(7).unwrap();
    assert_eq!(view.via_points[0].label, "YKM");
    assert!(view.route.is_none());
    assert!(view.message.contains("No useful VOR airway route"));
    assert!(
        !view
            .controls
            .iter()
            .map(|control| &control.button)
            .find(|button| button.label == "Apply")
            .unwrap()
            .enabled
    );
    let restored = choose_navigation(&vor, &original, "GNSS");
    assert_eq!(restored.view(7).unwrap().via_points[0].label, "YKM");
    assert!(restored.view(7).unwrap().route.is_some());
    let Transition::Editor(unpinned) = vor
        .advance(
            &store(),
            &original,
            &view.via_points[0].remove_action.action_id,
            7,
        )
        .unwrap()
    else {
        panic!("remove pin")
    };
    assert!(unpinned.view(7).unwrap().via_points.is_empty());
    assert!(unpinned.view(7).unwrap().route.is_some());
    assert_eq!(unpinned.navigation_mode(), Some(AirwayNavigationMode::Vor));
    assert!(
        unpinned
            .advance(&store(), &original, &view.controls[1].button.action_id, 7)
            .is_err(),
        "stale toggle action"
    );
}

#[test]
fn airway_editor_previews_lower_mea_route_then_applies_as_one_undoable_edit() {
    let original = plan();
    let store = store();
    let editor = open(&original, &store);
    let view = editor.view(7).unwrap();
    assert!(view.map_open);
    assert!(view.route.is_some());
    assert!(view
        .route
        .as_ref()
        .unwrap()
        .legs
        .iter()
        .any(|leg| leg.highest_mea && leg.label.contains("10,000 ft")));
    let graph = graph();
    let at = position(&graph.nodes[2]);
    let preview = editor
        .drag(
            7,
            &view.edit_id,
            UiAirwayRouteDragPhase::Preview,
            at,
            1.0,
            0,
            None,
        )
        .unwrap();
    assert_eq!(
        preview.editor.as_ref().unwrap().draft.as_ref().unwrap().via,
        Vec::<u32>::new(),
        "preview is temporary"
    );
    let lower = preview.view(7).unwrap();
    assert!(lower
        .route
        .as_ref()
        .unwrap()
        .legs
        .iter()
        .any(|leg| leg.highest_mea && leg.label.contains("7,000 ft")));
    assert!(lower
        .route
        .as_ref()
        .unwrap()
        .legs
        .iter()
        .any(|leg| leg.label.contains("GNSS MEA 4,000 ft")));
    assert!(lower
        .route
        .as_ref()
        .unwrap()
        .crossings
        .iter()
        .any(|crossing| crossing.label == "BEEZR · V298 MCA 7,500 ft"));
    let committed = preview
        .drag(
            7,
            &view.edit_id,
            UiAirwayRouteDragPhase::Commit,
            at,
            1.0,
            0,
            None,
        )
        .unwrap();
    let Transition::Apply(applied) = click(&committed, &original, "Apply") else {
        panic!("apply")
    };
    assert_eq!(
        crate::project_ui_state(&applied)
            .display_rows
            .iter()
            .filter(|row| row.depth == 0)
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        "START V2 BEEZR V298 END"
    );
    assert_eq!(
        applied.route_component_uids.first(),
        original.route_component_uids.first()
    );
    assert_eq!(
        applied.route_component_uids.last(),
        original.route_component_uids.last()
    );
    let mut controller = FlightPlanController::new(original.clone(), Vec::new()).unwrap();
    controller.set_routing_editor(committed);
    controller.apply_definition_edit(applied).unwrap();
    assert!(controller.routing_editor().view(7).is_none());
    controller.undo_definition_edit().unwrap();
    assert_eq!(
        controller.active_plan().unwrap().route_components,
        original.route_components
    );
    assert!(!controller.can_undo());
}
#[test]
fn airway_editor_cancel_invalid_drop_stale_actions_and_epoch_do_not_change_the_plan() {
    let plan = plan();
    let editor = open(&plan, &store());
    let view = editor.view(7).unwrap();
    let preview = editor
        .drag(
            7,
            &view.edit_id,
            UiAirwayRouteDragPhase::Preview,
            position(&graph().nodes[2]),
            1.0,
            0,
            None,
        )
        .unwrap();
    let cancel = preview
        .drag(
            7,
            &view.edit_id,
            UiAirwayRouteDragPhase::Cancel,
            position(&graph().nodes[2]),
            1.0,
            0,
            None,
        )
        .unwrap();
    assert_eq!(
        cancel.editor.as_ref().unwrap().draft,
        editor.editor.as_ref().unwrap().draft
    );
    let failed = editor
        .drag(
            7,
            &view.edit_id,
            UiAirwayRouteDragPhase::Commit,
            position(&graph().nodes[5]),
            1.0,
            0,
            None,
        )
        .unwrap();
    assert_eq!(
        failed.editor.as_ref().unwrap().draft,
        editor.editor.as_ref().unwrap().draft
    );
    assert!(cancel
        .advance(&store(), &plan, &view.controls[4].button.action_id, 7)
        .is_err());
    assert!(editor.view(8).is_none());
    assert!(editor
        .drag(
            8,
            &view.edit_id,
            UiAirwayRouteDragPhase::Commit,
            position(&graph().nodes[2]),
            1.0,
            0,
            None
        )
        .is_err());
    let Transition::Editor(closed) = click(&editor, &plan, "Cancel") else {
        panic!("cancel")
    };
    assert!(closed.view(7).is_none());
    assert_eq!(plan.route_components.len(), 2);
}
#[test]
fn airway_editor_move_remove_and_undo_pin_preserve_a_reopenable_interval() {
    let original = plan();
    let editor = open(&original, &store());
    let id = editor.view(7).unwrap().edit_id;
    let first = editor
        .drag(
            7,
            &id,
            UiAirwayRouteDragPhase::Commit,
            position(&graph().nodes[2]),
            1.0,
            0,
            None,
        )
        .unwrap();
    let moved = first
        .drag(
            7,
            &id,
            UiAirwayRouteDragPhase::Commit,
            position(&graph().nodes[1]),
            1.0,
            0,
            Some(0),
        )
        .unwrap();
    assert_eq!(moved.view(7).unwrap().via_points[0].label, "YKM");
    let Transition::Editor(undone) = click(&moved, &original, "Undo") else {
        panic!("undo")
    };
    assert_eq!(undone.view(7).unwrap().via_points[0].label, "BEEZR");
    let remove = undone.view(7).unwrap().via_points[0]
        .remove_action
        .action_id
        .clone();
    let Transition::Editor(removed) = undone.advance(&store(), &original, &remove, 7).unwrap()
    else {
        panic!("remove")
    };
    assert!(removed.view(7).unwrap().via_points.is_empty());
    let Transition::Apply(applied) = click(&first, &original, "Apply") else {
        panic!("apply")
    };
    let reopened = open(&applied, &store());
    assert!(
        reopened.view(7).unwrap().via_points.is_empty(),
        "no hidden history"
    );
    assert!(reopened.view(7).unwrap().route.is_some());
    let frame = reopened.viewport(7, 800.0, 1000.0, 35.0).unwrap();
    assert!(frame.zoom.is_finite());
    assert_eq!(frame.rotation_deg, 35.0);
}

#[test]
fn airway_editor_intervals_stop_at_procedures_and_resume_after_them() {
    let waypoint = |name: &str| RouteComponent::Waypoint {
        waypoint: NavRef::Fix(name.into()),
    };
    let plan = FlightPlan {
        route_components: vec![
            RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KRNT".into()),
            },
            RouteComponent::Procedure {
                procedure: crate::ProcedureSegment {
                    airport_id: crate::AirportId("KRNT".into()),
                    procedure_id: "BELVU4".into(),
                    display_label: None,
                    kind: crate::ProcedureKind::Sid,
                    runway_transition: None,
                    enroute_transition: None,
                    terminal_discontinuity: None,
                    data_quality: Vec::new(),
                },
            },
            waypoint("START"),
            waypoint("MID"),
            waypoint("END"),
        ],
        ..FlightPlan::default()
    };
    assert!(eligible_ends(&plan, 0).is_empty());
    assert!(eligible_ends(&plan, 1).is_empty());
    assert_eq!(eligible_ends(&plan, 2), vec![3, 4]);
    assert_eq!(eligible_ends(&plan, 3), vec![4]);
    assert!(eligible_ends(&plan, 4).is_empty());
}

#[test]
fn airway_editor_original_map_path_ends_at_the_selected_boundary() {
    let mut plan = plan();
    plan.route_components.push(RouteComponent::Waypoint {
        waypoint: NavRef::Fix("BEEZR".into()),
    });
    let plan = crate::build_flight_plan(plan).unwrap();
    let row = crate::project_ui_state(&plan).display_rows[0].uid.clone();
    let editor = RoutingEditor::default()
        .open(&store(), &plan, &row, 7, AirwayNavigationMode::Gnss)
        .unwrap();
    let action = editor.view(7).unwrap().endpoints[0].action_id.clone();
    let Transition::Editor(editor) = editor.advance(&store(), &plan, &action, 7).unwrap() else {
        panic!("draft")
    };
    let view = editor.view(7).unwrap();
    let graph = graph();
    assert!(
        !view
            .original_path
            .contains(&ui_position(position(&graph.nodes[2]))),
        "the following interval stays outside this editor's map path"
    );
    assert_eq!(
        view.original_path.last(),
        Some(&ui_position(position(&graph.nodes[4])))
    );
}

#[test]
fn airway_editor_undo_redo_pin_move_and_unpin_are_separate_from_plan_history() {
    let original = plan();
    let editor = open(&original, &store());
    let id = editor.view(7).unwrap().edit_id;
    let edit = |editor: &RoutingEditor, label: &str| {
        let Transition::Editor(next) = click(editor, &original, label) else {
            panic!("editor")
        };
        next
    };
    let pin = |editor: &RoutingEditor, node: usize, moving| {
        editor
            .drag(
                7,
                &id,
                UiAirwayRouteDragPhase::Commit,
                position(&graph().nodes[node]),
                1.0,
                0,
                moving,
            )
            .unwrap()
    };
    let first = pin(&editor, 2, None);
    let moved = pin(&first, 1, Some(0));
    let remove = moved.view(7).unwrap().via_points[0]
        .remove_action
        .action_id
        .clone();
    let Transition::Editor(removed) = moved.advance(&store(), &original, &remove, 7).unwrap()
    else {
        panic!("editor")
    };
    assert!(removed.view(7).unwrap().via_points.is_empty());
    let undo_remove = edit(&removed, "Undo");
    assert_eq!(undo_remove.view(7).unwrap().via_points[0].label, "YKM");
    let undo_move = edit(&undo_remove, "Undo");
    assert_eq!(undo_move.view(7).unwrap().via_points[0].label, "BEEZR");
    let undo_pin = edit(&undo_move, "Undo");
    assert!(undo_pin.view(7).unwrap().via_points.is_empty());
    let redo_pin = edit(&undo_pin, "Redo");
    assert_eq!(redo_pin.view(7).unwrap().via_points[0].label, "BEEZR");
    let redo_move = edit(&redo_pin, "Redo");
    assert_eq!(redo_move.view(7).unwrap().via_points[0].label, "YKM");
    let redo_remove = edit(&redo_move, "Redo");
    assert!(redo_remove.view(7).unwrap().via_points.is_empty());
    let branch = pin(&undo_pin, 1, None);
    assert!(
        !branch
            .view(7)
            .unwrap()
            .controls
            .iter()
            .map(|control| &control.button)
            .find(|b| b.label == "Redo")
            .unwrap()
            .enabled
    );
    assert_eq!(original.route_components.len(), 2);
}

#[test]
fn airway_editor_starting_again_replaces_the_existing_activity() {
    let original = plan();
    let editor = open(&original, &store());
    let row = crate::project_ui_state(&original).display_rows[0]
        .uid
        .clone();
    let fresh = editor
        .open(&store(), &original, &row, 7, AirwayNavigationMode::Gnss)
        .unwrap();
    assert_ne!(
        editor.view(7).unwrap().edit_id,
        fresh.view(7).unwrap().edit_id
    );
    assert!(fresh.view(7).unwrap().map_open);
    assert!(fresh.view(7).unwrap().endpoints.is_empty());
}

fn custom_store(graph: &Graph) -> NavKvStore {
    let mut records = crate::airway_routing::tests::records(graph);
    for node in &graph.nodes {
        let key = crate::navkv::nav_kv_key_for_query(&crate::NavKvQuery::NavRefSymbol {
            nav_ref: node_ref(node),
        })
        .unwrap();
        records.push((key, serde_json::to_vec(&serde_json::json!({
            "kind": "yrep-pt", "label": label(&node_ref(node)),
            "symbol_kind": if matches!(node_ref(node), NavRef::Navaid(_)) { "nav" } else { "fix" },
            "style_class": "",
        })).unwrap()));
    }
    records.sort_by(|a, b| a.0.cmp(&b.0));
    crate::navkv::nav_kv_store_for_test(
        &records
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect::<Vec<_>>(),
        4096,
    )
}

#[test]
fn airway_editor_counts_gnss_only_meas_and_keeps_missing_altitudes_explicit() {
    let original = plan();
    let mut graph = graph();
    graph.nodes[0].edges.retain(|edge| edge.to == 2);
    let edge = graph.nodes[3]
        .edges
        .iter_mut()
        .find(|edge| edge.to == 4)
        .unwrap();
    edge.airway_name = "T261".into();
    edge.gnss_mea_ft = Some(9000);
    let editor = open(&original, &custom_store(&graph));
    let route = editor.view(7).unwrap().route.unwrap();
    let highest = route
        .legs
        .iter()
        .filter(|leg| leg.highest_mea)
        .collect::<Vec<_>>();
    assert_eq!(highest.len(), 1);
    assert_eq!(highest[0].label, "T261 · GNSS MEA 9,000 ft");
    // GNSS mode falls back to conventional MEA when no GNSS MEA is published.
    assert!(route
        .legs
        .iter()
        .any(|leg| leg.label.contains("V298 · MEA 7,000 ft")));
    graph.nodes[3]
        .edges
        .iter_mut()
        .find(|edge| edge.to == 4)
        .unwrap()
        .gnss_mea_ft = None;
    let route = open(&original, &custom_store(&graph))
        .view(7)
        .unwrap()
        .route
        .unwrap();
    assert!(route
        .legs
        .iter()
        .any(|leg| leg.label == "T261 · MEA unknown"));
}

#[test]
fn airway_editor_places_crossing_altitudes_at_the_named_fix_not_the_source_segment() {
    let mut graph = graph();
    graph.nodes[0].nav_ref = product_contracts::AirwayRoutingReference::Navaid("MLD".into());
    graph.nodes[1].nav_ref = product_contracts::AirwayRoutingReference::Fix("HUSEM".into());
    graph.nodes[4].nav_ref = product_contracts::AirwayRoutingReference::Fix("ORNEY".into());
    graph.nodes[0].edges.retain(|edge| edge.to == 1);
    for (from, to) in [(0, 1), (1, 4)] {
        let edge = graph.nodes[from]
            .edges
            .iter_mut()
            .find(|edge| edge.to == to)
            .unwrap();
        edge.airway_name = "V142".into();
        edge.branch_key.clear();
        edge.mea_ft = Some(10400);
        // Duplicate source mentions still produce only one point callout.
        edge.crossing_altitude_ft = Some(11200);
        edge.crossing_point = "V142 *ORNEY*UT".into();
    }
    let make_plan = |to| {
        crate::build_flight_plan(FlightPlan {
            route_components: [NavRef::Navaid("MLD".into()), NavRef::Fix(to)]
                .into_iter()
                .map(|waypoint| RouteComponent::Waypoint { waypoint })
                .collect(),
            ..FlightPlan::default()
        })
        .unwrap()
    };
    let store = custom_store(&graph);
    let route = open(&make_plan("ORNEY".into()), &store)
        .view(7)
        .unwrap()
        .route
        .unwrap();
    assert_eq!(
        route.crossings,
        vec![UiAirwayRouteCrossing {
            label: "ORNEY · V142 MCA 11,200 ft".into(),
            position: ui_position(position(&graph.nodes[4])),
        }]
    );
    assert!(route
        .legs
        .iter()
        .all(|leg| leg.label == "V142 · MEA 10,400 ft"));
    assert!(route.legs.iter().all(|leg| leg.highest_mea));
    assert_eq!(route.crossings[0].label, "ORNEY · V142 MCA 11,200 ft");
    let short = open(&make_plan("HUSEM".into()), &store)
        .view(7)
        .unwrap()
        .route
        .unwrap();
    assert!(
        short.crossings.is_empty(),
        "ORNEY is outside this route's airway portion"
    );
    assert!(short.crossings.is_empty());
}

#[test]
fn airway_editor_labels_changes_with_nav_symbols_and_does_not_duplicate_pins() {
    let original = plan();
    let mut graph = graph();
    graph.nodes[0].edges.retain(|edge| edge.to == 2);
    graph.nodes[3].nav_ref = product_contracts::AirwayRoutingReference::Navaid("ELN".into());
    graph.nodes[3]
        .edges
        .iter_mut()
        .find(|edge| edge.to == 4)
        .unwrap()
        .airway_name = "V8".into();
    let store = custom_store(&graph);
    let editor = open(&original, &store);
    let view = editor.view_with_symbols(7, Some(&store)).unwrap().unwrap();
    let junctions = &view.route.as_ref().unwrap().junctions;
    assert_eq!(
        junctions
            .iter()
            .map(|j| (
                j.label.as_str(),
                j.symbol_feature.as_ref().unwrap().symbol_kind.as_str()
            ))
            .collect::<Vec<_>>(),
        [("BEEZR", "fix"), ("ELN", "nav")]
    );
    let pinned = editor
        .drag(
            7,
            &view.edit_id,
            UiAirwayRouteDragPhase::Commit,
            position(&graph.nodes[2]),
            1.0,
            0,
            None,
        )
        .unwrap();
    let pinned_view = pinned.view_with_symbols(7, Some(&store)).unwrap().unwrap();
    assert_eq!(pinned_view.via_points[0].label, "BEEZR");
    assert_eq!(
        pinned_view
            .route
            .unwrap()
            .junctions
            .iter()
            .map(|j| j.label.as_str())
            .collect::<Vec<_>>(),
        ["ELN"]
    );
    // Pure projection must not change the draft or the flight plan.
    assert_eq!(editor.view(7).unwrap().via_points.len(), 0);
    assert_eq!(original, plan());
}
