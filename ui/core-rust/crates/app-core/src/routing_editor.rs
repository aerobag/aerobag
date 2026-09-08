// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    airway_routing::{node_ref, position, AirwayNavigationMode, Graph, Path},
    had_ops::{nav_ref_position, HadReadError},
    AirwaySegment, FlightPlan, LatLon, NavKvStore, NavRef, ResolvedLeg, ResolvedLegSource,
    RouteComponent,
};
use app_ui_contracts::session::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, Clone, PartialEq)]
enum Action {
    End(usize),
    Remove(usize),
    Undo,
    Redo,
    Apply,
    Close,
    Navigation(AirwayNavigationMode),
}
#[derive(Debug, Clone, PartialEq)]
struct EditState {
    via: Vec<u32>,
    mode: AirwayNavigationMode,
}
#[derive(Debug, Clone, PartialEq)]
struct Draft {
    graph: Arc<Graph>,
    end: usize,
    origin: NavRef,
    destination: NavRef,
    origin_position: LatLon,
    destination_position: LatLon,
    via: Vec<u32>,
    mode: AirwayNavigationMode,
    paths: Vec<Path>,
    history: Vec<EditState>,
    future: Vec<EditState>,
    original: Vec<UiAirwayRoutePosition>,
    drag: Option<Drag>,
}
#[derive(Debug, Clone, PartialEq)]
struct Drag {
    target: Option<u32>,
    via: Vec<u32>,
    paths: Vec<Path>,
}
#[derive(Debug, Clone, PartialEq)]
struct Editor {
    epoch: u64,
    initial_mode: AirwayNavigationMode,
    start: usize,
    row_uid: String,
    edit_id: String,
    ends: Vec<(usize, String)>,
    draft: Option<Draft>,
    actions: BTreeMap<String, Action>,
    view: UiAirwayRouting,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct RoutingEditor {
    generation: u64,
    serial: u64,
    editor: Option<Editor>,
}
pub(crate) enum Transition {
    Editor(RoutingEditor),
    Apply(FlightPlan),
}

fn fatal(message: &str) -> HadReadError {
    HadReadError::Fatal(message.into())
}
fn label(nav_ref: &NavRef) -> String {
    crate::nav_ref_picker_label(nav_ref)
}
fn ui_position(position: LatLon) -> UiAirwayRoutePosition {
    UiAirwayRoutePosition {
        lat: position.lat,
        lon: position.lon,
    }
}

/// NASR can store an MCA on an earlier segment than its crossing fix (for
/// example MLD–HUSEM carries ORNEY's MCA). Resolve that named point within the
/// portion of the same airway/branch we actually fly, never at the record's
/// segment midpoint. Strip the source's airway/state/type encoding from UI text.
fn route_crossings(graph: &Graph, path: &Path) -> (Vec<UiAirwayRouteCrossing>, Option<u32>) {
    let mut crossings = BTreeMap::new();
    for step in &path.steps {
        let edge = &graph.nodes[step.from as usize].edges[step.edge];
        let Some(altitude_ft) = edge.crossing_altitude_ft else {
            continue;
        };
        let point = edge
            .crossing_point
            .split('*')
            .nth(1)
            .unwrap_or(&edge.crossing_point)
            .trim();
        if point.is_empty() {
            continue;
        }
        let node = path.steps.iter().find_map(|candidate| {
            let next = &graph.nodes[candidate.from as usize].edges[candidate.edge];
            if next.airway_name != edge.airway_name || next.branch_key != edge.branch_key {
                return None;
            }
            [candidate.from, next.to]
                .into_iter()
                .map(|id| &graph.nodes[id as usize])
                .find(|node| label(&node_ref(node)) == point)
        });
        if let Some(node) = node {
            crossings.insert((node.id, edge.airway_name.clone(), altitude_ft), node);
        }
    }
    let highest = crossings.keys().map(|(_, _, altitude)| *altitude).max();
    let labels = crossings
        .into_iter()
        .map(|((_, airway, altitude_ft), node)| UiAirwayRouteCrossing {
            label: format!(
                "{} · {airway} MCA {} ft",
                label(&node_ref(node)),
                altitude(altitude_ft)
            ),
            position: ui_position(position(node)),
        })
        .collect();
    (labels, highest)
}

pub(crate) fn eligible_ends(plan: &FlightPlan, start: usize) -> Vec<usize> {
    if !matches!(
        plan.route_components.get(start),
        Some(RouteComponent::Waypoint { .. })
    ) || crate::flight_plan_has_direct_to_overlay(plan)
    {
        return Vec::new();
    }
    plan.route_components
        .iter()
        .enumerate()
        .skip(start + 1)
        .take_while(|(_, component)| !matches!(component, RouteComponent::Procedure { .. }))
        .filter_map(|(index, component)| {
            matches!(component, RouteComponent::Waypoint { .. }).then_some(index)
        })
        .collect()
}

impl RoutingEditor {
    pub fn viewport(
        &self,
        epoch: u64,
        width: f64,
        height: f64,
        rotation_deg: f64,
    ) -> Option<crate::MapViewport> {
        use crate::ui_geometry::{ui_lat_lon_to_world, ui_world_to_lat_lon, UiGeometryPoint};
        let editor = self
            .editor
            .as_ref()
            .filter(|editor| editor.epoch == epoch)?;
        let draft = editor.draft.as_ref()?;
        if !width.is_finite()
            || !height.is_finite()
            || !rotation_deg.is_finite()
            || width <= 0.0
            || height <= 0.0
        {
            return None;
        }
        let anchor = ui_lat_lon_to_world(draft.origin_position);
        let (sin, cos) = rotation_deg.to_radians().sin_cos();
        let mut lo = UiGeometryPoint {
            x: f64::INFINITY,
            y: f64::INFINITY,
        };
        let mut hi = UiGeometryPoint {
            x: f64::NEG_INFINITY,
            y: f64::NEG_INFINITY,
        };
        let view = &editor.view;
        for point in view.original_path.iter().chain(
            view.route
                .iter()
                .flat_map(|path| path.legs.iter().flat_map(|leg| [&leg.from, &leg.to])),
        ) {
            let world = ui_lat_lon_to_world(LatLon {
                lat: point.lat,
                lon: point.lon,
            });
            let x = world.x + ((anchor.x - world.x) / 256.0).round() * 256.0 - anchor.x;
            let y = world.y - anchor.y;
            let rotated = UiGeometryPoint {
                x: x * cos + y * sin,
                y: y * cos - x * sin,
            };
            lo.x = lo.x.min(rotated.x);
            lo.y = lo.y.min(rotated.y);
            hi.x = hi.x.max(rotated.x);
            hi.y = hi.y.max(rotated.y);
        }
        let top_inset = 80.0_f64.min(height * 0.2);
        let usable_height = height * 0.58 - top_inset;
        let zoom = ((width * 0.8 / (hi.x - lo.x).max(0.001))
            .min(usable_height * 0.8 / (hi.y - lo.y).max(0.001)))
        .log2()
        .clamp(2.0, 12.0);
        let x = (lo.x + hi.x) / 2.0;
        let y = (lo.y + hi.y) / 2.0 + (height * 0.42 - top_inset) / 2.0 / 2.0_f64.powf(zoom);
        Some(crate::MapViewport {
            center: ui_world_to_lat_lon(UiGeometryPoint {
                x: anchor.x + x * cos - y * sin,
                y: anchor.y + x * sin + y * cos,
            }),
            zoom,
            rotation_deg,
            pitch_deg: 0.0,
        })
    }

