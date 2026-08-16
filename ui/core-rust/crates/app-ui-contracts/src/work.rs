// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiSessionWorkKind {
    ChartAsset,
    MapOverlay,
    MapSelection,
    MapSelectionForNavRef,
    NexradOverlay,
    NexradTile,
    TerrainOverlay,
    TerrainTile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSessionWorkRequest {
    pub id: u64,
    pub kind: UiSessionWorkKind,
    pub coalesce_key: Option<String>,
    pub requested_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiSessionWorkRequestDecision {
    Start { request: UiSessionWorkRequest },
    Queued { replaced_request_id: Option<u64> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiSessionWorkResultAction {
    Land,
    Drop { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiSessionWorkCompletionDecision {
    pub result_action: UiSessionWorkResultAction,
    pub next: Option<UiSessionWorkRequest>,
}
