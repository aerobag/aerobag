// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct NexradOverlayQueryResult {
    pub status: NexradOverlayStatus,
    pub tiles: Vec<NexradOverlayTile>,
    pub stats: NexradOverlayStats,
    pub animation: NexradOverlayAnimation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_plan: Option<NexradOverlayCachePlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct NexradOverlayCachePlan {
    pub retained_frame_versions: Vec<String>,
    pub fetch_resources: Vec<NexradOverlayCacheResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct NexradOverlayCacheResource {
    pub frame_version: String,
    pub src: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct NexradOverlayAnimation {
    pub phase: NexradOverlayAnimationPhase,
    pub selected_frame_index: Option<usize>,
    pub frame_count: usize,
    pub age_labels: Vec<String>,
    pub age_summary: String,
    pub next_update_delay_ms: Option<u32>,
    pub next_update_epoch_ms: Option<i64>,
}

impl Default for NexradOverlayAnimation {
    fn default() -> Self {
        Self::idle()
    }
}

impl NexradOverlayAnimation {
    pub fn idle() -> Self {
        Self {
            phase: NexradOverlayAnimationPhase::Idle,
            selected_frame_index: None,
            frame_count: 0,
            age_labels: Vec::new(),
            age_summary: "---".to_string(),
            next_update_delay_ms: None,
            next_update_epoch_ms: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum NexradOverlayAnimationPhase {
    Idle,
    Frame,
    Blank,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct NexradOverlayStats {
    pub source_tile_count: usize,
    pub render_piece_count: usize,
    pub split_count: usize,
    pub max_affine_error_px: f64,
    pub level_pixel_span_px: f64,
    pub max_level_pixel_stretch_px: f64,
    pub max_stack_depth: usize,
    pub res: Option<u32>,
    pub observed_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum NexradOverlayStatus {
    Hidden,
    Loading,
    Unavailable { reason: String },
    Ready { count: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct NexradOverlayTile {
    pub key: String,
    pub src: String,
    pub res: u32,
    pub x: u32,
    pub y: u32,
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub corners: NexradOverlayTileCorners,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct NexradOverlayTileCorners {
    pub nw: NexradOverlayScreenPoint,
    pub ne: NexradOverlayScreenPoint,
    pub se: NexradOverlayScreenPoint,
    pub sw: NexradOverlayScreenPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct NexradOverlayScreenPoint {
    pub x: f64,
    pub y: f64,
}
