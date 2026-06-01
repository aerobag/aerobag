use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiStatusSeverity {
    Ok,
    Info,
    Caution,
    Warning,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiStatusActionStyle {
    Normal,
    Hush,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiStatusAction {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub style: UiStatusActionStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiDataStatusBox {
    pub id: String,
    pub label: String,
    pub value: Option<String>,
    pub severity: UiStatusSeverity,
    pub drives_caution: bool,
    pub detail: String,
    pub actions: Vec<UiStatusAction>,
    pub hushed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiDataStatusState {
    pub boxes: Vec<UiDataStatusBox>,
    pub launcher_count: Option<String>,
    pub launcher_severity: UiStatusSeverity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiDataStatusPageTimeDisplay {
    Ago,
    Old,
    Until,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiDataStatusPageFact {
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_display: Option<UiDataStatusPageTimeDisplay>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiDataStatusPageRow {
    pub id: String,
    pub label: String,
    pub value: String,
    pub severity: UiStatusSeverity,
    pub detail: String,
    #[serde(default)]
    pub facts: Vec<UiDataStatusPageFact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiDataStatusPageState {
    pub title: String,
    pub summary: String,
    pub rows: Vec<UiDataStatusPageRow>,
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
        }
    }
}

pub enum UiStatusActionCommand {
    Hush(String),
    Unhush(String),
}

const STATUS_HUSH_PREFIX: &str = "status:hush:";
const STATUS_UNHUSH_PREFIX: &str = "status:unhush:";

pub fn parse_status_action_id(action_id: &str) -> Option<UiStatusActionCommand> {
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
                actions: if record.hushable {
                    vec![hush_action(&record.id, hushed)]
                } else {
                    Vec::new()
                },
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
    let active_launcher_count = active_launcher_boxes.iter().count();
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
}
