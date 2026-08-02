// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, Payload},
    ChaCha20Poly1305, KeyInit, Nonce,
};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    data_status::{DataStatusRecord, UiStatusSeverity},
    AppError, AppErrorKind, AppResult, FlightPlan, OfflinePackagePreferences,
    OfflinePackageSelection,
};

const CLOUD_PERSISTENCE_VERSION: u32 = 4;
const CLOUD_ENVELOPE_VERSION: u32 = 1;
const CLOUD_PAGE_VERSION: u32 = 1;
const CLOUD_NODE_VERSION: u32 = 1;
const DEVICE_SETUP_CODE_VERSION: u32 = 2;
const FLIGHT_PLAN_RECORD_KEY: &str = "flight_plan/current";
const FLIGHT_PLAN_SCHEMA_VERSION: u32 = 2;
const OFFLINE_PACKAGE_REGION_RECORD_PREFIX: &str = "offline_packages/region/";
const OFFLINE_PACKAGE_PRODUCT_RECORD_PREFIX: &str = "offline_packages/product/";
const OFFLINE_PACKAGE_SELECTION_SCHEMA_VERSION: u32 = 1;
const LEGACY_UNKNOWN_MUTATION_EPOCH_MS: i64 = i64::MIN;
const CLOUD_POLL_INTERVAL_MS: i64 = 60_000;
const CLOUD_TRANSIENT_RETRY_MS: i64 = 5_000;
const GOOGLE_DRIVE_APPDATA_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";
pub const CLOUD_STATUS_ID: &str = "cloud:provider";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
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

    fn recovery_label(self) -> &'static str {
        match self {
            Self::GoogleDrive => "Google Drive",
            Self::AerobagCloud => "Aerobag Cloud",
        }
    }

    fn is_available(self) -> bool {
        matches!(self, Self::GoogleDrive)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProviderAuthorizationState {
    #[default]
    NotAuthorized,
    Authorizing,
    Authorized {
        #[serde(default)]
        expires_at_epoch_ms: Option<i64>,
        principal: CloudProviderPrincipal,
    },
    AuthorizationRequired {
        detail: String,
    },
    Denied {
        detail: String,
    },
    TransientFailure {
        detail: String,
        retry_at_epoch_ms: i64,
    },
    Failed {
        detail: String,
    },
}

impl ProviderAuthorizationState {
    pub fn is_ready(&self, now_epoch_ms: i64) -> bool {
        match self {
            Self::Authorized {
                expires_at_epoch_ms,
                ..
            } => expires_at_epoch_ms.is_none_or(|expires| expires > now_epoch_ms),
            _ => false,
        }
    }

    fn principal(&self) -> Option<&CloudProviderPrincipal> {
        match self {
            Self::Authorized { principal, .. } => Some(principal),
            _ => None,
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::AuthorizationRequired { detail }
            | Self::Denied { detail }
            | Self::TransientFailure { detail, .. }
            | Self::Failed { detail } => Some(detail),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudProviderPrincipal {
    pub stable_id: String,
    pub display_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ProviderAccountBinding {
    stable_principal_fingerprint: String,
    display_hint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CloudProviderErrorKind {
    Unauthorized,
    Transient,
    Permanent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloudProviderOperation {
    AllocateIds {
        count: usize,
    },
    Read {
        id: String,
    },
    CreateOnce {
        id: String,
        name: String,
        bytes_base64: String,
    },
    #[allow(dead_code)] // Required by the provider contract for future unreachable-object GC.
    Delete {
        id: String,
    },
    #[allow(dead_code)] // Required by the provider contract for future recovery and GC.
    List {
        page_token: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloudProviderObject {
    pub id: String,
    pub size_bytes: u64,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloudProviderRequest {
    pub request_id: u64,
    pub provider: CloudProviderKind,
    pub operation: CloudProviderOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloudProviderResponse {
    AllocatedIds {
        ids: Vec<String>,
    },
    Read {
        bytes_base64: Option<String>,
    },
    Created,
    AlreadyExists,
    Deleted {
        existed: bool,
    },
    Listed {
        objects: Vec<CloudProviderObject>,
        next_page_token: Option<String>,
    },
    Error {
        kind: CloudProviderErrorKind,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloudAction {
    BeginSetupFromDevice,
    BeginCreateAccount,
    BackSetup,
    SelectProvider { provider: CloudProviderKind },
    CreateAccount,
    AcceptDeviceSetupCode { setup_code: String },
    BackUpDeviceSetupCode,
    AddAnotherDevice,
    CloseLinkedAccountDetail,
    BeginUnlinkDevice,
    ConfirmUnlinkDevice,
    SyncNow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(rename_all = "snake_case")]
pub enum CloudUiFieldId {
    DeviceSetupCode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudUiFieldValue {
    pub id: CloudUiFieldId,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiQrCode {
    pub rows: Vec<String>,
    pub quiet_zone_modules: u32,
    pub accessibility_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
pub enum CloudAuthorizationMode {
    Silent,
    Interactive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudAuthorizationRequest {
    pub request_id: u64,
    pub provider: CloudProviderKind,
    pub mode: CloudAuthorizationMode,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
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
#[serde(rename_all = "snake_case")]
pub enum CloudHttpMethod {
    Get,
    Post,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudHttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(tag = "result", rename_all = "snake_case")]
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
pub struct UiCloudAction {
    pub id: CloudUiActionId,
    pub label: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    #[serde(default)]
    pub required_fields: Vec<CloudUiFieldId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_effect: Option<CloudPlatformEffect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(tag = "kind", rename_all = "snake_case")]
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
pub struct UiCloudTimeFact {
    pub label: String,
    pub epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiCloudPanel {
    pub id: String,
    pub title: String,
    pub state: UiCloudPanelState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub time_facts: Vec<UiCloudTimeFact>,
    pub actions: Vec<UiCloudAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<UiCloudPanelControl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudStatusFact {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudStatusSummary {
    pub label: String,
    pub severity: UiStatusSeverity,
    pub detail: String,
    pub facts: Vec<CloudStatusFact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CloudRecord {
    schema_version: u32,
    #[serde(default)]
    modified_at_epoch_ms: Option<i64>,
    value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
struct SynchronizedRecordStore {
    #[serde(default)]
    cached: BTreeMap<String, CloudRecord>,
    #[serde(default)]
    pending_keys: BTreeSet<String>,
    #[serde(default)]
    deferred_adoption: BTreeMap<String, CloudRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct StampedFlightPlan {
    plan: FlightPlan,
    modified_at_epoch_ms: i64,
}

impl<'de> Deserialize<'de> for StampedFlightPlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Current {
                plan: FlightPlan,
                modified_at_epoch_ms: i64,
            },
            Legacy(FlightPlan),
        }

        Ok(match Wire::deserialize(deserializer)? {
            Wire::Current {
                plan,
                modified_at_epoch_ms,
            } => Self {
                plan,
                modified_at_epoch_ms,
            },
            Wire::Legacy(plan) => Self {
                plan,
                modified_at_epoch_ms: LEGACY_UNKNOWN_MUTATION_EPOCH_MS,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CloudPage {
    version: u32,
    records: BTreeMap<String, CloudRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CloudNode {
    version: u32,
    generation: u64,
    #[serde(default)]
    published_at_epoch_ms: Option<i64>,
    parent_node_id: Option<String>,
    parent_node_hash: Option<String>,
    merkle_root_id: String,
    merkle_root_hash: String,
    next_slot_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CloudEnvelope {
    version: u32,
    account_tag: String,
    role: String,
    nonce_base64: String,
    ciphertext_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DeviceSetupCodePayload {
    version: u32,
    provider: CloudProviderKind,
    provider_account_binding: ProviderAccountBinding,
    genesis_id: String,
    root_secret_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct VerifiedTip {
    node_id: String,
    node_hash: String,
    generation: u64,
    #[serde(default)]
    published_at_epoch_ms: Option<i64>,
    merkle_root_id: String,
    merkle_root_hash: String,
    next_slot_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CloudAccount {
    provider: CloudProviderKind,
    #[serde(default)]
    provider_account_binding: Option<ProviderAccountBinding>,
    root_secret_base64: String,
    genesis_id: String,
    #[serde(default)]
    tip: Option<VerifiedTip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StagedPublication {
    purpose: PublicationPurpose,
    node_id: String,
    page_id: String,
    next_slot_id: String,
    page_bytes_base64: String,
    node_bytes_base64: String,
    node_hash: String,
    merkle_root_hash: String,
    generation: u64,
    published_at_epoch_ms: i64,
    local_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PublicationPurpose {
    CreateAccount,
    Publish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReadPurpose {
    Link,
    Poll,
    PublishRace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum CloudWorkflow {
    CreateAwaitIds,
    PublishAwaitIds,
    CreatePage {
        staged: StagedPublication,
    },
    VerifyPage {
        staged: StagedPublication,
    },
    CreateNode {
        staged: StagedPublication,
    },
    VerifyNode {
        staged: StagedPublication,
    },
    ReadNode {
        node_id: String,
        purpose: ReadPurpose,
    },
    ReadPage {
        node_id: String,
        node_hash: String,
        node: CloudNode,
        purpose: ReadPurpose,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CloudOnboardingIntent {
    SetupFromDevice,
    CreateAccount,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CloudProviderFailure {
    kind: CloudProviderErrorKind,
    detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPersistentState {
    version: u32,
    #[serde(default, alias = "provider")]
    selected_provider: Option<CloudProviderKind>,
    #[serde(default)]
    onboarding_intent: Option<CloudOnboardingIntent>,
    #[serde(default)]
    account: Option<CloudAccount>,
    #[serde(default)]
    workflow: Option<CloudWorkflow>,
    #[serde(default)]
    records: SynchronizedRecordStore,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cached_flight_plan: Option<StampedFlightPlan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_flight_plan: Option<StampedFlightPlan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_remote_flight_plan: Option<StampedFlightPlan>,
    #[serde(default)]
    local_revision: u64,
    #[serde(default)]
    next_request_id: u64,
    #[serde(default)]
    last_success_epoch_ms: Option<i64>,
    #[serde(default)]
    last_read_epoch_ms: Option<i64>,
    #[serde(default)]
    last_write_epoch_ms: Option<i64>,
    #[serde(default)]
    last_poll_epoch_ms: Option<i64>,
    #[serde(default)]
    next_retry_epoch_ms: Option<i64>,
    #[serde(default)]
    force_poll: bool,
    #[serde(default)]
    last_provider_failure: Option<CloudProviderFailure>,
}

impl Default for CloudPersistentState {
    fn default() -> Self {
        Self {
            version: CLOUD_PERSISTENCE_VERSION,
            selected_provider: None,
            onboarding_intent: None,
            account: None,
            workflow: None,
            records: SynchronizedRecordStore::default(),
            cached_flight_plan: None,
            pending_flight_plan: None,
            pending_remote_flight_plan: None,
            local_revision: 0,
            next_request_id: 1,
            last_success_epoch_ms: None,
            last_read_epoch_ms: None,
            last_write_epoch_ms: None,
            last_poll_epoch_ms: None,
            next_retry_epoch_ms: None,
            force_poll: false,
            last_provider_failure: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CloudEngine {
    persistent: CloudPersistentState,
    authorizations: BTreeMap<CloudProviderKind, ProviderAuthorizationState>,
    authorization_request_in_flight: Option<CloudAuthorizationRequest>,
    pending_authorization_mode: Option<CloudAuthorizationMode>,
    next_authorization_request_id: u64,
    provider_request_in_flight: Option<CloudProviderRequest>,
    linked_account_detail: Option<LinkedAccountDetail>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkedAccountDetail {
    BackupCode,
    AddDevice,
    ConfirmUnlink,
}

#[derive(Debug, Clone, Default)]
pub struct CloudCompletion {
    changed_records: BTreeMap<String, CloudRecord>,
}

impl CloudCompletion {
    pub fn remote_flight_plan(&self) -> AppResult<Option<FlightPlan>> {
        self.changed_records
            .get(FLIGHT_PLAN_RECORD_KEY)
            .map(flight_plan_from_record)
            .transpose()
            .map(|record| record.map(|record| record.plan))
    }
}

impl CloudEngine {
    pub fn new(mut persistent: CloudPersistentState) -> Self {
        let persisted_version = persistent.version;
        persistent.version = CLOUD_PERSISTENCE_VERSION;
        if persisted_version < 4 {
            if let Some(record) = persistent.cached_flight_plan.take() {
                if let Ok(record) = cloud_record_for_flight_plan(&record) {
                    persistent
                        .records
                        .cached
                        .insert(FLIGHT_PLAN_RECORD_KEY.to_string(), record);
                }
            }
            if let Some(record) = persistent.pending_flight_plan.take() {
                if let Ok(record) = cloud_record_for_flight_plan(&record) {
                    persistent
                        .records
                        .cached
                        .insert(FLIGHT_PLAN_RECORD_KEY.to_string(), record);
                    persistent
                        .records
                        .pending_keys
                        .insert(FLIGHT_PLAN_RECORD_KEY.to_string());
                }
            }
            if let Some(record) = persistent.pending_remote_flight_plan.take() {
                if let Ok(record) = cloud_record_for_flight_plan(&record) {
                    persistent
                        .records
                        .deferred_adoption
                        .insert(FLIGHT_PLAN_RECORD_KEY.to_string(), record);
                }
            }
        }
        if persisted_version < 3 {
            // Old outbox entries have no user-mutation time, so they must never
            // overwrite a value merely because this client reconnects later.
            persistent.records.pending_keys.clear();
            persistent.records.deferred_adoption.clear();
            persistent.force_poll = persistent.account.is_some();
        }
        if persistent.account.is_some() {
            persistent.selected_provider = None;
        } else if persisted_version < CLOUD_PERSISTENCE_VERSION {
            persistent.selected_provider = None;
        }
        Self {
            persistent,
            authorizations: BTreeMap::new(),
            authorization_request_in_flight: None,
            pending_authorization_mode: None,
            next_authorization_request_id: 1,
            provider_request_in_flight: None,
            linked_account_detail: None,
        }
    }

    pub fn persistent(&self) -> &CloudPersistentState {
        &self.persistent
    }

    pub fn cached_flight_plan(&self) -> Option<FlightPlan> {
        self.persistent
            .records
            .cached
            .get(FLIGHT_PLAN_RECORD_KEY)
            .and_then(|record| flight_plan_from_record(record).ok())
            .map(|record| record.plan)
    }

    pub fn pending_remote_flight_plan(&self) -> Option<FlightPlan> {
        self.persistent
            .records
            .deferred_adoption
            .get(FLIGHT_PLAN_RECORD_KEY)
            .and_then(|record| flight_plan_from_record(record).ok())
            .map(|record| record.plan)
    }

    pub fn clear_pending_remote_flight_plan(&mut self) {
        self.persistent
            .records
            .deferred_adoption
            .remove(FLIGHT_PLAN_RECORD_KEY);
    }

    pub fn take_pending_remote_flight_plan(&mut self) -> Option<FlightPlan> {
        self.persistent
            .records
            .deferred_adoption
            .remove(FLIGHT_PLAN_RECORD_KEY)
            .and_then(|record| flight_plan_from_record(&record).ok())
            .map(|record| record.plan)
    }

    pub fn set_authorization_state(
        &mut self,
        provider: CloudProviderKind,
        state: ProviderAuthorizationState,
    ) {
        let principal = state.principal().cloned();
        self.authorizations.insert(provider, state);
        if self.current_provider().ok() == Some(provider) {
            self.provider_request_in_flight = None;
        }
        if let Some(principal) = principal {
            if let Some(account) = self.persistent.account.as_mut() {
                if account.provider == provider
                    && account.tip.is_some()
                    && account.provider_account_binding.is_none()
                {
                    account.provider_account_binding = Some(provider_account_binding(&principal));
                }
            }
            if self.current_provider().ok() == Some(provider) {
                self.persistent.next_retry_epoch_ms = None;
                self.persistent.last_provider_failure = None;
                self.persistent.force_poll = true;
            }
        }
    }

    pub fn take_authorization_request(
        &mut self,
        now_epoch_ms: i64,
    ) -> AppResult<Option<CloudAuthorizationRequest>> {
        if self.authorization_request_in_flight.is_some() {
            return Ok(None);
        }
        let Ok(provider) = self.current_provider() else {
            return Ok(None);
        };
        if !provider.is_available() {
            return Ok(None);
        }
        let mode = if let Some(mode) = self.pending_authorization_mode.take() {
            mode
        } else {
            match self.authorization_for(provider) {
                ProviderAuthorizationState::NotAuthorized => CloudAuthorizationMode::Silent,
                ProviderAuthorizationState::Authorized {
                    expires_at_epoch_ms: Some(expires_at_epoch_ms),
                    ..
                } if now_epoch_ms > 0 && expires_at_epoch_ms <= now_epoch_ms => {
                    CloudAuthorizationMode::Silent
                }
                ProviderAuthorizationState::TransientFailure {
                    retry_at_epoch_ms, ..
                } if retry_at_epoch_ms <= now_epoch_ms => CloudAuthorizationMode::Silent,
                _ => return Ok(None),
            }
        };
        let request_id = self.next_authorization_request_id.max(1);
        self.next_authorization_request_id = request_id.saturating_add(1);
        let request = CloudAuthorizationRequest {
            request_id,
            provider,
            mode,
            scopes: provider_authorization_scopes(provider),
        };
        self.authorizations
            .insert(provider, ProviderAuthorizationState::Authorizing);
        self.authorization_request_in_flight = Some(request.clone());
        Ok(Some(request))
    }

    pub fn complete_authorization(
        &mut self,
        request_id: u64,
        response: CloudAuthorizationResponse,
        now_epoch_ms: i64,
    ) -> AppResult<()> {
        let request = self.authorization_request_in_flight.take().ok_or_else(|| {
            cloud_error(format!(
                "cloud authorization response {request_id} arrived with no request in flight"
            ))
        })?;
        if request.request_id != request_id {
            self.authorization_request_in_flight = Some(request.clone());
            return Err(cloud_error(format!(
                "cloud authorization response {request_id} does not match in-flight request {}",
                request.request_id
            )));
        }
        let provider = request.provider;
        let state = match response {
            CloudAuthorizationResponse::Authorized {
                expires_at_epoch_ms,
                principal,
            } => ProviderAuthorizationState::Authorized {
                expires_at_epoch_ms,
                principal,
            },
            CloudAuthorizationResponse::InteractionRequired { diagnostic } => {
                ProviderAuthorizationState::AuthorizationRequired {
                    detail: provider_authorization_detail(
                        "Provider authorization was not completed.",
                        diagnostic.as_deref(),
                    ),
                }
            }
            CloudAuthorizationResponse::Denied { diagnostic } => {
                ProviderAuthorizationState::Denied {
                    detail: provider_authorization_detail(
                        "Provider authorization was denied.",
                        diagnostic.as_deref(),
                    ),
                }
            }
            CloudAuthorizationResponse::TransientFailure { diagnostic } => {
                ProviderAuthorizationState::TransientFailure {
                    detail: provider_authorization_detail(
                        "Provider authorization is temporarily unavailable.",
                        diagnostic.as_deref(),
                    ),
                    retry_at_epoch_ms: now_epoch_ms.saturating_add(CLOUD_TRANSIENT_RETRY_MS),
                }
            }
            CloudAuthorizationResponse::PermanentFailure { diagnostic } => {
                ProviderAuthorizationState::Failed {
                    detail: provider_authorization_detail(
                        "Provider authorization failed.",
                        diagnostic.as_deref(),
                    ),
                }
            }
        };
        self.set_authorization_state(provider, state);
        Ok(())
    }

    fn authorization_for(&self, provider: CloudProviderKind) -> ProviderAuthorizationState {
        self.authorizations
            .get(&provider)
            .cloned()
            .unwrap_or_default()
    }

    fn authorization_for_at(
        &self,
        provider: CloudProviderKind,
        now_epoch_ms: i64,
    ) -> ProviderAuthorizationState {
        let authorization = self.authorization_for(provider);
        match authorization {
            ProviderAuthorizationState::Authorized {
                expires_at_epoch_ms: Some(expires_at_epoch_ms),
                ..
            } if now_epoch_ms > 0 && expires_at_epoch_ms <= now_epoch_ms => {
                ProviderAuthorizationState::AuthorizationRequired {
                    detail: format!(
                        "{} authorization expired. Authorize it again to resume cloud synchronization.",
                        provider.label()
                    ),
                }
            }
            authorization => authorization,
        }
    }

    fn next_authorization_refresh_epoch_ms(&self, now_epoch_ms: i64) -> Option<i64> {
        let provider = self.current_provider().ok()?;
        match self.authorization_for(provider) {
            ProviderAuthorizationState::Authorized {
                expires_at_epoch_ms: Some(expires_at_epoch_ms),
                ..
            } if expires_at_epoch_ms > now_epoch_ms => Some(expires_at_epoch_ms),
            ProviderAuthorizationState::TransientFailure {
                retry_at_epoch_ms, ..
            } if retry_at_epoch_ms > now_epoch_ms => Some(retry_at_epoch_ms),
            _ => None,
        }
    }

    fn current_provider(&self) -> AppResult<CloudProviderKind> {
        self.persistent
            .account
            .as_ref()
            .map(|account| account.provider)
            .or(self.persistent.selected_provider)
            .ok_or_else(|| cloud_error("no cloud provider is selected"))
    }

    fn require_unlinked_and_idle(&self) -> AppResult<()> {
        if self.persistent.account.is_some() || self.persistent.workflow.is_some() {
            return Err(cloud_error(
                "unlink this device before changing Sync Account setup",
            ));
        }
        Ok(())
    }

    fn require_linked_account(&self) -> AppResult<()> {
        if self.has_linked_account() {
            Ok(())
        } else {
            Err(cloud_error("no verified Sync Account is linked"))
        }
    }

    fn has_linked_account(&self) -> bool {
        self.persistent.account.as_ref().is_some_and(|account| {
            !account.genesis_id.is_empty()
                && account.tip.is_some()
                && account.provider_account_binding.is_some()
        })
    }

    fn provider_principal_mismatch(&self) -> Option<String> {
        let account = self.persistent.account.as_ref()?;
        let expected = account.provider_account_binding.as_ref()?;
        let actual = self.authorization_for(account.provider);
        let actual = actual.principal()?;
        (expected.stable_principal_fingerprint != principal_fingerprint(&actual.stable_id)).then(
            || {
                format!(
                    "This Sync Account uses {}, but {} was authorized.",
                    expected.display_hint, actual.display_label
                )
            },
        )
    }

    pub fn record_local_flight_plan_mutation(
        &mut self,
        before: &FlightPlan,
        after: &FlightPlan,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let before = cloud_flight_plan_definition(before);
        let after = cloud_flight_plan_definition(after);
        if before == after {
            return Ok(false);
        }
        let record = StampedFlightPlan {
            plan: after,
            modified_at_epoch_ms: self
                .next_record_mutation_epoch_ms(FLIGHT_PLAN_RECORD_KEY, now_epoch_ms),
        };
        self.record_local_cloud_record(
            FLIGHT_PLAN_RECORD_KEY,
            cloud_record_for_flight_plan(&record)?,
        );
        Ok(true)
    }

    pub fn record_local_offline_package_preferences(
        &mut self,
        preferences: &OfflinePackagePreferences,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let mut changed = false;
        for (prefix, selections) in [
            (OFFLINE_PACKAGE_REGION_RECORD_PREFIX, &preferences.regions),
            (OFFLINE_PACKAGE_PRODUCT_RECORD_PREFIX, &preferences.products),
        ] {
            for (id, selection) in selections {
                let key = format!("{prefix}{id}");
                let existing = self
                    .persistent
                    .records
                    .cached
                    .get(&key)
                    .map(offline_package_selection_from_record)
                    .transpose()?;
                if existing == Some(*selection) {
                    continue;
                }
                let record = CloudRecord {
                    schema_version: OFFLINE_PACKAGE_SELECTION_SCHEMA_VERSION,
                    modified_at_epoch_ms: Some(
                        self.next_record_mutation_epoch_ms(&key, now_epoch_ms),
                    ),
                    value: serde_json::to_value(selection).map_err(cloud_json_error)?,
                };
                self.record_local_cloud_record(&key, record);
                changed = true;
            }
        }
        Ok(changed)
    }

    pub fn offline_package_preferences(&self) -> AppResult<OfflinePackagePreferences> {
        let mut preferences = OfflinePackagePreferences::default();
        for (key, record) in &self.persistent.records.cached {
            let target = if let Some(id) = key.strip_prefix(OFFLINE_PACKAGE_REGION_RECORD_PREFIX) {
                Some((&mut preferences.regions, id))
            } else if let Some(id) = key.strip_prefix(OFFLINE_PACKAGE_PRODUCT_RECORD_PREFIX) {
                Some((&mut preferences.products, id))
            } else {
                None
            };
            if let Some((selections, id)) = target {
                selections.insert(
                    id.to_string(),
                    offline_package_selection_from_record(record)?,
                );
            }
        }
        Ok(preferences)
    }

    fn next_record_mutation_epoch_ms(&self, key: &str, now_epoch_ms: i64) -> i64 {
        let latest = self
            .persistent
            .records
            .cached
            .get(key)
            .into_iter()
            .chain(self.persistent.records.deferred_adoption.get(key))
            .filter_map(|record| record.modified_at_epoch_ms)
            .max()
            .unwrap_or(LEGACY_UNKNOWN_MUTATION_EPOCH_MS);
        now_epoch_ms.max(latest.saturating_add(1))
    }

    fn record_local_cloud_record(&mut self, key: &str, record: CloudRecord) {
        self.persistent.local_revision = self.persistent.local_revision.saturating_add(1);
        self.persistent
            .records
            .cached
            .insert(key.to_string(), record);
        self.persistent.records.deferred_adoption.remove(key);
        if self.persistent.account.is_some() {
            self.persistent.records.pending_keys.insert(key.to_string());
        }
    }

    #[cfg(test)]
    fn perform_action(&mut self, action: CloudAction, current_plan: &FlightPlan) -> AppResult<()> {
        self.perform_action_at(action, current_plan, 0)
    }

    fn perform_action_at(
        &mut self,
        action: CloudAction,
        current_plan: &FlightPlan,
        now_epoch_ms: i64,
    ) -> AppResult<()> {
        match action {
            CloudAction::BeginSetupFromDevice => {
                self.require_unlinked_and_idle()?;
                self.persistent.onboarding_intent = Some(CloudOnboardingIntent::SetupFromDevice);
                self.persistent.selected_provider = None;
                self.persistent.last_provider_failure = None;
            }
            CloudAction::BeginCreateAccount => {
                self.require_unlinked_and_idle()?;
                self.persistent.onboarding_intent = Some(CloudOnboardingIntent::CreateAccount);
                self.persistent.last_provider_failure = None;
            }
            CloudAction::BackSetup => {
                if self
                    .persistent
                    .account
                    .as_ref()
                    .is_some_and(|account| account.tip.is_some())
                {
                    return Err(cloud_error("unlink this device before starting over"));
                }
                match self.persistent.onboarding_intent {
                    Some(CloudOnboardingIntent::SetupFromDevice) => {
                        if self.persistent.account.is_some() {
                            self.persistent.account = None;
                        } else {
                            self.persistent.onboarding_intent = None;
                        }
                    }
                    Some(CloudOnboardingIntent::CreateAccount) => {
                        if let Some(account) = self.persistent.account.take() {
                            self.persistent.selected_provider = Some(account.provider);
                        } else if self.persistent.selected_provider.take().is_none() {
                            self.persistent.onboarding_intent = None;
                        }
                    }
                    None => return Err(cloud_error("cloud setup has no earlier step")),
                }
                self.persistent.workflow = None;
                self.persistent.records.pending_keys.clear();
                self.persistent.records.deferred_adoption.clear();
                self.persistent.last_provider_failure = None;
                self.authorization_request_in_flight = None;
                self.pending_authorization_mode = None;
                self.provider_request_in_flight = None;
                self.linked_account_detail = None;
            }
            CloudAction::SelectProvider { provider } => {
                self.require_unlinked_and_idle()?;
                if self.persistent.onboarding_intent != Some(CloudOnboardingIntent::CreateAccount) {
                    return Err(cloud_error(
                        "choose to create a Sync Account before selecting its provider",
                    ));
                }
                if !provider.is_available() {
                    return Err(cloud_error(format!(
                        "{} is not available in this build",
                        provider.label()
                    )));
                }
                self.persistent.selected_provider = Some(provider);
                self.persistent.last_provider_failure = None;
            }
            CloudAction::CreateAccount => {
                self.require_unlinked_and_idle()?;
                let provider = self.persistent.selected_provider.ok_or_else(|| {
                    cloud_error("select a provider before creating a Sync Account")
                })?;
                let authorization = self.authorization_for(provider);
                let principal = authorization.principal().ok_or_else(|| {
                    cloud_error("authorize the provider before creating a Sync Account")
                })?;
                let account_binding = provider_account_binding(principal);
                let secret = random_bytes::<32>()?;
                self.reset_sync_activity();
                self.persistent.account = Some(CloudAccount {
                    provider,
                    provider_account_binding: Some(account_binding),
                    root_secret_base64: URL_SAFE_NO_PAD.encode(secret),
                    genesis_id: String::new(),
                    tip: None,
                });
                self.persistent.selected_provider = None;
                let plan = cloud_flight_plan_definition(current_plan);
                let record = self
                    .persistent
                    .records
                    .cached
                    .get(FLIGHT_PLAN_RECORD_KEY)
                    .and_then(|record| flight_plan_from_record(record).ok())
                    .filter(|record| {
                        record.plan == plan
                            && record.modified_at_epoch_ms != LEGACY_UNKNOWN_MUTATION_EPOCH_MS
                    })
                    .unwrap_or(StampedFlightPlan {
                        plan,
                        modified_at_epoch_ms: now_epoch_ms,
                    });
                self.persistent.records.cached.insert(
                    FLIGHT_PLAN_RECORD_KEY.to_string(),
                    cloud_record_for_flight_plan(&record)?,
                );
                self.persistent.records.pending_keys =
                    self.persistent.records.cached.keys().cloned().collect();
                self.persistent.workflow = Some(CloudWorkflow::CreateAwaitIds);
                self.persistent.last_provider_failure = None;
            }
            CloudAction::AcceptDeviceSetupCode { setup_code } => {
                if self.persistent.workflow.is_some() {
                    return Err(cloud_error("cloud synchronization is already busy"));
                }
                if self.persistent.account.is_some() {
                    return Err(cloud_error(
                        "unlink this device before accepting another device setup code",
                    ));
                }
                let payload = decode_device_setup_code(&setup_code)?;
                if !payload.provider.is_available() {
                    return Err(cloud_error(format!(
                        "{} is not available in this build",
                        payload.provider.label()
                    )));
                }
                self.persistent.onboarding_intent = Some(CloudOnboardingIntent::SetupFromDevice);
                self.persistent.selected_provider = None;
                self.reset_sync_activity();
                self.persistent.account = Some(CloudAccount {
                    provider: payload.provider,
                    provider_account_binding: Some(payload.provider_account_binding),
                    root_secret_base64: payload.root_secret_base64,
                    genesis_id: payload.genesis_id.clone(),
                    tip: None,
                });
                self.persistent.records.pending_keys.clear();
                self.persistent.records.deferred_adoption.clear();
                self.persistent.workflow = Some(CloudWorkflow::ReadNode {
                    node_id: payload.genesis_id,
                    purpose: ReadPurpose::Link,
                });
                self.persistent.last_provider_failure = None;
            }
            CloudAction::BackUpDeviceSetupCode => {
                self.require_linked_account()?;
                self.linked_account_detail = Some(LinkedAccountDetail::BackupCode);
            }
            CloudAction::AddAnotherDevice => {
                self.require_linked_account()?;
                self.linked_account_detail = Some(LinkedAccountDetail::AddDevice);
            }
            CloudAction::CloseLinkedAccountDetail => {
                self.linked_account_detail = None;
            }
            CloudAction::BeginUnlinkDevice => {
                self.require_linked_account()?;
                self.linked_account_detail = Some(LinkedAccountDetail::ConfirmUnlink);
            }
            CloudAction::ConfirmUnlinkDevice => {
                self.require_linked_account()?;
                if self.linked_account_detail != Some(LinkedAccountDetail::ConfirmUnlink) {
                    return Err(cloud_error(
                        "confirm unlink before deleting this device's copy",
                    ));
                }
                self.persistent.account = None;
                self.persistent.selected_provider = None;
                self.persistent.onboarding_intent = None;
                self.persistent.workflow = None;
                self.persistent.records.pending_keys.clear();
                self.persistent.records.deferred_adoption.clear();
                self.persistent.last_provider_failure = None;
                self.reset_sync_activity();
                self.authorization_request_in_flight = None;
                self.pending_authorization_mode = None;
                self.provider_request_in_flight = None;
                self.linked_account_detail = None;
            }
            CloudAction::SyncNow => self.persistent.force_poll = true,
        }
        Ok(())
    }

    fn reset_sync_activity(&mut self) {
        self.persistent.last_success_epoch_ms = None;
        self.persistent.last_read_epoch_ms = None;
        self.persistent.last_write_epoch_ms = None;
        self.persistent.last_poll_epoch_ms = None;
        self.persistent.force_poll = false;
    }

    pub fn perform_ui_action(
        &mut self,
        action_id: CloudUiActionId,
        fields: &[CloudUiFieldValue],
        current_plan: &FlightPlan,
        now_epoch_ms: i64,
    ) -> AppResult<()> {
        let action = match action_id {
            CloudUiActionId::BeginSetup => CloudAction::BeginSetupFromDevice,
            CloudUiActionId::BeginCreate => CloudAction::BeginCreateAccount,
            CloudUiActionId::BackSetup => CloudAction::BackSetup,
            CloudUiActionId::SelectProviderGoogleDrive => CloudAction::SelectProvider {
                provider: CloudProviderKind::GoogleDrive,
            },
            CloudUiActionId::SelectProviderAerobagCloud => CloudAction::SelectProvider {
                provider: CloudProviderKind::AerobagCloud,
            },
            CloudUiActionId::CreateAccount => {
                let provider = self.current_provider()?;
                if !self
                    .authorization_for_at(provider, now_epoch_ms)
                    .is_ready(now_epoch_ms)
                {
                    return Err(cloud_error(
                        "authorize the provider before creating a Sync Account",
                    ));
                }
                CloudAction::CreateAccount
            }
            CloudUiActionId::AcceptSetupCode => CloudAction::AcceptDeviceSetupCode {
                setup_code: required_ui_field(fields, CloudUiFieldId::DeviceSetupCode)?,
            },
            CloudUiActionId::BackupSetupCode => CloudAction::BackUpDeviceSetupCode,
            CloudUiActionId::AddDevice => CloudAction::AddAnotherDevice,
            CloudUiActionId::CloseLinkedDetail => CloudAction::CloseLinkedAccountDetail,
            CloudUiActionId::BeginUnlink => CloudAction::BeginUnlinkDevice,
            CloudUiActionId::ConfirmUnlink => CloudAction::ConfirmUnlinkDevice,
            CloudUiActionId::SyncNow => CloudAction::SyncNow,
            CloudUiActionId::AuthorizeProvider => {
                let provider = self.current_provider()?;
                if !provider.is_available() {
                    return Err(cloud_error(format!(
                        "{} cannot be authorized in this build",
                        provider.label()
                    )));
                }
                self.pending_authorization_mode = Some(CloudAuthorizationMode::Interactive);
                self.set_authorization_state(provider, ProviderAuthorizationState::Authorizing);
                return Ok(());
            }
            CloudUiActionId::CopySetupCode => {
                self.device_setup_code()?;
                return Ok(());
            }
            CloudUiActionId::ScanSetupCode => {
                return Ok(());
            }
        };
        self.perform_action_at(action, current_plan, now_epoch_ms)?;
        Ok(())
    }

    pub fn take_provider_request(
        &mut self,
        now_epoch_ms: i64,
    ) -> AppResult<Option<CloudHttpRequest>> {
        let Ok(provider) = self.current_provider() else {
            return Ok(None);
        };
        if self.provider_request_in_flight.is_some()
            || !self.authorization_for(provider).is_ready(now_epoch_ms)
            || self.provider_principal_mismatch().is_some()
            || self
                .persistent
                .next_retry_epoch_ms
                .is_some_and(|retry| retry > now_epoch_ms)
        {
            return Ok(None);
        }
        self.ensure_workflow(now_epoch_ms)?;
        let Some(workflow) = self.persistent.workflow.as_ref() else {
            return Ok(None);
        };
        let operation = operation_for_workflow(workflow)?;
        let request_id = self.persistent.next_request_id.max(1);
        self.persistent.next_request_id = request_id.saturating_add(1);
        let request = CloudProviderRequest {
            request_id,
            provider,
            operation,
        };
        let http_request = crate::cloud_google_drive::plan_request(&request)?;
        self.provider_request_in_flight = Some(request);
        Ok(Some(http_request))
    }

    pub fn complete_provider_request(
        &mut self,
        request_id: u64,
        response: CloudHttpResponse,
        now_epoch_ms: i64,
    ) -> AppResult<CloudCompletion> {
        let Some(request) = self.provider_request_in_flight.as_ref() else {
            return Err(cloud_error(format!(
                "cloud provider response {request_id} arrived with no request in flight"
            )));
        };
        if request.request_id != request_id {
            return Err(cloud_error(format!(
                "cloud provider response {request_id} does not match in-flight request {}",
                request.request_id
            )));
        }
        let request = self
            .provider_request_in_flight
            .take()
            .expect("provider request checked above");
        let response = crate::cloud_google_drive::parse_response(&request, response);
        let provider = self.current_provider()?;
        if let CloudProviderResponse::Error { kind, detail } = response {
            self.persistent.last_provider_failure = Some(CloudProviderFailure {
                kind,
                detail: detail.clone(),
            });
            match kind {
                CloudProviderErrorKind::Unauthorized => {
                    self.authorizations.insert(
                        provider,
                        ProviderAuthorizationState::AuthorizationRequired { detail },
                    );
                }
                CloudProviderErrorKind::Transient => {
                    self.persistent.next_retry_epoch_ms =
                        Some(now_epoch_ms.saturating_add(CLOUD_TRANSIENT_RETRY_MS));
                }
                CloudProviderErrorKind::Permanent => {
                    self.authorizations
                        .insert(provider, ProviderAuthorizationState::Failed { detail });
                }
            }
            return Ok(CloudCompletion::default());
        }

        self.persistent.last_provider_failure = None;
        self.persistent.next_retry_epoch_ms = None;
        let workflow = self
            .persistent
            .workflow
            .clone()
            .ok_or_else(|| cloud_error("cloud provider completed with no active workflow"))?;
        let completion = self.advance_workflow(workflow, response, now_epoch_ms)?;
        self.persistent.last_success_epoch_ms = Some(now_epoch_ms);
        Ok(completion)
    }

    fn ensure_workflow(&mut self, now_epoch_ms: i64) -> AppResult<()> {
        if self.persistent.workflow.is_some() {
            return Ok(());
        }
        let Some(account) = self.persistent.account.as_ref() else {
            return Ok(());
        };
        if account.tip.is_none() || account.genesis_id.is_empty() {
            return Ok(());
        }
        if !self.persistent.records.pending_keys.is_empty() {
            self.persistent.workflow = Some(CloudWorkflow::PublishAwaitIds);
            return Ok(());
        }
        let poll_due = self.persistent.force_poll
            || self
                .persistent
                .last_poll_epoch_ms
                .is_none_or(|last| now_epoch_ms.saturating_sub(last) >= CLOUD_POLL_INTERVAL_MS);
        if poll_due {
            let tip = account.tip.as_ref().expect("tip checked above");
            self.persistent.force_poll = false;
            self.persistent.workflow = Some(CloudWorkflow::ReadNode {
                node_id: tip.next_slot_id.clone(),
                purpose: ReadPurpose::Poll,
            });
        }
        Ok(())
    }

    fn advance_workflow(
        &mut self,
        workflow: CloudWorkflow,
        response: CloudProviderResponse,
        now_epoch_ms: i64,
    ) -> AppResult<CloudCompletion> {
        match workflow {
            CloudWorkflow::CreateAwaitIds => {
                let ids = expect_allocated_ids(response, 3)?;
                let staged = self.stage_publication(
                    PublicationPurpose::CreateAccount,
                    ids[0].clone(),
                    ids[1].clone(),
                    ids[2].clone(),
                    0,
                    None,
                    now_epoch_ms,
                )?;
                self.account_mut()?.genesis_id = staged.node_id.clone();
                self.persistent.workflow = Some(CloudWorkflow::CreatePage { staged });
            }
            CloudWorkflow::PublishAwaitIds => {
                let ids = expect_allocated_ids(response, 2)?;
                let tip = self.tip()?.clone();
                let staged = self.stage_publication(
                    PublicationPurpose::Publish,
                    tip.next_slot_id.clone(),
                    ids[0].clone(),
                    ids[1].clone(),
                    tip.generation.saturating_add(1),
                    Some(&tip),
                    now_epoch_ms,
                )?;
                self.persistent.workflow = Some(CloudWorkflow::CreatePage { staged });
            }
            CloudWorkflow::CreatePage { staged } => match response {
                CloudProviderResponse::Created => {
                    self.persistent.workflow = Some(CloudWorkflow::CreateNode { staged });
                }
                CloudProviderResponse::AlreadyExists => {
                    self.persistent.workflow = Some(CloudWorkflow::VerifyPage { staged });
                }
                other => return Err(unexpected_response("create page", other)),
            },
            CloudWorkflow::VerifyPage { staged } => {
                let bytes = expect_read_bytes(response, &staged.page_id)?;
                if URL_SAFE_NO_PAD.encode(bytes) != staged.page_bytes_base64 {
                    return Err(cloud_error(
                        "an occupied cloud page slot contained different data",
                    ));
                }
                self.persistent.workflow = Some(CloudWorkflow::CreateNode { staged });
            }
            CloudWorkflow::CreateNode { staged } => match response {
                CloudProviderResponse::Created => self.finish_publication(staged)?,
                CloudProviderResponse::AlreadyExists => {
                    self.persistent.workflow = match staged.purpose {
                        PublicationPurpose::CreateAccount => {
                            Some(CloudWorkflow::VerifyNode { staged })
                        }
                        PublicationPurpose::Publish => Some(CloudWorkflow::ReadNode {
                            node_id: staged.node_id,
                            purpose: ReadPurpose::PublishRace,
                        }),
                    };
                }
                other => return Err(unexpected_response("create state node", other)),
            },
            CloudWorkflow::VerifyNode { staged } => {
                let bytes = expect_read_bytes(response, &staged.node_id)?;
                if URL_SAFE_NO_PAD.encode(bytes) != staged.node_bytes_base64 {
                    return Err(cloud_error(
                        "an occupied cloud genesis slot contained different data",
                    ));
                }
                self.finish_publication(staged)?;
            }
            CloudWorkflow::ReadNode { node_id, purpose } => {
                let Some(bytes) = expect_optional_read_bytes(response)? else {
                    match purpose {
                        ReadPurpose::Link => {
                            return Err(cloud_error(
                                "the Device Setup Code's Sync Account was not found",
                            ));
                        }
                        ReadPurpose::Poll | ReadPurpose::PublishRace => {
                            self.persistent.last_read_epoch_ms = Some(now_epoch_ms);
                            self.persistent.last_poll_epoch_ms = Some(now_epoch_ms);
                            self.persistent.workflow = None;
                        }
                    }
                    return Ok(CloudCompletion::default());
                };
                let node_hash = sha256_hex(&bytes);
                let node: CloudNode = self.decrypt(&bytes, "state_node")?;
                self.validate_node(&node_id, &node_hash, &node, purpose)?;
                self.persistent.workflow = Some(CloudWorkflow::ReadPage {
                    node_id,
                    node_hash,
                    node,
                    purpose,
                });
            }
            CloudWorkflow::ReadPage {
                node_id,
                node_hash,
                node,
                purpose,
            } => {
                let bytes = expect_read_bytes(response, &node.merkle_root_id)?;
                let actual_hash = sha256_hex(&bytes);
                if actual_hash != node.merkle_root_hash {
                    return Err(cloud_error(format!(
                        "cloud page hash mismatch: expected {}, got {actual_hash}",
                        node.merkle_root_hash
                    )));
                }
                let page: CloudPage = self.decrypt(&bytes, "merkle_page")?;
                validate_cloud_page(&page)?;
                self.account_mut()?.tip = Some(VerifiedTip {
                    node_id,
                    node_hash,
                    generation: node.generation,
                    published_at_epoch_ms: node.published_at_epoch_ms,
                    merkle_root_id: node.merkle_root_id,
                    merkle_root_hash: node.merkle_root_hash,
                    next_slot_id: node.next_slot_id,
                });
                self.persistent.last_read_epoch_ms = Some(now_epoch_ms);
                self.persistent.last_poll_epoch_ms = Some(now_epoch_ms);
                self.persistent.workflow = None;
                let completion = self.reconcile_page(page, purpose)?;
                if !completion.changed_records.is_empty() {
                    self.persistent.force_poll = true;
                    return Ok(completion);
                }
                if matches!(purpose, ReadPurpose::PublishRace) {
                    self.persistent.force_poll = true;
                }
            }
        }
        Ok(CloudCompletion::default())
    }

    fn reconcile_page(
        &mut self,
        page: CloudPage,
        purpose: ReadPurpose,
    ) -> AppResult<CloudCompletion> {
        let mut changed_records = BTreeMap::new();
        let local_keys = self
            .persistent
            .records
            .cached
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let remote_keys = page.records.keys().cloned().collect::<BTreeSet<_>>();

        for (key, remote) in page.records {
            validate_known_record(&key, &remote)?;
            let local = self.persistent.records.cached.get(&key);
            let remote_wins = matches!(purpose, ReadPurpose::Link)
                || local
                    .map(|local| compare_cloud_records(&remote, local))
                    .transpose()?
                    .is_none_or(|ordering| ordering != Ordering::Less);
            if remote_wins {
                let changed = matches!(purpose, ReadPurpose::Link)
                    || local.is_none_or(|local| local != &remote);
                self.persistent
                    .records
                    .cached
                    .insert(key.clone(), remote.clone());
                self.persistent.records.pending_keys.remove(&key);
                if changed {
                    changed_records.insert(key, remote);
                }
            } else {
                self.persistent.records.pending_keys.insert(key);
            }
        }

        // A full page that predates a newly introduced record does not delete
        // the local value. Publish the local record into the next successor.
        for key in local_keys {
            if !remote_keys.contains(&key) {
                self.persistent.records.pending_keys.insert(key);
            }
        }

        Ok(CloudCompletion { changed_records })
    }

    fn stage_publication(
        &self,
        purpose: PublicationPurpose,
        node_id: String,
        page_id: String,
        next_slot_id: String,
        generation: u64,
        parent: Option<&VerifiedTip>,
        now_epoch_ms: i64,
    ) -> AppResult<StagedPublication> {
        let page = page_for_records(&self.persistent.records.cached);
        let page_bytes = self.encrypt(&page, "merkle_page")?;
        let page_hash = sha256_hex(&page_bytes);
        let node = CloudNode {
            version: CLOUD_NODE_VERSION,
            generation,
            published_at_epoch_ms: Some(now_epoch_ms),
            parent_node_id: parent.map(|tip| tip.node_id.clone()),
            parent_node_hash: parent.map(|tip| tip.node_hash.clone()),
            merkle_root_id: page_id.clone(),
            merkle_root_hash: page_hash.clone(),
            next_slot_id: next_slot_id.clone(),
        };
        let node_bytes = self.encrypt(&node, "state_node")?;
        Ok(StagedPublication {
            purpose,
            node_id,
            page_id,
            next_slot_id,
            page_bytes_base64: URL_SAFE_NO_PAD.encode(page_bytes),
            node_hash: sha256_hex(&node_bytes),
            node_bytes_base64: URL_SAFE_NO_PAD.encode(node_bytes),
            merkle_root_hash: page_hash,
            generation,
            published_at_epoch_ms: now_epoch_ms,
            local_revision: self.persistent.local_revision,
        })
    }

    fn finish_publication(&mut self, staged: StagedPublication) -> AppResult<()> {
        self.account_mut()?.tip = Some(VerifiedTip {
            node_id: staged.node_id,
            node_hash: staged.node_hash,
            generation: staged.generation,
            published_at_epoch_ms: Some(staged.published_at_epoch_ms),
            merkle_root_id: staged.page_id,
            merkle_root_hash: staged.merkle_root_hash,
            next_slot_id: staged.next_slot_id,
        });
        self.persistent.last_write_epoch_ms = Some(staged.published_at_epoch_ms);
        if self.persistent.local_revision == staged.local_revision {
            self.persistent.records.pending_keys.clear();
        }
        self.persistent.workflow = None;
        self.persistent.force_poll = true;
        Ok(())
    }

    fn validate_node(
        &self,
        node_id: &str,
        _node_hash: &str,
        node: &CloudNode,
        purpose: ReadPurpose,
    ) -> AppResult<()> {
        if node.version != CLOUD_NODE_VERSION {
            return Err(cloud_error(format!(
                "unsupported cloud state node version {}",
                node.version
            )));
        }
        match purpose {
            ReadPurpose::Link => {
                let account = self.account()?;
                if node_id != account.genesis_id
                    || node.generation != 0
                    || node.parent_node_id.is_some()
                    || node.parent_node_hash.is_some()
                {
                    return Err(cloud_error(
                        "Device Setup Code did not resolve to a valid account genesis node",
                    ));
                }
            }
            ReadPurpose::Poll | ReadPurpose::PublishRace => {
                let tip = self.tip()?;
                if node_id != tip.next_slot_id
                    || node.generation != tip.generation.saturating_add(1)
                    || node.parent_node_id.as_deref() != Some(tip.node_id.as_str())
                    || node.parent_node_hash.as_deref() != Some(tip.node_hash.as_str())
                {
                    return Err(cloud_error(
                        "cloud successor does not continue the verified chain",
                    ));
                }
            }
        }
        Ok(())
    }

    fn encrypt<T: Serialize>(&self, value: &T, role: &str) -> AppResult<Vec<u8>> {
        let account = self.account()?;
        let secret = account_secret(account)?;
        let account_tag = account_tag(&secret);
        let key = derive_key(&secret, role)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|_| cloud_error("invalid cloud encryption key"))?;
        let nonce = random_bytes::<12>()?;
        let aad = envelope_aad(CLOUD_ENVELOPE_VERSION, &account_tag, role);
        let plaintext = serde_json::to_vec(value).map_err(cloud_json_error)?;
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| cloud_error("cloud encryption failed"))?;
        serde_json::to_vec(&CloudEnvelope {
            version: CLOUD_ENVELOPE_VERSION,
            account_tag,
            role: role.to_string(),
            nonce_base64: URL_SAFE_NO_PAD.encode(nonce),
            ciphertext_base64: URL_SAFE_NO_PAD.encode(ciphertext),
        })
        .map_err(cloud_json_error)
    }

    fn decrypt<T: for<'de> Deserialize<'de>>(&self, bytes: &[u8], role: &str) -> AppResult<T> {
        let envelope: CloudEnvelope = serde_json::from_slice(bytes).map_err(cloud_json_error)?;
        let account = self.account()?;
        let secret = account_secret(account)?;
        let expected_tag = account_tag(&secret);
        if envelope.version != CLOUD_ENVELOPE_VERSION
            || envelope.account_tag != expected_tag
            || envelope.role != role
        {
            return Err(cloud_error(
                "cloud envelope binding does not match this account",
            ));
        }
        let nonce = URL_SAFE_NO_PAD
            .decode(&envelope.nonce_base64)
            .map_err(|_| cloud_error("cloud envelope nonce is invalid"))?;
        let nonce: [u8; 12] = nonce
            .try_into()
            .map_err(|_| cloud_error("cloud envelope nonce has the wrong size"))?;
        let ciphertext = URL_SAFE_NO_PAD
            .decode(&envelope.ciphertext_base64)
            .map_err(|_| cloud_error("cloud envelope ciphertext is invalid"))?;
        let key = derive_key(&secret, role)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|_| cloud_error("invalid cloud decryption key"))?;
        let aad = envelope_aad(envelope.version, &envelope.account_tag, role);
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| cloud_error("cloud envelope authentication failed"))?;
        serde_json::from_slice(&plaintext).map_err(cloud_json_error)
    }

    fn account(&self) -> AppResult<&CloudAccount> {
        self.persistent
            .account
            .as_ref()
            .ok_or_else(|| cloud_error("no cloud account is linked"))
    }

    fn account_mut(&mut self) -> AppResult<&mut CloudAccount> {
        self.persistent
            .account
            .as_mut()
            .ok_or_else(|| cloud_error("no cloud account is linked"))
    }

    fn tip(&self) -> AppResult<&VerifiedTip> {
        self.account()?
            .tip
            .as_ref()
            .ok_or_else(|| cloud_error("cloud account has no verified tip"))
    }

    pub fn set_pending_remote_flight_plan(&mut self, plan: FlightPlan) -> AppResult<()> {
        let record = self
            .persistent
            .records
            .cached
            .get(FLIGHT_PLAN_RECORD_KEY)
            .filter(|record| {
                flight_plan_from_record(record).is_ok_and(|record| record.plan == plan)
            })
            .cloned()
            .ok_or_else(|| {
                cloud_error("remote flight-plan completion did not match the reconciled record")
            })?;
        self.persistent
            .records
            .deferred_adoption
            .insert(FLIGHT_PLAN_RECORD_KEY.to_string(), record);
        Ok(())
    }

    pub fn page_state(&self, now_epoch_ms: i64) -> UiCloudPageState {
        self.page_state_with_qr_scanner(now_epoch_ms, false)
    }

    pub fn page_state_with_qr_scanner(
        &self,
        now_epoch_ms: i64,
        qr_scanner_available: bool,
    ) -> UiCloudPageState {
        let mut panels = Vec::new();
        let account = self.persistent.account.as_ref();
        let linked = self.has_linked_account();
        let intent = self.persistent.onboarding_intent.or_else(|| {
            account.map(|account| {
                if account.genesis_id.is_empty() {
                    CloudOnboardingIntent::CreateAccount
                } else {
                    CloudOnboardingIntent::SetupFromDevice
                }
            })
        });

        if account.is_none() && intent.is_none() {
            panels.push(cloud_panel(
                "get_started",
                "Get started",
                UiCloudPanelState::Active,
                Some("Connect this device to a shared, encrypted Sync Account."),
                vec![
                    cloud_action(
                        CloudUiActionId::BeginSetup,
                        "Set up from another device",
                        true,
                        "",
                    ),
                    cloud_action(
                        CloudUiActionId::BeginCreate,
                        "Create new Sync Account",
                        true,
                        "",
                    ),
                ],
                None,
            ));
            return self.cloud_page_state(panels, now_epoch_ms);
        }

        match intent {
            Some(CloudOnboardingIntent::SetupFromDevice) => {
                panels.push(cloud_panel(
                    "get_started",
                    "Get started: Set up from another device",
                    UiCloudPanelState::Complete,
                    None,
                    Vec::new(),
                    None,
                ));
                if account.is_none() {
                    panels.push(cloud_panel(
                        "receive_setup",
                        "Set up from another device",
                        UiCloudPanelState::Active,
                        Some("Scan the other device's QR code or paste its Device Setup Code."),
                        vec![
                            if qr_scanner_available {
                                cloud_effect_action(
                                    CloudUiActionId::ScanSetupCode,
                                    "Scan a QR code",
                                    true,
                                    "",
                                    CloudPlatformEffect::ScanQrCode {
                                        completion_action: CloudUiActionId::AcceptSetupCode,
                                        field_id: CloudUiFieldId::DeviceSetupCode,
                                    },
                                )
                            } else {
                                cloud_action(
                                    CloudUiActionId::ScanSetupCode,
                                    "Scan a QR code",
                                    false,
                                    "QR scanning is not available on this device.",
                                )
                            },
                            cloud_action(
                                CloudUiActionId::AcceptSetupCode,
                                "Use Device Setup Code",
                                true,
                                "",
                            ),
                            cloud_action(CloudUiActionId::BackSetup, "Back", true, ""),
                        ],
                        Some(UiCloudPanelControl::DeviceSetupCodeInput {
                            field_id: CloudUiFieldId::DeviceSetupCode,
                            label: "Paste Device Setup Code".to_string(),
                            placeholder: "AB2...".to_string(),
                        }),
                    ));
                    return self.cloud_page_state(panels, now_epoch_ms);
                }
                panels.push(cloud_panel(
                    "receive_setup",
                    "Sync Account setup received",
                    UiCloudPanelState::Complete,
                    None,
                    Vec::new(),
                    None,
                ));
            }
            Some(CloudOnboardingIntent::CreateAccount) => {
                panels.push(cloud_panel(
                    "get_started",
                    "Get started: Create new Sync Account",
                    UiCloudPanelState::Complete,
                    None,
                    Vec::new(),
                    None,
                ));
                let provider = self.current_provider().ok();
                if provider.is_none() {
                    panels.push(cloud_panel(
                        "provider",
                        "Select storage provider",
                        UiCloudPanelState::Active,
                        Some("Choose where Aerobag stores your encrypted sync data."),
                        vec![
                            cloud_action(
                                CloudUiActionId::SelectProviderGoogleDrive,
                                "My Google Drive",
                                CloudProviderKind::GoogleDrive.is_available(),
                                "My Google Drive is not available in this build.",
                            ),
                            cloud_action(
                                CloudUiActionId::SelectProviderAerobagCloud,
                                "Aerobag Cloud",
                                CloudProviderKind::AerobagCloud.is_available(),
                                "Aerobag Cloud is not available in this build yet.",
                            ),
                            cloud_action(CloudUiActionId::BackSetup, "Back", true, ""),
                        ],
                        None,
                    ));
                    return self.cloud_page_state(panels, now_epoch_ms);
                }
                panels.push(cloud_panel(
                    "provider",
                    "Storage provider selected",
                    UiCloudPanelState::Complete,
                    None,
                    Vec::new(),
                    None,
                ));
            }
            None => {}
        }

        if linked {
            if let Some(detail) = self.linked_account_detail {
                panels.push(self.linked_summary_panel(UiCloudPanelState::Complete));
                panels.push(self.linked_account_detail_panel(detail));
                return self.cloud_page_state(panels, now_epoch_ms);
            }
        }

        let account_is_pending = account.is_some_and(|account| account.tip.is_none());
        let authorization_complete = self.provider_authorization_complete(now_epoch_ms);

        if account_is_pending {
            let creating = account.is_some_and(|account| account.genesis_id.is_empty());
            let (title, state, summary) = if !authorization_complete {
                let provider = self
                    .current_provider()
                    .expect("pending Sync Account must name a provider");
                (
                    "Sync Account waiting for provider",
                    UiCloudPanelState::Active,
                    format!(
                        "Authorize {} in the provider panel to continue.",
                        provider.label()
                    ),
                )
            } else {
                let title = if creating {
                    "Creating Sync Account..."
                } else {
                    "Linking Sync Account..."
                };
                let state = if self.persistent.last_provider_failure.is_some() {
                    UiCloudPanelState::Error
                } else {
                    UiCloudPanelState::Working
                };
                let summary = self
                    .persistent
                    .last_provider_failure
                    .as_ref()
                    .map(|failure| failure.detail.clone())
                    .unwrap_or_else(|| {
                        "Aerobag is verifying encrypted account state with the provider."
                            .to_string()
                    });
                (title, state, summary)
            };
            panels.push(cloud_panel(
                if creating {
                    "create_account"
                } else {
                    "link_account"
                },
                title,
                state,
                Some(&summary),
                vec![cloud_action(CloudUiActionId::BackSetup, "Back", true, "")],
                None,
            ));
        } else if account.is_none() {
            let provider = self
                .current_provider()
                .expect("account creation must have a selected provider");
            let authorization = self.authorization_for_at(provider, now_epoch_ms);
            let principal = authorization.principal();
            let suffix = principal
                .map(|principal| format!(" as {}", principal.display_label))
                .unwrap_or_default();
            let create_disabled_reason = format!(
                "Authorize {} in the provider panel first.",
                provider.label()
            );
            panels.push(cloud_panel(
                "create_account",
                &format!(
                    "Create a new Sync Account in {}{suffix}",
                    provider.label()
                ),
                UiCloudPanelState::Active,
                Some(
                    "This always creates a new Sync Account. It will not find or replace another account already stored by this provider. To use an existing account, go back and choose Set up from another device.",
                ),
                vec![
                    cloud_action(
                        CloudUiActionId::CreateAccount,
                        "Create new Sync Account",
                        authorization_complete,
                        &create_disabled_reason,
                    ),
                    cloud_action(CloudUiActionId::BackSetup, "Back", true, ""),
                ],
                None,
            ));
        } else if linked {
            panels.push(self.linked_summary_panel(UiCloudPanelState::Active));
        }

        self.cloud_page_state(panels, now_epoch_ms)
    }

    fn cloud_page_state(
        &self,
        sync_account_panels: Vec<UiCloudPanel>,
        now_epoch_ms: i64,
    ) -> UiCloudPageState {
        UiCloudPageState {
            title: "Cloud".to_string(),
            summary: "Keep your Aerobag state synchronized between devices.".to_string(),
            sync_account_heading: "Sync Account".to_string(),
            provider_heading: "Provider".to_string(),
            overall_status_label: "Overall Cloud status".to_string(),
            sync_account_panels,
            provider_card: self
                .current_provider()
                .ok()
                .map(|provider| self.provider_card(provider, now_epoch_ms)),
            overall_status: self.overall_status_panel(now_epoch_ms),
            next_refresh_epoch_ms: self.next_authorization_refresh_epoch_ms(now_epoch_ms),
        }
    }

    fn overall_status_panel(&self, now_epoch_ms: i64) -> UiCloudPanel {
        if !self.has_linked_account() {
            return cloud_panel(
                "overall_status",
                "Cloud not active",
                UiCloudPanelState::Informational,
                Some("No Sync Account linked yet."),
                Vec::new(),
                None,
            );
        }

        if !self.provider_authorization_complete(now_epoch_ms) {
            let detail = if self.current_provider().ok().is_some_and(|provider| {
                matches!(
                    self.authorization_for_at(provider, now_epoch_ms),
                    ProviderAuthorizationState::Authorizing
                )
            }) {
                "Sync Account linked, but provider authorization is in progress."
            } else {
                "Sync Account linked, but provider requires authorization."
            };
            return cloud_panel(
                "overall_status",
                "Cloud not active",
                UiCloudPanelState::Caution,
                Some(detail),
                Vec::new(),
                None,
            );
        }

        if self
            .persistent
            .last_provider_failure
            .as_ref()
            .is_some_and(|failure| failure.kind == CloudProviderErrorKind::Transient)
        {
            return cloud_panel(
                "overall_status",
                "Cloud not active",
                UiCloudPanelState::Informational,
                Some("Sync Account linked, but provider is temporarily unavailable."),
                Vec::new(),
                None,
            );
        }

        let mut panel = cloud_panel(
            "overall_status",
            "Cloud active",
            UiCloudPanelState::Complete,
            Some("Sync Account linked, provider connected."),
            Vec::new(),
            None,
        );
        panel.time_facts = self.sync_time_facts();
        panel
    }

    fn sync_time_facts(&self) -> Vec<UiCloudTimeFact> {
        let mut facts = Vec::new();
        if let Some(epoch_ms) = self.persistent.last_read_epoch_ms {
            facts.push(UiCloudTimeFact {
                label: "Last read from cloud".to_string(),
                epoch_ms,
            });
        }
        if let Some(epoch_ms) = self
            .persistent
            .account
            .as_ref()
            .and_then(|account| account.tip.as_ref())
            .and_then(|tip| tip.published_at_epoch_ms)
        {
            facts.push(UiCloudTimeFact {
                label: "Last update on cloud".to_string(),
                epoch_ms,
            });
        }
        if let Some(epoch_ms) = self.persistent.last_write_epoch_ms {
            facts.push(UiCloudTimeFact {
                label: "Last write to cloud from this device".to_string(),
                epoch_ms,
            });
        }
        facts
    }

    fn provider_authorization_complete(&self, now_epoch_ms: i64) -> bool {
        let Ok(provider) = self.current_provider() else {
            return false;
        };
        self.provider_principal_mismatch().is_none()
            && matches!(
                self.authorization_for_at(provider, now_epoch_ms),
                authorization if authorization.is_ready(now_epoch_ms)
            )
    }

    pub fn status_summary(&self, now_epoch_ms: i64) -> CloudStatusSummary {
        let linked = self
            .persistent
            .account
            .as_ref()
            .is_some_and(|account| account.tip.is_some());
        let provider = self.current_provider().ok();
        let authorization = provider
            .map(|provider| self.authorization_for_at(provider, now_epoch_ms))
            .unwrap_or_default();
        let mut facts = Vec::new();
        if let Some(provider) = provider {
            facts.push(CloudStatusFact {
                label: "Provider".to_string(),
                value: provider.label().to_string(),
            });
        }
        facts.push(CloudStatusFact {
            label: "Sync Account".to_string(),
            value: if linked { "Linked" } else { "Not linked" }.to_string(),
        });
        if let Some(principal) = authorization.principal() {
            facts.push(CloudStatusFact {
                label: "Authorized as".to_string(),
                value: principal.display_label.clone(),
            });
        }
        if let Some(account) = self.persistent.account.as_ref() {
            if let Some(tip) = account.tip.as_ref() {
                facts.push(CloudStatusFact {
                    label: "Generation".to_string(),
                    value: tip.generation.to_string(),
                });
            }
        }
        facts.push(CloudStatusFact {
            label: "Pending local records".to_string(),
            value: self.persistent.records.pending_keys.len().to_string(),
        });
        if let Some(last_success) = self.persistent.last_success_epoch_ms {
            facts.push(CloudStatusFact {
                label: "Last provider success".to_string(),
                value: format_epoch_ms_utc(last_success),
            });
        }

        if let Some(detail) = self.provider_principal_mismatch() {
            return CloudStatusSummary {
                label: "ACCOUNT".to_string(),
                severity: UiStatusSeverity::Caution,
                detail,
                facts,
            };
        }
        if linked {
            match &authorization {
                ProviderAuthorizationState::NotAuthorized
                | ProviderAuthorizationState::AuthorizationRequired { .. }
                | ProviderAuthorizationState::Denied { .. } => {
                    return CloudStatusSummary {
                        label: "AUTH".to_string(),
                        severity: UiStatusSeverity::Caution,
                        detail: authorization
                            .detail()
                            .unwrap_or("Provider authorization is required to resume syncing.")
                            .to_string(),
                        facts,
                    };
                }
                ProviderAuthorizationState::Failed { detail } => {
                    return CloudStatusSummary {
                        label: "FAILED".to_string(),
                        severity: UiStatusSeverity::Caution,
                        detail: detail.clone(),
                        facts,
                    };
                }
                ProviderAuthorizationState::Authorizing => {
                    return CloudStatusSummary {
                        label: "AUTHORIZING".to_string(),
                        severity: UiStatusSeverity::Info,
                        detail: "Provider authorization is in progress.".to_string(),
                        facts,
                    };
                }
                ProviderAuthorizationState::TransientFailure { detail, .. } => {
                    return CloudStatusSummary {
                        label: "OFFLINE".to_string(),
                        severity: UiStatusSeverity::Info,
                        detail: detail.clone(),
                        facts,
                    };
                }
                ProviderAuthorizationState::Authorized { .. } => {}
            }
            if let Some(failure) = self.persistent.last_provider_failure.as_ref() {
                if failure.kind == CloudProviderErrorKind::Transient {
                    return CloudStatusSummary {
                        label: "OFFLINE".to_string(),
                        severity: UiStatusSeverity::Info,
                        detail: failure.detail.clone(),
                        facts,
                    };
                }
            }
            return CloudStatusSummary {
                label: "OK".to_string(),
                severity: UiStatusSeverity::Ok,
                detail: if !self.persistent.records.pending_keys.is_empty() {
                    "Local changes are waiting to sync."
                } else {
                    "Sync Account is up to date."
                }
                .to_string(),
                facts,
            };
        }

        CloudStatusSummary {
            label: if matches!(authorization, ProviderAuthorizationState::Authorizing) {
                "AUTHORIZING"
            } else {
                "NOT SET UP"
            }
            .to_string(),
            severity: UiStatusSeverity::Info,
            detail: "This device is not linked to a Sync Account.".to_string(),
            facts,
        }
    }

    fn provider_card(&self, provider: CloudProviderKind, now_epoch_ms: i64) -> UiCloudPanel {
        let provider_label = provider.label();
        let authorization = self.authorization_for_at(provider, now_epoch_ms);
        if let Some(detail) = self.provider_principal_mismatch() {
            return cloud_panel(
                "provider",
                provider_label,
                UiCloudPanelState::Error,
                Some(&detail),
                vec![cloud_authorization_action(
                    CloudUiActionId::AuthorizeProvider,
                    "Authorize a different Google account",
                    provider,
                    true,
                    "",
                )],
                None,
            );
        }
        match &authorization {
            ProviderAuthorizationState::Authorized { principal, .. } => cloud_panel(
                "provider",
                provider_label,
                UiCloudPanelState::Complete,
                Some(&format!("Authorized as {}", principal.display_label)),
                Vec::new(),
                None,
            ),
            ProviderAuthorizationState::Authorizing => cloud_panel(
                "provider",
                provider_label,
                UiCloudPanelState::Working,
                Some("Authorization is in progress."),
                Vec::new(),
                None,
            ),
            ProviderAuthorizationState::TransientFailure { .. } => cloud_panel(
                "provider",
                provider_label,
                UiCloudPanelState::Working,
                authorization.detail(),
                Vec::new(),
                None,
            ),
            ProviderAuthorizationState::NotAuthorized
            | ProviderAuthorizationState::AuthorizationRequired { .. }
            | ProviderAuthorizationState::Denied { .. }
            | ProviderAuthorizationState::Failed { .. } => {
                let detail = authorization.detail().unwrap_or(
                    "Authorization is required before this device can use the provider.",
                );
                cloud_panel(
                    "provider",
                    provider_label,
                    if matches!(&authorization, ProviderAuthorizationState::Failed { .. }) {
                        UiCloudPanelState::Error
                    } else {
                        UiCloudPanelState::Active
                    },
                    Some(detail),
                    vec![cloud_authorization_action(
                        CloudUiActionId::AuthorizeProvider,
                        &format!("Authorize {provider_label}"),
                        provider,
                        provider.is_available(),
                        "This provider cannot be authorized in this build.",
                    )],
                    None,
                )
            }
        }
    }

    fn linked_summary_panel(&self, state: UiCloudPanelState) -> UiCloudPanel {
        let provider = self
            .persistent
            .account
            .as_ref()
            .expect("linked summary requires a verified Sync Account")
            .provider
            .recovery_label();
        let summary = format!(
            "Backup Advice: Your Sync Account is set up. If it is removed from this device, it can only be recovered using your Device Setup Code. {provider} cannot recover it. Use the Back up Device Setup Code button and store your Device Setup Code in a password manager or another secure place."
        );
        let actions = (state == UiCloudPanelState::Active)
            .then(|| {
                vec![
                    cloud_action(
                        CloudUiActionId::BackupSetupCode,
                        "Back up Device Setup Code",
                        true,
                        "",
                    ),
                    cloud_action(CloudUiActionId::AddDevice, "Add another device", true, ""),
                    cloud_action(CloudUiActionId::BeginUnlink, "Unlink this device", true, ""),
                ]
            })
            .unwrap_or_default();
        cloud_panel(
            "linked",
            "Sync Account linked",
            state,
            Some(&summary),
            actions,
            None,
        )
    }

    fn linked_account_detail_panel(&self, detail: LinkedAccountDetail) -> UiCloudPanel {
        match detail {
            LinkedAccountDetail::BackupCode => cloud_panel(
                "backup_code",
                "Back up Device Setup Code",
                UiCloudPanelState::Active,
                Some("Store this code in a password manager or another secure place."),
                vec![cloud_action(CloudUiActionId::CloseLinkedDetail, "Back", true, "")],
                Some(self.device_setup_code_output()),
            ),
            LinkedAccountDetail::AddDevice => cloud_panel(
                "add_device",
                "Add another device",
                UiCloudPanelState::Active,
                Some("Use this Device Setup Code to set up the other device."),
                vec![cloud_action(CloudUiActionId::CloseLinkedDetail, "Back", true, "")],
                Some(self.device_setup_code_output()),
            ),
            LinkedAccountDetail::ConfirmUnlink => cloud_panel(
                "confirm_unlink",
                "Unlink this device?",
                UiCloudPanelState::Caution,
                Some(
                    "This device's secret copy will be irretrievably deleted. Be sure you have a copy of the Device Setup Code before proceeding.",
                ),
                vec![
                    cloud_action(CloudUiActionId::CloseLinkedDetail, "Back", true, ""),
                    cloud_action(
                        CloudUiActionId::ConfirmUnlink,
                        "Yes, delete Sync Account from this device",
                        true,
                        "",
                    ),
                ],
                None,
            ),
        }
    }

    fn device_setup_code_output(&self) -> UiCloudPanelControl {
        let setup_code = self
            .device_setup_code()
            .expect("linked account detail requires a Device Setup Code");
        UiCloudPanelControl::DeviceSetupCodeOutput {
            qr_code: qr_code_for_setup_code(&setup_code),
            copy_action: cloud_effect_action(
                CloudUiActionId::CopySetupCode,
                "Copy Device Setup Code",
                true,
                "",
                CloudPlatformEffect::CopyText {
                    text: setup_code.clone(),
                    completion_label: "Copied".to_string(),
                },
            ),
            setup_code,
        }
    }

    pub fn status_record(&self, now_epoch_ms: i64) -> Option<DataStatusRecord> {
        let linked = self
            .persistent
            .account
            .as_ref()
            .is_some_and(|account| account.tip.is_some());
        if !linked {
            return None;
        }
        if let Some(detail) = self.provider_principal_mismatch() {
            return Some(DataStatusRecord::new(
                CLOUD_STATUS_ID,
                "CLOUD",
                Some("ACCOUNT".to_string()),
                UiStatusSeverity::Caution,
                true,
                detail,
            ));
        }
        let provider = self.current_provider().ok()?;
        let authorization = self.authorization_for_at(provider, now_epoch_ms);
        match &authorization {
            ProviderAuthorizationState::AuthorizationRequired { detail }
            | ProviderAuthorizationState::Denied { detail } => Some(DataStatusRecord::new(
                CLOUD_STATUS_ID,
                "CLOUD",
                Some("AUTH".to_string()),
                UiStatusSeverity::Caution,
                true,
                detail.clone(),
            )),
            ProviderAuthorizationState::Failed { detail } => Some(DataStatusRecord::new(
                CLOUD_STATUS_ID,
                "CLOUD",
                Some("FAILED".to_string()),
                UiStatusSeverity::Caution,
                true,
                detail.clone(),
            )),
            ProviderAuthorizationState::NotAuthorized => Some(DataStatusRecord::new(
                CLOUD_STATUS_ID,
                "CLOUD",
                Some("AUTH".to_string()),
                UiStatusSeverity::Caution,
                true,
                "Cloud synchronization requires provider authorization.".to_string(),
            )),
            ProviderAuthorizationState::TransientFailure { detail, .. } => {
                Some(DataStatusRecord::new(
                    CLOUD_STATUS_ID,
                    "CLOUD",
                    Some("OFFLINE".to_string()),
                    UiStatusSeverity::Info,
                    false,
                    detail.clone(),
                ))
            }
            _ if self
                .persistent
                .last_provider_failure
                .as_ref()
                .is_some_and(|failure| failure.kind == CloudProviderErrorKind::Transient) =>
            {
                let detail = self
                    .persistent
                    .last_provider_failure
                    .as_ref()
                    .map(|failure| failure.detail.clone())
                    .unwrap_or_default();
                Some(DataStatusRecord::new(
                    CLOUD_STATUS_ID,
                    "CLOUD",
                    Some("OFFLINE".to_string()),
                    UiStatusSeverity::Info,
                    false,
                    detail,
                ))
            }
            _ => None,
        }
    }

    pub fn device_setup_code(&self) -> AppResult<String> {
        let account = self.account()?;
        if account.genesis_id.is_empty() || account.tip.is_none() {
            return Err(cloud_error("cloud account creation has not completed"));
        }
        encode_device_setup_code(&DeviceSetupCodePayload {
            version: DEVICE_SETUP_CODE_VERSION,
            provider: account.provider,
            provider_account_binding: account
                .provider_account_binding
                .clone()
                .ok_or_else(|| cloud_error("provider account identity is unavailable"))?,
            genesis_id: account.genesis_id.clone(),
            root_secret_base64: account.root_secret_base64.clone(),
        })
    }
}

fn cloud_action(
    id: CloudUiActionId,
    label: &str,
    enabled: bool,
    disabled_reason: &str,
) -> UiCloudAction {
    UiCloudAction {
        id,
        label: label.to_string(),
        enabled,
        disabled_reason: (!enabled).then(|| disabled_reason.to_string()),
        required_fields: (id == CloudUiActionId::AcceptSetupCode)
            .then_some(CloudUiFieldId::DeviceSetupCode)
            .into_iter()
            .collect(),
        platform_effect: None,
    }
}

fn cloud_effect_action(
    id: CloudUiActionId,
    label: &str,
    enabled: bool,
    disabled_reason: &str,
    platform_effect: CloudPlatformEffect,
) -> UiCloudAction {
    UiCloudAction {
        platform_effect: Some(platform_effect),
        ..cloud_action(id, label, enabled, disabled_reason)
    }
}

fn qr_code_for_setup_code(setup_code: &str) -> UiQrCode {
    let code = qrcode::QrCode::new(setup_code.as_bytes())
        .expect("a Device Setup Code must fit in a QR code");
    let width = code.width();
    let colors = code.to_colors();
    let rows = colors
        .chunks(width)
        .map(|row| {
            row.iter()
                .map(|color| match color {
                    qrcode::Color::Dark => '1',
                    qrcode::Color::Light => '0',
                })
                .collect()
        })
        .collect();
    UiQrCode {
        rows,
        quiet_zone_modules: 4,
        accessibility_label: "Device Setup Code QR code".to_string(),
    }
}

fn cloud_authorization_action(
    id: CloudUiActionId,
    label: &str,
    provider: CloudProviderKind,
    enabled: bool,
    disabled_reason: &str,
) -> UiCloudAction {
    cloud_effect_action(
        id,
        label,
        enabled,
        disabled_reason,
        CloudPlatformEffect::BeginAuthorization {
            provider,
            scopes: provider_authorization_scopes(provider),
        },
    )
}

fn required_ui_field(fields: &[CloudUiFieldValue], field_id: CloudUiFieldId) -> AppResult<String> {
    let value = fields
        .iter()
        .find(|field| field.id == field_id)
        .map(|field| field.value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| cloud_error("Device Setup Code is required"))?;
    Ok(value.to_string())
}

fn provider_authorization_detail(message: &str, diagnostic: Option<&str>) -> String {
    diagnostic
        .map(str::trim)
        .filter(|diagnostic| !diagnostic.is_empty())
        .map(|diagnostic| format!("{message} {diagnostic}"))
        .unwrap_or_else(|| message.to_string())
}

fn provider_authorization_scopes(provider: CloudProviderKind) -> Vec<String> {
    match provider {
        CloudProviderKind::GoogleDrive => vec![GOOGLE_DRIVE_APPDATA_SCOPE.to_string()],
        CloudProviderKind::AerobagCloud => Vec::new(),
    }
}

fn cloud_panel(
    id: &str,
    title: &str,
    state: UiCloudPanelState,
    summary: Option<&str>,
    actions: Vec<UiCloudAction>,
    control: Option<UiCloudPanelControl>,
) -> UiCloudPanel {
    UiCloudPanel {
        id: id.to_string(),
        title: title.to_string(),
        state,
        state_label: match state {
            UiCloudPanelState::Complete => Some("Complete".to_string()),
            UiCloudPanelState::Working => Some("Working".to_string()),
            _ => None,
        },
        summary: summary.map(str::to_string),
        time_facts: Vec::new(),
        actions,
        control,
    }
}

fn principal_fingerprint(stable_id: &str) -> String {
    sha256_hex(stable_id.as_bytes())
}

fn provider_account_binding(principal: &CloudProviderPrincipal) -> ProviderAccountBinding {
    ProviderAccountBinding {
        stable_principal_fingerprint: principal_fingerprint(&principal.stable_id),
        display_hint: principal.display_label.clone(),
    }
}

fn operation_for_workflow(workflow: &CloudWorkflow) -> AppResult<CloudProviderOperation> {
    Ok(match workflow {
        CloudWorkflow::CreateAwaitIds => CloudProviderOperation::AllocateIds { count: 3 },
        CloudWorkflow::PublishAwaitIds => CloudProviderOperation::AllocateIds { count: 2 },
        CloudWorkflow::CreatePage { staged } => create_operation(
            &staged.page_id,
            staged.generation,
            "page",
            &staged.page_bytes_base64,
        ),
        CloudWorkflow::VerifyPage { staged } => CloudProviderOperation::Read {
            id: staged.page_id.clone(),
        },
        CloudWorkflow::CreateNode { staged } => create_operation(
            &staged.node_id,
            staged.generation,
            "node",
            &staged.node_bytes_base64,
        ),
        CloudWorkflow::VerifyNode { staged } => CloudProviderOperation::Read {
            id: staged.node_id.clone(),
        },
        CloudWorkflow::ReadNode { node_id, .. } => CloudProviderOperation::Read {
            id: node_id.clone(),
        },
        CloudWorkflow::ReadPage { node, .. } => CloudProviderOperation::Read {
            id: node.merkle_root_id.clone(),
        },
    })
}

fn create_operation(
    id: &str,
    generation: u64,
    role: &str,
    bytes_base64: &str,
) -> CloudProviderOperation {
    CloudProviderOperation::CreateOnce {
        id: id.to_string(),
        name: format!("aerobag-cloud-v1-{role}-{generation}"),
        bytes_base64: bytes_base64.to_string(),
    }
}

fn expect_allocated_ids(response: CloudProviderResponse, count: usize) -> AppResult<Vec<String>> {
    match response {
        CloudProviderResponse::AllocatedIds { ids }
            if ids.len() == count && ids.iter().all(|id| !id.trim().is_empty()) =>
        {
            Ok(ids)
        }
        other => Err(unexpected_response("allocate IDs", other)),
    }
}

fn expect_optional_read_bytes(response: CloudProviderResponse) -> AppResult<Option<Vec<u8>>> {
    match response {
        CloudProviderResponse::Read { bytes_base64 } => bytes_base64
            .map(|bytes| {
                URL_SAFE_NO_PAD
                    .decode(bytes)
                    .map_err(|_| cloud_error("provider returned invalid base64 object bytes"))
            })
            .transpose(),
        other => Err(unexpected_response("read object", other)),
    }
}

fn expect_read_bytes(response: CloudProviderResponse, id: &str) -> AppResult<Vec<u8>> {
    expect_optional_read_bytes(response)?
        .ok_or_else(|| cloud_error(format!("cloud object {id} is missing")))
}

fn page_for_records(records: &BTreeMap<String, CloudRecord>) -> CloudPage {
    CloudPage {
        version: CLOUD_PAGE_VERSION,
        records: records.clone(),
    }
}

fn validate_cloud_page(page: &CloudPage) -> AppResult<()> {
    if page.version != CLOUD_PAGE_VERSION {
        return Err(cloud_error(format!(
            "unsupported cloud page version {}",
            page.version
        )));
    }
    for (key, record) in &page.records {
        validate_known_record(key, record)?;
    }
    Ok(())
}

fn cloud_record_for_flight_plan(record: &StampedFlightPlan) -> AppResult<CloudRecord> {
    Ok(CloudRecord {
        schema_version: FLIGHT_PLAN_SCHEMA_VERSION,
        modified_at_epoch_ms: Some(record.modified_at_epoch_ms),
        value: serde_json::to_value(&record.plan).map_err(cloud_json_error)?,
    })
}

fn flight_plan_from_record(record: &CloudRecord) -> AppResult<StampedFlightPlan> {
    let modified_at_epoch_ms = match record.schema_version {
        1 if record.modified_at_epoch_ms.is_none() => LEGACY_UNKNOWN_MUTATION_EPOCH_MS,
        FLIGHT_PLAN_SCHEMA_VERSION => record.modified_at_epoch_ms.ok_or_else(|| {
            cloud_error("cloud flight-plan record has no user-mutation timestamp")
        })?,
        version => {
            return Err(cloud_error(format!(
                "unsupported cloud flight-plan schema {version}"
            )))
        }
    };
    let plan = serde_json::from_value(record.value.clone()).map_err(cloud_json_error)?;
    Ok(StampedFlightPlan {
        plan,
        modified_at_epoch_ms,
    })
}

fn offline_package_selection_from_record(
    record: &CloudRecord,
) -> AppResult<OfflinePackageSelection> {
    if record.schema_version != OFFLINE_PACKAGE_SELECTION_SCHEMA_VERSION {
        return Err(cloud_error(format!(
            "unsupported offline-package selection schema {}",
            record.schema_version
        )));
    }
    if record.modified_at_epoch_ms.is_none() {
        return Err(cloud_error(
            "offline-package selection has no user-mutation timestamp",
        ));
    }
    serde_json::from_value(record.value.clone()).map_err(cloud_json_error)
}

fn validate_known_record(key: &str, record: &CloudRecord) -> AppResult<()> {
    if key == FLIGHT_PLAN_RECORD_KEY {
        flight_plan_from_record(record)?;
    } else if key.starts_with(OFFLINE_PACKAGE_REGION_RECORD_PREFIX)
        || key.starts_with(OFFLINE_PACKAGE_PRODUCT_RECORD_PREFIX)
    {
        offline_package_selection_from_record(record)?;
    }
    Ok(())
}

fn compare_cloud_records(left: &CloudRecord, right: &CloudRecord) -> AppResult<Ordering> {
    let time_order = left
        .modified_at_epoch_ms
        .unwrap_or(LEGACY_UNKNOWN_MUTATION_EPOCH_MS)
        .cmp(
            &right
                .modified_at_epoch_ms
                .unwrap_or(LEGACY_UNKNOWN_MUTATION_EPOCH_MS),
        );
    if time_order != Ordering::Equal {
        return Ok(time_order);
    }
    let left_bytes =
        serde_json::to_vec(&(left.schema_version, &left.value)).map_err(cloud_json_error)?;
    let right_bytes =
        serde_json::to_vec(&(right.schema_version, &right.value)).map_err(cloud_json_error)?;
    let left_digest: [u8; 32] = Sha256::digest(left_bytes).into();
    let right_digest: [u8; 32] = Sha256::digest(right_bytes).into();
    Ok(left_digest.cmp(&right_digest))
}

fn cloud_flight_plan_definition(plan: &FlightPlan) -> FlightPlan {
    let mut plan = plan.clone();
    plan.guidance = None;
    plan
}

fn account_secret(account: &CloudAccount) -> AppResult<[u8; 32]> {
    URL_SAFE_NO_PAD
        .decode(&account.root_secret_base64)
        .map_err(|_| cloud_error("cloud root secret is not valid base64"))?
        .try_into()
        .map_err(|_| cloud_error("cloud root secret has the wrong size"))
}

fn derive_key(secret: &[u8; 32], role: &str) -> AppResult<[u8; 32]> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"aerobag-cloud-storage-v1"), secret);
    let mut key = [0_u8; 32];
    hkdf.expand(format!("payload-encryption:{role}").as_bytes(), &mut key)
        .map_err(|_| cloud_error("cloud key derivation failed"))?;
    Ok(key)
}

fn account_tag(secret: &[u8; 32]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"aerobag-cloud-account-locator-v1");
    hash.update(secret);
    hex_bytes(&hash.finalize()[..16])
}

fn envelope_aad(version: u32, account_tag: &str, role: &str) -> String {
    format!("aerobag-cloud-envelope:{version}:{account_tag}:{role}")
}

fn encode_device_setup_code(payload: &DeviceSetupCodePayload) -> AppResult<String> {
    let bytes = serde_json::to_vec(payload).map_err(cloud_json_error)?;
    let body = URL_SAFE_NO_PAD.encode(&bytes);
    let checksum = &sha256_hex(&bytes)[..16];
    Ok(format!("AB2.{body}.{checksum}"))
}

fn decode_device_setup_code(setup_code: &str) -> AppResult<DeviceSetupCodePayload> {
    let parts = setup_code.trim().split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0] != "AB2" {
        return Err(cloud_error("Device Setup Code has an unsupported format"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| cloud_error("Device Setup Code is not valid base64"))?;
    if &sha256_hex(&bytes)[..16] != parts[2] {
        return Err(cloud_error("Device Setup Code checksum does not match"));
    }
    let payload: DeviceSetupCodePayload =
        serde_json::from_slice(&bytes).map_err(cloud_json_error)?;
    if payload.version != DEVICE_SETUP_CODE_VERSION
        || payload.genesis_id.trim().is_empty()
        || URL_SAFE_NO_PAD
            .decode(&payload.root_secret_base64)
            .ok()
            .is_none_or(|bytes| bytes.len() != 32)
    {
        return Err(cloud_error("Device Setup Code content is invalid"));
    }
    Ok(payload)
}

fn random_bytes<const N: usize>() -> AppResult<[u8; N]> {
    let mut bytes = [0_u8; N];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| cloud_error(format!("secure random generation failed: {error}")))?;
    Ok(bytes)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn format_epoch_ms_utc(epoch_ms: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(epoch_ms)
        .map(|value| value.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| epoch_ms.to_string())
}

fn cloud_json_error(error: serde_json::Error) -> AppError {
    cloud_error(error.to_string())
}

fn cloud_error(message: impl Into<String>) -> AppError {
    AppError {
        kind: AppErrorKind::Internal,
        message: message.into(),
    }
}

fn unexpected_response(context: &str, response: CloudProviderResponse) -> AppError {
    cloud_error(format!(
        "unexpected provider response for {context}: {response:?}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{planning::RouteComponent, NavRef};

    #[derive(Default)]
    struct MemoryProvider {
        next_id: u64,
        objects: BTreeMap<String, String>,
    }

    impl MemoryProvider {
        fn execute(&mut self, request: &CloudProviderRequest) -> CloudProviderResponse {
            match &request.operation {
                CloudProviderOperation::AllocateIds { count } => {
                    let ids = (0..*count)
                        .map(|_| {
                            self.next_id += 1;
                            format!("object-{}", self.next_id)
                        })
                        .collect();
                    CloudProviderResponse::AllocatedIds { ids }
                }
                CloudProviderOperation::Read { id } => CloudProviderResponse::Read {
                    bytes_base64: self.objects.get(id).cloned(),
                },
                CloudProviderOperation::CreateOnce {
                    id, bytes_base64, ..
                } => {
                    if self.objects.contains_key(id) {
                        CloudProviderResponse::AlreadyExists
                    } else {
                        self.objects.insert(id.clone(), bytes_base64.clone());
                        CloudProviderResponse::Created
                    }
                }
                CloudProviderOperation::Delete { id } => CloudProviderResponse::Deleted {
                    existed: self.objects.remove(id).is_some(),
                },
                CloudProviderOperation::List { page_token } => {
                    assert!(page_token.is_none(), "test provider has one list page");
                    CloudProviderResponse::Listed {
                        objects: self
                            .objects
                            .iter()
                            .map(|(id, bytes)| CloudProviderObject {
                                id: id.clone(),
                                size_bytes: bytes.len() as u64,
                                created_at: None,
                            })
                            .collect(),
                        next_page_token: None,
                    }
                }
            }
        }
    }

    fn wire_response(
        request: &CloudProviderRequest,
        response: CloudProviderResponse,
    ) -> CloudHttpResponse {
        let (status_code, body) = match response {
            CloudProviderResponse::AllocatedIds { ids } => (
                200,
                serde_json::to_vec(&serde_json::json!({
                    "ids": ids,
                    "space": "appDataFolder",
                }))
                .unwrap(),
            ),
            CloudProviderResponse::Read {
                bytes_base64: Some(bytes),
            } => (200, URL_SAFE_NO_PAD.decode(bytes).unwrap()),
            CloudProviderResponse::Read { bytes_base64: None } => (404, Vec::new()),
            CloudProviderResponse::Created => (
                200,
                serde_json::to_vec(&serde_json::json!({
                    "id": match &request.operation {
                        CloudProviderOperation::CreateOnce { id, .. } => id,
                        _ => "created",
                    },
                }))
                .unwrap(),
            ),
            CloudProviderResponse::AlreadyExists => (409, Vec::new()),
            CloudProviderResponse::Deleted { existed } => {
                (if existed { 204 } else { 404 }, Vec::new())
            }
            CloudProviderResponse::Listed {
                objects,
                next_page_token,
            } => (
                200,
                serde_json::to_vec(&serde_json::json!({
                    "files": objects.into_iter().map(|object| serde_json::json!({
                        "id": object.id,
                        "size": object.size_bytes.to_string(),
                        "createdTime": object.created_at,
                    })).collect::<Vec<_>>(),
                    "nextPageToken": next_page_token,
                }))
                .unwrap(),
            ),
            CloudProviderResponse::Error {
                kind: CloudProviderErrorKind::Transient,
                detail,
            } => return CloudHttpResponse::TransportError { detail },
            CloudProviderResponse::Error {
                kind: CloudProviderErrorKind::Unauthorized,
                detail,
            } => (401, detail.into_bytes()),
            CloudProviderResponse::Error {
                kind: CloudProviderErrorKind::Permanent,
                detail,
            } => (400, detail.into_bytes()),
        };
        CloudHttpResponse::Completed {
            status_code,
            body_base64: URL_SAFE_NO_PAD.encode(body),
        }
    }

    fn pump(engine: &mut CloudEngine, provider: &mut MemoryProvider, now: i64) -> Vec<FlightPlan> {
        let mut remote_plans = Vec::new();
        for _ in 0..32 {
            let Some(request) = engine.take_provider_request(now).unwrap() else {
                return remote_plans;
            };
            let semantic_request = engine
                .provider_request_in_flight
                .as_ref()
                .expect("semantic provider request")
                .clone();
            let response = wire_response(&semantic_request, provider.execute(&semantic_request));
            let completion = engine
                .complete_provider_request(request.request_id, response, now)
                .unwrap();
            remote_plans.extend(completion.remote_flight_plan().unwrap());
        }
        panic!("cloud test pump did not quiesce");
    }

    fn plan(idents: &[&str]) -> FlightPlan {
        FlightPlan {
            route_components: idents
                .iter()
                .map(|ident| RouteComponent::Waypoint {
                    waypoint: NavRef::Airport((*ident).to_string()),
                })
                .collect(),
            ..FlightPlan::default()
        }
        .normalized()
    }

    fn ready(engine: &mut CloudEngine) {
        authorize_as(engine, "google-principal-1", "pilot@example.com");
    }

    fn authorize_as(engine: &mut CloudEngine, stable_id: &str, display_label: &str) {
        engine.set_authorization_state(
            CloudProviderKind::GoogleDrive,
            ProviderAuthorizationState::Authorized {
                expires_at_epoch_ms: Some(100_000),
                principal: CloudProviderPrincipal {
                    stable_id: stable_id.to_string(),
                    display_label: display_label.to_string(),
                },
            },
        );
    }

    fn active_panel(engine: &CloudEngine) -> UiCloudPanel {
        engine
            .page_state(0)
            .sync_account_panels
            .into_iter()
            .find(|panel| panel.state != UiCloudPanelState::Complete)
            .expect("active Cloud panel")
    }

    fn provider_card(engine: &CloudEngine) -> UiCloudPanel {
        engine
            .page_state(0)
            .provider_card
            .expect("Cloud provider card")
    }

    fn complete(
        engine: &mut CloudEngine,
        now: i64,
        response: CloudProviderResponse,
    ) -> CloudCompletion {
        let request = engine
            .take_provider_request(now)
            .unwrap()
            .expect("provider request");
        let semantic_request = engine
            .provider_request_in_flight
            .as_ref()
            .expect("semantic provider request")
            .clone();
        engine
            .complete_provider_request(
                request.request_id,
                wire_response(&semantic_request, response),
                now,
            )
            .unwrap()
    }

    fn create_account(engine: &mut CloudEngine, initial: &FlightPlan) -> String {
        engine
            .perform_action(CloudAction::BeginCreateAccount, initial)
            .unwrap();
        engine
            .perform_action(
                CloudAction::SelectProvider {
                    provider: CloudProviderKind::GoogleDrive,
                },
                initial,
            )
            .unwrap();
        ready(engine);
        engine
            .perform_action(CloudAction::CreateAccount, initial)
            .unwrap();
        complete(
            engine,
            1,
            CloudProviderResponse::AllocatedIds {
                ids: vec!["genesis".into(), "page0".into(), "slot1".into()],
            },
        );
        complete(engine, 2, CloudProviderResponse::Created);
        complete(engine, 3, CloudProviderResponse::Created);
        engine.device_setup_code().unwrap()
    }

    #[test]
    fn account_creation_emits_encrypted_create_once_chain_and_device_setup_code() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        let setup_code = create_account(&mut engine, &plan(&["KRNT", "KPAE"]));
        assert!(setup_code.starts_with("AB2."));
        assert_eq!(
            engine
                .persistent
                .account
                .as_ref()
                .unwrap()
                .tip
                .as_ref()
                .unwrap()
                .generation,
            0
        );
        assert!(engine.persistent.records.pending_keys.is_empty());
        assert!(!setup_code.contains("KRNT"));
    }

    #[test]
    fn new_user_flow_reveals_exactly_one_next_step_at_a_time() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        assert_eq!(active_panel(&engine).id, "get_started");

        engine
            .perform_action(CloudAction::BeginCreateAccount, &initial)
            .unwrap();
        assert_eq!(active_panel(&engine).id, "provider");

        engine
            .perform_action(
                CloudAction::SelectProvider {
                    provider: CloudProviderKind::GoogleDrive,
                },
                &initial,
            )
            .unwrap();
        assert_eq!(active_panel(&engine).id, "create_account");
        assert!(
            !active_panel(&engine)
                .actions
                .iter()
                .find(|action| action.id == CloudUiActionId::CreateAccount)
                .unwrap()
                .enabled
        );
        assert_eq!(provider_card(&engine).state, UiCloudPanelState::Active);

        ready(&mut engine);
        assert_eq!(active_panel(&engine).id, "create_account");

        engine
            .perform_action(CloudAction::CreateAccount, &initial)
            .unwrap();
        assert_eq!(active_panel(&engine).id, "create_account");
        assert_eq!(active_panel(&engine).state, UiCloudPanelState::Working);
        assert!(engine.persistent.account.is_some());
        assert!(engine.persistent.selected_provider.is_none());
    }

    #[test]
    fn ui_action_ids_are_interpreted_and_validated_only_by_core() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        engine
            .perform_ui_action(CloudUiActionId::BeginCreate, &[], &initial, 1)
            .unwrap();
        engine
            .perform_ui_action(CloudUiActionId::SelectProviderGoogleDrive, &[], &initial, 1)
            .unwrap();

        let authorize = engine
            .page_state(1)
            .provider_card
            .unwrap()
            .actions
            .into_iter()
            .find(|action| action.id == CloudUiActionId::AuthorizeProvider)
            .unwrap();
        assert_eq!(
            authorize.platform_effect,
            Some(CloudPlatformEffect::BeginAuthorization {
                provider: CloudProviderKind::GoogleDrive,
                scopes: vec![GOOGLE_DRIVE_APPDATA_SCOPE.to_string()],
            })
        );
        engine
            .perform_ui_action(CloudUiActionId::AuthorizeProvider, &[], &initial, 1)
            .unwrap();
        assert_eq!(
            engine.authorization_for(CloudProviderKind::GoogleDrive),
            ProviderAuthorizationState::Authorizing
        );
        let authorization_request = engine.take_authorization_request(1).unwrap().unwrap();
        assert_eq!(
            authorization_request.mode,
            CloudAuthorizationMode::Interactive
        );
        assert_eq!(
            authorization_request.scopes,
            vec![GOOGLE_DRIVE_APPDATA_SCOPE]
        );

        let mut receiver = CloudEngine::new(CloudPersistentState::default());
        receiver
            .perform_ui_action(CloudUiActionId::BeginSetup, &[], &initial, 1)
            .unwrap();
        let error = receiver
            .perform_ui_action(
                CloudUiActionId::AcceptSetupCode,
                &[CloudUiFieldValue {
                    id: CloudUiFieldId::DeviceSetupCode,
                    value: "   ".to_string(),
                }],
                &initial,
                1,
            )
            .unwrap_err();
        assert!(error.message.contains("Device Setup Code is required"));
    }

    #[test]
    fn qr_scanner_capability_enables_a_typed_setup_completion_effect() {
        let initial = plan(&[]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        engine
            .perform_ui_action(CloudUiActionId::BeginSetup, &[], &initial, 0)
            .unwrap();

        let scan_action = |page: UiCloudPageState| {
            page.sync_account_panels
                .into_iter()
                .find(|panel| panel.id == "receive_setup")
                .unwrap()
                .actions
                .into_iter()
                .find(|action| action.id == CloudUiActionId::ScanSetupCode)
                .unwrap()
        };
        assert!(!scan_action(engine.page_state(0)).enabled);

        let available = scan_action(engine.page_state_with_qr_scanner(0, true));
        assert!(available.enabled);
        assert_eq!(
            available.platform_effect,
            Some(CloudPlatformEffect::ScanQrCode {
                completion_action: CloudUiActionId::AcceptSetupCode,
                field_id: CloudUiFieldId::DeviceSetupCode,
            })
        );
        engine
            .perform_ui_action(CloudUiActionId::ScanSetupCode, &[], &initial, 0)
            .unwrap();
        assert!(engine.persistent.account.is_none());
    }

    #[test]
    fn authorization_expiry_is_projected_by_core_clock() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        create_account(&mut engine, &initial);

        let before = engine.page_state(99_999);
        assert_eq!(before.next_refresh_epoch_ms, Some(100_000));
        assert_eq!(before.overall_status.state, UiCloudPanelState::Complete);

        let expired = engine.page_state(100_000);
        assert_eq!(expired.next_refresh_epoch_ms, None);
        assert_eq!(expired.overall_status.state, UiCloudPanelState::Caution);
        let provider = expired.provider_card.unwrap();
        assert_eq!(provider.state, UiCloudPanelState::Active);
        assert!(provider
            .summary
            .as_deref()
            .unwrap()
            .contains("authorization expired"));
        assert_eq!(engine.status_summary(100_000).label, "AUTH");

        let mut create_engine = CloudEngine::new(CloudPersistentState::default());
        create_engine
            .perform_ui_action(CloudUiActionId::BeginCreate, &[], &initial, 1)
            .unwrap();
        create_engine
            .perform_ui_action(CloudUiActionId::SelectProviderGoogleDrive, &[], &initial, 1)
            .unwrap();
        ready(&mut create_engine);
        let error = create_engine
            .perform_ui_action(CloudUiActionId::CreateAccount, &[], &initial, 100_000)
            .unwrap_err();
        assert!(error
            .message
            .contains("authorize the provider before creating a Sync Account"));
    }

    #[test]
    fn unavailable_provider_cannot_be_selected_by_bypassing_the_ui() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        engine
            .perform_action(CloudAction::BeginCreateAccount, &initial)
            .unwrap();

        let error = engine
            .perform_action(
                CloudAction::SelectProvider {
                    provider: CloudProviderKind::AerobagCloud,
                },
                &initial,
            )
            .unwrap_err();

        assert!(error.message.contains("not available"));
        assert!(engine.persistent.selected_provider.is_none());
        assert_eq!(active_panel(&engine).id, "provider");
    }

    #[test]
    fn backing_out_of_setup_preserves_unexpired_provider_authorization() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        engine
            .perform_action(CloudAction::BeginCreateAccount, &initial)
            .unwrap();
        engine
            .perform_action(
                CloudAction::SelectProvider {
                    provider: CloudProviderKind::GoogleDrive,
                },
                &initial,
            )
            .unwrap();
        ready(&mut engine);
        assert_eq!(active_panel(&engine).id, "create_account");

        engine
            .perform_action(CloudAction::BackSetup, &initial)
            .unwrap();
        assert_eq!(active_panel(&engine).id, "provider");
        engine
            .perform_action(
                CloudAction::SelectProvider {
                    provider: CloudProviderKind::GoogleDrive,
                },
                &initial,
            )
            .unwrap();

        assert_eq!(active_panel(&engine).id, "create_account");
    }

    #[test]
    fn received_setup_code_selects_provider_then_rejects_wrong_provider_identity() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut source = CloudEngine::new(CloudPersistentState::default());
        let setup_code = create_account(&mut source, &initial);

        let mut target = CloudEngine::new(CloudPersistentState::default());
        target
            .perform_action(CloudAction::BeginSetupFromDevice, &FlightPlan::default())
            .unwrap();
        assert_eq!(active_panel(&target).id, "receive_setup");
        target
            .perform_action(
                CloudAction::AcceptDeviceSetupCode { setup_code },
                &FlightPlan::default(),
            )
            .unwrap();
        assert_eq!(
            target.current_provider().unwrap(),
            CloudProviderKind::GoogleDrive
        );
        assert!(target.persistent.selected_provider.is_none());
        assert_eq!(active_panel(&target).id, "link_account");
        assert_eq!(provider_card(&target).state, UiCloudPanelState::Active);

        authorize_as(&mut target, "wrong-google-principal", "wrong@example.com");
        let mismatch = provider_card(&target);
        assert_eq!(mismatch.id, "provider");
        assert_eq!(mismatch.state, UiCloudPanelState::Error);
        assert!(mismatch
            .summary
            .as_deref()
            .is_some_and(|summary| summary.contains("pilot@example.com")
                && summary.contains("wrong@example.com")));
        assert!(target.take_provider_request(10).unwrap().is_none());
    }

    #[test]
    fn setup_back_unwinds_received_account_before_leaving_the_receive_branch() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut source = CloudEngine::new(CloudPersistentState::default());
        let setup_code = create_account(&mut source, &initial);
        let mut target = CloudEngine::new(CloudPersistentState::default());
        target
            .perform_action(CloudAction::BeginSetupFromDevice, &FlightPlan::default())
            .unwrap();
        target
            .perform_action(
                CloudAction::AcceptDeviceSetupCode { setup_code },
                &FlightPlan::default(),
            )
            .unwrap();
        assert_eq!(active_panel(&target).id, "link_account");

        target
            .perform_action(CloudAction::BackSetup, &FlightPlan::default())
            .unwrap();
        assert!(target.persistent.account.is_none());
        assert_eq!(active_panel(&target).id, "receive_setup");

        target
            .perform_action(CloudAction::BackSetup, &FlightPlan::default())
            .unwrap();
        assert_eq!(active_panel(&target).id, "get_started");
    }

    #[test]
    fn authorization_failure_drives_caution_but_network_failure_does_not() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        create_account(&mut engine, &plan(&["KRNT", "KPAE"]));
        engine.set_authorization_state(
            CloudProviderKind::GoogleDrive,
            ProviderAuthorizationState::AuthorizationRequired {
                detail: "Authorize Google Drive again.".into(),
            },
        );
        let auth = engine.status_record(0).unwrap();
        assert_eq!(auth.severity, UiStatusSeverity::Caution);
        assert!(auth.drives_caution);

        ready(&mut engine);
        engine
            .perform_action(CloudAction::SyncNow, &FlightPlan::default())
            .unwrap();
        let request = engine.take_provider_request(50).unwrap().unwrap();
        engine
            .complete_provider_request(
                request.request_id,
                CloudHttpResponse::TransportError {
                    detail: "Network is unavailable.".into(),
                },
                50,
            )
            .unwrap();
        let network = engine.status_record(0).unwrap();
        assert_eq!(network.severity, UiStatusSeverity::Info);
        assert!(!network.drives_caution);
    }

    #[test]
    fn overall_status_distinguishes_account_and_provider_readiness() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        let unlinked = engine.page_state(0).overall_status;
        assert_eq!(unlinked.title, "Cloud not active");
        assert_eq!(unlinked.state, UiCloudPanelState::Informational);
        assert_eq!(
            unlinked.summary.as_deref(),
            Some("No Sync Account linked yet.")
        );

        create_account(&mut engine, &initial);
        let active = engine.page_state(0).overall_status;
        assert_eq!(active.title, "Cloud active");
        assert_eq!(active.state, UiCloudPanelState::Complete);
        assert_eq!(
            active.summary.as_deref(),
            Some("Sync Account linked, provider connected.")
        );

        engine.set_authorization_state(
            CloudProviderKind::GoogleDrive,
            ProviderAuthorizationState::AuthorizationRequired {
                detail: "Authorize Google Drive again.".into(),
            },
        );
        let authorization_required = engine.page_state(0).overall_status;
        assert_eq!(authorization_required.title, "Cloud not active");
        assert_eq!(authorization_required.state, UiCloudPanelState::Caution);
        assert_eq!(
            authorization_required.summary.as_deref(),
            Some("Sync Account linked, but provider requires authorization.")
        );
    }

    #[test]
    fn linked_account_can_be_unlinked_while_authorization_is_absent() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        create_account(&mut engine, &initial);
        engine.set_authorization_state(
            CloudProviderKind::GoogleDrive,
            ProviderAuthorizationState::NotAuthorized,
        );

        let page = engine.page_state(0);
        let provider = page.provider_card.unwrap();
        assert_eq!(provider.id, "provider");
        assert!(!provider
            .actions
            .iter()
            .any(|action| action.id == CloudUiActionId::BeginUnlink && action.enabled));
        let linked = page
            .sync_account_panels
            .iter()
            .find(|panel| panel.id == "linked")
            .unwrap();
        assert!(linked
            .summary
            .as_deref()
            .unwrap()
            .contains("Your Sync Account is set up"));
        assert_eq!(
            linked
                .actions
                .iter()
                .map(|action| action.id)
                .collect::<Vec<_>>(),
            vec![
                CloudUiActionId::BackupSetupCode,
                CloudUiActionId::AddDevice,
                CloudUiActionId::BeginUnlink,
            ]
        );

        engine
            .perform_action(CloudAction::BeginUnlinkDevice, &initial)
            .unwrap();
        let confirmation = active_panel(&engine);
        assert_eq!(confirmation.id, "confirm_unlink");
        assert_eq!(confirmation.state, UiCloudPanelState::Caution);
        assert!(confirmation
            .summary
            .as_deref()
            .unwrap()
            .contains("irretrievably deleted"));
        assert!(engine.persistent.account.is_some());

        engine
            .perform_action(CloudAction::CloseLinkedAccountDetail, &initial)
            .unwrap();
        assert!(engine.persistent.account.is_some());
        engine
            .perform_action(CloudAction::BeginUnlinkDevice, &initial)
            .unwrap();
        engine
            .perform_action(CloudAction::ConfirmUnlinkDevice, &initial)
            .unwrap();
        assert_eq!(active_panel(&engine).id, "get_started");
        assert!(engine.persistent.account.is_none());
    }

    #[test]
    fn backup_and_add_device_reveal_the_same_code_for_distinct_intents() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        create_account(&mut engine, &initial);

        engine
            .perform_action(CloudAction::BackUpDeviceSetupCode, &initial)
            .unwrap();
        let backup = active_panel(&engine);
        assert_eq!(backup.id, "backup_code");
        assert!(engine
            .page_state(0)
            .sync_account_panels
            .iter()
            .find(|panel| panel.id == "linked")
            .unwrap()
            .actions
            .is_empty());
        let backup_code = match backup.control.unwrap() {
            UiCloudPanelControl::DeviceSetupCodeOutput {
                setup_code,
                qr_code,
                ..
            } => {
                assert!(!qr_code.rows.is_empty());
                assert_eq!(qr_code.rows.len(), qr_code.rows[0].len());
                assert!(qr_code
                    .rows
                    .iter()
                    .all(|row| row.len() == qr_code.rows.len()
                        && row.bytes().all(|value| matches!(value, b'0' | b'1'))));
                assert_eq!(qr_code.quiet_zone_modules, 4);
                setup_code
            }
            control => panic!("unexpected backup control: {control:?}"),
        };

        engine
            .perform_action(CloudAction::CloseLinkedAccountDetail, &initial)
            .unwrap();
        engine
            .perform_action(CloudAction::AddAnotherDevice, &initial)
            .unwrap();
        let add_device = active_panel(&engine);
        assert_eq!(add_device.id, "add_device");
        let add_device_code = match add_device.control.unwrap() {
            UiCloudPanelControl::DeviceSetupCodeOutput { setup_code, .. } => setup_code,
            control => panic!("unexpected add-device control: {control:?}"),
        };
        assert_eq!(backup_code, add_device_code);
    }

    #[test]
    fn unlinking_sync_account_does_not_log_out_of_provider() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        create_account(&mut engine, &initial);
        engine
            .perform_action(CloudAction::BeginUnlinkDevice, &initial)
            .unwrap();
        engine
            .perform_action(CloudAction::ConfirmUnlinkDevice, &initial)
            .unwrap();
        engine
            .perform_action(CloudAction::BeginCreateAccount, &initial)
            .unwrap();
        engine
            .perform_action(
                CloudAction::SelectProvider {
                    provider: CloudProviderKind::GoogleDrive,
                },
                &initial,
            )
            .unwrap();
        assert_eq!(active_panel(&engine).id, "create_account");
    }

    #[test]
    fn local_plan_mutation_becomes_durable_outbox_work() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        let initial = plan(&["KRNT", "KPAE"]);
        create_account(&mut engine, &initial);
        engine
            .record_local_flight_plan_mutation(&initial, &plan(&["KRNT", "KSEA", "KPAE"]), 9)
            .unwrap();
        let _request = engine.take_provider_request(10).unwrap().unwrap();
        let semantic_request = engine
            .provider_request_in_flight
            .as_ref()
            .expect("semantic provider request");
        assert_eq!(
            semantic_request.operation,
            CloudProviderOperation::AllocateIds { count: 2 }
        );
        assert!(engine
            .persistent
            .records
            .pending_keys
            .contains(FLIGHT_PLAN_RECORD_KEY));
    }

    #[test]
    fn device_setup_code_checksum_rejects_typo() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        let mut setup_code = create_account(&mut engine, &plan(&["KRNT", "KPAE"]));
        setup_code.push('x');
        assert!(decode_device_setup_code(&setup_code).is_err());
    }

    #[test]
    fn two_independent_clients_crossfill_flight_plan_through_provider_protocol() {
        let initial = plan(&["KRNT", "KPAE"]);
        let changed = plan(&["KRNT", "KSEA", "KPAE"]);
        let mut provider = MemoryProvider::default();
        let mut first = CloudEngine::new(CloudPersistentState::default());
        first
            .perform_action(CloudAction::BeginCreateAccount, &initial)
            .unwrap();
        first
            .perform_action(
                CloudAction::SelectProvider {
                    provider: CloudProviderKind::GoogleDrive,
                },
                &initial,
            )
            .unwrap();
        ready(&mut first);
        first
            .perform_action(CloudAction::CreateAccount, &initial)
            .unwrap();
        assert!(pump(&mut first, &mut provider, 10).is_empty());
        assert_eq!(first.persistent.last_write_epoch_ms, Some(10));
        assert_eq!(first.tip().unwrap().published_at_epoch_ms, Some(10));
        let setup_code = first.device_setup_code().unwrap();

        let mut second = CloudEngine::new(CloudPersistentState::default());
        ready(&mut second);
        second
            .perform_action(
                CloudAction::AcceptDeviceSetupCode { setup_code },
                &FlightPlan::default(),
            )
            .unwrap();
        assert_eq!(pump(&mut second, &mut provider, 20), vec![initial.clone()]);
        assert_eq!(second.persistent.last_read_epoch_ms, Some(20));
        assert_eq!(second.persistent.last_write_epoch_ms, None);
        assert_eq!(second.tip().unwrap().published_at_epoch_ms, Some(10));

        first
            .record_local_flight_plan_mutation(&initial, &changed, 29)
            .unwrap();
        assert!(pump(&mut first, &mut provider, 30).is_empty());
        assert_eq!(first.persistent.last_write_epoch_ms, Some(30));
        assert_eq!(first.tip().unwrap().published_at_epoch_ms, Some(30));
        assert_eq!(
            first.page_state(30).overall_status.time_facts,
            vec![
                UiCloudTimeFact {
                    label: "Last read from cloud".to_string(),
                    epoch_ms: 30,
                },
                UiCloudTimeFact {
                    label: "Last update on cloud".to_string(),
                    epoch_ms: 30,
                },
                UiCloudTimeFact {
                    label: "Last write to cloud from this device".to_string(),
                    epoch_ms: 30,
                },
            ]
        );
        second
            .perform_action(CloudAction::SyncNow, &FlightPlan::default())
            .unwrap();
        assert_eq!(pump(&mut second, &mut provider, 40), vec![changed]);
        assert_eq!(second.persistent.last_read_epoch_ms, Some(40));
        assert_eq!(second.persistent.last_write_epoch_ms, None);
        assert_eq!(second.tip().unwrap().published_at_epoch_ms, Some(30));
        assert_eq!(
            second.page_state(40).overall_status.time_facts,
            vec![
                UiCloudTimeFact {
                    label: "Last read from cloud".to_string(),
                    epoch_ms: 40,
                },
                UiCloudTimeFact {
                    label: "Last update on cloud".to_string(),
                    epoch_ms: 30,
                },
            ]
        );
    }

    #[test]
    fn independent_offline_package_edits_merge_without_clobbering() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut provider = MemoryProvider::default();
        let mut first = CloudEngine::new(CloudPersistentState::default());
        first
            .perform_action(CloudAction::BeginCreateAccount, &initial)
            .unwrap();
        first
            .perform_action(
                CloudAction::SelectProvider {
                    provider: CloudProviderKind::GoogleDrive,
                },
                &initial,
            )
            .unwrap();
        ready(&mut first);
        first
            .perform_action(CloudAction::CreateAccount, &initial)
            .unwrap();
        assert!(pump(&mut first, &mut provider, 10).is_empty());

        let mut second = CloudEngine::new(CloudPersistentState::default());
        ready(&mut second);
        second
            .perform_action(
                CloudAction::AcceptDeviceSetupCode {
                    setup_code: first.device_setup_code().unwrap(),
                },
                &FlightPlan::default(),
            )
            .unwrap();
        assert_eq!(pump(&mut second, &mut provider, 20), vec![initial]);

        first
            .record_local_offline_package_preferences(
                &OfflinePackagePreferences {
                    regions: BTreeMap::from([(
                        "nw".to_string(),
                        OfflinePackageSelection::Unselected,
                    )]),
                    products: BTreeMap::new(),
                },
                100,
            )
            .unwrap();
        second
            .record_local_offline_package_preferences(
                &OfflinePackagePreferences {
                    regions: BTreeMap::new(),
                    products: BTreeMap::from([(
                        "terrain".to_string(),
                        OfflinePackageSelection::Pause,
                    )]),
                },
                110,
            )
            .unwrap();

        assert!(pump(&mut first, &mut provider, 120).is_empty());
        assert!(pump(&mut second, &mut provider, 130).is_empty());
        first
            .perform_action(CloudAction::SyncNow, &FlightPlan::default())
            .unwrap();
        assert!(pump(&mut first, &mut provider, 140).is_empty());

        let expected = OfflinePackagePreferences {
            regions: BTreeMap::from([("nw".to_string(), OfflinePackageSelection::Unselected)]),
            products: BTreeMap::from([("terrain".to_string(), OfflinePackageSelection::Pause)]),
        };
        assert_eq!(first.offline_package_preferences().unwrap(), expected);
        assert_eq!(second.offline_package_preferences().unwrap(), expected);
    }

    #[test]
    fn older_catalog_cannot_erase_unknown_package_dimensions() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        engine
            .record_local_offline_package_preferences(
                &OfflinePackagePreferences {
                    regions: BTreeMap::from([
                        ("nw".to_string(), OfflinePackageSelection::Play),
                        ("future-region".to_string(), OfflinePackageSelection::Pause),
                    ]),
                    products: BTreeMap::new(),
                },
                100,
            )
            .unwrap();
        engine
            .record_local_offline_package_preferences(
                &OfflinePackagePreferences {
                    regions: BTreeMap::from([(
                        "nw".to_string(),
                        OfflinePackageSelection::Unselected,
                    )]),
                    products: BTreeMap::new(),
                },
                200,
            )
            .unwrap();

        assert_eq!(
            engine.offline_package_preferences().unwrap().regions,
            BTreeMap::from([
                ("nw".to_string(), OfflinePackageSelection::Unselected),
                ("future-region".to_string(), OfflinePackageSelection::Pause,),
            ])
        );
    }

    #[test]
    fn delayed_authentication_cannot_publish_an_older_user_edit_over_a_newer_one() {
        let initial = plan(&["KRNT", "KPAE"]);
        let older_edit = plan(&["KRNT", "KSEA", "KPAE"]);
        let newer_edit = plan(&["KRNT", "KSUS", "KPAE"]);
        let mut provider = MemoryProvider::default();

        let mut delayed = CloudEngine::new(CloudPersistentState::default());
        delayed
            .perform_action(CloudAction::BeginCreateAccount, &initial)
            .unwrap();
        delayed
            .perform_action(
                CloudAction::SelectProvider {
                    provider: CloudProviderKind::GoogleDrive,
                },
                &initial,
            )
            .unwrap();
        ready(&mut delayed);
        delayed
            .perform_action_at(CloudAction::CreateAccount, &initial, 10)
            .unwrap();
        assert!(pump(&mut delayed, &mut provider, 10).is_empty());
        let setup_code = delayed.device_setup_code().unwrap();

        let mut current = CloudEngine::new(CloudPersistentState::default());
        ready(&mut current);
        current
            .perform_action(
                CloudAction::AcceptDeviceSetupCode { setup_code },
                &FlightPlan::default(),
            )
            .unwrap();
        assert_eq!(pump(&mut current, &mut provider, 20), vec![initial.clone()]);

        delayed
            .record_local_flight_plan_mutation(&initial, &older_edit, 100)
            .unwrap();
        let delayed_persistent: CloudPersistentState = serde_json::from_slice(
            &serde_json::to_vec(delayed.persistent()).expect("serialize delayed outbox"),
        )
        .expect("restore delayed outbox");
        delayed = CloudEngine::new(delayed_persistent);
        ready(&mut delayed);
        current
            .record_local_flight_plan_mutation(&initial, &newer_edit, 200)
            .unwrap();
        assert!(pump(&mut current, &mut provider, 210).is_empty());

        assert_eq!(
            pump(&mut delayed, &mut provider, 300),
            vec![newer_edit.clone()]
        );
        assert!(delayed.persistent.records.pending_keys.is_empty());
        assert_eq!(delayed.cached_flight_plan(), Some(newer_edit));
        assert_eq!(delayed.tip().unwrap().generation, 1);
    }

    #[test]
    fn delayed_publication_preserves_the_original_user_mutation_time() {
        let initial = plan(&["KRNT", "KPAE"]);
        let earlier_edit = plan(&["KRNT", "KSEA", "KPAE"]);
        let later_edit = plan(&["KRNT", "KSUS", "KPAE"]);
        let mut provider = MemoryProvider::default();

        let mut later = CloudEngine::new(CloudPersistentState::default());
        later
            .perform_action(CloudAction::BeginCreateAccount, &initial)
            .unwrap();
        later
            .perform_action(
                CloudAction::SelectProvider {
                    provider: CloudProviderKind::GoogleDrive,
                },
                &initial,
            )
            .unwrap();
        ready(&mut later);
        later
            .perform_action_at(CloudAction::CreateAccount, &initial, 10)
            .unwrap();
        assert!(pump(&mut later, &mut provider, 10).is_empty());
        let setup_code = later.device_setup_code().unwrap();

        let mut earlier = CloudEngine::new(CloudPersistentState::default());
        ready(&mut earlier);
        earlier
            .perform_action(
                CloudAction::AcceptDeviceSetupCode { setup_code },
                &FlightPlan::default(),
            )
            .unwrap();
        assert_eq!(pump(&mut earlier, &mut provider, 20), vec![initial.clone()]);

        earlier
            .record_local_flight_plan_mutation(&initial, &earlier_edit, 200)
            .unwrap();
        assert!(pump(&mut earlier, &mut provider, 210).is_empty());
        later
            .record_local_flight_plan_mutation(&initial, &later_edit, 300)
            .unwrap();
        assert_eq!(
            later
                .persistent
                .records
                .cached
                .get(FLIGHT_PLAN_RECORD_KEY)
                .and_then(|record| record.modified_at_epoch_ms),
            Some(300)
        );
        assert!(pump(&mut later, &mut provider, 10_000).is_empty());

        earlier
            .perform_action(CloudAction::SyncNow, &earlier_edit)
            .unwrap();
        assert_eq!(pump(&mut earlier, &mut provider, 10_100), vec![later_edit]);
    }

    #[test]
    fn cloud_nodes_written_before_publication_timestamps_remain_readable() {
        let node: CloudNode = serde_json::from_value(serde_json::json!({
            "version": CLOUD_NODE_VERSION,
            "generation": 3,
            "parent_node_id": "parent",
            "parent_node_hash": "parent-hash",
            "merkle_root_id": "page",
            "merkle_root_hash": "page-hash",
            "next_slot_id": "next"
        }))
        .unwrap();

        assert_eq!(node.published_at_epoch_ms, None);
    }

    #[test]
    fn version_three_flight_plan_outbox_migrates_into_generic_records() {
        let plan = plan(&["KRNT", "KPAE"]);
        let stamped = StampedFlightPlan {
            plan: plan.clone(),
            modified_at_epoch_ms: 123,
        };
        let mut wire = serde_json::to_value(CloudPersistentState::default()).unwrap();
        let object = wire.as_object_mut().unwrap();
        object.insert("version".to_string(), serde_json::json!(3));
        object.remove("records");
        object.insert(
            "cached_flight_plan".to_string(),
            serde_json::to_value(&stamped).unwrap(),
        );
        object.insert(
            "pending_flight_plan".to_string(),
            serde_json::to_value(&stamped).unwrap(),
        );

        let engine = CloudEngine::new(serde_json::from_value(wire).unwrap());
        assert_eq!(engine.cached_flight_plan(), Some(plan));
        assert!(engine
            .persistent
            .records
            .pending_keys
            .contains(FLIGHT_PLAN_RECORD_KEY));
        let migrated = serde_json::to_value(engine.persistent()).unwrap();
        assert!(migrated.get("records").is_some());
        assert!(migrated.get("cached_flight_plan").is_none());
        assert!(migrated.get("pending_flight_plan").is_none());
    }

    #[test]
    fn cloud_page_does_not_require_a_flight_plan_record() {
        let page = CloudPage {
            version: CLOUD_PAGE_VERSION,
            records: BTreeMap::from([(
                "offline_packages/product/terrain".to_string(),
                CloudRecord {
                    schema_version: OFFLINE_PACKAGE_SELECTION_SCHEMA_VERSION,
                    modified_at_epoch_ms: Some(100),
                    value: serde_json::to_value(OfflinePackageSelection::Play).unwrap(),
                },
            )]),
        };

        validate_cloud_page(&page).unwrap();
        assert_eq!(page_for_records(&page.records), page);
    }
}
#[test]
fn unconfigured_engine_has_no_provider_work() {
    let mut engine = CloudEngine::new(CloudPersistentState::default());

    assert!(engine.take_provider_request(0).unwrap().is_none());
}
