// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeSet, sync::Arc};

use app_ui_contracts::session::{
    UiDisclaimerState, UiDisplayPolicy, UiSettingsAction, UiSettingsGridItem, UiSettingsPageRow,
    UiSettingsPageState, UiSettingsSliderStop,
};
use serde::{Deserialize, Serialize};

use crate::{AppError, AppErrorKind, AppResult, FlightDataBannerModel};

const NO_WARRANTY_DISCLAIMER_HTML: &str = include_str!("../../../../shared/no-warranty.html");
const NO_WARRANTY_DISCLAIMER_AGREEMENT_ID: &str = "no-warranty-v1";
const DISPLAY_DIM_TIMEOUT_ROW_ID: &str = "display_dim_timeout";
const DISPLAY_DIM_TIMEOUT_ACTION_ID: &str = "display_dim_timeout";
const FLIGHT_DATA_VISIBILITY_ROW_ID: &str = "flight_data_visibility";
const FLIGHT_DATA_VISIBILITY_ACTION_ID: &str = "flight_data_visibility";
const DISPLAY_DIM_BRIGHTNESS: f32 = 0.05;

pub trait SettingsStorage: Send + Sync {
    fn read_settings(&self) -> AppResult<Option<Vec<u8>>>;
    fn write_settings(&self, bytes: &[u8]) -> AppResult<()>;
}

pub type SettingsStorageHandle = Arc<dyn SettingsStorage>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayDimTimeout {
    #[serde(rename = "10s")]
    TenSeconds,
    #[serde(rename = "30s")]
    ThirtySeconds,
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "2m")]
    TwoMinutes,
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "never")]
    Never,
}

