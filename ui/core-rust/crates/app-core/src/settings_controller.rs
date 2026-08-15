// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeSet, sync::Arc};

use app_ui_contracts::session::{
    DebugFlagId, UiDebugState, UiDisclaimerState, UiDisplayPolicy, UiSettingsAction,
    UiSettingsGridItem, UiSettingsPageRow, UiSettingsPageSection, UiSettingsPageState,
    UiSettingsSliderStop,
};
use serde::{Deserialize, Serialize};

use crate::{AppError, AppErrorKind, AppResult, FlightDataBannerModel};

const NO_WARRANTY_DISCLAIMER_HTML: &str = include_str!("../../../../shared/no-warranty.html");
const NO_WARRANTY_DISCLAIMER_AGREEMENT_ID: &str = "no-warranty-v1";
const DISPLAY_DIM_TIMEOUT_ROW_ID: &str = "display_dim_timeout";
const DISPLAY_DIM_TIMEOUT_ACTION_ID: &str = "display_dim_timeout";
const FLIGHT_DATA_VISIBILITY_ROW_ID: &str = "flight_data_visibility";
const FLIGHT_DATA_VISIBILITY_ACTION_ID: &str = "flight_data_visibility";
const DEBUG_DIAGNOSTICS_SECTION_ID: &str = "debug_diagnostics";
const DEBUG_FLAG_ACTION_PREFIX: &str = "debug_flag.";
const SETTINGS_TOGGLE_ON: &str = "on";
const SETTINGS_TOGGLE_OFF: &str = "off";
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
    debug_state: UiDebugState,
    projection: SettingsProjection,
}

#[derive(Clone, Default)]
pub(crate) struct SettingsController {
    preferences: SettingsPreferences,
    revision: u64,
    static_revision: u64,
    projection_cache: Option<SettingsProjectionCache>,
}

pub(crate) struct SettingsModelCheckpoint {
    preferences: SettingsPreferences,
    revision: u64,
    static_revision: u64,
}

impl SettingsController {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn static_revision(&self) -> u64 {
        self.static_revision
    }

    pub fn checkpoint_model(&self) -> SettingsModelCheckpoint {
        SettingsModelCheckpoint {
            preferences: self.preferences.clone(),
            revision: self.revision,
            static_revision: self.static_revision,
        }
    }

    pub fn rollback_model(&mut self, checkpoint: SettingsModelCheckpoint) {
        self.preferences = checkpoint.preferences;
        self.revision = checkpoint.revision;
        self.static_revision = checkpoint.static_revision;
        self.projection_cache = None;
    }

    pub fn persistent_preferences(&self) -> SettingsPreferences {
        self.preferences.clone()
    }

    pub fn restore_preferences(&mut self, preferences: SettingsPreferences) -> bool {
        let static_changed = self.preferences.display_dim_timeout
            != preferences.display_dim_timeout
            || self.preferences.accepted_disclaimer_agreement_ids
                != preferences.accepted_disclaimer_agreement_ids;
        let flight_data_changed = self.preferences.disabled_flight_data_cell_ids
            != preferences.disabled_flight_data_cell_ids;
        if !static_changed && !flight_data_changed {
            return false;
        }
        self.preferences = preferences;
        self.note_change(static_changed);
        true
    }

