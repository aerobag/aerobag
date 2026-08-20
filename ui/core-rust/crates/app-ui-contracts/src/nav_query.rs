// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WaypointSuggestionPosition {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WaypointSuggestionNavRef {
    Airport {
        code: String,
    },
    Navaid {
        code: String,
    },
    ArincNavaid {
        identifier: String,
        icao_code: String,
        section_code: String,
        subsection_code: String,
    },
    TerminalNavaid {
        airport_id: String,
        identifier: String,
        icao_code: String,
        section_code: String,
        subsection_code: String,
    },
    Fix {
        code: String,
    },
    LatLon {
        position: WaypointSuggestionPosition,
    },
    Spot {
        position: WaypointSuggestionPosition,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct NavSymbolFeature {
    pub kind: String,
    pub label: String,
    pub symbol_kind: String,
    pub style_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obstacle_variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obstacle_tone: Option<String>,
    #[serde(default)]
    pub towered: bool,
    #[serde(default)]
    pub fuel_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_paved_runway: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heliport: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_water_runway: Option<bool>,
    #[serde(default)]
    pub runway_length_ratio: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub longest_runway_heading_true_deg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation_msl_ft: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WaypointIdentifierSuggestion {
    pub identifier: String,
    pub nav_ref: WaypointSuggestionNavRef,
    pub kind: String,
    pub display_name: String,
    pub distance_from_anchor_nm: f64,
    pub distance_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_feature: Option<NavSymbolFeature>,
}
