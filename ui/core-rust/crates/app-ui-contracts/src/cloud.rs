// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CloudProviderKind {
    #[default]
    GoogleDrive,
    AerobagCloud,
}

impl CloudProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::GoogleDrive => "My Google Drive",
            Self::AerobagCloud => "Aerobag Cloud",
        }
    }

    pub fn recovery_label(self) -> &'static str {
        match self {
            Self::GoogleDrive => "Google Drive",
            Self::AerobagCloud => "Aerobag Cloud",
        }
    }

    pub fn uses_platform_authorization(self) -> bool {
        matches!(self, Self::GoogleDrive)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CloudProviderPrincipal {
    pub stable_id: String,
    pub display_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CloudUiActionId {
    BeginSetup,
    BeginCreate,
    BackSetup,
    SelectProviderGoogleDrive,
    SelectProviderAerobagCloud,
    ScanSetupCode,
    AcceptSetupCode,
    AuthorizeProvider,
    CreateAccount,
    BackupSetupCode,
    AddDevice,
    CloseLinkedDetail,
    BeginUnlink,
    ConfirmUnlink,
    SyncNow,
    CopySetupCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CloudUiFieldId {
    DeviceSetupCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CloudUiFieldValue {
    pub id: CloudUiFieldId,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiQrCode {
    pub rows: Vec<String>,
    pub quiet_zone_modules: u32,
    pub accessibility_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CloudPlatformEffect {
    BeginAuthorization {
        provider: CloudProviderKind,
        scopes: Vec<String>,
    },
    ScanQrCode {
        completion_action: CloudUiActionId,
        field_id: CloudUiFieldId,
    },
    CopyText {
        text: String,
        completion_label: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CloudAuthorizationMode {
    Silent,
    Interactive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CloudAuthorizationRequest {
    pub request_id: u64,
    pub provider: CloudProviderKind,
    pub mode: CloudAuthorizationMode,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum CloudAuthorizationResponse {
    Authorized {
        expires_at_epoch_ms: Option<i64>,
        principal: CloudProviderPrincipal,
    },
    InteractionRequired {
        diagnostic: Option<String>,
    },
    Denied {
        diagnostic: Option<String>,
    },
    TransientFailure {
        diagnostic: Option<String>,
    },
    PermanentFailure {
        diagnostic: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CloudHttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CloudHttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CloudHttpRequest {
    pub request_id: u64,
    pub provider: CloudProviderKind,
    pub method: CloudHttpMethod,
    pub url: String,
    pub headers: Vec<CloudHttpHeader>,
    pub body_base64: Option<String>,
    pub max_response_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CloudEventStreamPlan {
    pub stream_id: u64,
    pub url: String,
    pub connect_timeout_ms: i64,
    pub idle_timeout_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CloudEventStreamEventKind {
    Connecting,
    Connected,
    Message,
    Error,
    Closed,
    IdleTimeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct CloudEventStreamEvent {
    pub stream_id: u64,
    pub kind: CloudEventStreamEventKind,
    pub data: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum CloudHttpResponse {
    Completed {
        status_code: u16,
        body_base64: String,
    },
    TransportError {
        detail: String,
    },
    ResponseTooLarge {
        limit_bytes: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiCloudAction {
    pub id: CloudUiActionId,
    pub label: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    pub required_fields: Vec<CloudUiFieldId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_effect: Option<CloudPlatformEffect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum UiCloudPanelState {
    Complete,
    Active,
    Working,
    Informational,
    Caution,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UiCloudPanelControl {
    DeviceSetupCodeInput {
        field_id: CloudUiFieldId,
        label: String,
        placeholder: String,
    },
    DeviceSetupCodeOutput {
        setup_code: String,
        qr_code: UiQrCode,
        copy_action: UiCloudAction,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiCloudTimeFact {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiCloudPanel {
    pub id: String,
    pub title: String,
    pub state: UiCloudPanelState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub time_facts: Vec<UiCloudTimeFact>,
    pub actions: Vec<UiCloudAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<UiCloudPanelControl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct UiCloudPageState {
    pub title: String,
    pub summary: String,
    pub sync_account_heading: String,
    pub provider_heading: String,
    pub overall_status_label: String,
    pub sync_account_panels: Vec<UiCloudPanel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_card: Option<UiCloudPanel>,
    pub overall_status: UiCloudPanel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_refresh_epoch_ms: Option<i64>,
}
