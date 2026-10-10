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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UiReceiverPanel {
    pub title: String,
    pub status: String,
    pub detail: String,
    pub actions: Vec<UiReceiverAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReceiverDevice {
    pub address: String,
    pub name: String,
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
    FileRead {
        request_id: String,
        succeeded: bool,
    },
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

// This private transport envelope can contain authentication bytes. Never log it.
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ReceiverHostEffect {
    ShareFile {
        path: String,
        mime_type: String,
        title: String,
    },
    ReadPrivateFile {
        request_id: String,
        max_bytes: u32,
    },
    RequestPermission,
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