    pub fn close(&mut self) {
        self.editor = None;
    }
    pub fn navigation_mode(&self) -> Option<AirwayNavigationMode> {
        self.editor.as_ref().map(|editor| {
            editor
                .draft
                .as_ref()
                .map_or(editor.initial_mode, |draft| draft.mode)
        })
    }
    pub fn view(&self, epoch: u64) -> Option<UiAirwayRouting> {
        self.editor
            .as_ref()
            .filter(|editor| editor.epoch == epoch)
            .map(|editor| editor.view.clone())
    }
    /// Resolve junction symbols through the same NAVKV symbol reader as map
    /// features and flight-plan rows. Collect pages across the whole route.
    pub fn view_with_symbols(
        &self,
        epoch: u64,
        store: Option<&NavKvStore>,
    ) -> Result<Option<UiAirwayRouting>, HadReadError> {
        let Some(mut view) = self.view(epoch) else {
            return Ok(None);
        };
        let Some(store) = store else {
            return Ok(Some(view));
        };
        let mut missing = BTreeSet::new();
        if let (Some(route), Some(draft)) = (
            view.route.as_mut(),
            self.editor
                .as_ref()
                .and_then(|editor| editor.draft.as_ref()),
        ) {
            for junction in &mut route.junctions {
                let node = &draft.graph.nodes[junction.node_id as usize];
                match crate::had_ops::nav_symbol_feature(store, &node_ref(node)) {
                    Ok(symbol) => junction.symbol_feature = symbol,
                    Err(HadReadError::NeedPages(pages)) => missing.extend(pages),
                    Err(error) => return Err(error),
                }
            }
        }
        if !missing.is_empty() {
            return Err(HadReadError::NeedPages(missing.into_iter().collect()));
        }
        Ok(Some(view))
    }

