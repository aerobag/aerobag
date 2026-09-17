// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use app_ui_contracts::session::{
    DebugFlagId, UiAircraftLibraryState, UiDebugState, UiDisclaimerState, UiDisplayPolicy,
    UiSettingsAction, UiSettingsGridItem, UiSettingsPageBlock, UiSettingsPageRow,
    UiSettingsPageSection, UiSettingsPageState, UiSettingsRowKind, UiSettingsSliderStop,
    UiSettingsSyncIndicator,
};
use serde::{Deserialize, Serialize};

use crate::{AppError, AppErrorKind, AppResult, FlightDataBannerModel};

const NO_WARRANTY_DISCLAIMER_HTML: &str = include_str!("../../../../shared/no-warranty.html");
const NO_WARRANTY_DISCLAIMER_AGREEMENT_ID: &str = "no-warranty-v1";
const DISPLAY_DIM_TIMEOUT_ROW_ID: &str = "display_dim_timeout";
const DISPLAY_DIM_TIMEOUT_ACTION_ID: &str = "display_dim_timeout";
const DISPLAY_DIM_ON_BATTERY_ONLY_ID: &str = "display_dim_on_battery_only";
const INACTIVITY_SLEEP_TIMEOUT_ROW_ID: &str = "inactivity_sleep_timeout";
const INACTIVITY_SLEEP_TIMEOUT_ACTION_ID: &str = "inactivity_sleep_timeout";
const NEXRAD_COVERAGE_ACTION_ID: &str = "nexrad_coverage";
const NEXRAD_OFFLINE_PROFILE_ACTION_ID: &str = "nexrad_offline_profile";
const NEXRAD_SHOWN_CADENCE_ACTION_ID: &str = "nexrad_shown_cadence";
const NEXRAD_HIDDEN_CADENCE_ACTION_ID: &str = "nexrad_hidden_cadence";
const NEXRAD_ASLEEP_CADENCE_ACTION_ID: &str = "nexrad_asleep_cadence";
const FLIGHT_DATA_VISIBILITY_ROW_ID: &str = "flight_data_visibility";
const FLIGHT_DATA_VISIBILITY_ACTION_ID: &str = "flight_data_visibility";
const DEBUG_DIAGNOSTICS_SECTION_ID: &str = "debug_diagnostics";
const DEBUG_FLAG_ACTION_PREFIX: &str = "debug_flag.";
const SETTINGS_TOGGLE_ON: &str = "on";
const SETTINGS_TOGGLE_OFF: &str = "off";
const DISPLAY_DIM_BRIGHTNESS: f32 = 0.05;
const FLIGHT_DATA_VISIBILITY_HELP: &str = "Select which data items appear on the chart page.";
const DISPLAY_DIM_TIMEOUT_HELP: &str = "Save power while keeping the screen unlocked. Select 'Only dim when on battery' to stay bright while plugged in.";
const INACTIVITY_SLEEP_TIMEOUT_HELP: &str =
    "Avoid battery drain when you leave your tablet in your flight bag.";
const NEXRAD_COVERAGE_HELP: &str = "Visible loads on demand, uses less data. Full offline loads eagerly, useful when network coverage is sketchy.";
const NEXRAD_OFFLINE_DETAIL_HELP: &str = "Reduce detail to use less data.";
const NEXRAD_SHOWN_CADENCE_HELP: &str = "Load fewer frames to use less data.";
const NEXRAD_HIDDEN_CADENCE_HELP: &str = "Eagerly fetch fewer frames to use less data.";
const NEXRAD_ASLEEP_CADENCE_HELP: &str =
    "Save data by not fetching nexrad if app is entirely asleep";
const DEBUG_SECTION_ACTION_ID: &str = "section.debug_diagnostics";
const NEXRAD_UPDATES_PER_HOUR: f64 = 12.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloudSyncedSettingsRecord {
    InactivitySleepTimeout,
    NexradAcquisition,
}

pub trait SettingsStorage: Send + Sync {
    fn read_settings(&self) -> AppResult<Option<Vec<u8>>>;
    fn write_settings(&self, bytes: &[u8]) -> AppResult<()>;
    fn read_tour_introduction(&self) -> AppResult<Option<Vec<u8>>> {
        Ok(None)
    }
    fn write_tour_introduction(&self, _bytes: &[u8]) -> AppResult<()> {
        Ok(())
    }
}

pub type SettingsStorageHandle = Arc<dyn SettingsStorage>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayDimTimeout {
    #[serde(rename = "10s")]
    TenSeconds,
    #[serde(rename = "30s")]
    ThirtySeconds,
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "2m")]
    #[default]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InactivitySleepTimeout {
    #[serde(rename = "30m")]
    ThirtyMinutes,
    #[serde(rename = "1h")]
    #[default]
    OneHour,
    #[serde(rename = "2h")]
    TwoHours,
    #[serde(rename = "4h")]
    FourHours,
    #[serde(rename = "never")]
    Never,
}

