// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::{
    flight_plan_controller::{FlightPlanController, FlightPlanModelCheckpoint},
    navigation_start::NavigationStartInput,
    AppError, AppErrorKind, AppResult, FlightPlan, FlightPlanUiState,
};

pub use app_ui_contracts::session::FlightPlanSlot;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlightPlanWorkspaceUi {
    pub open_slot: FlightPlanSlot,
    pub active_label: String,
    pub standby_label: String,
    pub active_summary: String,
    pub standby_summary: String,
    pub standby_plan: Option<FlightPlanUiState>,
    pub controls: Vec<crate::FlightPlanControlUiView>,
    pub clear_standby: crate::FlightPlanControlUiView,
    pub entry_focus_revision: u64,
    pub notice: Option<String>,
}

#[derive(Default)]
pub(crate) struct FlightPlanWorkspace {
    pub active: FlightPlanController,
    pub standby: FlightPlanController,
    pub open_slot: FlightPlanSlot,
    pub entry_focus_revision: u64,
    pub notice: Option<String>,
    pub revision: u64,
}

#[derive(Clone)]
pub(crate) struct FlightPlanWorkspaceCheckpoint {
    active: FlightPlanModelCheckpoint,
    standby: FlightPlanModelCheckpoint,
    open_slot: FlightPlanSlot,
    entry_focus_revision: u64,
    notice: Option<String>,
    revision: u64,
}

impl FlightPlanWorkspace {
    pub fn route_revision(&self) -> u64 {
        self.revision
            .saturating_add(self.active.route_revision())
            .saturating_add(self.standby.route_revision())
    }

    pub fn project(&self, standby_plan: Option<FlightPlanUiState>) -> FlightPlanWorkspaceUi {
        use crate::FlightPlanControlId::*;
        let control = |id, label: &str, reason: Option<&str>| crate::FlightPlanControlUiView {
            id,
            symbol_id: if id == ToggleOpenPlan && self.open_slot == FlightPlanSlot::Standby {
                "move_up".into()
            } else {
                crate::planning::flight_plan_control_symbol_id(id).into()
            },
            label: label.into(),
            enabled: reason.is_none(),
            selected: false,
            disabled_reason: reason.map(str::to_owned),
        };
        let has_standby = self
            .standby
            .active_plan()
            .is_some_and(|p| !p.route_components.is_empty());
        FlightPlanWorkspaceUi {
            open_slot: self.open_slot,
            active_label: "ACTIVE PLAN".into(),
            standby_label: "STANDBY PLAN".into(),
            active_summary: summary(self.active.active_plan()),
            standby_summary: summary(self.standby.active_plan()),
            standby_plan,
            controls: vec![
                control(
                    ToggleOpenPlan,
                    if self.open_slot == FlightPlanSlot::Active {
                        "Open Standby"
                    } else {
                        "Open Active"
                    },
                    None,
                ),
                control(
                    SwapPlans,
                    "Swap Active and Standby",
                    (!has_standby).then_some("Enter a Standby route before swapping flight plans."),
                ),
                control(CopyActiveToStandby, "Copy Active to Standby", None),
            ],
            clear_standby: control(ClearStandby, "Clear Standby and enter a new route", None),
            entry_focus_revision: self.entry_focus_revision,
            notice: self.notice.clone(),
        }
    }

    pub fn new(active: FlightPlanController) -> AppResult<Self> {
        let standby = FlightPlan {
            id: format!(
                "{}-standby",
                active.active_plan().map_or("plan", |p| p.id.as_str())
            ),
            ..FlightPlan::empty()
        };
        Ok(Self {
            active,
            standby: FlightPlanController::new(standby, Vec::new())?,
            ..Self::default()
        })
    }

    pub fn open(&self) -> &FlightPlanController {
        match self.open_slot {
            FlightPlanSlot::Active => &self.active,
            FlightPlanSlot::Standby => &self.standby,
        }
    }

    pub fn open_mut(&mut self) -> &mut FlightPlanController {
        match self.open_slot {
            FlightPlanSlot::Active => &mut self.active,
            FlightPlanSlot::Standby => &mut self.standby,
        }
    }

