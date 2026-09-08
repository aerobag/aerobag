// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};

use app_ui_contracts::session::{UiAirwayPicker, UiAirwayPickerButton, UiAirwayPickerSection};

use crate::{
    had_ops::{nav_ref_position, read_optional, read_required, HadReadError},
    navdb_types::{AirwayBranch, AirwaySpatialPoint},
    AirwayPresentationPlan, AirwayPresentationSelection, LatLon, NavKvQuery, NavKvStore, NavRef,
};

const NEARBY_AIRWAY_LIMIT: usize = 10;
const SEARCH_RADII_NM: [f64; 5] = [25.0, 50.0, 100.0, 200.0, 400.0];

#[cfg(test)]
pub(crate) mod tests;

#[derive(Debug, Clone, PartialEq)]
struct Catalog {
    exact: Vec<String>,
    nearby: Vec<String>,
}

fn catalog(store: &NavKvStore, anchor: &NavRef) -> Result<Catalog, HadReadError> {
    let position = nav_ref_position(store, anchor, None)?;
    let mut visited = BTreeSet::new();
    let mut points = Vec::new();
    for radius in SEARCH_RADII_NM {
        let lat_delta = radius / 60.0;
        let lon_delta = radius / (60.0 * position.lat.to_radians().cos().abs().max(0.1));
        for lat_tile in
            (position.lat - lat_delta).floor() as i32..=(position.lat + lat_delta).floor() as i32
        {
            for lon_tile in (position.lon - lon_delta).floor() as i32
                ..=(position.lon + lon_delta).floor() as i32
            {
                if !visited.insert((lat_tile, lon_tile)) {
                    continue;
                }
                if let Some(tile) = read_optional::<Vec<AirwaySpatialPoint>>(
                    store,
                    NavKvQuery::AirwaySpatial { lat_tile, lon_tile },
                )? {
                    points.extend(tile);
                }
            }
        }
        let result = classify_points(anchor, position, &points);
        if result.nearby.len() == NEARBY_AIRWAY_LIMIT || radius == SEARCH_RADII_NM[4] {
            return Ok(result);
        }
    }
    unreachable!("search radii always include a final radius")
}