impl InactivitySleepTimeout {
    fn id(self) -> &'static str {
        match self {
            Self::ThirtyMinutes => "30m",
            Self::OneHour => "1h",
            Self::TwoHours => "2h",
            Self::FourHours => "4h",
            Self::Never => "never",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::ThirtyMinutes => "30m",
            Self::OneHour => "1h",
            Self::TwoHours => "2h",
            Self::FourHours => "4h",
            Self::Never => "Never",
        }
    }

    fn sleep_after_ms(self) -> Option<u64> {
        match self {
            Self::ThirtyMinutes => Some(30 * 60 * 1_000),
            Self::OneHour => Some(60 * 60 * 1_000),
            Self::TwoHours => Some(2 * 60 * 60 * 1_000),
            Self::FourHours => Some(4 * 60 * 60 * 1_000),
            Self::Never => None,
        }
    }

    fn from_value_id(value_id: &str) -> Option<Self> {
        match value_id {
            "30m" => Some(Self::ThirtyMinutes),
            "1h" => Some(Self::OneHour),
            "2h" => Some(Self::TwoHours),
            "4h" => Some(Self::FourHours),
            "never" => Some(Self::Never),
            _ => None,
        }
    }

    fn all_stops() -> [Self; 5] {
        [
            Self::ThirtyMinutes,
            Self::OneHour,
            Self::TwoHours,
            Self::FourHours,
            Self::Never,
        ]
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NexradCoverageMode {
    FullOffline,
    #[default]
    ViewportOnly,
}

impl NexradCoverageMode {
    pub fn id(self) -> &'static str {
        match self {
            Self::FullOffline => "full_offline",
            Self::ViewportOnly => "viewport_only",
        }
    }

    fn from_value_id(value_id: &str) -> Option<Self> {
        match value_id {
            "full_offline" => Some(Self::FullOffline),
            "viewport_only" => Some(Self::ViewportOnly),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum NexradOfflineProfile {
    #[serde(rename = "offline_0")]
    Offline0,
    #[serde(rename = "offline_low1")]
    #[default]
    OfflineLow1,
}

impl NexradOfflineProfile {
    pub fn id(self) -> &'static str {
        match self {
            Self::Offline0 => "offline_0",
            Self::OfflineLow1 => "offline_low1",
        }
    }

    pub fn base_resolution(self) -> u32 {
        match self {
            Self::Offline0 => 0,
            Self::OfflineLow1 => 1,
        }
    }

    fn from_value_id(value_id: &str) -> Option<Self> {
        match value_id {
            "offline_0" => Some(Self::Offline0),
            "offline_low1" => Some(Self::OfflineLow1),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NexradUpdateCadence {
    #[default]
    Never,
    ThirtyMinutes,
    TenMinutes,
    EveryUpdate,
}

impl NexradUpdateCadence {
    pub fn id(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::ThirtyMinutes => "30m",
            Self::TenMinutes => "10m",
            Self::EveryUpdate => "every_update",
        }
    }

    pub fn interval_ms(self) -> Option<i64> {
        match self {
            Self::Never => None,
            Self::ThirtyMinutes => Some(30 * 60 * 1_000),
            Self::TenMinutes => Some(10 * 60 * 1_000),
            Self::EveryUpdate => Some(0),
        }
    }

    fn from_value_id(value_id: &str) -> Option<Self> {
        match value_id {
            "never" => Some(Self::Never),
            "30m" => Some(Self::ThirtyMinutes),
            "10m" => Some(Self::TenMinutes),
            "every_update" => Some(Self::EveryUpdate),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NexradAcquisitionPreferences {
    #[serde(default)]
    pub coverage: NexradCoverageMode,
    #[serde(default)]
    pub offline_profile: NexradOfflineProfile,
    #[serde(default = "default_nexrad_shown_cadence")]
    pub shown_cadence: NexradUpdateCadence,
    #[serde(default = "default_nexrad_hidden_cadence")]
    pub hidden_cadence: NexradUpdateCadence,
    #[serde(default)]
    pub asleep_cadence: NexradUpdateCadence,
}

const fn default_nexrad_shown_cadence() -> NexradUpdateCadence {
    NexradUpdateCadence::TenMinutes
}

const fn default_nexrad_hidden_cadence() -> NexradUpdateCadence {
    NexradUpdateCadence::Never
}

impl Default for NexradAcquisitionPreferences {
    fn default() -> Self {
        Self {
            coverage: NexradCoverageMode::ViewportOnly,
            offline_profile: NexradOfflineProfile::OfflineLow1,
            shown_cadence: default_nexrad_shown_cadence(),
            hidden_cadence: default_nexrad_hidden_cadence(),
            asleep_cadence: NexradUpdateCadence::Never,
        }
    }
}

impl NexradAcquisitionPreferences {
    fn normalize(mut self) -> Self {
        self.hidden_cadence = self.hidden_cadence.min(self.shown_cadence);
        self.asleep_cadence = self
            .asleep_cadence
            .min(self.hidden_cadence)
            .min(NexradUpdateCadence::ThirtyMinutes);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NexradAcquisitionDirective {
    pub coverage: NexradCoverageMode,
    pub offline_profile: NexradOfflineProfile,
    pub cadence: NexradUpdateCadence,
}

impl Default for NexradAcquisitionDirective {
    fn default() -> Self {
        let preferences = NexradAcquisitionPreferences::default();
        Self {
            coverage: preferences.coverage,
            offline_profile: preferences.offline_profile,
            cadence: NexradUpdateCadence::EveryUpdate,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SettingsPreferences {
    #[serde(default)]
    pub display_dim_timeout: DisplayDimTimeout,
    #[serde(default)]
    pub display_dim_on_battery_only: bool,
    #[serde(default)]
    pub inactivity_sleep_timeout: InactivitySleepTimeout,
    #[serde(default)]
    pub nexrad_acquisition: NexradAcquisitionPreferences,
    #[serde(default)]
    pub disabled_flight_data_cell_ids: BTreeSet<String>,
    #[serde(default)]
    pub flight_plan_ete_scope: crate::FlightPlanEteScope,
    #[serde(default)]
    pub airway_navigation_mode: crate::AirwayNavigationMode,
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
    sync_indicator: Option<UiSettingsSyncIndicator>,
    aircraft_library: Option<UiAircraftLibraryState>,
    nexrad_profile_bytes: Option<BTreeMap<String, u64>>,
    flight_data_banner: FlightDataBannerModel,
    debug_state: UiDebugState,
    projection: SettingsProjection,
}

#[derive(Clone, Default)]
pub(crate) struct SettingsController {
    preferences: SettingsPreferences,
    external_power_connected: bool,
    debug_section_expanded: bool,
    aircraft_editor: Option<crate::aircraft_library::AircraftLibraryEditorModel>,
    revision: u64,
    static_revision: u64,
    projection_cache: Option<SettingsProjectionCache>,
}

#[derive(Clone)]
pub(crate) struct SettingsModelCheckpoint {
    preferences: SettingsPreferences,
    external_power_connected: bool,
    debug_section_expanded: bool,
    aircraft_editor: Option<crate::aircraft_library::AircraftLibraryEditorModel>,
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
            external_power_connected: self.external_power_connected,
            debug_section_expanded: self.debug_section_expanded,
            aircraft_editor: self.aircraft_editor.clone(),
            revision: self.revision,
            static_revision: self.static_revision,
        }
    }

    pub fn rollback_model(&mut self, checkpoint: SettingsModelCheckpoint) {
        self.preferences = checkpoint.preferences;
        self.external_power_connected = checkpoint.external_power_connected;
        self.debug_section_expanded = checkpoint.debug_section_expanded;
        self.aircraft_editor = checkpoint.aircraft_editor;
        self.revision = checkpoint.revision;
        self.static_revision = checkpoint.static_revision;
        self.projection_cache = None;
    }

    pub fn disclaimer_required(&self) -> bool {
        project_disclaimer_state(&self.preferences).required
    }

    pub fn persistent_preferences(&self) -> SettingsPreferences {
        self.preferences.clone()
    }

    pub fn set_external_power_connected(&mut self, connected: bool) {
        if self.external_power_connected != connected {
            self.external_power_connected = connected;
            // Display policy shares the static settings projection group.
            self.note_change(true);
        }
    }

    pub fn airway_navigation_mode(&self) -> crate::AirwayNavigationMode {
        self.preferences.airway_navigation_mode
    }

    pub fn set_airway_navigation_mode(&mut self, mode: crate::AirwayNavigationMode) {
        if self.preferences.airway_navigation_mode != mode {
            self.preferences.airway_navigation_mode = mode;
            self.note_change(false);
        }
    }

    pub fn aircraft_editor(&self) -> Option<&crate::aircraft_library::AircraftLibraryEditorModel> {
        self.aircraft_editor.as_ref()
    }

    pub fn replace_aircraft_editor(
        &mut self,
        editor: Option<crate::aircraft_library::AircraftLibraryEditorModel>,
    ) -> bool {
        if self.aircraft_editor == editor {
            return false;
        }
        self.aircraft_editor = editor;
        self.note_change(true);
        true
    }

    pub fn note_aircraft_library_change(&mut self) {
        self.note_change(true);
    }

    pub fn restore_preferences(&mut self, mut preferences: SettingsPreferences) -> bool {
        preferences.nexrad_acquisition = preferences.nexrad_acquisition.normalize();
        let static_changed = self.preferences.display_dim_timeout
            != preferences.display_dim_timeout
            || self.preferences.display_dim_on_battery_only
                != preferences.display_dim_on_battery_only
            || self.preferences.inactivity_sleep_timeout != preferences.inactivity_sleep_timeout
            || self.preferences.nexrad_acquisition != preferences.nexrad_acquisition
            || self.preferences.accepted_disclaimer_agreement_ids
                != preferences.accepted_disclaimer_agreement_ids;
        if self.preferences == preferences {
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
        nexrad_settings_available: bool,
    ) -> AppResult<bool> {
        if matches!(
            action.action_id.as_str(),
            NEXRAD_COVERAGE_ACTION_ID
                | NEXRAD_OFFLINE_PROFILE_ACTION_ID
                | NEXRAD_SHOWN_CADENCE_ACTION_ID
                | NEXRAD_HIDDEN_CADENCE_ACTION_ID
                | NEXRAD_ASLEEP_CADENCE_ACTION_ID
        ) && !nexrad_settings_available
        {
            return Err(invalid_settings_action(&action.action_id));
        }
        let (changed, static_changed) = match action.action_id.as_str() {
            DEBUG_SECTION_ACTION_ID => {
                let expanded = match action.value_id.as_str() {
                    "on" => true,
                    "off" => false,
                    _ => {
                        return Err(invalid_settings_action_value(
                            &action.action_id,
                            &action.value_id,
                        ))
                    }
                };
                let changed = self.debug_section_expanded != expanded;
                self.debug_section_expanded = expanded;
                (changed, changed)
            }
            DISPLAY_DIM_ON_BATTERY_ONLY_ID => {
                if !display_policy_available {
                    return Err(invalid_settings_action(&action.action_id));
                }
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
                let changed = self.preferences.display_dim_on_battery_only != enabled;
                self.preferences.display_dim_on_battery_only = enabled;
                (changed, changed)
            }
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
            INACTIVITY_SLEEP_TIMEOUT_ACTION_ID => {
                if !display_policy_available {
                    return Err(invalid_settings_action(&action.action_id));
                }
                let timeout =
                    InactivitySleepTimeout::from_value_id(&action.value_id).ok_or_else(|| {
                        invalid_settings_action_value(&action.action_id, &action.value_id)
                    })?;
                if self.preferences.inactivity_sleep_timeout == timeout {
                    (false, false)
                } else {
                    self.preferences.inactivity_sleep_timeout = timeout;
                    (true, true)
                }
            }
            NEXRAD_COVERAGE_ACTION_ID => {
                let coverage =
                    NexradCoverageMode::from_value_id(&action.value_id).ok_or_else(|| {
                        invalid_settings_action_value(&action.action_id, &action.value_id)
                    })?;
                let changed = self.preferences.nexrad_acquisition.coverage != coverage;
                self.preferences.nexrad_acquisition.coverage = coverage;
                (changed, changed)
            }
            NEXRAD_OFFLINE_PROFILE_ACTION_ID => {
                let profile =
                    NexradOfflineProfile::from_value_id(&action.value_id).ok_or_else(|| {
                        invalid_settings_action_value(&action.action_id, &action.value_id)
                    })?;
                let changed = self.preferences.nexrad_acquisition.offline_profile != profile;
                self.preferences.nexrad_acquisition.offline_profile = profile;
                (changed, changed)
            }
            NEXRAD_SHOWN_CADENCE_ACTION_ID
            | NEXRAD_HIDDEN_CADENCE_ACTION_ID
            | NEXRAD_ASLEEP_CADENCE_ACTION_ID => {
                let cadence =
                    NexradUpdateCadence::from_value_id(&action.value_id).ok_or_else(|| {
                        invalid_settings_action_value(&action.action_id, &action.value_id)
                    })?;
                if (action.action_id == NEXRAD_SHOWN_CADENCE_ACTION_ID
                    && cadence == NexradUpdateCadence::Never)
                    || (action.action_id == NEXRAD_ASLEEP_CADENCE_ACTION_ID
                        && cadence > NexradUpdateCadence::ThirtyMinutes)
                {
                    return Err(invalid_settings_action_value(
                        &action.action_id,
                        &action.value_id,
                    ));
                }
                let old = self.preferences.nexrad_acquisition;
                match action.action_id.as_str() {
                    NEXRAD_SHOWN_CADENCE_ACTION_ID => {
                        self.preferences.nexrad_acquisition.shown_cadence = cadence;
                        self.preferences.nexrad_acquisition.hidden_cadence = self
                            .preferences
                            .nexrad_acquisition
                            .hidden_cadence
                            .min(cadence);
                        self.preferences.nexrad_acquisition.asleep_cadence = self
                            .preferences
                            .nexrad_acquisition
                            .asleep_cadence
                            .min(self.preferences.nexrad_acquisition.hidden_cadence);
                    }
                    NEXRAD_HIDDEN_CADENCE_ACTION_ID => {
                        self.preferences.nexrad_acquisition.hidden_cadence = cadence;
                        self.preferences.nexrad_acquisition.shown_cadence = self
                            .preferences
                            .nexrad_acquisition
                            .shown_cadence
                            .max(cadence);
                        self.preferences.nexrad_acquisition.asleep_cadence = self
                            .preferences
                            .nexrad_acquisition
                            .asleep_cadence
                            .min(cadence);
                    }
                    NEXRAD_ASLEEP_CADENCE_ACTION_ID => {
                        self.preferences.nexrad_acquisition.asleep_cadence = cadence;
                        self.preferences.nexrad_acquisition.hidden_cadence = self
                            .preferences
                            .nexrad_acquisition
                            .hidden_cadence
                            .max(cadence);
                        self.preferences.nexrad_acquisition.shown_cadence = self
                            .preferences
                            .nexrad_acquisition
                            .shown_cadence
                            .max(self.preferences.nexrad_acquisition.hidden_cadence);
                    }
                    _ => unreachable!(),
                }
                (old != self.preferences.nexrad_acquisition, true)
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

    pub fn flight_plan_ete_scope(&self) -> crate::FlightPlanEteScope {
        self.preferences.flight_plan_ete_scope
    }

    pub fn inactivity_sleep_timeout(&self) -> InactivitySleepTimeout {
        self.preferences.inactivity_sleep_timeout
    }

    pub fn nexrad_acquisition_preferences(&self) -> NexradAcquisitionPreferences {
        self.preferences.nexrad_acquisition
    }

    pub fn set_nexrad_acquisition_preferences(
        &mut self,
        preferences: NexradAcquisitionPreferences,
    ) -> bool {
        let preferences = preferences.normalize();
        if self.preferences.nexrad_acquisition == preferences {
            return false;
        }
        self.preferences.nexrad_acquisition = preferences;
        self.note_change(true);
        true
    }

    pub fn set_inactivity_sleep_timeout(&mut self, timeout: InactivitySleepTimeout) -> bool {
        if self.preferences.inactivity_sleep_timeout == timeout {
            return false;
        }
        self.preferences.inactivity_sleep_timeout = timeout;
        self.note_change(true);
        true
    }

    pub fn toggle_flight_plan_ete_scope(&mut self) {
        self.preferences.flight_plan_ete_scope = self.preferences.flight_plan_ete_scope.toggled();
        self.note_change(false);
    }

    pub fn project(
        &mut self,
        display_policy_available: bool,
        sync_indicator: Option<UiSettingsSyncIndicator>,
        nexrad_profile_bytes: Option<&BTreeMap<String, u64>>,
        flight_data_banner: &FlightDataBannerModel,
        debug_state: &UiDebugState,
        aircraft_library: Option<&UiAircraftLibraryState>,
    ) -> SettingsProjectionResult {
        if let Some(cache) = self.projection_cache.as_ref() {
            if cache.settings_revision == self.revision
                && cache.display_policy_available == display_policy_available
                && cache.sync_indicator == sync_indicator
                && cache.aircraft_library.as_ref() == aircraft_library
                && cache.nexrad_profile_bytes.as_ref() == nexrad_profile_bytes
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
                sync_indicator.as_ref(),
                nexrad_profile_bytes,
                flight_data_banner,
                debug_state,
                self.debug_section_expanded,
                aircraft_library,
            ),
            display_policy: project_display_policy(
                &self.preferences,
                display_policy_available,
                self.external_power_connected,
            ),
            disclaimer_state: project_disclaimer_state(&self.preferences),
            flight_data_banner: filtered_flight_data_banner(&self.preferences, flight_data_banner),
        };
        self.projection_cache = Some(SettingsProjectionCache {
            settings_revision: self.revision,
            display_policy_available,
            sync_indicator,
            aircraft_library: aircraft_library.cloned(),
            nexrad_profile_bytes: nexrad_profile_bytes.cloned(),
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
    sync_indicator: Option<&UiSettingsSyncIndicator>,
    nexrad_profile_bytes: Option<&BTreeMap<String, u64>>,
    flight_data_banner: &FlightDataBannerModel,
    debug_state: &UiDebugState,
    debug_section_expanded: bool,
    aircraft_library: Option<&UiAircraftLibraryState>,
) -> UiSettingsPageState {
    let mut rows = vec![UiSettingsPageRow {
        kind: UiSettingsRowKind::GridChoices,
        id: FLIGHT_DATA_VISIBILITY_ROW_ID.to_string(),
        title: "Flight data grid".to_string(),
        help_text: Some(FLIGHT_DATA_VISIBILITY_HELP.to_string()),
        sync_indicator: None,
        indent_level: 0,
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
            kind: UiSettingsRowKind::Slider,
            id: DISPLAY_DIM_TIMEOUT_ROW_ID.to_string(),
            title: "\u{1F50B} Display dims after...".to_string(),
            help_text: Some(DISPLAY_DIM_TIMEOUT_HELP.to_string()),
            sync_indicator: None,
            indent_level: 0,
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
        rows.push(UiSettingsPageRow {
            kind: UiSettingsRowKind::Toggle,
            id: DISPLAY_DIM_ON_BATTERY_ONLY_ID.to_string(),
            title: "Only dim when on battery".to_string(),
            help_text: None,
            sync_indicator: None,
            indent_level: 1,
            value_id: if preferences.display_dim_on_battery_only {
                SETTINGS_TOGGLE_ON
            } else {
                SETTINGS_TOGGLE_OFF
            }
            .to_string(),
            stops: Vec::new(),
            items: Vec::new(),
            action_id: DISPLAY_DIM_ON_BATTERY_ONLY_ID.to_string(),
        });
        rows.push(UiSettingsPageRow {
            kind: UiSettingsRowKind::Slider,
            id: INACTIVITY_SLEEP_TIMEOUT_ROW_ID.to_string(),
            title: "\u{1F50B} Screen and GPS sleep after...".to_string(),
            help_text: Some(INACTIVITY_SLEEP_TIMEOUT_HELP.to_string()),
            sync_indicator: None,
            indent_level: 0,
            value_id: preferences.inactivity_sleep_timeout.id().to_string(),
            stops: InactivitySleepTimeout::all_stops()
                .into_iter()
                .map(|timeout| UiSettingsSliderStop {
                    id: timeout.id().to_string(),
                    label: timeout.label().to_string(),
                })
                .collect(),
            items: Vec::new(),
            action_id: INACTIVITY_SLEEP_TIMEOUT_ACTION_ID.to_string(),
        });
    }
    if let Some(profile_bytes) = nexrad_profile_bytes {
        append_nexrad_settings_rows(&mut rows, preferences, profile_bytes);
    }
    let mut sections = vec![UiSettingsPageSection {
        id: DEBUG_DIAGNOSTICS_SECTION_ID.to_string(),
        title: "Debug Diagnostics".to_string(),
        expanded: debug_section_expanded,
        toggle_action: UiSettingsAction {
            action_id: DEBUG_SECTION_ACTION_ID.to_string(),
            value_id: if debug_section_expanded { "off" } else { "on" }.to_string(),
        },
        rows: all_debug_flags()
            .into_iter()
            .map(|flag_id| {
                let (id, title) = debug_flag_spec(flag_id);
                UiSettingsPageRow {
                    kind: UiSettingsRowKind::Toggle,
                    id: format!("debug_{id}"),
                    title: title.to_string(),
                    help_text: None,
                    sync_indicator: None,
                    indent_level: 0,
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
    if let Some(indicator) = sync_indicator {
        for row in rows.iter_mut().chain(
            sections
                .iter_mut()
                .flat_map(|section| section.rows.iter_mut()),
        ) {
            if settings_action_is_cloud_synced(&row.action_id) {
                row.sync_indicator = Some(indicator.clone());
            }
        }
    }
    let mut blocks = vec![UiSettingsPageBlock::Controls { rows }];
    if let Some(library) = aircraft_library {
        blocks.push(UiSettingsPageBlock::AircraftLibrary {
            library: library.clone(),
        });
    }
    blocks.extend(
        sections
            .into_iter()
            .map(|section| UiSettingsPageBlock::Section { section }),
    );
    UiSettingsPageState {
        title: "Settings".to_string(),
        summary: String::new(),
        blocks,
    }
}

fn append_nexrad_settings_rows(
    rows: &mut Vec<UiSettingsPageRow>,
    preferences: &SettingsPreferences,
    profile_bytes: &BTreeMap<String, u64>,
) {
    let nexrad = preferences.nexrad_acquisition;
    rows.push(settings_slider_row(
        "nexrad_coverage",
        "NEXRAD coverage",
        Some(NEXRAD_COVERAGE_HELP),
        nexrad.coverage.id(),
        NEXRAD_COVERAGE_ACTION_ID,
        [
            (
                NexradCoverageMode::ViewportOnly.id(),
                "Visible area only".to_string(),
            ),
            (
                NexradCoverageMode::FullOffline.id(),
                "Full offline".to_string(),
            ),
        ],
        0,
    ));
    if nexrad.coverage == NexradCoverageMode::ViewportOnly {
        return;
    }

    rows.push(settings_slider_row(
        "nexrad_offline_profile",
        "NEXRAD offline detail",
        Some(NEXRAD_OFFLINE_DETAIL_HELP),
        nexrad.offline_profile.id(),
        NEXRAD_OFFLINE_PROFILE_ACTION_ID,
        [
            (
                NexradOfflineProfile::OfflineLow1.id(),
                "Reduced".to_string(),
            ),
            (NexradOfflineProfile::Offline0.id(), "Full".to_string()),
        ],
        1,
    ));
    let bytes_per_update = profile_bytes.get(nexrad.offline_profile.id()).copied();
    rows.push(nexrad_cadence_row(
        "nexrad_shown_cadence",
        "NEXRAD updates while shown",
        Some(NEXRAD_SHOWN_CADENCE_HELP),
        nexrad.shown_cadence,
        NEXRAD_SHOWN_CADENCE_ACTION_ID,
        &[
            NexradUpdateCadence::ThirtyMinutes,
            NexradUpdateCadence::TenMinutes,
            NexradUpdateCadence::EveryUpdate,
        ],
        bytes_per_update,
    ));
    rows.push(nexrad_cadence_row(
        "nexrad_hidden_cadence",
        "NEXRAD updates while hidden",
        Some(NEXRAD_HIDDEN_CADENCE_HELP),
        nexrad.hidden_cadence,
        NEXRAD_HIDDEN_CADENCE_ACTION_ID,
        &[
            NexradUpdateCadence::Never,
            NexradUpdateCadence::ThirtyMinutes,
            NexradUpdateCadence::TenMinutes,
            NexradUpdateCadence::EveryUpdate,
        ],
        bytes_per_update,
    ));
    rows.push(nexrad_cadence_row(
        "nexrad_asleep_cadence",
        "NEXRAD updates while app sleeps",
        Some(NEXRAD_ASLEEP_CADENCE_HELP),
        nexrad.asleep_cadence,
        NEXRAD_ASLEEP_CADENCE_ACTION_ID,
        &[
            NexradUpdateCadence::Never,
            NexradUpdateCadence::ThirtyMinutes,
        ],
        bytes_per_update,
    ));
}

fn settings_slider_row<const N: usize>(
    id: &str,
    title: &str,
    help_text: Option<&str>,
    value_id: &str,
    action_id: &str,
    stops: [(&str, String); N],
    indent_level: u8,
) -> UiSettingsPageRow {
    UiSettingsPageRow {
        kind: UiSettingsRowKind::Slider,
        id: id.to_string(),
        title: title.to_string(),
        help_text: help_text.map(str::to_string),
        sync_indicator: None,
        indent_level,
        value_id: value_id.to_string(),
        stops: stops
            .into_iter()
            .map(|(id, label)| UiSettingsSliderStop {
                id: id.to_string(),
                label,
            })
            .collect(),
        items: Vec::new(),
        action_id: action_id.to_string(),
    }
}

fn nexrad_cadence_row(
    id: &str,
    title: &str,
    help_text: Option<&str>,
    value: NexradUpdateCadence,
    action_id: &str,
    cadences: &[NexradUpdateCadence],
    bytes_per_update: Option<u64>,
) -> UiSettingsPageRow {
    let title = bytes_per_update
        .map(|bytes| format!("{title} · {}", nexrad_usage_label(value, bytes)))
        .unwrap_or_else(|| title.to_string());
    UiSettingsPageRow {
        kind: UiSettingsRowKind::Slider,
        id: id.to_string(),
        title,
        help_text: help_text.map(str::to_string),
        sync_indicator: None,
        indent_level: 1,
        value_id: value.id().to_string(),
        stops: cadences
            .iter()
            .copied()
            .map(|cadence| UiSettingsSliderStop {
                id: cadence.id().to_string(),
                label: nexrad_cadence_label(cadence),
            })
            .collect(),
        items: Vec::new(),
        action_id: action_id.to_string(),
    }
}

fn nexrad_cadence_label(cadence: NexradUpdateCadence) -> String {
    match cadence {
        NexradUpdateCadence::Never => "Never",
        NexradUpdateCadence::ThirtyMinutes => "30m",
        NexradUpdateCadence::TenMinutes => "10m",
        NexradUpdateCadence::EveryUpdate => "Every",
    }
    .to_string()
}

fn nexrad_usage_label(cadence: NexradUpdateCadence, bytes_per_update: u64) -> String {
    let updates_per_hour = match cadence {
        NexradUpdateCadence::Never => 0.0,
        NexradUpdateCadence::ThirtyMinutes => 2.0,
        NexradUpdateCadence::TenMinutes => 6.0,
        NexradUpdateCadence::EveryUpdate => NEXRAD_UPDATES_PER_HOUR,
    };
    let mib_per_hour = bytes_per_update as f64 * updates_per_hour / (1024.0 * 1024.0);
    format!("{mib_per_hour:.1} MiB/h")
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

pub(crate) fn cloud_synced_settings_record(action_id: &str) -> Option<CloudSyncedSettingsRecord> {
    match action_id {
        INACTIVITY_SLEEP_TIMEOUT_ACTION_ID => {
            Some(CloudSyncedSettingsRecord::InactivitySleepTimeout)
        }
        NEXRAD_COVERAGE_ACTION_ID
        | NEXRAD_OFFLINE_PROFILE_ACTION_ID
        | NEXRAD_SHOWN_CADENCE_ACTION_ID
        | NEXRAD_HIDDEN_CADENCE_ACTION_ID
        | NEXRAD_ASLEEP_CADENCE_ACTION_ID => Some(CloudSyncedSettingsRecord::NexradAcquisition),
        _ => None,
    }
}

fn settings_action_is_cloud_synced(action_id: &str) -> bool {
    cloud_synced_settings_record(action_id).is_some()
        || action_id
            .strip_prefix(DEBUG_FLAG_ACTION_PREFIX)
            .and_then(debug_flag_from_id)
            .is_some()
}

pub(crate) fn all_debug_flags() -> [DebugFlagId; 10] {
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

pub(crate) fn debug_flag_id(flag_id: DebugFlagId) -> &'static str {
    debug_flag_spec(flag_id).0
}

pub(crate) fn debug_flag_from_id(id: &str) -> Option<DebugFlagId> {
    all_debug_flags()
        .into_iter()
        .find(|flag_id| debug_flag_id(*flag_id) == id)
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
    external_power_connected: bool,
) -> Option<UiDisplayPolicy> {
    display_policy_available.then(|| UiDisplayPolicy {
        keep_screen_on: true,
        dim_after_ms: if preferences.display_dim_on_battery_only && external_power_connected {
            None
        } else {
            preferences.display_dim_timeout.dim_after_ms()
        },
        dim_brightness: DISPLAY_DIM_BRIGHTNESS,
        allow_screen_off_after_ms: preferences.inactivity_sleep_timeout.sleep_after_ms(),
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

    impl SettingsController {
        fn test_project(
            &mut self,
            display_policy_available: bool,
            linked: bool,
            bytes: Option<&BTreeMap<String, u64>>,
            banner: &FlightDataBannerModel,
            debug: &UiDebugState,
        ) -> SettingsProjectionResult {
            self.project(
                display_policy_available,
                linked.then(|| UiSettingsSyncIndicator {
                    symbol: "cloud".to_string(),
                    help_text: "Synced".to_string(),
                    tone: app_ui_contracts::session::UiStatusSeverity::Ok,
                }),
                bytes,
                banner,
                debug,
                None,
            )
        }
    }

    fn banner() -> FlightDataBannerModel {
        FlightDataBannerModel {
            editor: None,
            cells: vec![
                FlightDataCell {
                    id: "altitude".to_string(),
                    label: "ALT".to_string(),
                    value: Some("12000".to_string()),
                    action: None,
                    tone: Default::default(),
                    estimate_kind: Default::default(),
                    attention: None,
                },
                FlightDataCell {
                    id: "nexrad_age".to_string(),
                    label: "NEXRAD".to_string(),
                    value: Some("2m".to_string()),
                    action: None,
                    tone: Default::default(),
                    estimate_kind: Default::default(),
                    attention: None,
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
    fn battery_only_dimming_combines_preference_and_observed_power_in_core() {
        for timeout in DisplayDimTimeout::all_stops() {
            for only_on_battery in [false, true] {
                let mut controller = SettingsController::default();
                controller.restore_preferences(SettingsPreferences {
                    display_dim_timeout: timeout,
                    display_dim_on_battery_only: only_on_battery,
                    ..Default::default()
                });
                // Includes initial unknown power (conservatively battery), plug/unplug,
                // and repeated observations with a warm projection cache.
                for connected in [false, true, true, false, true, false] {
                    controller.set_external_power_connected(connected);
                    let projection =
                        controller.test_project(true, false, None, &banner(), &debug_state());
                    let policy = projection.projection.display_policy.unwrap();
                    let expected_timeout = if only_on_battery && connected {
                        None
                    } else {
                        timeout.dim_after_ms()
                    };
                    assert_eq!(policy.dim_after_ms, expected_timeout);
                    assert!(policy.keep_screen_on);
                    assert_eq!(policy.allow_screen_off_after_ms, Some(3_600_000));
                    assert!(
                        !controller
                            .test_project(true, false, None, &banner(), &debug_state())
                            .rebuilt
                    );
                }
            }
        }
    }

    #[test]
    fn battery_only_checkbox_is_capability_gated_and_rejects_invalid_values() {
        let mut controller = SettingsController::default();
        let action = UiSettingsAction {
            action_id: DISPLAY_DIM_ON_BATTERY_ONLY_ID.to_string(),
            value_id: "on".to_string(),
        };
        assert!(controller.perform_action(&action, false, false).is_err());
        assert!(!controller
            .test_project(false, false, None, &banner(), &debug_state())
            .projection
            .settings_page_state
            .controls()
            .iter()
            .any(|row| row.id == DISPLAY_DIM_ON_BATTERY_ONLY_ID));
        assert!(controller
            .perform_action(
                &UiSettingsAction {
                    value_id: "surprise".to_string(),
                    ..action.clone()
                },
                true,
                false
            )
            .is_err());
        assert!(controller.perform_action(&action, true, false).unwrap());
        assert!(!controller.perform_action(&action, true, false).unwrap());
        let projection = controller
            .test_project(true, false, None, &banner(), &debug_state())
            .projection;
        let row = &projection.settings_page_state.controls()[2];
        assert_eq!(row.id, DISPLAY_DIM_ON_BATTERY_ONLY_ID);
        assert_eq!(row.title, "Only dim when on battery");
        assert_eq!(row.kind, UiSettingsRowKind::Toggle);
        assert_eq!(row.value_id, "on");
        assert_eq!(row.action_id, DISPLAY_DIM_ON_BATTERY_ONLY_ID);
    }

    #[test]
    fn old_preferences_preserve_dimming_on_external_power() {
        let preferences: SettingsPreferences =
            serde_json::from_str(r#"{"display_dim_timeout":"30s"}"#).unwrap();
        assert!(!preferences.display_dim_on_battery_only);
        let mut controller = SettingsController::default();
        controller.restore_preferences(preferences);
        controller.set_external_power_connected(true);
        assert_eq!(
            controller
                .test_project(true, false, None, &banner(), &debug_state())
                .projection
                .display_policy
                .unwrap()
                .dim_after_ms,
            Some(30_000)
        );
    }

    #[test]
    fn projection_cache_tracks_owned_and_typed_external_inputs() {
        let mut controller = SettingsController::default();
        let input = banner();
        let debug = debug_state();

        let first = controller.test_project(false, false, None, &input, &debug);
        assert!(first.rebuilt);
        assert_eq!(first.projection.settings_page_state.controls().len(), 1);
        assert!(first.projection.display_policy.is_none());

        let cached = controller.test_project(false, false, None, &input, &debug);
        assert!(!cached.rebuilt);
        assert_eq!(cached.projection, first.projection);

        let capability_changed = controller.test_project(true, false, None, &input, &debug);
        assert!(capability_changed.rebuilt);
        assert_eq!(
            capability_changed
                .projection
                .settings_page_state
                .controls()
                .len(),
            4
        );

        let mut changed_banner = input;
        changed_banner.cells[0].value = Some("13000".to_string());
        assert!(
            controller
                .test_project(true, false, None, &changed_banner, &debug)
                .rebuilt
        );
    }

    #[test]
    fn cloud_indicators_follow_the_core_owned_record_classifier() {
        let mut controller = SettingsController::default();
        let bytes = BTreeMap::from([
            ("offline_low1".to_string(), 1024),
            ("offline_0".to_string(), 2048),
        ]);
        let unlinked = controller
            .test_project(true, false, Some(&bytes), &banner(), &debug_state())
            .projection
            .settings_page_state;
        assert!(unlinked
            .controls()
            .iter()
            .chain(
                unlinked
                    .sections()
                    .iter()
                    .flat_map(|section| section.rows.iter())
            )
            .all(|row| row.sync_indicator.is_none()));

        let linked = controller
            .test_project(true, true, Some(&bytes), &banner(), &debug_state())
            .projection
            .settings_page_state;
        for local_id in [
            FLIGHT_DATA_VISIBILITY_ROW_ID,
            DISPLAY_DIM_TIMEOUT_ROW_ID,
            DISPLAY_DIM_ON_BATTERY_ONLY_ID,
        ] {
            assert!(linked
                .controls()
                .iter()
                .find(|row| row.id == local_id)
                .is_some_and(|row| row.sync_indicator.is_none()));
        }
        for synced_id in [INACTIVITY_SLEEP_TIMEOUT_ROW_ID, "nexrad_coverage"] {
            assert!(linked
                .controls()
                .iter()
                .find(|row| row.id == synced_id)
                .is_some_and(|row| row.sync_indicator.is_some()));
        }
        assert!(linked
            .sections()
            .iter()
            .flat_map(|section| section.rows.iter())
            .all(|row| row.sync_indicator.is_some()));
    }

    #[test]
    fn settings_mutations_own_revision_and_projection_invalidation() {
        let mut controller = SettingsController::default();
        let input = banner();
        let debug = debug_state();
        controller.test_project(true, false, None, &input, &debug);

        assert!(controller
            .perform_action(
                &UiSettingsAction {
                    action_id: DISPLAY_DIM_TIMEOUT_ACTION_ID.to_string(),
                    value_id: "30s".to_string(),
                },
                true,
                false,
            )
            .expect("display action"));
        assert_eq!(controller.revision(), 1);
        assert_eq!(controller.static_revision(), 1);
        assert!(
            controller
                .test_project(true, false, None, &input, &debug)
                .rebuilt
        );

        assert!(!controller
            .perform_action(
                &UiSettingsAction {
                    action_id: DISPLAY_DIM_TIMEOUT_ACTION_ID.to_string(),
                    value_id: "30s".to_string(),
                },
                true,
                false,
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
                false,
            )
            .expect("visibility action"));
        assert_eq!(controller.revision(), 2);
        assert_eq!(controller.static_revision(), 1);
        let projection = controller
            .test_project(true, false, None, &input, &debug)
            .projection;
        assert!(!projection
            .flight_data_banner
            .cells
            .iter()
            .any(|cell| cell.id == "nexrad_age"));
        assert!(
            !projection.settings_page_state.controls()[0]
                .items
                .iter()
                .find(|item| item.cell.id == "nexrad_age")
                .expect("settings item")
                .enabled
        );
    }

    #[test]
    fn android_nexrad_rows_include_core_estimates_and_enforce_cadence_order() {
        let mut controller = SettingsController::default();
        let bytes = BTreeMap::from([
            ("offline_0".to_string(), 1024 * 1024),
            ("offline_low1".to_string(), 512 * 1024),
        ]);
        let projection = controller
            .test_project(true, false, Some(&bytes), &banner(), &debug_state())
            .projection;
        let coverage = projection
            .settings_page_state
            .controls()
            .iter()
            .find(|row| row.id == "nexrad_coverage")
            .unwrap();
        assert_eq!(coverage.indent_level, 0);
        assert_eq!(
            coverage
                .stops
                .iter()
                .map(|stop| stop.id.as_str())
                .collect::<Vec<_>>(),
            ["viewport_only", "full_offline"]
        );
        assert_eq!(coverage.value_id, "viewport_only");
        assert!(!projection
            .settings_page_state
            .controls()
            .iter()
            .any(|row| row.id == "nexrad_offline_profile"));

        controller
            .perform_action(
                &UiSettingsAction {
                    action_id: NEXRAD_COVERAGE_ACTION_ID.to_string(),
                    value_id: "full_offline".to_string(),
                },
                true,
                true,
            )
            .unwrap();
        let projection = controller
            .test_project(true, false, Some(&bytes), &banner(), &debug_state())
            .projection;
        let offline_detail = projection
            .settings_page_state
            .controls()
            .iter()
            .find(|row| row.id == "nexrad_offline_profile")
            .unwrap();
        assert_eq!(
            offline_detail
                .stops
                .iter()
                .map(|stop| stop.id.as_str())
                .collect::<Vec<_>>(),
            ["offline_low1", "offline_0"]
        );
        assert!(projection
            .settings_page_state
            .controls()
            .iter()
            .filter(|row| row.id.starts_with("nexrad_") && row.id != "nexrad_coverage")
            .all(|row| row.indent_level == 1));
        let shown = projection
            .settings_page_state
            .controls()
            .iter()
            .find(|row| row.id == "nexrad_shown_cadence")
            .unwrap();
        assert_eq!(shown.stops.last().unwrap().label, "Every");
        assert_eq!(shown.value_id, "10m");
        assert!(shown.title.ends_with("3.0 MiB/h"));

        controller
            .perform_action(
                &UiSettingsAction {
                    action_id: NEXRAD_HIDDEN_CADENCE_ACTION_ID.to_string(),
                    value_id: "every_update".to_string(),
                },
                true,
                true,
            )
            .unwrap();
        assert_eq!(
            controller.nexrad_acquisition_preferences().shown_cadence,
            NexradUpdateCadence::EveryUpdate
        );
        controller
            .perform_action(
                &UiSettingsAction {
                    action_id: NEXRAD_SHOWN_CADENCE_ACTION_ID.to_string(),
                    value_id: "30m".to_string(),
                },
                true,
                true,
            )
            .unwrap();
        assert_eq!(
            controller.nexrad_acquisition_preferences().hidden_cadence,
            NexradUpdateCadence::ThirtyMinutes
        );

        controller
            .perform_action(
                &UiSettingsAction {
                    action_id: NEXRAD_COVERAGE_ACTION_ID.to_string(),
                    value_id: "viewport_only".to_string(),
                },
                true,
                true,
            )
            .unwrap();
        let projection = controller
            .test_project(true, false, Some(&bytes), &banner(), &debug_state())
            .projection;
        assert!(projection
            .settings_page_state
            .controls()
            .iter()
            .any(|row| row.id == "nexrad_coverage"));
        assert!(!projection
            .settings_page_state
            .controls()
            .iter()
            .any(|row| row.id == "nexrad_offline_profile"));
    }

    #[test]
    fn settings_help_text_is_projected_by_core() {
        let mut controller = SettingsController::default();
        let bytes = BTreeMap::from([
            ("offline_0".to_string(), 1024 * 1024),
            ("offline_low1".to_string(), 512 * 1024),
        ]);
        controller
            .perform_action(
                &UiSettingsAction {
                    action_id: NEXRAD_COVERAGE_ACTION_ID.to_string(),
                    value_id: "full_offline".to_string(),
                },
                true,
                true,
            )
            .unwrap();
        let projection = controller
            .test_project(true, false, Some(&bytes), &banner(), &debug_state())
            .projection;
        let rows = &projection.settings_page_state.controls();
        let help = |id: &str| {
            rows.iter()
                .find(|row| row.id == id)
                .unwrap_or_else(|| panic!("missing settings row {id}"))
                .help_text
                .as_deref()
        };

        assert_eq!(
            help(FLIGHT_DATA_VISIBILITY_ROW_ID),
            Some(FLIGHT_DATA_VISIBILITY_HELP)
        );
        assert_eq!(
            help(DISPLAY_DIM_TIMEOUT_ROW_ID),
            Some(DISPLAY_DIM_TIMEOUT_HELP)
        );
        assert_eq!(
            help(INACTIVITY_SLEEP_TIMEOUT_ROW_ID),
            Some(INACTIVITY_SLEEP_TIMEOUT_HELP)
        );
        assert_eq!(help("nexrad_coverage"), Some(NEXRAD_COVERAGE_HELP));
        assert_eq!(
            help("nexrad_offline_profile"),
            Some(NEXRAD_OFFLINE_DETAIL_HELP)
        );
        assert_eq!(
            help("nexrad_shown_cadence"),
            Some(NEXRAD_SHOWN_CADENCE_HELP)
        );
        assert_eq!(
            help("nexrad_hidden_cadence"),
            Some(NEXRAD_HIDDEN_CADENCE_HELP)
        );
        assert_eq!(
            help("nexrad_asleep_cadence"),
            Some(NEXRAD_ASLEEP_CADENCE_HELP)
        );
    }

    #[test]
    fn fresh_nexrad_preferences_minimize_data_demand() {
        let expected = NexradAcquisitionPreferences {
            coverage: NexradCoverageMode::ViewportOnly,
            offline_profile: NexradOfflineProfile::OfflineLow1,
            shown_cadence: NexradUpdateCadence::TenMinutes,
            hidden_cadence: NexradUpdateCadence::Never,
            asleep_cadence: NexradUpdateCadence::Never,
        };

        assert_eq!(NexradAcquisitionPreferences::default(), expected);
        assert_eq!(
            serde_json::from_str::<NexradAcquisitionPreferences>("{}").unwrap(),
            expected,
            "missing persisted fields must use the same low-data defaults",
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
