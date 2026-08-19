// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};

pub use app_ui_contracts::session::{
    UiDataStatusBox, UiDataStatusPageFact, UiDataStatusPageRow, UiDataStatusPageState,
    UiDataStatusState, UiStatusAction, UiStatusActionDecision, UiStatusActionStyle,
    UiStatusPlatformEffect, UiStatusSeverity, UiSurfaceStatusControl, UiSurfaceStatusControlId,
    UiSurfaceStatusState,
};

pub fn map_surface_status_state(global: UiDataStatusState) -> UiSurfaceStatusState {
    UiSurfaceStatusState {
        controls: vec![UiSurfaceStatusControl {
            id: UiSurfaceStatusControlId::Global,
            state: global,
        }],
    }
}

pub fn charts_surface_status_state(
    procedure_geometry: UiDataStatusState,
    global: UiDataStatusState,
) -> UiSurfaceStatusState {
    UiSurfaceStatusState {
        controls: vec![
            UiSurfaceStatusControl {
                id: UiSurfaceStatusControlId::ProcedureGeometry,
                state: procedure_geometry,
            },
            UiSurfaceStatusControl {
                id: UiSurfaceStatusControlId::Global,
                state: global,
            },
        ],
    }
}

pub fn status_action_decision(action_id: &str) -> Option<UiStatusActionDecision> {
    parse_status_action_id(action_id).map(|command| match command {
        UiStatusActionCommand::Hush(_) | UiStatusActionCommand::Unhush(_) => {
            UiStatusActionDecision {
                platform_effect: None,
                perform_session_mutation: true,
            }
        }
        UiStatusActionCommand::ReloadApplication => UiStatusActionDecision {
            platform_effect: Some(UiStatusPlatformEffect::ReloadApplication),
            perform_session_mutation: false,
        },
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataStatusRecord {
    pub id: String,
    pub label: String,
    pub value: Option<String>,
    pub severity: UiStatusSeverity,
    pub drives_caution: bool,
    pub detail: String,
    pub hushable: bool,
    pub actions: Vec<UiStatusAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureGeometryWarningContext<'a> {
    LoadedProcedure,
    PublishedPlate { transition: Option<&'a str> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGeometryWarningPresentation {
    pub value: String,
    pub detail: String,
}

pub fn procedure_geometry_warning_presentation(
    airport_id: &str,
    procedure_label: &str,
    context: ProcedureGeometryWarningContext<'_>,
    messages: &[String],
) -> ProcedureGeometryWarningPresentation {
    let value = procedure_label.trim().to_string();
    let prefix = match context {
        ProcedureGeometryWarningContext::LoadedProcedure => {
            format!("Loaded procedure {airport_id} {value}")
        }
        ProcedureGeometryWarningContext::PublishedPlate { transition } => {
            let transition = transition
                .map(str::trim)
                .filter(|transition| !transition.is_empty())
                .map(|transition| format!(" from {transition}"))
                .unwrap_or_default();
            format!(
                "This publication reports a procedure geometry warning for {airport_id} {value}{transition}"
            )
        }
    };
    let detail = format!("{prefix}:\n{}", messages.join("\n"));
    ProcedureGeometryWarningPresentation { value, detail }
}

impl DataStatusRecord {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        value: Option<String>,
        severity: UiStatusSeverity,
        drives_caution: bool,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            value,
            severity,
            drives_caution,
            detail: detail.into(),
            hushable: true,
            actions: Vec::new(),
        }
    }

    pub fn with_action(mut self, action: UiStatusAction) -> Self {
        self.actions.push(action);
        self
    }
}

pub enum UiStatusActionCommand {
    Hush(String),
    Unhush(String),
    ReloadApplication,
}

const STATUS_HUSH_PREFIX: &str = "status:hush:";
const STATUS_UNHUSH_PREFIX: &str = "status:unhush:";
pub const RELOAD_APPLICATION_ACTION_ID: &str = "app:reload";

pub fn parse_status_action_id(action_id: &str) -> Option<UiStatusActionCommand> {
    if action_id == RELOAD_APPLICATION_ACTION_ID {
        return Some(UiStatusActionCommand::ReloadApplication);
    }
    if let Some(status_id) = action_id.strip_prefix(STATUS_HUSH_PREFIX) {
        return Some(UiStatusActionCommand::Hush(status_id.to_string()));
    }
    if let Some(status_id) = action_id.strip_prefix(STATUS_UNHUSH_PREFIX) {
        return Some(UiStatusActionCommand::Unhush(status_id.to_string()));
    }
    None
}

pub fn project_data_status_state(
    records: &BTreeMap<String, DataStatusRecord>,
    hushed_ids: &BTreeSet<String>,
) -> UiDataStatusState {
    let mut records = records.values().collect::<Vec<_>>();
    records.sort_by(|left, right| {
        severity_rank(right.severity)
            .cmp(&severity_rank(left.severity))
            .then_with(|| right.drives_caution.cmp(&left.drives_caution))
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.id.cmp(&right.id))
    });
    let boxes = records
        .into_iter()
        .map(|record| {
            let hushed = record.hushable && hushed_ids.contains(&record.id);
            UiDataStatusBox {
                id: record.id.clone(),
                label: record.label.clone(),
                value: record.value.clone(),
                severity: record.severity,
                drives_caution: record.drives_caution,
                detail: record.detail.clone(),
                actions: record
                    .actions
                    .iter()
                    .cloned()
                    .chain(record.hushable.then(|| hush_action(&record.id, hushed)))
                    .collect(),
                hushed,
            }
        })
        .collect::<Vec<_>>();
    let active_launcher_boxes = boxes.iter().filter(|box_| !box_.hushed).collect::<Vec<_>>();
    let launcher_severity = active_launcher_boxes
        .iter()
        .map(|box_| box_.severity)
        .max_by_key(|severity| severity_rank(*severity))
        .unwrap_or(UiStatusSeverity::Info);
    let active_launcher_count = active_launcher_boxes.len();
    UiDataStatusState {
        boxes,
        launcher_count: (active_launcher_count > 0).then(|| active_launcher_count.to_string()),
        launcher_severity,
    }
}

fn hush_action(status_id: &str, hushed: bool) -> UiStatusAction {
    let (prefix, label) = if hushed {
        (STATUS_UNHUSH_PREFIX, "Unhush")
    } else {
        (STATUS_HUSH_PREFIX, "Hush")
    };
    UiStatusAction {
        id: format!("{prefix}{status_id}"),
        label: label.to_string(),
        enabled: true,
        style: UiStatusActionStyle::Hush,
    }
}

fn severity_rank(severity: UiStatusSeverity) -> u8 {
    match severity {
        UiStatusSeverity::Warning => 5,
        UiStatusSeverity::Unavailable => 4,
        UiStatusSeverity::Caution => 3,
        UiStatusSeverity::Info => 2,
        UiStatusSeverity::Ok => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_presentation_is_core_owned_and_blank_when_zero() {
        let mut records = BTreeMap::new();
        records.insert(
            "metars".to_string(),
            DataStatusRecord::new(
                "metars",
                "METARs",
                None,
                UiStatusSeverity::Unavailable,
                true,
                "METAR feed unavailable.",
            ),
        );
        records.insert(
            "airspace".to_string(),
            DataStatusRecord::new(
                "airspace",
                "Airspace",
                Some("LIMIT".to_string()),
                UiStatusSeverity::Info,
                false,
                "If it appears in the data-status panel, the launcher counts it.",
            ),
        );
        records.insert(
            "warning_without_legacy_flag".to_string(),
            DataStatusRecord::new(
                "warning_without_legacy_flag",
                "Warning",
                None,
                UiStatusSeverity::Warning,
                false,
                "Severity, not a platform-side flag, drives launcher presentation.",
            ),
        );

        let state = project_data_status_state(&records, &BTreeSet::new());
        assert_eq!(state.launcher_count, Some("3".to_string()));
        assert_eq!(state.launcher_severity, UiStatusSeverity::Warning);

        let hushed_state = project_data_status_state(
            &records,
            &BTreeSet::from([
                "airspace".to_string(),
                "metars".to_string(),
                "warning_without_legacy_flag".to_string(),
            ]),
        );
        assert_eq!(hushed_state.launcher_count, None);
        assert_eq!(hushed_state.launcher_severity, UiStatusSeverity::Info);
    }

    #[test]
    fn launcher_severity_is_the_worst_unhushed_notification() {
        let records = BTreeMap::from([
            (
                "adsb".to_string(),
                DataStatusRecord::new(
                    "adsb",
                    "ADS-B",
                    None,
                    UiStatusSeverity::Info,
                    false,
                    "ADS-B ownship is active.",
                ),
            ),
            (
                "procedure".to_string(),
                DataStatusRecord::new(
                    "procedure",
                    "Procedure",
                    None,
                    UiStatusSeverity::Caution,
                    true,
                    "Procedure caution.",
                ),
            ),
            (
                "terrain".to_string(),
                DataStatusRecord::new(
                    "terrain",
                    "Terrain",
                    None,
                    UiStatusSeverity::Warning,
                    true,
                    "Terrain warning.",
                ),
            ),
        ]);

        assert_eq!(
            project_data_status_state(&records, &BTreeSet::new()).launcher_severity,
            UiStatusSeverity::Warning
        );
        assert_eq!(
            project_data_status_state(&records, &BTreeSet::from(["terrain".to_string()]))
                .launcher_severity,
            UiStatusSeverity::Caution
        );
        assert_eq!(
            project_data_status_state(
                &records,
                &BTreeSet::from(["terrain".to_string(), "procedure".to_string()]),
            )
            .launcher_severity,
            UiStatusSeverity::Info
        );
    }

    #[test]
    fn surface_status_projection_owns_membership_and_order() {
        let global = UiDataStatusState {
            boxes: Vec::new(),
            launcher_count: Some("global".to_string()),
            launcher_severity: UiStatusSeverity::Info,
        };
        let procedure = UiDataStatusState {
            boxes: Vec::new(),
            launcher_count: Some("procedure".to_string()),
            launcher_severity: UiStatusSeverity::Caution,
        };

        let map = map_surface_status_state(global.clone());
        assert_eq!(map.controls.len(), 1);
        assert_eq!(map.controls[0].id, UiSurfaceStatusControlId::Global);
        assert_eq!(map.controls[0].state, global);

        let charts = charts_surface_status_state(procedure.clone(), global.clone());
        assert_eq!(
            charts
                .controls
                .iter()
                .map(|control| control.id)
                .collect::<Vec<_>>(),
            vec![
                UiSurfaceStatusControlId::ProcedureGeometry,
                UiSurfaceStatusControlId::Global,
            ]
        );
        assert_eq!(charts.controls[0].state, procedure);
        assert_eq!(charts.controls[1].state, global);
    }

    #[test]
    fn status_action_decision_separates_mutation_from_platform_effect() {
        assert_eq!(
            status_action_decision("status:hush:metars"),
            Some(UiStatusActionDecision {
                platform_effect: None,
                perform_session_mutation: true,
            })
        );
        assert_eq!(
            status_action_decision(RELOAD_APPLICATION_ACTION_ID),
            Some(UiStatusActionDecision {
                platform_effect: Some(UiStatusPlatformEffect::ReloadApplication),
                perform_session_mutation: false,
            })
        );
        assert_eq!(status_action_decision("unknown"), None);
    }
}