fn classify_points(anchor: &NavRef, position: LatLon, points: &[AirwaySpatialPoint]) -> Catalog {
    let exact = points
        .iter()
        .filter(|point| &point.nav_ref == anchor)
        .map(|point| point.airway_name.clone())
        .collect::<BTreeSet<_>>();
    let mut nearby = BTreeMap::<String, f64>::new();
    for point in points {
        if exact.contains(&point.airway_name) {
            continue;
        }
        let distance = crate::flight_leg_distance_nm(position, point.position);
        if distance > SEARCH_RADII_NM[4] {
            continue;
        }
        nearby
            .entry(point.airway_name.clone())
            .and_modify(|prior| *prior = prior.min(distance))
            .or_insert(distance);
    }
    let mut nearby = nearby.into_iter().collect::<Vec<_>>();
    nearby.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    Catalog {
        exact: exact.into_iter().collect(),
        nearby: nearby
            .into_iter()
            .take(NEARBY_AIRWAY_LIMIT)
            .map(|(name, _)| name)
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Stage {
    Airways,
    Entry(AirwayPresentationPlan),
    Exit {
        presentation: AirwayPresentationPlan,
        entry_uid: String,
        automatic_entry: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum Action {
    Airway { name: String, exact: bool },
    Entry(String),
    Exit(String),
    Back,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq)]
struct Picker {
    row_uid: String,
    origin: NavRef,
    destination: Option<NavRef>,
    nav_epoch: u64,
    catalog: Catalog,
    stage: Stage,
    view: UiAirwayPicker,
    actions: BTreeMap<String, Action>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct AirwayPickerController {
    generation: u64,
    picker: Option<Picker>,
}

pub(crate) enum Transition {
    Picker(AirwayPickerController),
    Insert {
        row_uid: String,
        selection: AirwayPresentationSelection,
    },
}

impl AirwayPickerController {
    pub fn open(
        &self,
        store: &NavKvStore,
        row_uid: String,
        origin: NavRef,
        destination: Option<NavRef>,
        nav_epoch: u64,
    ) -> Result<Self, HadReadError> {
        let catalog = catalog(store, &origin)?;
        let mut next = self.clone();
        next.picker = Some(Picker {
            row_uid: row_uid.clone(),
            origin,
            destination,
            nav_epoch,
            catalog,
            stage: Stage::Airways,
            view: UiAirwayPicker {
                row_uid,
                title: String::new(),
                sections: Vec::new(),
                footer: Vec::new(),
                dismiss_action_id: String::new(),
            },
            actions: BTreeMap::new(),
        });
        next.render();
        Ok(next)
    }

    pub fn close(&mut self) {
        self.picker = None;
    }

    pub fn view(&self, nav_epoch: u64) -> Option<UiAirwayPicker> {
        self.picker
            .as_ref()
            .filter(|picker| picker.nav_epoch == nav_epoch)
            .map(|picker| picker.view.clone())
    }

    pub fn advance(
        &self,
        store: &NavKvStore,
        action_id: &str,
        nav_epoch: u64,
    ) -> Result<Transition, HadReadError> {
        let picker = self
            .picker
            .as_ref()
            .filter(|picker| picker.nav_epoch == nav_epoch)
            .ok_or_else(|| {
                HadReadError::Fatal(
                    "Airway picker is no longer current. Open Select Airway again.".into(),
                )
            })?;
        let action = picker
            .actions
            .get(action_id)
            .ok_or_else(|| HadReadError::Fatal("Airway choice is no longer current.".into()))?;
        let mut next = self.clone();
        let next_picker = next.picker.as_mut().expect("current picker");
        match action {
            Action::Airway { name, exact } => {
                let mut branches = read_required::<Vec<AirwayBranch>>(
                    store,
                    NavKvQuery::AirwayBranches {
                        airway_name: name.clone(),
                    },
                    "airway branches",
                )?;
                if *exact {
                    branches.retain(|branch| {
                        branch
                            .points
                            .iter()
                            .any(|point| point.nav_ref == picker.origin)
                    });
                }
                let presentation = crate::prepare_airway_presentation(
                    name,
                    branches,
                    nav_ref_position(store, &picker.origin, None)?,
                    picker
                        .destination
                        .as_ref()
                        .map(|target| nav_ref_position(store, target, None))
                        .transpose()?,
                )?;
                let entries = presentation
                    .points
                    .iter()
                    .filter(|point| point.nav_ref == picker.origin)
                    .collect::<Vec<_>>();
                next_picker.stage = if *exact && entries.len() == 1 {
                    Stage::Exit {
                        entry_uid: entries[0].uid.clone(),
                        presentation,
                        automatic_entry: true,
                    }
                } else {
                    Stage::Entry(presentation)
                };
            }
            Action::Entry(uid) => {
                let Stage::Entry(presentation) = &picker.stage else {
                    unreachable!("registered entry action")
                };
                next_picker.stage = Stage::Exit {
                    presentation: presentation.clone(),
                    entry_uid: uid.clone(),
                    automatic_entry: false,
                };
            }
            Action::Exit(uid) => {
                let Stage::Exit {
                    presentation,
                    entry_uid,
                    ..
                } = &picker.stage
                else {
                    unreachable!("registered exit action")
                };
                return Ok(Transition::Insert {
                    row_uid: picker.row_uid.clone(),
                    selection: AirwayPresentationSelection {
                        airway_name: presentation.airway_name.clone(),
                        branch_key: presentation.branch_key.clone(),
                        entry_point_uid: entry_uid.clone(),
                        exit_point_uid: uid.clone(),
                    },
                });
            }
            Action::Back => {
                next_picker.stage = match &picker.stage {
                    Stage::Exit {
                        presentation,
                        automatic_entry: false,
                        ..
                    } => Stage::Entry(presentation.clone()),
                    _ => Stage::Airways,
                }
            }
            Action::Dismiss => {
                next.picker = None;
                return Ok(Transition::Picker(next));
            }
        }
        next.render();
        Ok(Transition::Picker(next))
    }

    fn render(&mut self) {
        let picker = self.picker.as_mut().expect("render open picker");
        self.generation += 1;
        picker.actions.clear();
        let mut counter = 0;
        let mut button = |label: String,
                          action: Action,
                          suggested: bool,
                          disabled_reason: Option<String>,
                          test_id: String| {
            let action_id = format!("airway-picker:{}:{counter}", self.generation);
            counter += 1;
            if disabled_reason.is_none() {
                picker.actions.insert(action_id.clone(), action);
            }
            UiAirwayPickerButton {
                action_id,
                label,
                enabled: disabled_reason.is_none(),
                disabled_reason,
                suggested,
                test_id,
            }
        };
        let dismiss = button(
            "Close".into(),
            Action::Dismiss,
            false,
            None,
            "plan-airway-close".into(),
        );
        let anchor_label = crate::nav_ref_picker_label(&picker.origin);
        let mut view = UiAirwayPicker {
            row_uid: picker.row_uid.clone(),
            title: format!("AIRWAY {anchor_label}"),
            sections: Vec::new(),
            footer: Vec::new(),
            dismiss_action_id: dismiss.action_id,
        };
        match &picker.stage {
            Stage::Airways => {
                for (title, names, exact) in [
                    (
                        format!("Through {anchor_label}"),
                        &picker.catalog.exact,
                        true,
                    ),
                    ("Nearby".into(), &picker.catalog.nearby, false),
                ] {
                    view.sections.push(UiAirwayPickerSection {
                        title: if names.is_empty() {
                            format!("{title}: none")
                        } else {
                            title
                        },
                        dense: true,
                        buttons: names
                            .iter()
                            .map(|name| {
                                button(
                                    name.clone(),
                                    Action::Airway {
                                        name: name.clone(),
                                        exact,
                                    },
                                    false,
                                    None,
                                    format!("parity:plan-airway-suggestion:{name}"),
                                )
                            })
                            .collect(),
                    });
                }
            }
            Stage::Entry(presentation) | Stage::Exit { presentation, .. } => {
                let exit = match &picker.stage {
                    Stage::Exit { entry_uid, .. } => Some(entry_uid),
                    _ => None,
                };
                let phase = if exit.is_some() { "exit" } else { "entry" };
                view.title = format!("{}: Select {phase}", presentation.airway_name);
                let buttons = presentation
                    .points
                    .iter()
                    .map(|point| {
                        let disabled = exit.is_some_and(|entry| entry == &point.uid);
                        let suggested = !disabled
                            && if exit.is_some() {
                                presentation.suggested_exit_uid.as_ref() == Some(&point.uid)
                            } else {
                                presentation.suggested_entry_uid == point.uid
                            };
                        button(
                            point.label.clone(),
                            if exit.is_some() {
                                Action::Exit(point.uid.clone())
                            } else {
                                Action::Entry(point.uid.clone())
                            },
                            suggested,
                            disabled.then(|| point.same_point_exit_disabled_reason.clone()),
                            format!("parity:plan-airway-{phase}:{}", point.label),
                        )
                    })
                    .collect();
                view.sections.push(UiAirwayPickerSection {
                    title: String::new(),
                    dense: false,
                    buttons,
                });
                view.footer.push(button(
                    "Back".into(),
                    Action::Back,
                    false,
                    None,
                    "plan-airway-back".into(),
                ));
            }
        }
        picker.view = view;
    }
}