    pub fn require_open_plan_id(&self, plan_id: &str) -> AppResult<()> {
        if self
            .open()
            .active_plan()
            .is_some_and(|plan| plan.id == plan_id)
        {
            Ok(())
        } else {
            Err(AppError { kind: AppErrorKind::UnsupportedOperation,
                message: "The open flight plan changed while this edit was loading. Try the edit again on the intended plan.".into() })
        }
    }

    pub fn select(&mut self, slot: FlightPlanSlot) {
        self.open_slot = slot;
        self.notice = None;
        self.revision = self.revision.saturating_add(1);
    }

    pub fn copy_active(&mut self) -> AppResult<()> {
        let mut plan = self.active.active_plan().cloned().ok_or_else(|| AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: "There is no Active flight plan to copy.".into(),
        })?;
        plan.id = self
            .standby
            .active_plan()
            .ok_or_else(|| AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "There is no Standby flight plan to replace.".into(),
            })?
            .id
            .clone();
        plan.guidance = None;
        self.standby.apply_definition_edit(plan)?;
        self.select(FlightPlanSlot::Standby);
        Ok(())
    }

    pub fn clear_standby(&mut self) -> AppResult<()> {
        let previous = self.standby.active_plan();
        let mut empty = FlightPlan::empty();
        if let Some(previous) = previous {
            empty.id = previous.id.clone();
            empty.aircraft = previous.aircraft.clone();
            empty.cruise_altitude_ft = previous.cruise_altitude_ft;
        }
        self.standby.apply_definition_edit(empty)?;
        self.select(FlightPlanSlot::Standby);
        self.entry_focus_revision = self.entry_focus_revision.saturating_add(1);
        Ok(())
    }

    pub fn swap(&mut self, input: NavigationStartInput) -> AppResult<()> {
        if self
            .standby
            .active_plan()
            .is_none_or(|plan| plan.route_components.is_empty())
        {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: "Enter a Standby route before swapping flight plans.".into(),
            });
        }
        let was_navigating = self
            .active
            .active_plan()
            .is_some_and(|plan| plan.guidance.is_some());
        self.active.park_navigation()?;
        self.standby.park_navigation()?;
        std::mem::swap(&mut self.active, &mut self.standby);
        self.select(FlightPlanSlot::Active);
        if was_navigating {
            match self.active.plan_after_start_navigation(input) {
                Ok(plan) => {
                    self.active.apply_navigation_update(plan)?;
                }
                Err(error) => self.notice = Some(error.message),
            }
        }
        Ok(())
    }

    pub fn checkpoint_model(&self) -> FlightPlanWorkspaceCheckpoint {
        FlightPlanWorkspaceCheckpoint {
            active: self.active.checkpoint_model(),
            standby: self.standby.checkpoint_model(),
            open_slot: self.open_slot,
            entry_focus_revision: self.entry_focus_revision,
            notice: self.notice.clone(),
            revision: self.revision,
        }
    }

    pub fn rollback_model(&mut self, saved: FlightPlanWorkspaceCheckpoint) {
        self.active.rollback_model(saved.active);
        self.standby.rollback_model(saved.standby);
        self.open_slot = saved.open_slot;
        self.entry_focus_revision = saved.entry_focus_revision;
        self.notice = saved.notice;
        self.revision = saved.revision;
    }
}

