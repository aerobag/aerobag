// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiTourPage {
    Map,
    FlightPlan,
    Charts,
    Home,
    AltitudePlanner,
    OfflinePackages,
    Cloud,
}

/// Transient presentation owned by the tour. These open the normal application
/// surfaces; the platform never decides which demonstration comes next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiTourSurface {
    None,
    BaseMap,
    Layers,
    FlightPlanRow,
    RouteEntry,
    PlateAirports,
    PlateFolder,
    AircraftModels,
    Inspector,
    Weather,
    Notams,
    AirportInfo,
    Ownship,
    OfflineRegions,
    OfflineProducts,
    OfflineHelp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiTourPlacement {
    Auto,
    TopRight,
    BottomRight,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiTourPresentation {
    Callout,
    TitleCard,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiTourViewport {
    pub lat: f64,
    pub lon: f64,
    pub zoom: f64,
    pub track_up: bool,
    pub centered: bool,
}

/// Geographic subject of an inspector scene, independent of camera framing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiTourMapPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiGuidedTour {
    pub generation: u64,
    pub step_id: String,
    pub chapter: String,
    pub title: String,
    pub body: String,
    pub placement: UiTourPlacement,
    pub presentation: UiTourPresentation,
    pub position: u32,
    pub total: u32,
    pub page: UiTourPage,
    pub surface: UiTourSurface,
    pub subject: String,
    pub row_uid: Option<String>,
    pub option_uid: Option<String>,
    pub targets: Vec<String>,
    pub viewport: UiTourViewport,
    pub map_point: Option<UiTourMapPoint>,
    pub back_enabled: bool,
    pub next_label: String,
    pub close_label: String,
    pub restart_label: Option<String>,
    pub shortcuts: Vec<UiTourShortcut>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiTourShortcut {
    /// Lowercase key name; platforms normalize physical keyboard events.
    pub key: String,
    pub action: UiTourAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiTourAction {
    Start,
    StartIntroduction,
    Restart,
    Next,
    Back,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiTourCommand {
    pub action: UiTourAction,
    pub expected_generation: Option<u64>,
}