impl DisplayDimTimeout {
    fn id(self) -> &'static str {
        match self {
            Self::TenSeconds => "10s",
            Self::ThirtySeconds => "30s",
            Self::OneMinute => "1m",
            Self::TwoMinutes => "2m",
            Self::FiveMinutes => "5m",
            Self::Never => "never",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TenSeconds => "10s",
            Self::ThirtySeconds => "30s",
            Self::OneMinute => "1m",
            Self::TwoMinutes => "2m",
            Self::FiveMinutes => "5m",
            Self::Never => "Never",
        }
    }

    fn dim_after_ms(self) -> Option<u64> {
        match self {
            Self::TenSeconds => Some(10_000),
            Self::ThirtySeconds => Some(30_000),
            Self::OneMinute => Some(60_000),
            Self::TwoMinutes => Some(120_000),
            Self::FiveMinutes => Some(300_000),
            Self::Never => None,
        }
    }

    fn from_value_id(value_id: &str) -> Option<Self> {
        match value_id {
            "10s" => Some(Self::TenSeconds),
            "30s" => Some(Self::ThirtySeconds),
            "1m" => Some(Self::OneMinute),
            "2m" => Some(Self::TwoMinutes),
            "5m" => Some(Self::FiveMinutes),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    fn all_stops() -> [Self; 6] {
        [
            Self::TenSeconds,
            Self::ThirtySeconds,
            Self::OneMinute,
            Self::TwoMinutes,
            Self::FiveMinutes,
            Self::Never,
        ]
    }
}

impl Default for DisplayDimTimeout {
    fn default() -> Self {
        Self::TwoMinutes
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SettingsPreferences {
    #[serde(default)]
    pub display_dim_timeout: DisplayDimTimeout,
    #[serde(default)]
    pub disabled_flight_data_cell_ids: BTreeSet<String>,
    #[serde(default)]
    pub accepted_disclaimer_agreement_ids: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SettingsProjection {
    pub settings_page_state: UiSettingsPageState,
    pub display_policy: Option<UiDisplayPolicy>,
    pub disclaimer_state: UiDisclaimerState,
    pub flight_data_banner: FlightDataBannerModel,
}

pub(crate) struct SettingsProjectionResult {
    pub projection: SettingsProjection,
    pub rebuilt: bool,
}

#[derive(Clone)]
struct SettingsProjectionCache {
    settings_revision: u64,
    display_policy_available: bool,
    flight_data_banner: FlightDataBannerModel,
    projection: SettingsProjection,
}

#[derive(Clone, Default)]
pub(crate) struct SettingsController {
    preferences: SettingsPreferences,
    revision: u64,
    projection_cache: Option<SettingsProjectionCache>,
}

impl SettingsController {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn persistent_preferences(&self) -> SettingsPreferences {
        self.preferences.clone()
    }

    pub fn restore_preferences(&mut self, preferences: SettingsPreferences) -> bool {
        if self.preferences == preferences {
            return false;
        }
        self.preferences = preferences;
        self.note_change();
        true
    }

    pub fn perform_action(
        &mut self,
        action: &UiSettingsAction,
        display_policy_available: bool,
    ) -> AppResult<bool> {
        let changed = match action.action_id.as_str() {
            DISPLAY_DIM_TIMEOUT_ACTION_ID => {
                if !display_policy_available {
                    return Err(invalid_settings_action(&action.action_id));
                }
                let timeout =
                    DisplayDimTimeout::from_value_id(&action.value_id).ok_or_else(|| {
                        invalid_settings_action_value(&action.action_id, &action.value_id)
                    })?;
                if self.preferences.display_dim_timeout == timeout {
                    false
                } else {
                    self.preferences.display_dim_timeout = timeout;
                    true
                }
            }
            FLIGHT_DATA_VISIBILITY_ACTION_ID => {
                if !crate::flight_data::is_flight_data_banner_cell_id(&action.value_id) {
                    return Err(invalid_settings_action_value(
                        &action.action_id,
                        &action.value_id,
                    ));
                }
                if !self
                    .preferences
                    .disabled_flight_data_cell_ids
                    .remove(&action.value_id)
                {
                    self.preferences
                        .disabled_flight_data_cell_ids
                        .insert(action.value_id.clone());
                }
                true
            }
            _ => return Err(invalid_settings_action(&action.action_id)),
        };
        if changed {
            self.note_change();
        }
        Ok(changed)
    }

    pub fn accept_disclaimer(&mut self, agreement_id: &str) -> AppResult<bool> {
        if agreement_id != NO_WARRANTY_DISCLAIMER_AGREEMENT_ID {
            return Err(AppError {
                kind: AppErrorKind::UnsupportedOperation,
                message: format!("unsupported disclaimer agreement id: {agreement_id}"),
            });
        }
        let changed = self
            .preferences
            .accepted_disclaimer_agreement_ids
            .insert(agreement_id.to_string());
        if changed {
            self.note_change();
        }
        Ok(changed)
    }

    pub fn flight_data_cell_enabled(&self, cell_id: &str) -> bool {
        !self
            .preferences
            .disabled_flight_data_cell_ids
            .contains(cell_id)
    }

    pub fn disclaimer_state(&self) -> UiDisclaimerState {
        project_disclaimer_state(&self.preferences)
    }

    pub fn project(
        &mut self,
        display_policy_available: bool,
        flight_data_banner: &FlightDataBannerModel,
    ) -> SettingsProjectionResult {
        if let Some(cache) = self.projection_cache.as_ref() {
            if cache.settings_revision == self.revision
                && cache.display_policy_available == display_policy_available
                && cache.flight_data_banner == *flight_data_banner
            {
                return SettingsProjectionResult {
                    projection: cache.projection.clone(),
                    rebuilt: false,
                };
            }
        }

        let projection = SettingsProjection {
            settings_page_state: project_settings_page_state(
                &self.preferences,
                display_policy_available,
                flight_data_banner,
            ),
            display_policy: project_display_policy(&self.preferences, display_policy_available),
            disclaimer_state: project_disclaimer_state(&self.preferences),
            flight_data_banner: filtered_flight_data_banner(&self.preferences, flight_data_banner),
        };
        self.projection_cache = Some(SettingsProjectionCache {
            settings_revision: self.revision,
            display_policy_available,
            flight_data_banner: flight_data_banner.clone(),
            projection: projection.clone(),
        });
        SettingsProjectionResult {
            projection,
            rebuilt: true,
        }
    }

    fn note_change(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.projection_cache = None;
    }
}

pub(crate) fn default_settings_page_state() -> UiSettingsPageState {
    UiSettingsPageState {
        title: "Settings".to_string(),
        summary: String::new(),
        rows: Vec::new(),
    }
}

fn project_settings_page_state(
    preferences: &SettingsPreferences,
    display_policy_available: bool,
    flight_data_banner: &FlightDataBannerModel,
) -> UiSettingsPageState {
    let mut rows = vec![UiSettingsPageRow {
        kind: "grid_choices".to_string(),
        id: FLIGHT_DATA_VISIBILITY_ROW_ID.to_string(),
        title: "Flight data grid".to_string(),
        value_id: String::new(),
        stops: Vec::new(),
        items: flight_data_banner
            .cells
            .iter()
            .map(|cell| UiSettingsGridItem {
                cell: cell.clone(),
                enabled: !preferences.disabled_flight_data_cell_ids.contains(&cell.id),
            })
            .collect(),
        action_id: FLIGHT_DATA_VISIBILITY_ACTION_ID.to_string(),
    }];
    if display_policy_available {
        rows.push(UiSettingsPageRow {
            kind: "slider".to_string(),
            id: DISPLAY_DIM_TIMEOUT_ROW_ID.to_string(),
            title: "\u{1F50B} Display dims after...".to_string(),
            value_id: preferences.display_dim_timeout.id().to_string(),
            stops: DisplayDimTimeout::all_stops()
                .into_iter()
                .map(|timeout| UiSettingsSliderStop {
                    id: timeout.id().to_string(),
                    label: timeout.label().to_string(),
                })
                .collect(),
            items: Vec::new(),
            action_id: DISPLAY_DIM_TIMEOUT_ACTION_ID.to_string(),
        });
    }
    UiSettingsPageState {
        title: "Settings".to_string(),
        summary: if rows.is_empty() {
            "No platform settings are available.".to_string()
        } else {
            String::new()
        },
        rows,
    }
}

fn project_display_policy(
    preferences: &SettingsPreferences,
    display_policy_available: bool,
) -> Option<UiDisplayPolicy> {
    display_policy_available.then(|| UiDisplayPolicy {
        keep_screen_on: true,
        dim_after_ms: preferences.display_dim_timeout.dim_after_ms(),
        dim_brightness: DISPLAY_DIM_BRIGHTNESS,
    })
}

fn project_disclaimer_state(preferences: &SettingsPreferences) -> UiDisclaimerState {
    UiDisclaimerState {
        agreement_id: NO_WARRANTY_DISCLAIMER_AGREEMENT_ID.to_string(),
        required: !preferences
            .accepted_disclaimer_agreement_ids
            .contains(NO_WARRANTY_DISCLAIMER_AGREEMENT_ID),
        html: NO_WARRANTY_DISCLAIMER_HTML.to_string(),
        text: no_warranty_disclaimer_text(),
        accept_label: "I understand and agree".to_string(),
    }
}

fn filtered_flight_data_banner(
    preferences: &SettingsPreferences,
    flight_data_banner: &FlightDataBannerModel,
) -> FlightDataBannerModel {
    let mut filtered = flight_data_banner.clone();
    filtered
        .cells
        .retain(|cell| !preferences.disabled_flight_data_cell_ids.contains(&cell.id));
    filtered
}

fn no_warranty_disclaimer_text() -> String {
    let stripped = NO_WARRANTY_DISCLAIMER_HTML
        .replace("<p>", "")
        .replace("</p>", "")
        .replace("<strong>", "")
        .replace("</strong>", "");
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn invalid_settings_action(action_id: &str) -> AppError {
    AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: format!("unknown settings action: {action_id}"),
    }
}

fn invalid_settings_action_value(action_id: &str, value_id: &str) -> AppError {
    AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: format!("unknown settings value for {action_id}: {value_id}"),
    }
}

#[cfg(test)]
mod tests {
    use app_ui_contracts::session::FlightDataCell;

    use super::*;

    fn banner() -> FlightDataBannerModel {
        FlightDataBannerModel {
            cells: vec![
                FlightDataCell {
                    id: "altitude".to_string(),
                    label: "ALT".to_string(),
                    value: Some("12000".to_string()),
                    tone: Default::default(),
                    estimate_kind: Default::default(),
                },
                FlightDataCell {
                    id: "nexrad_age".to_string(),
                    label: "NEXRAD".to_string(),
                    value: Some("2m".to_string()),
                    tone: Default::default(),
                    estimate_kind: Default::default(),
                },
            ],
        }
    }

    #[test]
    fn projection_cache_tracks_owned_and_typed_external_inputs() {
        let mut controller = SettingsController::default();
        let input = banner();

        let first = controller.project(false, &input);
        assert!(first.rebuilt);
        assert_eq!(first.projection.settings_page_state.rows.len(), 1);
        assert!(first.projection.display_policy.is_none());

        let cached = controller.project(false, &input);
        assert!(!cached.rebuilt);
        assert_eq!(cached.projection, first.projection);

        let capability_changed = controller.project(true, &input);
        assert!(capability_changed.rebuilt);
        assert_eq!(
            capability_changed.projection.settings_page_state.rows.len(),
            2
        );

        let mut changed_banner = input;
        changed_banner.cells[0].value = Some("13000".to_string());
        assert!(controller.project(true, &changed_banner).rebuilt);
    }

    #[test]
    fn settings_mutations_own_revision_and_projection_invalidation() {
        let mut controller = SettingsController::default();
        let input = banner();
        controller.project(true, &input);

        assert!(controller
            .perform_action(
                &UiSettingsAction {
                    action_id: DISPLAY_DIM_TIMEOUT_ACTION_ID.to_string(),
                    value_id: "30s".to_string(),
                },
                true,
            )
            .expect("display action"));
        assert_eq!(controller.revision(), 1);
        assert!(controller.project(true, &input).rebuilt);

        assert!(!controller
            .perform_action(
                &UiSettingsAction {
                    action_id: DISPLAY_DIM_TIMEOUT_ACTION_ID.to_string(),
                    value_id: "30s".to_string(),
                },
                true,
            )
            .expect("idempotent display action"));
        assert_eq!(controller.revision(), 1);

        assert!(controller
            .perform_action(
                &UiSettingsAction {
                    action_id: FLIGHT_DATA_VISIBILITY_ACTION_ID.to_string(),
                    value_id: "nexrad_age".to_string(),
                },
                true,
            )
            .expect("visibility action"));
        assert_eq!(controller.revision(), 2);
        let projection = controller.project(true, &input).projection;
        assert!(!projection
            .flight_data_banner
            .cells
            .iter()
            .any(|cell| cell.id == "nexrad_age"));
        assert!(
            !projection.settings_page_state.rows[0]
                .items
                .iter()
                .find(|item| item.cell.id == "nexrad_age")
                .expect("settings item")
                .enabled
        );
    }

    #[test]
    fn restore_and_disclaimer_mutations_are_revisioned_only_when_changed() {
        let mut controller = SettingsController::default();
        assert!(!controller.restore_preferences(SettingsPreferences::default()));
        assert_eq!(controller.revision(), 0);

        assert!(controller
            .accept_disclaimer(NO_WARRANTY_DISCLAIMER_AGREEMENT_ID)
            .expect("accept disclaimer"));
        assert_eq!(controller.revision(), 1);
        assert!(!controller.disclaimer_state().required);
        assert!(!controller
            .accept_disclaimer(NO_WARRANTY_DISCLAIMER_AGREEMENT_ID)
            .expect("accept disclaimer again"));
        assert_eq!(controller.revision(), 1);

        let before = controller.clone();
        assert!(controller.accept_disclaimer("unknown").is_err());
        assert_eq!(
            controller.persistent_preferences(),
            before.persistent_preferences()
        );
        assert_eq!(controller.revision(), before.revision());
    }
}
