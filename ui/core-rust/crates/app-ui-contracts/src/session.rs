// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiNavigationPageId {
    Map,
    Charts,
    FlightPlan,
    AltitudePlanner,
    DataStatus,
    Settings,
    Home,
    OfflinePackages,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiNavigationPageOption {
    pub id: UiNavigationPageId,
    pub label: String,
    pub launcher_label: String,
    pub chart_or_plate_return_target: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiNavigationPageState {
    pub options: Vec<UiNavigationPageOption>,
    pub max_history_depth: usize,
    pub default_chart_or_plate_return_target: UiNavigationPageId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum FlightDataCellTone {
    #[default]
    Planned,
    Passed,
    Active,
}

impl FlightDataCellTone {
    pub fn is_planned(&self) -> bool {
        matches!(self, Self::Planned)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum FlightEstimateKind {
    #[default]
    Basic,
    Modeled,
}

impl FlightEstimateKind {
    pub fn is_basic(&self) -> bool {
        matches!(self, Self::Basic)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FlightDataCell {
    pub id: String,
    pub label: String,
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "FlightDataCellTone::is_planned")]
    pub tone: FlightDataCellTone,
    #[serde(default, skip_serializing_if = "FlightEstimateKind::is_basic")]
    pub estimate_kind: FlightEstimateKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FlightDataColumn {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct FlightDataBannerModel {
    pub cells: Vec<FlightDataCell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiStatusSeverity {
    Ok,
    Info,
    Caution,
    Warning,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiStatusActionStyle {
    Normal,
    Hush,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiStatusAction {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub style: UiStatusActionStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiDataStatusState {
    pub boxes: Vec<UiDataStatusBox>,
    pub launcher_count: Option<String>,
    pub launcher_severity: UiStatusSeverity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiDataStatusPageFact {
    pub label: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiDataStatusPageState {
    pub title: String,
    pub summary: String,
    pub rows: Vec<UiDataStatusPageRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiChartPageState {
    pub ordered_airport_ids: Vec<String>,
    pub recent_airport_ids: Vec<String>,
    #[serde(default)]
    pub plate_target_airport_id: Option<String>,
    pub selected_airport_id: String,
    #[serde(default)]
    pub selected_reference_family_id: Option<String>,
    pub selected_chart_id: String,
    #[serde(default)]
    pub suggested_chart_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiMapLayerToggleState {
    pub visible: bool,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiMapLayerOption {
    pub layer_id: MapLayerId,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiMapLayerState {
    pub options: Vec<UiMapLayerOption>,
    pub world_basemap: UiMapLayerToggleState,
    pub vectors: UiMapLayerToggleState,
    pub metars: UiMapLayerToggleState,
    pub nexrad: UiMapLayerToggleState,
    pub traffic: UiMapLayerToggleState,
    pub terrain_warning: UiMapLayerToggleState,
    pub offline_regions: UiMapLayerToggleState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum MapLayerId {
    WorldBasemap,
    Vectors,
    Metars,
    Nexrad,
    Traffic,
    TerrainWarning,
    OfflineRegions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum DebugFlagId {
    TileLabels,
    NexradTileLabels,
    FastTiles,
    OfflineSimulatedClockButtons,
    SequencingFinishLines,
    PlateFlightPlan,
    BadAutopilot,
    InternetAdsb,
    GpsCapture,
    DebugLogToDeveloperServer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiDebugState {
    pub tile_labels: bool,
    #[serde(default)]
    pub nexrad_tile_labels: bool,
    pub fast_tiles: bool,
    pub offline_simulated_clock_buttons: bool,
    #[serde(default)]
    pub sequencing_finish_lines: bool,
    #[serde(default)]
    pub plate_flight_plan: bool,
    #[serde(default)]
    pub bad_autopilot: bool,
    #[serde(default)]
    pub internet_adsb: bool,
    #[serde(default)]
    pub gps_capture: bool,
    #[serde(default)]
    pub debug_log_to_developer_server: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSettingsSliderStop {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSettingsGridItem {
    pub cell: FlightDataCell,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSettingsPageRow {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub value_id: String,
    pub stops: Vec<UiSettingsSliderStop>,
    #[serde(default)]
    pub items: Vec<UiSettingsGridItem>,
    pub action_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSettingsPageSection {
    pub id: String,
    pub title: String,
    pub collapsed_by_default: bool,
    pub rows: Vec<UiSettingsPageRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSettingsPageState {
    pub title: String,
    pub summary: String,
    pub rows: Vec<UiSettingsPageRow>,
    #[serde(default)]
    pub sections: Vec<UiSettingsPageSection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiDisplayPolicy {
    pub keep_screen_on: bool,
    pub dim_after_ms: Option<u64>,
    pub dim_brightness: f32,
    pub allow_screen_off_after_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiDisclaimerState {
    pub agreement_id: String,
    pub required: bool,
    pub html: String,
    pub text: String,
    pub accept_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSettingsAction {
    pub action_id: String,
    pub value_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiPlaybackPanelState {
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiNavDbIdentity {
    pub package_id: String,
    pub filename: String,
    pub contract_id: Option<String>,
    pub cycle: Option<String>,
    pub cycle_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformDisplayPolicyCapability {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformOfflinePackagesCapability {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformCloudCapability {
    #[serde(default)]
    pub qr_scan: bool,
    #[serde(default)]
    pub aerobag_cloud_base_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LiveFeedAcquisitionPolicy {
    JitPublicResources,
    DurableCompleteStates,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformLiveFeedsCapability {
    pub acquisition_policy: LiveFeedAcquisitionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ClientBuildInfo {
    pub platform: String,
    pub version: String,
    #[serde(default)]
    pub built_at_utc: Option<String>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PlatformCapabilities {
    #[serde(default)]
    pub display_policy: Option<PlatformDisplayPolicyCapability>,
    #[serde(default)]
    pub offline_packages: Option<PlatformOfflinePackagesCapability>,
    #[serde(default)]
    pub cloud: Option<PlatformCloudCapability>,
    #[serde(default)]
    pub live_feeds: Option<PlatformLiveFeedsCapability>,
    #[serde(default)]
    pub client_build: Option<ClientBuildInfo>,
    #[serde(default)]
    pub local_time_zone: Option<String>,
}

// Schema root used only to collect all session-page and platform-command DTOs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSessionPageContracts {
    pub navigation: UiNavigationPageState,
    pub chart: UiChartPageState,
    pub map_layers: UiMapLayerState,
    pub status: UiDataStatusState,
    pub status_page: UiDataStatusPageState,
    pub settings: UiSettingsPageState,
    pub display_policy: UiDisplayPolicy,
    pub disclaimer: UiDisclaimerState,
    pub debug: UiDebugState,
    pub playback_panel: UiPlaybackPanelState,
    pub nav_db: UiNavDbIdentity,
    pub capabilities: PlatformCapabilities,
    pub settings_action: UiSettingsAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSessionProjectionPatch {
    pub version: u64,
    pub assignments: Vec<UiSessionProjectionAssignment>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSessionProjectionAssignment {
    pub path: Vec<String>,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSessionUpdate {
    pub ui_contract_version: u32,
    pub session_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nav_data: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub application_shell: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight_plan: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownship: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flight_data: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub situation: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charts: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packages: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<UiSessionProjectionPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug: Option<UiSessionProjectionPatch>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum UiSessionUpdateGroup {
    NavData,
    ApplicationShell,
    FlightPlan,
    Ownship,
    FlightData,
    Situation,
    Charts,
    Map,
    Status,
    Settings,
    Cloud,
    Packages,
    Home,
    Debug,
}

impl UiSessionUpdateGroup {
    pub const COUNT: usize = 14;
}

impl UiSessionUpdate {
    pub fn projection_patches(
        &self,
    ) -> [(UiSessionUpdateGroup, Option<&UiSessionProjectionPatch>); UiSessionUpdateGroup::COUNT]
    {
        [
            (UiSessionUpdateGroup::NavData, self.nav_data.as_ref()),
            (
                UiSessionUpdateGroup::ApplicationShell,
                self.application_shell.as_ref(),
            ),
            (UiSessionUpdateGroup::FlightPlan, self.flight_plan.as_ref()),
            (UiSessionUpdateGroup::Ownship, self.ownship.as_ref()),
            (UiSessionUpdateGroup::FlightData, self.flight_data.as_ref()),
            (UiSessionUpdateGroup::Situation, self.situation.as_ref()),
            (UiSessionUpdateGroup::Charts, self.charts.as_ref()),
            (UiSessionUpdateGroup::Map, self.map.as_ref()),
            (UiSessionUpdateGroup::Status, self.status.as_ref()),
            (UiSessionUpdateGroup::Settings, self.settings.as_ref()),
            (UiSessionUpdateGroup::Cloud, self.cloud.as_ref()),
            (UiSessionUpdateGroup::Packages, self.packages.as_ref()),
            (UiSessionUpdateGroup::Home, self.home.as_ref()),
            (UiSessionUpdateGroup::Debug, self.debug.as_ref()),
        ]
    }
}