    pub fn perform_action(
        &mut self,
        action: &UiSettingsAction,
        display_policy_available: bool,
    ) -> AppResult<bool> {
        let (changed, static_changed) = match action.action_id.as_str() {
            DISPLAY_DIM_TIMEOUT_ACTION_ID => {
                if !display_policy_available {
                    return Err(invalid_settings_action(&action.action_id));
                }
                let timeout =
                    DisplayDimTimeout::from_value_id(&action.value_id).ok_or_else(|| {
                        invalid_settings_action_value(&action.action_id, &action.value_id)
                    })?;
                if self.preferences.display_dim_timeout == timeout {
                    (false, false)
                } else {
                    self.preferences.display_dim_timeout = timeout;
                    (true, true)
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
                (true, false)
            }
            _ => return Err(invalid_settings_action(&action.action_id)),
        };
        if changed {
            self.note_change(static_changed);
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
            self.note_change(true);
        }
        Ok(changed)
    }

    pub fn flight_data_cell_enabled(&self, cell_id: &str) -> bool {
        !self
            .preferences
            .disabled_flight_data_cell_ids
            .contains(cell_id)
    }

    pub fn project(
        &mut self,
        display_policy_available: bool,
        flight_data_banner: &FlightDataBannerModel,
        debug_state: &UiDebugState,
    ) -> SettingsProjectionResult {
        if let Some(cache) = self.projection_cache.as_ref() {
            if cache.settings_revision == self.revision
                && cache.display_policy_available == display_policy_available
                && cache.flight_data_banner == *flight_data_banner
                && cache.debug_state == *debug_state
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
                debug_state,
            ),
            display_policy: project_display_policy(&self.preferences, display_policy_available),
            disclaimer_state: project_disclaimer_state(&self.preferences),
            flight_data_banner: filtered_flight_data_banner(&self.preferences, flight_data_banner),
        };
        self.projection_cache = Some(SettingsProjectionCache {
            settings_revision: self.revision,
            display_policy_available,
            flight_data_banner: flight_data_banner.clone(),
            debug_state: debug_state.clone(),
            projection: projection.clone(),
        });
        SettingsProjectionResult {
            projection,
            rebuilt: true,
        }
    }

    fn note_change(&mut self, static_changed: bool) {
        self.revision = self.revision.wrapping_add(1);
        if static_changed {
            self.static_revision = self.static_revision.wrapping_add(1);
        }
        self.projection_cache = None;
    }
}

fn project_settings_page_state(
    preferences: &SettingsPreferences,
    display_policy_available: bool,
    flight_data_banner: &FlightDataBannerModel,
    debug_state: &UiDebugState,
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
    let sections = vec![UiSettingsPageSection {
        id: DEBUG_DIAGNOSTICS_SECTION_ID.to_string(),
        title: "Debug Diagnostics".to_string(),
        collapsed_by_default: true,
        rows: all_debug_flags()
            .into_iter()
            .map(|flag_id| {
                let (id, title) = debug_flag_spec(flag_id);
                UiSettingsPageRow {
                    kind: "toggle".to_string(),
                    id: format!("debug_{id}"),
                    title: title.to_string(),
                    value_id: if debug_flag_enabled(debug_state, flag_id) {
                        SETTINGS_TOGGLE_ON
                    } else {
                        SETTINGS_TOGGLE_OFF
                    }
                    .to_string(),
                    stops: Vec::new(),
                    items: Vec::new(),
                    action_id: format!("{DEBUG_FLAG_ACTION_PREFIX}{id}"),
                }
            })
            .collect(),
    }];
    UiSettingsPageState {
        title: "Settings".to_string(),
        summary: if rows.is_empty() && sections.is_empty() {
            "No platform settings are available.".to_string()
        } else {
            String::new()
        },
        rows,
        sections,
    }
}

pub(crate) fn debug_flag_settings_action(
    action: &UiSettingsAction,
) -> AppResult<Option<(DebugFlagId, bool)>> {
    let Some(id) = action.action_id.strip_prefix(DEBUG_FLAG_ACTION_PREFIX) else {
        return Ok(None);
    };
    let flag_id =
        debug_flag_from_id(id).ok_or_else(|| invalid_settings_action(&action.action_id))?;
    let enabled = match action.value_id.as_str() {
        SETTINGS_TOGGLE_ON => true,
        SETTINGS_TOGGLE_OFF => false,
        _ => {
            return Err(invalid_settings_action_value(
                &action.action_id,
                &action.value_id,
            ))
        }
    };
    Ok(Some((flag_id, enabled)))
}

fn all_debug_flags() -> [DebugFlagId; 10] {
    [
        DebugFlagId::TileLabels,
        DebugFlagId::NexradTileLabels,
        DebugFlagId::FastTiles,
        DebugFlagId::OfflineSimulatedClockButtons,
        DebugFlagId::SequencingFinishLines,
        DebugFlagId::PlateFlightPlan,
        DebugFlagId::BadAutopilot,
        DebugFlagId::InternetAdsb,
        DebugFlagId::GpsCapture,
        DebugFlagId::DebugLogToDeveloperServer,
    ]
}