    pub fn open(
        &self,
        store: &NavKvStore,
        plan: &FlightPlan,
        row_uid: &str,
        epoch: u64,
        mode: AirwayNavigationMode,
    ) -> Result<Self, HadReadError> {
        let row = crate::project_ui_state(plan)
            .display_rows
            .into_iter()
            .find(|row| row.uid == row_uid && row.depth == 0)
            .ok_or_else(|| fatal("Choose a standalone waypoint."))?;
        let start = row
            .component_uid
            .as_ref()
            .and_then(|uid| {
                plan.route_component_uids
                    .iter()
                    .position(|item| item == uid)
            })
            .ok_or_else(|| fatal("The routing start is no longer available."))?;
        let ends = eligible_ends(plan, start)
            .into_iter()
            .map(|index| match &plan.route_components[index] {
                RouteComponent::Waypoint { waypoint } => (index, label(waypoint)),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        if ends.is_empty() {
            return Err(fatal(
                "There is no later waypoint before the next procedure.",
            ));
        }
        let mut next = self.clone();
        next.generation += 1;
        let edit_id = format!("airway-route-{}", next.generation);
        next.editor = Some(Editor {
            epoch,
            initial_mode: mode,
            start,
            row_uid: row_uid.into(),
            edit_id: edit_id.clone(),
            ends,
            draft: None,
            actions: BTreeMap::new(),
            view: UiAirwayRouting {
                edit_id,
                row_uid: row_uid.into(),
                title: format!("Route from {} to…", row.label),
                map_open: false,
                message: String::new(),
                endpoints: Vec::new(),
                controls: Vec::new(),
                route: None,
                via_points: Vec::new(),
                dismiss_action_id: String::new(),
                original_path: Vec::new(),
                drag_target: None,
                drag_label: String::new(),
            },
        });
        if let Some(editor) = next.editor.as_mut().filter(|editor| editor.ends.len() == 1) {
            editor.select_end(store, plan, editor.ends[0].0)?;
        }
        next.render();
        Ok(next)
    }

    pub fn advance(
        &self,
        store: &NavKvStore,
        plan: &FlightPlan,
        action_id: &str,
        epoch: u64,
    ) -> Result<Transition, HadReadError> {
        let mut next = self.clone();
        let editor = next
            .editor
            .as_mut()
            .filter(|editor| editor.epoch == epoch)
            .ok_or_else(|| fatal("The route editor is closed or navigation data changed."))?;
        let action = editor
            .actions
            .get(action_id)
            .cloned()
            .ok_or_else(|| fatal("That route editor action is stale."))?;
        match action {
            Action::Close => {
                next.close();
                return Ok(Transition::Editor(next));
            }
            Action::End(end) => editor.select_end(store, plan, end)?,
            Action::Apply => {
                let draft = editor
                    .draft
                    .as_ref()
                    .ok_or_else(|| fatal("Choose a destination first."))?;
                let path = draft
                    .paths
                    .first()
                    .ok_or_else(|| fatal("Choose an airway route first."))?;
                let segments = materialize(&draft.graph, path);
                let plan = crate::planning::replace_airway_route_span(
                    plan,
                    editor.start,
                    draft.end,
                    segments,
                )
                .map_err(|error| fatal(&error.message))?;
                return Ok(Transition::Apply(plan));
            }
            action => {
                let draft = editor
                    .draft
                    .as_mut()
                    .ok_or_else(|| fatal("Choose a destination first."))?;
                draft.drag = None;
                match action {
                    Action::Undo => {
                        if let Some(state) = draft.history.pop() {
                            draft.future.push(draft.edit_state());
                            draft.restore(state);
                        }
                    }
                    Action::Remove(index) => {
                        if index < draft.via.len() {
                            draft.history.push(draft.edit_state());
                            draft.future.clear();
                            draft.via.remove(index);
                            draft.recompute();
                        }
                    }
                    Action::Redo => {
                        if let Some(state) = draft.future.pop() {
                            draft.history.push(draft.edit_state());
                            draft.restore(state);
                        }
                    }
                    Action::Navigation(mode) => {
                        if draft.mode != mode {
                            draft.history.push(draft.edit_state());
                            draft.future.clear();
                            draft.mode = mode;
                            draft.recompute();
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        next.render();
        Ok(Transition::Editor(next))
    }

    pub fn drag(
        &self,
        epoch: u64,
        edit_id: &str,
        phase: UiAirwayRouteDragPhase,
        at: LatLon,
        radius_nm: f64,
        insert: u32,
        moving: Option<u32>,
    ) -> Result<Self, HadReadError> {
        let mut next = self.clone();
        let editor = next
            .editor
            .as_mut()
            .filter(|editor| editor.epoch == epoch && editor.edit_id == edit_id)
            .ok_or_else(|| fatal("The route editor changed during the drag."))?;
        let draft = editor
            .draft
            .as_mut()
            .ok_or_else(|| fatal("Choose a destination first."))?;
        if phase == UiAirwayRouteDragPhase::Cancel {
            draft.drag = None;
            next.render();
            return Ok(next);
        }
        if !at.lat.is_finite()
            || !at.lon.is_finite()
            || !radius_nm.is_finite()
            || radius_nm <= 0.0
            || insert as usize > draft.via.len()
            || moving.is_some_and(|index| index as usize >= draft.via.len())
        {
            return Err(fatal("Invalid route drag."));
        }
        let target = draft
            .graph
            .nodes
            .iter()
            .filter(|node| draft.graph.is_target(node.id, draft.mode))
            .map(|node| (node.id, crate::flight_leg_distance_nm(at, position(node))))
            .filter(|(_, distance)| *distance <= radius_nm.min(25.0))
            .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)))
            .map(|(id, _)| id);
        if draft.drag.as_ref().map(|drag| drag.target) != Some(target) {
            let mut via = draft.via.clone();
            if let Some(id) = target {
                if let Some(index) = moving {
                    via[index as usize] = id;
                } else {
                    via.insert(insert as usize, id);
                }
            }
            let paths = if target.is_some()
                && via.iter().collect::<std::collections::BTreeSet<_>>().len() == via.len()
            {
                draft.graph.routes(
                    &draft.origin,
                    draft.origin_position,
                    &draft.destination,
                    draft.destination_position,
                    &via,
                    draft.mode,
                )
            } else {
                Vec::new()
            };
            draft.drag = Some(Drag { target, via, paths });
        }
        if phase == UiAirwayRouteDragPhase::Commit {
            if let Some(drag) = draft.drag.take().filter(|drag| !drag.paths.is_empty()) {
                if drag.via != draft.via {
                    draft.history.push(draft.edit_state());
                    draft.future.clear();
                }
                draft.via = drag.via;
                draft.paths = drag.paths;
            }
        }
        next.render();
        Ok(next)
    }

    fn render(&mut self) {
        self.serial += 1;
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        editor.actions.clear();
        let mut index = 0;
        let mut button = |label: String, action: Action, enabled: bool| {
            index += 1;
            let id = format!("{}-{}-{index}", editor.edit_id, self.serial);
            if enabled {
                editor.actions.insert(id.clone(), action);
            }
            UiAirwayPickerButton {
                action_id: id.clone(),
                label,
                enabled,
                disabled_reason: None,
                suggested: false,
                test_id: id,
            }
        };
        let view = &mut editor.view;
        view.endpoints.clear();
        view.controls.clear();
        view.route = None;
        view.via_points.clear();
        view.drag_target = None;
        view.drag_label.clear();
        let close = button("Cancel".into(), Action::Close, true);
        view.dismiss_action_id = close.action_id.clone();
        let Some(draft) = &editor.draft else {
            for (end, label) in &editor.ends {
                view.endpoints
                    .push(button(label.clone(), Action::End(*end), true));
            }
            return;
        };
        view.map_open = true;
        for mode in [AirwayNavigationMode::Vor, AirwayNavigationMode::Gnss] {
            view.controls.push(UiAirwayRouteControl {
                button: button(
                    mode.label().into(),
                    Action::Navigation(mode),
                    draft.drag.is_none(),
                ),
                selected: draft.mode == mode,
                symbol_id: match mode {
                    AirwayNavigationMode::Vor => "vor",
                    AirwayNavigationMode::Gnss => "gnss",
                }
                .into(),
            });
        }
        view.title = format!(
            "Replace route: {} → {}",
            label(&draft.origin),
            label(&draft.destination)
        );
        view.original_path = draft.original.clone();
        let successful_drag = draft.drag.as_ref().filter(|drag| !drag.paths.is_empty());
        let paths = successful_drag.map_or(&draft.paths, |drag| &drag.paths);
        let via = successful_drag.map_or(&draft.via, |drag| &drag.via);
        let direct =
            crate::flight_leg_distance_nm(draft.origin_position, draft.destination_position);
        view.message = if paths.is_empty() {
            format!("No useful {} airway route found with these endpoints and pins. Direct distance: {direct:.0} NM. Move or remove a pin, or switch navigation mode.", draft.mode.label())
        } else {
            String::new()
        };
        if let Some(drag) = &draft.drag {
            if let Some(id) = drag.target {
                let node = &draft.graph.nodes[id as usize];
                view.drag_target = Some(ui_position(position(node)));
                view.drag_label = if drag.paths.is_empty() {
                    format!(
                        "No {} airway route via {}",
                        draft.mode.label(),
                        label(&node_ref(node))
                    )
                } else {
                    format!("Via {}", label(&node_ref(node)))
                };
            } else {
                view.drag_label = "Move to a named fix".into();
            }
        }
        for path in paths.iter() {
            let highest = path
                .steps
                .iter()
                .filter_map(|step| {
                    draft
                        .mode
                        .mea(&draft.graph.nodes[step.from as usize].edges[step.edge])
                        .map(|(value, _)| value)
                })
                .max();
            let mut legs = Vec::new();
            let mut names = Vec::new();
            let mut junctions = Vec::new();
            let mut via_index = 0;
            let first = &draft.graph.nodes[path.steps[0].from as usize];
            if node_ref(first) != draft.origin {
                legs.push(UiAirwayRouteLeg {
                    from: ui_position(draft.origin_position),
                    to: ui_position(position(first)),
                    label: "Direct".into(),
                    highest_mea: false,
                    direct: true,
                    via_insert_index: 0,
                });
            }
            for step in &path.steps {
                let from = &draft.graph.nodes[step.from as usize];
                let edge = &from.edges[step.edge];
                let to = &draft.graph.nodes[edge.to as usize];
                if via.get(via_index) == Some(&from.id) {
                    via_index += 1;
                }
                if names.last() != Some(&edge.airway_name) {
                    // A pinned junction already has its purple circle and label.
                    if !names.is_empty() && !via.contains(&from.id) {
                        junctions.push(UiAirwayRouteJunction {
                            node_id: from.id,
                            label: label(&node_ref(from)),
                            position: ui_position(position(from)),
                            symbol_feature: None,
                        });
                    }
                    names.push(edge.airway_name.clone());
                }
                let mea = draft.mode.mea(edge);
                let text = match mea {
                    Some((mea, gnss)) => {
                        format!(
                            "{} · {}MEA {} ft",
                            edge.airway_name,
                            if gnss { "GNSS " } else { "" },
                            altitude(mea)
                        )
                    }
                    None => format!("{} · MEA unknown", edge.airway_name),
                };
                legs.push(UiAirwayRouteLeg {
                    from: ui_position(position(from)),
                    to: ui_position(position(to)),
                    label: text,
                    highest_mea: highest.is_some() && mea.map(|(value, _)| value) == highest,
                    direct: false,
                    via_insert_index: via_index as u32,
                });
            }
            let last_step = path.steps.last().expect("airway path");
            let last = &draft.graph.nodes
                [draft.graph.nodes[last_step.from as usize].edges[last_step.edge].to as usize];
            if node_ref(last) != draft.destination {
                legs.push(UiAirwayRouteLeg {
                    from: ui_position(position(last)),
                    to: ui_position(draft.destination_position),
                    label: "Direct".into(),
                    highest_mea: false,
                    direct: true,
                    via_insert_index: via.len() as u32,
                });
            }
            let (crossings, _) = route_crossings(&draft.graph, path);
            view.route = Some(UiAirwayRoute {
                summary: route_distance_summary(direct, path.distance_nm),
                legs,
                junctions,
                crossings,
            });
        }
        for (index, id) in via.iter().enumerate() {
            let node = &draft.graph.nodes[*id as usize];
            let remove = button(
                format!("Remove via {}", label(&node_ref(node))),
                Action::Remove(index),
                successful_drag.is_none(),
            );
            view.via_points.push(UiAirwayRouteVia {
                label: label(&node_ref(node)),
                position: ui_position(position(node)),
                remove_action: remove,
                index: index as u32,
            });
        }
        for (label, action, symbol, enabled) in [
            (
                "Undo",
                Action::Undo,
                "undo",
                !draft.history.is_empty() && draft.drag.is_none(),
            ),
            (
                "Redo",
                Action::Redo,
                "redo",
                !draft.future.is_empty() && draft.drag.is_none(),
            ),
            (
                "Apply",
                Action::Apply,
                "apply_route",
                !draft.paths.is_empty() && draft.drag.is_none(),
            ),
        ] {
            view.controls.push(UiAirwayRouteControl {
                button: button(label.into(), action, enabled),
                selected: false,
                symbol_id: symbol.into(),
            });
        }
        view.controls.push(UiAirwayRouteControl {
            button: close,
            selected: false,
            symbol_id: "remove".into(),
        });
    }
}

impl Editor {
    fn select_end(
        &mut self,
        store: &NavKvStore,
        plan: &FlightPlan,
        end: usize,
    ) -> Result<(), HadReadError> {
        if !eligible_ends(plan, self.start).contains(&end) {
            return Err(fatal("The selected route interval changed."));
        }
        let waypoint = |index| match &plan.route_components[index] {
            RouteComponent::Waypoint { waypoint } => waypoint.clone(),
            _ => unreachable!(),
        };
        let origin = waypoint(self.start);
        let destination = waypoint(end);
        let origin_position = nav_ref_position(store, &origin, None)?;
        let destination_position = nav_ref_position(store, &destination, None)?;
        let graph = Arc::new(Graph::load(store)?);
        let paths = graph.routes(
            &origin,
            origin_position,
            &destination,
            destination_position,
            &[],
            self.initial_mode,
        );
        let mut original = vec![ui_position(origin_position)];
        for leg in &plan.resolved_legs {
            let ResolvedLegSource::RouteComponent { component_index } = leg.source else {
                continue;
            };
            if component_index >= self.start && component_index < end {
                let pos = nav_ref_position(store, &leg.to, None)?;
                if original.last() != Some(&ui_position(pos)) {
                    original.push(ui_position(pos));
                }
            }
        }
        original.push(ui_position(destination_position));
        self.draft = Some(Draft {
            graph,
            end,
            origin,
            destination,
            origin_position,
            destination_position,
            via: Vec::new(),
            mode: self.initial_mode,
            paths,
            history: Vec::new(),
            future: Vec::new(),
            original,
            drag: None,
        });
        Ok(())
    }
}

impl Draft {
    fn edit_state(&self) -> EditState {
        EditState {
            via: self.via.clone(),
            mode: self.mode,
        }
    }
    fn restore(&mut self, state: EditState) {
        self.via = state.via;
        self.mode = state.mode;
        self.recompute();
    }
    fn recompute(&mut self) {
        self.paths = self.graph.routes(
            &self.origin,
            self.origin_position,
            &self.destination,
            self.destination_position,
            &self.via,
            self.mode,
        );
    }
}
fn route_distance_summary(direct_nm: f64, airway_nm: f64) -> String {
    let comparison = if direct_nm > 0.0 {
        let percent = ((airway_nm / direct_nm - 1.0) * 100.0).round() as i64;
        format!(" {percent:+}%")
    } else {
        String::new()
    };
    format!("Direct: {direct_nm:.0} nm Airway: {airway_nm:.0} nm{comparison}")
}

fn altitude(value: u32) -> String {
    if value >= 1000 {
        format!("{},{:03}", value / 1000, value % 1000)
    } else {
        value.to_string()
    }
}

fn materialize(graph: &Graph, path: &Path) -> Vec<(AirwaySegment, Vec<ResolvedLeg>)> {
    let mut segments: Vec<(AirwaySegment, Vec<ResolvedLeg>)> = Vec::new();
    let mut previous_sequence = None;
    for step in &path.steps {
        let from = &graph.nodes[step.from as usize];
        let edge = &from.edges[step.edge];
        let to = &graph.nodes[edge.to as usize];
        let append = segments.last().is_some_and(|(airway, _)| {
            airway.name == edge.airway_name
                && airway.branch_key.as_ref() == Some(&edge.branch_key)
                && previous_sequence == Some(edge.from_sequence)
        });
        if !append {
            segments.push((
                AirwaySegment {
                    name: edge.airway_name.clone(),
                    branch_key: Some(edge.branch_key.clone()),
                    entry: node_ref(from),
                    exit: node_ref(to),
                },
                Vec::new(),
            ));
        }
        let index = segments.len() - 1;
        let (airway, legs) = segments.last_mut().unwrap();
        airway.exit = node_ref(to);
        legs.push(ResolvedLeg {
            id: format!("airway-{}-{}", edge.branch_key, legs.len()),
            from: node_ref(from),
            to: node_ref(to),
            source: ResolvedLegSource::RouteComponent {
                component_index: index,
            },
            procedure_provenance: None,
        });
        previous_sequence = Some(edge.to_sequence);
    }
    segments
}

#[cfg(test)]
mod tests;
