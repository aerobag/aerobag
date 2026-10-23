// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UiReceiverAction {
    pub action_id: String,
    pub label: String,
    pub enabled: bool,
    pub disabled_reason: Option<String>,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UiReceiverChooser {
    pub devices: Vec<UiReceiverAction>,
    pub empty_text: Option<String>,
    pub show_all: UiReceiverAction,
    pub show_all_checked: bool,
    pub refresh: UiReceiverAction,
    pub pair: UiReceiverAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UiReceiverDiagnostics {
    pub title: String,
    pub detail: String,
    pub actions: Vec<UiReceiverAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UiReceiverPanel {
    pub title: String,
    pub status: String,
    pub detail: String,
    pub actions: Vec<UiReceiverAction>,
    pub activity: Option<UiReceiverActivity>,
    pub chooser: Option<UiReceiverChooser>,
    pub diagnostics: Option<UiReceiverDiagnostics>,
}

/// Core lays out the graph in a 60000 x 1000 coordinate system. Hosts only paint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UiReceiverActivityRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub color_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UiReceiverActivityLegend {
    pub color_key: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UiReceiverActivityStrip {
    pub title: String,
    pub scale_label: String,
    pub rects: Vec<UiReceiverActivityRect>,
    pub legend: Vec<UiReceiverActivityLegend>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UiReceiverActivity {
    pub title: String,
    pub axis_start: String,
    pub axis_end: String,
    pub strips: Vec<UiReceiverActivityStrip>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReceiverDevice {
    pub address: String,
    pub name: String,
    /// Cached service identifiers reported by the OS; core interprets them.
    pub service_uuids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ReceiverInventoryResult {
    Ready { devices: Vec<ReceiverDevice> },
    PermissionRequired,
    BluetoothOff,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ReceiverIoFailure {
    Connection,
    Read,
    Write,
    Permission,
    BluetoothDisabled,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ReceiverHostEvent {
    HostFailed,
    HostUnavailable,
    RefreshInventory,
    Inventory {
        request_id: u64,
        result: ReceiverInventoryResult,
    },
    Action {
        action_id: String,
    },
    Connected {
        connection_id: u64,
    },
    Received {
        connection_id: u64,
    },
    Written {
        connection_id: u64,
        write_id: u64,
    },
    Failed {
        connection_id: u64,
        failure: ReceiverIoFailure,
    },
    Tick,
}

// Raw transport bytes belong in bounded captures, not ordinary application logs.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ReceiverHostEffect {
    ShareFile {
        path: String,
        mime_type: String,
        title: String,
    },
    RequestPermission,
    OpenBluetoothSettings,
    RefreshInventory {
        request_id: u64,
    },
    Connect {
        connection_id: u64,
        device: String,
        service_uuid: String,
    },
    Close {
        connection_id: u64,
    },
    Write {
        connection_id: u64,
        write_id: u64,
        bytes_base64: String,
    },
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReceiverHostOutput {
    pub delivery_ready: bool,
    pub effects: Vec<ReceiverHostEffect>,
    pub next_wake_monotonic_ms: Option<u64>,
    pub keep_alive: bool,
    pub panel: UiReceiverPanel,
}