fn debug_flag_spec(flag_id: DebugFlagId) -> (&'static str, &'static str) {
    match flag_id {
        DebugFlagId::TileLabels => ("tile_labels", "Tile labels"),
        DebugFlagId::NexradTileLabels => ("nexrad_tile_labels", "NEXRAD tile labels"),
        DebugFlagId::FastTiles => ("fast_tiles", "Fast tiles"),
        DebugFlagId::OfflineSimulatedClockButtons => (
            "offline_simulated_clock_buttons",
            "Offline simulated clock buttons",
        ),
        DebugFlagId::SequencingFinishLines => {
            ("sequencing_finish_lines", "Sequencing finish lines")
        }
        DebugFlagId::PlateFlightPlan => ("plate_flight_plan", "Flight plan on plates"),
        DebugFlagId::BadAutopilot => ("bad_autopilot", "Bad Autopilot"),
        DebugFlagId::InternetAdsb => ("internet_adsb", "Internet ADS-B"),
        DebugFlagId::GpsCapture => ("gps_capture", "Capture GPS samples"),
        DebugFlagId::DebugLogToDeveloperServer => (
            "debug_log_to_developer_server",
            "Debug log to developer server",
        ),
    }
}

fn debug_flag_from_id(id: &str) -> Option<DebugFlagId> {
    all_debug_flags()
        .into_iter()
        .find(|flag_id| debug_flag_spec(*flag_id).0 == id)
}

fn debug_flag_enabled(state: &UiDebugState, flag_id: DebugFlagId) -> bool {
    match flag_id {
        DebugFlagId::TileLabels => state.tile_labels,
        DebugFlagId::NexradTileLabels => state.nexrad_tile_labels,
        DebugFlagId::FastTiles => state.fast_tiles,
        DebugFlagId::OfflineSimulatedClockButtons => state.offline_simulated_clock_buttons,
        DebugFlagId::SequencingFinishLines => state.sequencing_finish_lines,
        DebugFlagId::PlateFlightPlan => state.plate_flight_plan,
        DebugFlagId::BadAutopilot => state.bad_autopilot,
        DebugFlagId::InternetAdsb => state.internet_adsb,
        DebugFlagId::GpsCapture => state.gps_capture,
        DebugFlagId::DebugLogToDeveloperServer => state.debug_log_to_developer_server,
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
                    action_id: None,
                    tone: Default::default(),
                    estimate_kind: Default::default(),
                },
                FlightDataCell {
                    id: "nexrad_age".to_string(),
                    label: "NEXRAD".to_string(),
                    value: Some("2m".to_string()),
                    action_id: None,
                    tone: Default::default(),
                    estimate_kind: Default::default(),
                },
            ],
        }
    }

    fn debug_state() -> UiDebugState {
        UiDebugState {
            tile_labels: false,
            nexrad_tile_labels: false,
            fast_tiles: false,
            offline_simulated_clock_buttons: false,
            sequencing_finish_lines: false,
            plate_flight_plan: false,
            bad_autopilot: false,
            internet_adsb: false,
            gps_capture: false,
            debug_log_to_developer_server: false,
        }
    }

    #[test]
    fn projection_cache_tracks_owned_and_typed_external_inputs() {
        let mut controller = SettingsController::default();
        let input = banner();
        let debug = debug_state();

        let first = controller.project(false, &input, &debug);
        assert!(first.rebuilt);
        assert_eq!(first.projection.settings_page_state.rows.len(), 1);
        assert!(first.projection.display_policy.is_none());

        let cached = controller.project(false, &input, &debug);
        assert!(!cached.rebuilt);
        assert_eq!(cached.projection, first.projection);

        let capability_changed = controller.project(true, &input, &debug);
        assert!(capability_changed.rebuilt);
        assert_eq!(
            capability_changed.projection.settings_page_state.rows.len(),
            2
        );

        let mut changed_banner = input;
        changed_banner.cells[0].value = Some("13000".to_string());
        assert!(controller.project(true, &changed_banner, &debug).rebuilt);
    }

    #[test]
    fn settings_mutations_own_revision_and_projection_invalidation() {
        let mut controller = SettingsController::default();
        let input = banner();
        let debug = debug_state();
        controller.project(true, &input, &debug);

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
        assert_eq!(controller.static_revision(), 1);
        assert!(controller.project(true, &input, &debug).rebuilt);

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
        assert_eq!(controller.static_revision(), 1);
        let projection = controller.project(true, &input, &debug).projection;
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
        assert!(!project_disclaimer_state(&controller.preferences).required);
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