fn summary(plan: Option<&FlightPlan>) -> String {
    let text = plan
        .map(|p| {
            p.route_components
                .iter()
                .map(crate::planning::component_summary)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    if text.is_empty() {
        "Empty route".into()
    } else {
        text
    }
}

pub(crate) fn remove_navigation_actions(ui: &mut FlightPlanUiState) {
    ui.guidance = None;
    ui.controls.retain(|c| {
        matches!(
            c.id,
            crate::FlightPlanControlId::Undo | crate::FlightPlanControlId::Redo
        )
    });
    for row in &mut ui.display_rows {
        for actions in &mut row.action_matrix {
            actions.retain(|a| {
                !matches!(
                    a.id,
                    crate::FlightPlanRowActionId::ActivateLeg
                        | crate::FlightPlanRowActionId::DirectTo
                )
            });
        }
        row.action_matrix.retain(|actions| !actions.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(lon: f64) -> NavigationStartInput {
        NavigationStartInput {
            position: Some(crate::LatLon { lat: 0.0, lon }),
            track_deg_true: Some(90.0),
            speed_kt: Some(120.0),
        }
    }

    fn workspace(navigating: bool) -> FlightPlanWorkspace {
        let plan = FlightPlan {
            route_components: [0.0, 1.0, 2.0]
                .map(|lon| crate::RouteComponent::Waypoint {
                    waypoint: crate::NavRef::LatLon(crate::LatLon { lat: 0.0, lon }),
                })
                .to_vec(),
            ..FlightPlan::empty()
        };
        let mut active = FlightPlanController::default();
        active.replace_plan(plan).unwrap();
        if navigating {
            let plan = active.plan_after_start_navigation(input(0.5)).unwrap();
            active.apply_navigation_update(plan).unwrap();
        }
        FlightPlanWorkspace::new(active).unwrap()
    }

    #[test]
    fn standby_copy_clear_and_undo_do_not_change_active_navigation() {
        let mut pair = workspace(true);
        let active = pair.active.active_plan().unwrap().clone();
        pair.copy_active().unwrap();
        assert_eq!(pair.open_slot, FlightPlanSlot::Standby);
        assert!(pair.standby.active_plan().unwrap().guidance.is_none());
        assert_eq!(
            pair.standby.active_plan().unwrap().route_components,
            active.route_components
        );
        assert_ne!(pair.standby.active_plan().unwrap().id, active.id);
        pair.clear_standby().unwrap();
        assert!(pair
            .open()
            .active_plan()
            .unwrap()
            .route_components
            .is_empty());
        assert_eq!(pair.entry_focus_revision, 1);
        pair.open_mut().undo_definition_edit().unwrap();
        assert_eq!(
            pair.open().active_plan().unwrap().route_components,
            active.route_components
        );
        assert_eq!(pair.active.active_plan(), Some(&active));
        assert!(!pair.active.can_undo());
    }

    #[test]
    fn swap_reselects_from_current_position_and_keeps_histories_with_plans() {
        let mut pair = workspace(true);
        pair.copy_active().unwrap();
        let active_id = pair.active.active_plan().unwrap().id.clone();
        let standby_id = pair.standby.active_plan().unwrap().id.clone();
        pair.swap(input(1.5)).unwrap();
        assert_eq!(pair.active.active_plan().unwrap().id, standby_id);
        assert_eq!(
            pair.active
                .active_plan()
                .unwrap()
                .guidance
                .as_ref()
                .unwrap()
                .active_leg_index,
            1
        );
        assert!(pair.standby.active_plan().unwrap().guidance.is_none());
        assert!(pair.active.can_undo());
        assert!(!pair.standby.can_undo());
        pair.swap(input(1.6)).unwrap();
        assert_eq!(pair.active.active_plan().unwrap().id, active_id);
        assert_eq!(
            pair.active
                .active_plan()
                .unwrap()
                .guidance
                .as_ref()
                .unwrap()
                .active_leg_index,
            1
        );
    }

    #[test]
    fn stopped_swap_stays_stopped_and_empty_swap_is_rejected_without_changes() {
        let mut pair = workspace(false);
        let original = pair.active.active_plan().unwrap().clone();
        assert!(pair.swap(input(0.5)).is_err());
        assert_eq!(pair.active.active_plan(), Some(&original));
        pair.copy_active().unwrap();
        pair.swap(input(0.5)).unwrap();
        assert!(pair.active.active_plan().unwrap().guidance.is_none());
        assert!(pair.standby.active_plan().unwrap().guidance.is_none());
    }

    #[test]
    fn ambiguous_swap_lands_plan_stopped_with_message() {
        let mut pair = workspace(true);
        pair.copy_active().unwrap();
        let standby_id = pair.standby.active_plan().unwrap().id.clone();
        pair.swap(NavigationStartInput {
            position: None,
            track_deg_true: None,
            speed_kt: None,
        })
        .unwrap();
        assert_eq!(pair.active.active_plan().unwrap().id, standby_id);
        assert!(pair.active.active_plan().unwrap().guidance.is_none());
        assert!(pair
            .notice
            .as_ref()
            .is_some_and(|notice| !notice.is_empty()));
    }
}
