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
use product_contracts::{
    acs_encrypted_value_associated_data, AcsCompareAndSwapRootRequest,
    AcsCompareAndSwapRootResponse, AcsCreateAccountRequest, AcsCreateAccountResponse,
    AcsCreateObjectRequest, AcsCreateSseTicketRequest, AcsCreateSseTicketResponse,
    AcsCreationChallengeResponse, AcsEncryptedValue, AcsEncryptedValueKind, AcsObjectSnapshot,
    AcsRateLimitGate, AcsRootSnapshot, AcsSseEvent, ACS_FIXED_ROOT_ID,
    ACS_KDF_PAYLOAD_ENCRYPTION_LABEL, ACS_KDF_SALT, AEROBAG_SSE_TRANSPORT_POLICY,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use app_ui_contracts::cloud::{
    CloudEventStreamEvent, CloudEventStreamEventKind, CloudEventStreamPlan, CloudHttpHeader,
    CloudHttpMethod, CloudHttpRequest, CloudHttpResponse, CloudPlatformEffect, CloudProviderKind,
    CloudUiActionId, CloudUiFieldId, CloudUiFieldValue, UiCloudAction, UiCloudPageState,
    UiCloudPanel, UiCloudPanelControl, UiCloudPanelState, UiCloudTimeFact, UiQrCode,
};

use crate::{
    data_status::{DataStatusRecord, UiStatusSeverity},
    device_setup_code::{
        decode_device_setup_code, encode_device_setup_code, DeviceSetupCode, DeviceSetupProvider,
    },
    settings_controller::{all_debug_flags, debug_flag_from_id, debug_flag_id},
    AppError, AppErrorKind, AppResult, DebugFlagId, FlightPlan, InactivitySleepTimeout,
    NexradAcquisitionPreferences, OfflinePackagePreferences, OfflinePackageSelection,
};

const CLOUD_PERSISTENCE_VERSION: u32 = 5;
const CLOUD_ENVELOPE_VERSION: u32 = 1;
const CLOUD_PAGE_VERSION: u32 = 1;
const CLOUD_NODE_VERSION: u32 = 1;
const FLIGHT_PLAN_RECORD_KEY: &str = "flight_plan/current";
const FLIGHT_PLAN_SCHEMA_VERSION: u32 = 2;
const OFFLINE_PACKAGE_REGION_RECORD_PREFIX: &str = "offline_packages/region/";
const OFFLINE_PACKAGE_PRODUCT_RECORD_PREFIX: &str = "offline_packages/product/";
const OFFLINE_PACKAGE_SELECTION_SCHEMA_VERSION: u32 = 1;
const INACTIVITY_SLEEP_TIMEOUT_RECORD_KEY: &str = "settings/inactivity_sleep_timeout";
const INACTIVITY_SLEEP_TIMEOUT_SCHEMA_VERSION: u32 = 1;
const NEXRAD_ACQUISITION_RECORD_KEY: &str = "settings/nexrad_acquisition";
const NEXRAD_ACQUISITION_SCHEMA_VERSION: u32 = 1;
const DEBUG_FLAG_RECORD_PREFIX: &str = "settings/debug/";
const DEBUG_FLAG_SCHEMA_VERSION: u32 = 1;
const AIRCRAFT_LIBRARY_RECORD_PREFIX: &str = "aircraft/library/";
const AIRCRAFT_LIBRARY_SCHEMA_VERSION: u32 = 1;
const LEGACY_UNKNOWN_MUTATION_EPOCH_MS: i64 = i64::MIN;
const CLOUD_POLL_INTERVAL_MS: i64 = 60_000;
const CLOUD_TRANSIENT_RETRY_MS: i64 = 5_000;
const ACS_CORRECTNESS_POLL_INTERVAL_MS: i64 = 30 * 60_000;
pub const CLOUD_STATUS_ID: &str = "cloud:provider";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CloudProviderErrorKind {
    Unauthorized,
    Transient,
    Permanent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloudProviderOperation {
    AcsIssueAccountChallenge,
    AcsCreateAccount {
        request: AcsCreateAccountRequest,
    },
    AcsCreateObject {
        request: AcsCreateObjectRequest,
    },
    AcsReadObject {
        id: String,
    },
    AcsReadRoot,
    AcsCompareAndSwapRoot {
        request: AcsCompareAndSwapRootRequest,
    },
    AcsCreateSseTicket {
        request: AcsCreateSseTicketRequest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloudProviderRequest {
    pub request_id: u64,
    pub provider: CloudProviderKind,
    pub operation: CloudProviderOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloudProviderResponse {
    Created,
    AlreadyExists,
    AcsCreationChallenge {
        response: AcsCreationChallengeResponse,
    },
    AcsAccountCreated {
        response: AcsCreateAccountResponse,
    },
    AcsObject {
        object: Option<AcsObjectSnapshot>,
    },
    AcsRoot {
        root: Option<AcsRootSnapshot>,
    },
    AcsRootCas {
        response: AcsCompareAndSwapRootResponse,
    },
    AcsSseTicket {
        response: AcsCreateSseTicketResponse,
    },
    Error {
        kind: CloudProviderErrorKind,
        detail: String,
        retry_after_ms: Option<u64>,
        rate_limit_gate: Option<AcsRateLimitGate>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CloudAction {
    BeginSetupFromDevice,
    BeginCreateAccount,
    BackSetup,
    CreateAccount,
    AcceptDeviceSetupCode { setup_code: String },
    BackUpDeviceSetupCode,
    AddAnotherDevice,
    CloseLinkedAccountDetail,
    BeginUnlinkDevice,
    ConfirmUnlinkDevice,
    SyncNow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudStatusFact {
    pub label: String,
    pub value: String,
    pub time_epoch_ms: Option<i64>,
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
    root_secret_base64: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    imported_device_setup_code: Option<String>,
    #[serde(default)]
    tip: Option<VerifiedTip>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    acs: Option<AcsAccountState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct AcsAccountState {
    base_url: String,
    account_locator: String,
    #[serde(default)]
    root_revision: u64,
    #[serde(default)]
    root_hash: Option<String>,
    #[serde(default)]
    last_event_sequence: Option<u64>,
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
    AcsCreateChallenge,
    AcsCreateAccount {
        challenge: String,
    },
    AcsCreatePage {
        staged: StagedPublication,
        expected_revision: u64,
        expected_root_hash: Option<String>,
    },
    AcsCommitRoot {
        staged: StagedPublication,
        expected_revision: u64,
        expected_root_hash: Option<String>,
    },
    AcsReadRoot {
        purpose: ReadPurpose,
    },
    AcsReadPage {
        root: AcsRootSnapshot,
        node: CloudNode,
        purpose: ReadPurpose,
    },
    AcsCreateSseTicket,
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
pub(crate) struct CloudEngine {
    persistent: CloudPersistentState,
    provider_request_in_flight: Option<CloudProviderRequest>,
    linked_account_detail: Option<LinkedAccountDetail>,
    acs_default_base_url: Option<String>,
    acs_event_stream_plan: Option<CloudEventStreamPlan>,
    acs_event_stream_connected: bool,
    acs_event_stream_next_retry_epoch_ms: Option<i64>,
    acs_event_stream_consecutive_failures: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkedAccountDetail {
    BackupCode,
    AddDevice,
    ConfirmUnlink,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct CloudCompletion {
    changed_records: BTreeMap<String, CloudRecord>,
}

impl CloudCompletion {
    pub(crate) fn remote_flight_plan(&self) -> AppResult<Option<FlightPlan>> {
        self.changed_records
            .get(FLIGHT_PLAN_RECORD_KEY)
            .map(flight_plan_from_record)
            .transpose()
            .map(|record| record.map(|record| record.plan))
    }

    pub(crate) fn offline_package_preferences_changed(&self) -> bool {
        self.changed_records.keys().any(|key| {
            key.starts_with(OFFLINE_PACKAGE_REGION_RECORD_PREFIX)
                || key.starts_with(OFFLINE_PACKAGE_PRODUCT_RECORD_PREFIX)
        })
    }

    pub(crate) fn inactivity_sleep_timeout_changed(&self) -> bool {
        self.changed_records
            .contains_key(INACTIVITY_SLEEP_TIMEOUT_RECORD_KEY)
    }

    pub(crate) fn nexrad_acquisition_changed(&self) -> bool {
        self.changed_records
            .contains_key(NEXRAD_ACQUISITION_RECORD_KEY)
    }

    pub(crate) fn aircraft_library_changed(&self) -> bool {
        self.changed_records.keys().any(|key| {
            key.starts_with(product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX)
                || key.starts_with(AIRCRAFT_LIBRARY_RECORD_PREFIX)
        })
    }

    pub(crate) fn debug_flags(&self) -> AppResult<Vec<(DebugFlagId, bool)>> {
        self.changed_records
            .iter()
            .filter_map(|(key, record)| {
                let flag_id = key
                    .strip_prefix(DEBUG_FLAG_RECORD_PREFIX)
                    .and_then(debug_flag_from_id)?;
                Some(debug_flag_from_record(record).map(|enabled| (flag_id, enabled)))
            })
            .collect()
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
        if let Some(CloudWorkflow::AcsReadPage { purpose, .. }) = persistent.workflow.as_ref() {
            // A page is valid only for the root snapshot that named it. An app
            // restart may outlive the server's retention of that old page, so
            // resume read-only synchronization from the current root.
            persistent.workflow = Some(CloudWorkflow::AcsReadRoot { purpose: *purpose });
        }
        Self {
            persistent,
            provider_request_in_flight: None,
            linked_account_detail: None,
            acs_default_base_url: None,
            acs_event_stream_plan: None,
            acs_event_stream_connected: false,
            acs_event_stream_next_retry_epoch_ms: None,
            acs_event_stream_consecutive_failures: 0,
        }
    }

    pub fn set_acs_default_base_url(&mut self, base_url: Option<String>) -> AppResult<()> {
        self.acs_default_base_url = base_url
            .map(|value| crate::cloud_acs::validate_base_url(&value))
            .transpose()?;
        Ok(())
    }

    pub fn event_stream_plan(&self) -> Option<CloudEventStreamPlan> {
        self.acs_event_stream_plan.clone()
    }

    pub fn report_event_stream_event(
        &mut self,
        event: CloudEventStreamEvent,
        now_epoch_ms: i64,
    ) -> AppResult<()> {
        if self
            .acs_event_stream_plan
            .as_ref()
            .is_none_or(|plan| plan.stream_id != event.stream_id)
        {
            return Ok(());
        }
        match event.kind {
            CloudEventStreamEventKind::Connecting => {}
            CloudEventStreamEventKind::Connected => {
                self.acs_event_stream_connected = true;
                self.acs_event_stream_consecutive_failures = 0;
                self.acs_event_stream_next_retry_epoch_ms = None;
                if self
                    .persistent
                    .last_provider_failure
                    .as_ref()
                    .is_some_and(|failure| failure.kind == CloudProviderErrorKind::Transient)
                {
                    self.persistent.last_provider_failure = None;
                }
            }
            CloudEventStreamEventKind::Message => {
                let data = event
                    .data
                    .as_deref()
                    .ok_or_else(|| cloud_error("Aerobag Cloud event has no data"))?;
                let message: AcsSseEvent = serde_json::from_str(data).map_err(|error| {
                    cloud_error(format!("Aerobag Cloud event is invalid: {error}"))
                })?;
                let account = self.account_mut()?;
                let acs = account
                    .acs
                    .as_mut()
                    .ok_or_else(|| cloud_error("Aerobag Cloud configuration is missing"))?;
                if acs
                    .last_event_sequence
                    .is_some_and(|sequence| message.sequence() <= sequence)
                {
                    return Ok(());
                }
                acs.last_event_sequence = Some(message.sequence());
                let (root_revision, root_hash, force_read) = match &message {
                    AcsSseEvent::Ready {
                        root_revision,
                        root_hash,
                        ..
                    }
                    | AcsSseEvent::Heartbeat {
                        root_revision,
                        root_hash,
                        ..
                    } => (*root_revision, root_hash.as_deref(), false),
                    AcsSseEvent::RootChanged {
                        root_revision,
                        root_hash,
                        ..
                    } => (*root_revision, Some(root_hash.as_str()), false),
                    AcsSseEvent::Reset {
                        root_revision,
                        root_hash,
                        ..
                    } => (*root_revision, root_hash.as_deref(), true),
                };
                if force_read
                    || root_revision != acs.root_revision
                    || root_hash != acs.root_hash.as_deref()
                {
                    self.persistent.force_poll = true;
                }
                self.acs_event_stream_connected = true;
                self.acs_event_stream_consecutive_failures = 0;
                self.acs_event_stream_next_retry_epoch_ms = None;
            }
            CloudEventStreamEventKind::Error
            | CloudEventStreamEventKind::Closed
            | CloudEventStreamEventKind::IdleTimeout => {
                self.acs_event_stream_plan = None;
                self.acs_event_stream_connected = false;
                self.acs_event_stream_consecutive_failures =
                    self.acs_event_stream_consecutive_failures.saturating_add(1);
                let delay = if event.kind == CloudEventStreamEventKind::IdleTimeout {
                    0
                } else {
                    AEROBAG_SSE_TRANSPORT_POLICY
                        .reconnect_delay_ms(self.acs_event_stream_consecutive_failures)
                };
                self.acs_event_stream_next_retry_epoch_ms =
                    Some(now_epoch_ms.saturating_add(delay));
                self.persistent.last_provider_failure = Some(CloudProviderFailure {
                    kind: CloudProviderErrorKind::Transient,
                    detail: event.detail.unwrap_or_else(|| {
                        "Aerobag Cloud notification stream disconnected; synchronization will retry."
                            .to_string()
                    }),
                });
            }
        }
        Ok(())
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

    pub fn take_pending_remote_flight_plan(&mut self) -> Option<FlightPlan> {
        self.persistent
            .records
            .deferred_adoption
            .remove(FLIGHT_PLAN_RECORD_KEY)
            .and_then(|record| flight_plan_from_record(&record).ok())
            .map(|record| record.plan)
    }

    fn current_provider(&self) -> AppResult<CloudProviderKind> {
        self.persistent
            .account
            .as_ref()
            .map(|account| account.provider)
            .or_else(|| {
                self.acs_default_base_url
                    .as_ref()
                    .map(|_| CloudProviderKind::AerobagCloud)
            })
            .ok_or_else(|| cloud_error("this build has no Aerobag Cloud server configured"))
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

    pub(crate) fn has_linked_account(&self) -> bool {
        self.persistent
            .account
            .as_ref()
            .is_some_and(|account| account.tip.is_some() && account.acs.is_some())
    }

    fn provider_available(&self) -> bool {
        self.acs_default_base_url.is_some()
    }

    fn provider_ready(&self) -> bool {
        self.persistent
            .account
            .as_ref()
            .and_then(|account| account.acs.as_ref())
            .is_some()
            || self.acs_default_base_url.is_some()
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

    pub fn record_local_inactivity_sleep_timeout(
        &mut self,
        timeout: InactivitySleepTimeout,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let existing = self
            .persistent
            .records
            .cached
            .get(INACTIVITY_SLEEP_TIMEOUT_RECORD_KEY)
            .map(inactivity_sleep_timeout_from_record)
            .transpose()?;
        if existing == Some(timeout) {
            return Ok(false);
        }
        let record =
            CloudRecord {
                schema_version: INACTIVITY_SLEEP_TIMEOUT_SCHEMA_VERSION,
                modified_at_epoch_ms: Some(self.next_record_mutation_epoch_ms(
                    INACTIVITY_SLEEP_TIMEOUT_RECORD_KEY,
                    now_epoch_ms,
                )),
                value: serde_json::to_value(timeout).map_err(cloud_json_error)?,
            };
        self.record_local_cloud_record(INACTIVITY_SLEEP_TIMEOUT_RECORD_KEY, record);
        Ok(true)
    }

    pub fn inactivity_sleep_timeout(&self) -> AppResult<Option<InactivitySleepTimeout>> {
        self.persistent
            .records
            .cached
            .get(INACTIVITY_SLEEP_TIMEOUT_RECORD_KEY)
            .map(inactivity_sleep_timeout_from_record)
            .transpose()
    }

    pub fn record_local_nexrad_acquisition(
        &mut self,
        preferences: NexradAcquisitionPreferences,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let existing = self
            .persistent
            .records
            .cached
            .get(NEXRAD_ACQUISITION_RECORD_KEY)
            .map(nexrad_acquisition_from_record)
            .transpose()?;
        if existing == Some(preferences) {
            return Ok(false);
        }
        let record = CloudRecord {
            schema_version: NEXRAD_ACQUISITION_SCHEMA_VERSION,
            modified_at_epoch_ms: Some(
                self.next_record_mutation_epoch_ms(NEXRAD_ACQUISITION_RECORD_KEY, now_epoch_ms),
            ),
            value: serde_json::to_value(preferences).map_err(cloud_json_error)?,
        };
        self.record_local_cloud_record(NEXRAD_ACQUISITION_RECORD_KEY, record);
        Ok(true)
    }

    pub fn nexrad_acquisition(&self) -> AppResult<Option<NexradAcquisitionPreferences>> {
        self.persistent
            .records
            .cached
            .get(NEXRAD_ACQUISITION_RECORD_KEY)
            .map(nexrad_acquisition_from_record)
            .transpose()
    }

    pub fn record_local_debug_flag(
        &mut self,
        flag_id: DebugFlagId,
        enabled: bool,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let key = debug_flag_record_key(flag_id);
        let existing = self
            .persistent
            .records
            .cached
            .get(&key)
            .map(debug_flag_from_record)
            .transpose()?;
        if existing == Some(enabled) {
            return Ok(false);
        }
        let record = CloudRecord {
            schema_version: DEBUG_FLAG_SCHEMA_VERSION,
            modified_at_epoch_ms: Some(self.next_record_mutation_epoch_ms(&key, now_epoch_ms)),
            value: serde_json::to_value(enabled).map_err(cloud_json_error)?,
        };
        self.record_local_cloud_record(&key, record);
        Ok(true)
    }

    pub fn debug_flags(&self) -> AppResult<Vec<(DebugFlagId, bool)>> {
        all_debug_flags()
            .into_iter()
            .filter_map(|flag_id| {
                self.persistent
                    .records
                    .cached
                    .get(&debug_flag_record_key(flag_id))
                    .map(|record| debug_flag_from_record(record).map(|enabled| (flag_id, enabled)))
            })
            .collect()
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

    pub fn aircraft_definitions(
        &self,
    ) -> AppResult<BTreeMap<String, product_contracts::AircraftDefinition>> {
        self.persistent
            .records
            .cached
            .iter()
            .filter_map(|(key, record)| {
                key.strip_prefix(product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX)
                    .map(|hash| {
                        aircraft_definition_from_record(hash, record)
                            .map(|value| (hash.to_string(), value))
                    })
            })
            .collect()
    }

    pub fn aircraft_library_digest(&self) -> AppResult<[u8; 32]> {
        let mut digest = Sha256::new();
        for (hash, definition) in self.aircraft_definitions()? {
            digest.update(hash.as_bytes());
            digest.update(definition.content_hash().map_err(cloud_error)?);
        }
        for (hash, membership) in self.aircraft_library_memberships()? {
            digest.update(hash.as_bytes());
            digest.update([u8::from(membership.included)]);
        }
        Ok(digest.finalize().into())
    }

    pub fn aircraft_library_memberships(
        &self,
    ) -> AppResult<BTreeMap<String, product_contracts::AircraftLibraryMembership>> {
        let mut memberships = BTreeMap::new();
        for (key, record) in &self.persistent.records.cached {
            let Some(hash) = key.strip_prefix(AIRCRAFT_LIBRARY_RECORD_PREFIX) else {
                continue;
            };
            product_contracts::validate_aircraft_definition_hash(hash).map_err(cloud_error)?;
            memberships.insert(
                hash.to_string(),
                aircraft_library_membership_from_record(record)?,
            );
        }
        Ok(memberships)
    }

    pub fn record_local_aircraft_definition(
        &mut self,
        definition: &product_contracts::AircraftDefinition,
    ) -> AppResult<bool> {
        let hash = definition.content_hash().map_err(cloud_error)?;
        let key = product_contracts::aircraft_definition_key(&hash).map_err(cloud_error)?;
        let record = CloudRecord {
            schema_version: product_contracts::AIRCRAFT_DEFINITION_SCHEMA_VERSION,
            modified_at_epoch_ms: None,
            value: serde_json::to_value(definition).map_err(cloud_json_error)?,
        };
        if self.persistent.records.cached.get(&key) == Some(&record) {
            return Ok(false);
        }
        self.record_local_cloud_record(&key, record);
        Ok(true)
    }

    pub fn record_local_aircraft_library_membership(
        &mut self,
        definition_hash: &str,
        membership: product_contracts::AircraftLibraryMembership,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        product_contracts::validate_aircraft_definition_hash(definition_hash)
            .map_err(cloud_error)?;
        let key = format!("{AIRCRAFT_LIBRARY_RECORD_PREFIX}{definition_hash}");
        let existing = self
            .persistent
            .records
            .cached
            .get(&key)
            .map(aircraft_library_membership_from_record)
            .transpose()?;
        if existing == Some(membership) {
            return Ok(false);
        }
        let record = CloudRecord {
            schema_version: AIRCRAFT_LIBRARY_SCHEMA_VERSION,
            modified_at_epoch_ms: Some(self.next_record_mutation_epoch_ms(&key, now_epoch_ms)),
            value: serde_json::to_value(membership).map_err(cloud_json_error)?,
        };
        self.record_local_cloud_record(&key, record);
        Ok(true)
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
                        if self.persistent.account.take().is_none() {
                            self.persistent.onboarding_intent = None;
                        }
                    }
                    None => return Err(cloud_error("cloud setup has no earlier step")),
                }
                self.persistent.workflow = None;
                self.persistent.records.pending_keys.clear();
                self.persistent.records.deferred_adoption.clear();
                self.persistent.last_provider_failure = None;
                self.provider_request_in_flight = None;
                self.linked_account_detail = None;
            }
            CloudAction::CreateAccount => {
                self.require_unlinked_and_idle()?;
                if self.persistent.onboarding_intent != Some(CloudOnboardingIntent::CreateAccount) {
                    return Err(cloud_error(
                        "choose to create a Sync Account before creating it",
                    ));
                }
                let base_url = self.acs_default_base_url.clone().ok_or_else(|| {
                    cloud_error("this build has no Aerobag Cloud server configured")
                })?;
                let secret = random_bytes::<32>()?;
                let identity = crate::cloud_acs::derive_identity(&secret)?;
                self.reset_sync_activity();
                self.persistent.account = Some(CloudAccount {
                    provider: CloudProviderKind::AerobagCloud,
                    root_secret_base64: URL_SAFE_NO_PAD.encode(secret),
                    imported_device_setup_code: None,
                    tip: None,
                    acs: Some(AcsAccountState {
                        base_url,
                        account_locator: identity.account_locator,
                        root_revision: 0,
                        root_hash: None,
                        last_event_sequence: None,
                    }),
                });
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
                self.persistent.workflow = Some(CloudWorkflow::AcsCreateChallenge);
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
                let payload = decode_device_setup_code(&setup_code).map_err(cloud_error)?;
                let root_secret = payload.root_secret;
                let DeviceSetupProvider::AerobagCloud {
                    base_url,
                    account_locator,
                } = payload.provider;
                let base_url = crate::cloud_acs::validate_base_url(&base_url)?;
                let identity = crate::cloud_acs::derive_identity(&root_secret)?;
                let encoded_locator = URL_SAFE_NO_PAD.encode(account_locator);
                if identity.account_locator != encoded_locator {
                    return Err(cloud_error(
                        "Device Setup Code account locator does not match its root secret",
                    ));
                };
                self.persistent.onboarding_intent = Some(CloudOnboardingIntent::SetupFromDevice);
                self.reset_sync_activity();
                self.persistent.account = Some(CloudAccount {
                    provider: CloudProviderKind::AerobagCloud,
                    root_secret_base64: URL_SAFE_NO_PAD.encode(root_secret),
                    imported_device_setup_code: Some(setup_code.trim().to_string()),
                    tip: None,
                    acs: Some(AcsAccountState {
                        base_url,
                        account_locator: encoded_locator,
                        root_revision: 0,
                        root_hash: None,
                        last_event_sequence: None,
                    }),
                });
                self.persistent.records.pending_keys.clear();
                self.persistent.records.deferred_adoption.clear();
                self.persistent.workflow = Some(CloudWorkflow::AcsReadRoot {
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
                self.persistent.onboarding_intent = None;
                self.persistent.workflow = None;
                self.persistent.records.pending_keys.clear();
                self.persistent.records.deferred_adoption.clear();
                self.persistent.last_provider_failure = None;
                self.reset_sync_activity();
                self.provider_request_in_flight = None;
                self.linked_account_detail = None;
            }
            CloudAction::SyncNow => {
                self.persistent.force_poll = true;
                self.persistent.next_retry_epoch_ms = None;
                self.persistent.last_provider_failure = None;
            }
        }
        Ok(())
    }

    fn reset_sync_activity(&mut self) {
        self.persistent.last_success_epoch_ms = None;
        self.persistent.last_read_epoch_ms = None;
        self.persistent.last_write_epoch_ms = None;
        self.persistent.last_poll_epoch_ms = None;
        self.persistent.force_poll = false;
        self.acs_event_stream_plan = None;
        self.acs_event_stream_connected = false;
        self.acs_event_stream_next_retry_epoch_ms = None;
        self.acs_event_stream_consecutive_failures = 0;
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
            CloudUiActionId::CreateAccount => CloudAction::CreateAccount,
            CloudUiActionId::AcceptSetupCode => CloudAction::AcceptDeviceSetupCode {
                setup_code: required_ui_field(fields, CloudUiFieldId::DeviceSetupCode)?,
            },
            CloudUiActionId::BackupSetupCode => CloudAction::BackUpDeviceSetupCode,
            CloudUiActionId::AddDevice => CloudAction::AddAnotherDevice,
            CloudUiActionId::CloseLinkedDetail => CloudAction::CloseLinkedAccountDetail,
            CloudUiActionId::BeginUnlink => CloudAction::BeginUnlinkDevice,
            CloudUiActionId::ConfirmUnlink => CloudAction::ConfirmUnlinkDevice,
            CloudUiActionId::SyncNow => CloudAction::SyncNow,
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
            || !self.provider_ready()
            || self
                .persistent
                .last_provider_failure
                .as_ref()
                .is_some_and(|failure| failure.kind != CloudProviderErrorKind::Transient)
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
        let operation = operation_for_workflow(workflow, self.persistent.account.as_ref())?;
        let request_id = self.persistent.next_request_id.max(1);
        self.persistent.next_request_id = request_id.saturating_add(1);
        let request = CloudProviderRequest {
            request_id,
            provider,
            operation,
        };
        let account = self.account()?;
        let acs = account
            .acs
            .as_ref()
            .ok_or_else(|| cloud_error("Aerobag Cloud account configuration is missing"))?;
        let http_request = crate::cloud_acs::plan_request(
            &request,
            &acs.base_url,
            &account_secret(account)?,
            &acs.account_locator,
            now_epoch_ms,
        )?;
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
        let response = crate::cloud_acs::parse_response(&request, response);
        let provider = request.provider;
        if let CloudProviderResponse::Error {
            kind,
            detail,
            retry_after_ms,
            rate_limit_gate,
        } = response
        {
            let account_creation_rate_limited = matches!(
                rate_limit_gate,
                Some(
                    AcsRateLimitGate::AccountCreationNetwork
                        | AcsRateLimitGate::AccountCreationGlobal
                )
            );
            let retry_delay_ms = retry_after_ms
                .and_then(|delay| i64::try_from(delay).ok())
                .unwrap_or(CLOUD_TRANSIENT_RETRY_MS);
            let detail = cloud_provider_failure_detail(
                provider,
                kind,
                &detail,
                rate_limit_gate,
                retry_after_ms,
            );
            self.persistent.last_provider_failure = Some(CloudProviderFailure {
                kind,
                detail: detail.clone(),
            });
            if account_creation_rate_limited
                && matches!(
                    self.persistent.workflow,
                    Some(CloudWorkflow::AcsCreateAccount { .. })
                )
            {
                // Creation challenges expire long before a creation bucket refills.
                self.persistent.workflow = Some(CloudWorkflow::AcsCreateChallenge);
            }
            match kind {
                CloudProviderErrorKind::Unauthorized | CloudProviderErrorKind::Permanent => {
                    self.persistent.next_retry_epoch_ms = None;
                    self.acs_event_stream_plan = None;
                    self.acs_event_stream_connected = false;
                    self.acs_event_stream_next_retry_epoch_ms = None;
                }
                CloudProviderErrorKind::Transient => {
                    self.persistent.next_retry_epoch_ms =
                        Some(now_epoch_ms.saturating_add(retry_delay_ms));
                }
            }
            return Ok(CloudCompletion::default());
        }

        self.persistent.last_provider_failure = None;
        self.persistent.next_retry_epoch_ms = None;
        let Some(workflow) = self.persistent.workflow.clone() else {
            return Ok(self.commit_provider_completion_failure(cloud_error(
                "cloud provider completed with no active workflow",
            )));
        };
        match self.advance_workflow(workflow, response, now_epoch_ms) {
            Ok(completion) => {
                self.persistent.last_success_epoch_ms = Some(now_epoch_ms);
                Ok(completion)
            }
            Err(error) => Ok(self.commit_provider_completion_failure(error)),
        }
    }

    fn commit_provider_completion_failure(&mut self, error: AppError) -> CloudCompletion {
        self.persistent.workflow = None;
        self.persistent.next_retry_epoch_ms = None;
        self.persistent.last_provider_failure = Some(CloudProviderFailure {
            kind: CloudProviderErrorKind::Permanent,
            detail: error.message,
        });
        self.acs_event_stream_plan = None;
        self.acs_event_stream_connected = false;
        self.acs_event_stream_next_retry_epoch_ms = None;
        CloudCompletion::default()
    }

    fn ensure_workflow(&mut self, now_epoch_ms: i64) -> AppResult<()> {
        if self.persistent.workflow.is_some() {
            return Ok(());
        }
        let Some(account) = self.persistent.account.as_ref() else {
            return Ok(());
        };
        if account.tip.is_none() {
            return Ok(());
        }
        if !self.persistent.records.pending_keys.is_empty() {
            let acs = account
                .acs
                .as_ref()
                .ok_or_else(|| cloud_error("Aerobag Cloud account configuration is missing"))?;
            let tip = account.tip.as_ref().expect("tip checked above");
            let staged = self.stage_acs_publication(
                PublicationPurpose::Publish,
                tip.generation.saturating_add(1),
                Some(tip),
                now_epoch_ms,
            )?;
            self.persistent.workflow = Some(CloudWorkflow::AcsCreatePage {
                staged,
                expected_revision: acs.root_revision,
                expected_root_hash: acs.root_hash.clone(),
            });
            return Ok(());
        }
        let poll_interval_ms = if self.acs_event_stream_connected {
            ACS_CORRECTNESS_POLL_INTERVAL_MS
        } else {
            CLOUD_POLL_INTERVAL_MS
        };
        let poll_due = self.persistent.force_poll
            || self
                .persistent
                .last_poll_epoch_ms
                .is_none_or(|last| now_epoch_ms.saturating_sub(last) >= poll_interval_ms);
        if poll_due {
            self.persistent.force_poll = false;
            self.persistent.workflow = Some(CloudWorkflow::AcsReadRoot {
                purpose: ReadPurpose::Poll,
            });
        }
        if self.persistent.workflow.is_none()
            && self.acs_event_stream_plan.is_none()
            && self
                .acs_event_stream_next_retry_epoch_ms
                .is_none_or(|retry| retry <= now_epoch_ms)
        {
            self.persistent.workflow = Some(CloudWorkflow::AcsCreateSseTicket);
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
            CloudWorkflow::AcsCreateChallenge => {
                let CloudProviderResponse::AcsCreationChallenge { response } = response else {
                    return Err(unexpected_response(
                        "request ACS account challenge",
                        response,
                    ));
                };
                if response.contract_id != product_contracts::ACS_CONTRACT_ID
                    || response.challenge.trim().is_empty()
                    || response.expires_at_epoch_ms <= now_epoch_ms
                {
                    return Err(cloud_error(
                        "Aerobag Cloud returned an invalid account-creation challenge",
                    ));
                }
                self.persistent.workflow = Some(CloudWorkflow::AcsCreateAccount {
                    challenge: response.challenge,
                });
            }
            CloudWorkflow::AcsCreateAccount { .. } => {
                let CloudProviderResponse::AcsAccountCreated { response } = response else {
                    return Err(unexpected_response("create ACS account", response));
                };
                let acs = self
                    .account()?
                    .acs
                    .as_ref()
                    .ok_or_else(|| cloud_error("Aerobag Cloud configuration is missing"))?;
                if response.contract_id != product_contracts::ACS_CONTRACT_ID
                    || response.account_locator != acs.account_locator
                {
                    return Err(cloud_error(
                        "Aerobag Cloud created a different account than requested",
                    ));
                }
                let staged = self.stage_acs_publication(
                    PublicationPurpose::CreateAccount,
                    0,
                    None,
                    now_epoch_ms,
                )?;
                self.persistent.workflow = Some(CloudWorkflow::AcsCreatePage {
                    staged,
                    expected_revision: 0,
                    expected_root_hash: None,
                });
            }
            CloudWorkflow::AcsCreatePage {
                staged,
                expected_revision,
                expected_root_hash,
            } => match response {
                CloudProviderResponse::Created | CloudProviderResponse::AlreadyExists => {
                    self.persistent.workflow = Some(CloudWorkflow::AcsCommitRoot {
                        staged,
                        expected_revision,
                        expected_root_hash,
                    });
                }
                other => return Err(unexpected_response("create ACS state page", other)),
            },
            CloudWorkflow::AcsCommitRoot { staged, .. } => {
                let CloudProviderResponse::AcsRootCas { response } = response else {
                    return Err(unexpected_response("commit ACS root", response));
                };
                match response {
                    AcsCompareAndSwapRootResponse::Committed { root } => {
                        validate_acs_root_snapshot(&root)?;
                        if root.root_hash != staged.node_hash {
                            return Err(cloud_error(
                                "Aerobag Cloud committed a different root than requested",
                            ));
                        }
                        let acs =
                            self.account_mut()?.acs.as_mut().ok_or_else(|| {
                                cloud_error("Aerobag Cloud configuration is missing")
                            })?;
                        acs.root_revision = root.revision;
                        acs.root_hash = Some(root.root_hash);
                        self.finish_publication(staged)?;
                    }
                    AcsCompareAndSwapRootResponse::Conflict { .. } => {
                        self.persistent.workflow = Some(CloudWorkflow::AcsReadRoot {
                            purpose: ReadPurpose::PublishRace,
                        });
                    }
                }
            }
            CloudWorkflow::AcsReadRoot { purpose } => {
                let CloudProviderResponse::AcsRoot { root } = response else {
                    return Err(unexpected_response("read ACS root", response));
                };
                let Some(root) = root else {
                    if matches!(purpose, ReadPurpose::Link) {
                        return Err(cloud_error(
                            "the Device Setup Code's Sync Account has no published root",
                        ));
                    }
                    self.persistent.last_read_epoch_ms = Some(now_epoch_ms);
                    self.persistent.last_poll_epoch_ms = Some(now_epoch_ms);
                    self.persistent.workflow = None;
                    return Ok(CloudCompletion::default());
                };
                validate_acs_root_snapshot(&root)?;
                let current = self
                    .account()?
                    .acs
                    .as_ref()
                    .ok_or_else(|| cloud_error("Aerobag Cloud configuration is missing"))?;
                if current.root_revision == root.revision
                    && current.root_hash.as_deref() == Some(root.root_hash.as_str())
                    && !matches!(purpose, ReadPurpose::Link)
                {
                    self.persistent.last_read_epoch_ms = Some(now_epoch_ms);
                    self.persistent.last_poll_epoch_ms = Some(now_epoch_ms);
                    self.persistent.workflow = None;
                    return Ok(CloudCompletion::default());
                }
                let node: CloudNode = self.decrypt_acs_value(
                    &root.value,
                    "state_node",
                    AcsEncryptedValueKind::Root,
                    ACS_FIXED_ROOT_ID,
                )?;
                validate_acs_node(&root, &node, self.account()?.tip.as_ref(), purpose)?;
                self.persistent.workflow = Some(CloudWorkflow::AcsReadPage {
                    root,
                    node,
                    purpose,
                });
            }
            CloudWorkflow::AcsReadPage {
                root,
                node,
                purpose,
            } => {
                let CloudProviderResponse::AcsObject { object } = response else {
                    return Err(unexpected_response("read ACS state page", response));
                };
                let object = object.ok_or_else(|| {
                    cloud_error(format!(
                        "Aerobag Cloud state page {} is missing",
                        node.merkle_root_id
                    ))
                })?;
                if object.object_id != node.merkle_root_id {
                    return Err(cloud_error("Aerobag Cloud returned the wrong state page"));
                }
                let actual_hash = object
                    .value
                    .authenticated_hash(AcsEncryptedValueKind::Object, &object.object_id)
                    .map_err(cloud_error)?;
                if actual_hash != node.merkle_root_hash {
                    return Err(cloud_error(format!(
                        "Aerobag Cloud page hash mismatch: expected {}, got {actual_hash}",
                        node.merkle_root_hash
                    )));
                }
                let page: CloudPage = self.decrypt_acs_value(
                    &object.value,
                    "merkle_page",
                    AcsEncryptedValueKind::Object,
                    &object.object_id,
                )?;
                validate_cloud_page(&page)?;
                self.account_mut()?.tip = Some(VerifiedTip {
                    node_id: ACS_FIXED_ROOT_ID.to_string(),
                    node_hash: root.root_hash.clone(),
                    generation: node.generation,
                    published_at_epoch_ms: node.published_at_epoch_ms,
                    merkle_root_id: node.merkle_root_id,
                    merkle_root_hash: node.merkle_root_hash,
                    next_slot_id: String::new(),
                });
                let acs = self
                    .account_mut()?
                    .acs
                    .as_mut()
                    .ok_or_else(|| cloud_error("Aerobag Cloud configuration is missing"))?;
                acs.root_revision = root.revision;
                acs.root_hash = Some(root.root_hash);
                self.persistent.last_read_epoch_ms = Some(now_epoch_ms);
                self.persistent.last_poll_epoch_ms = Some(now_epoch_ms);
                self.persistent.workflow = None;
                let completion = self.reconcile_page(page, purpose)?;
                if !completion.changed_records.is_empty()
                    || matches!(purpose, ReadPurpose::PublishRace)
                {
                    self.persistent.force_poll = true;
                }
                return Ok(completion);
            }
            CloudWorkflow::AcsCreateSseTicket => {
                let CloudProviderResponse::AcsSseTicket { response } = response else {
                    return Err(unexpected_response("create ACS event ticket", response));
                };
                if response.contract_id != product_contracts::ACS_CONTRACT_ID
                    || response.ticket.trim().is_empty()
                    || response.expires_at_epoch_ms <= now_epoch_ms
                {
                    return Err(cloud_error(
                        "Aerobag Cloud returned an invalid event-stream ticket",
                    ));
                }
                let base_url = self
                    .account()?
                    .acs
                    .as_ref()
                    .ok_or_else(|| cloud_error("Aerobag Cloud configuration is missing"))?
                    .base_url
                    .clone();
                let stream_id = self.persistent.next_request_id.max(1);
                self.persistent.next_request_id = stream_id.saturating_add(1);
                self.acs_event_stream_plan = Some(CloudEventStreamPlan {
                    stream_id,
                    url: crate::cloud_acs::resolve_event_url(&base_url, &response.events_url)?,
                    connect_timeout_ms: AEROBAG_SSE_TRANSPORT_POLICY.connect_timeout_ms,
                    idle_timeout_ms: AEROBAG_SSE_TRANSPORT_POLICY.idle_timeout_ms,
                });
                self.acs_event_stream_connected = false;
                self.acs_event_stream_next_retry_epoch_ms = None;
                self.persistent.workflow = None;
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

    fn stage_acs_publication(
        &self,
        purpose: PublicationPurpose,
        generation: u64,
        parent: Option<&VerifiedTip>,
        now_epoch_ms: i64,
    ) -> AppResult<StagedPublication> {
        let page_id = URL_SAFE_NO_PAD.encode(random_bytes::<24>()?);
        let page = page_for_records(&self.persistent.records.cached);
        let page_value = self.encrypt_acs_value(
            &page,
            "merkle_page",
            AcsEncryptedValueKind::Object,
            &page_id,
            Vec::new(),
        )?;
        let page_bytes = page_value.ciphertext().map_err(cloud_error)?;
        let page_hash = page_value
            .authenticated_hash(AcsEncryptedValueKind::Object, &page_id)
            .map_err(cloud_error)?;
        let node = CloudNode {
            version: CLOUD_NODE_VERSION,
            generation,
            published_at_epoch_ms: Some(now_epoch_ms),
            parent_node_id: parent.map(|tip| tip.node_id.clone()),
            parent_node_hash: parent.map(|tip| tip.node_hash.clone()),
            merkle_root_id: page_id.clone(),
            merkle_root_hash: page_hash.clone(),
            next_slot_id: String::new(),
        };
        let node_value = self.encrypt_acs_value(
            &node,
            "state_node",
            AcsEncryptedValueKind::Root,
            ACS_FIXED_ROOT_ID,
            vec![page_id.clone()],
        )?;
        let node_bytes = node_value.ciphertext().map_err(cloud_error)?;
        let node_hash = node_value
            .authenticated_hash(AcsEncryptedValueKind::Root, ACS_FIXED_ROOT_ID)
            .map_err(cloud_error)?;
        Ok(StagedPublication {
            purpose,
            node_id: ACS_FIXED_ROOT_ID.to_string(),
            page_id,
            next_slot_id: String::new(),
            page_bytes_base64: URL_SAFE_NO_PAD.encode(page_bytes),
            node_bytes_base64: URL_SAFE_NO_PAD.encode(node_bytes),
            node_hash,
            merkle_root_hash: page_hash,
            generation,
            published_at_epoch_ms: now_epoch_ms,
            local_revision: self.persistent.local_revision,
        })
    }

    fn encrypt_acs_value<T: Serialize>(
        &self,
        value: &T,
        role: &str,
        kind: AcsEncryptedValueKind,
        value_id: &str,
        child_object_ids: Vec<String>,
    ) -> AppResult<AcsEncryptedValue> {
        let account = self.account()?;
        let secret = account_secret(account)?;
        let account_tag = account_tag(&secret);
        let key = derive_acs_payload_key(&secret)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|_| cloud_error("invalid Aerobag Cloud encryption key"))?;
        let nonce = random_bytes::<12>()?;
        let aad = acs_encrypted_value_associated_data(kind, value_id, &child_object_ids)
            .map_err(cloud_error)?;
        let plaintext = serde_json::to_vec(value).map_err(cloud_json_error)?;
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| cloud_error("Aerobag Cloud encryption failed"))?;
        let envelope = serde_json::to_vec(&CloudEnvelope {
            version: CLOUD_ENVELOPE_VERSION,
            account_tag,
            role: role.to_string(),
            nonce_base64: URL_SAFE_NO_PAD.encode(nonce),
            ciphertext_base64: URL_SAFE_NO_PAD.encode(ciphertext),
        })
        .map_err(cloud_json_error)?;
        Ok(AcsEncryptedValue::from_ciphertext(
            &envelope,
            child_object_ids,
        ))
    }

    fn decrypt_acs_value<T: for<'de> Deserialize<'de>>(
        &self,
        value: &AcsEncryptedValue,
        role: &str,
        kind: AcsEncryptedValueKind,
        value_id: &str,
    ) -> AppResult<T> {
        value.validate().map_err(cloud_error)?;
        let envelope: CloudEnvelope =
            serde_json::from_slice(&value.ciphertext().map_err(cloud_error)?)
                .map_err(cloud_json_error)?;
        let account = self.account()?;
        let secret = account_secret(account)?;
        if envelope.version != CLOUD_ENVELOPE_VERSION
            || envelope.account_tag != account_tag(&secret)
            || envelope.role != role
        {
            return Err(cloud_error(
                "Aerobag Cloud envelope binding does not match this account",
            ));
        }
        let nonce: [u8; 12] = URL_SAFE_NO_PAD
            .decode(&envelope.nonce_base64)
            .map_err(|_| cloud_error("Aerobag Cloud envelope nonce is invalid"))?
            .try_into()
            .map_err(|_| cloud_error("Aerobag Cloud envelope nonce has the wrong size"))?;
        let ciphertext = URL_SAFE_NO_PAD
            .decode(&envelope.ciphertext_base64)
            .map_err(|_| cloud_error("Aerobag Cloud envelope ciphertext is invalid"))?;
        let aad = value.associated_data(kind, value_id).map_err(cloud_error)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&derive_acs_payload_key(&secret)?)
            .map_err(|_| cloud_error("invalid Aerobag Cloud decryption key"))?;
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| cloud_error("Aerobag Cloud envelope authentication failed"))?;
        serde_json::from_slice(&plaintext).map_err(cloud_json_error)
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

    #[cfg(test)]
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
        let intent = self.persistent.onboarding_intent;

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
                            placeholder: "AB3...".to_string(),
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
        if account_is_pending {
            let creating = intent == Some(CloudOnboardingIntent::CreateAccount);
            let failed = self.persistent.last_provider_failure.is_some();
            let title = match (creating, failed) {
                (true, true) => "Could not create Sync Account",
                (false, true) => "Could not link Sync Account",
                (true, false) => "Creating Sync Account...",
                (false, false) => "Linking Sync Account...",
            };
            let state = if failed {
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
                    "Aerobag is verifying encrypted account state with Aerobag Cloud.".to_string()
                });
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
            panels.push(cloud_panel(
                "create_account",
                "Create a new Sync Account in Aerobag Cloud",
                UiCloudPanelState::Active,
                Some(
                    "This always creates a new Sync Account. It will not find or replace another account. To use an existing account, go back and choose Set up from another device.",
                ),
                vec![
                    cloud_action(
                        CloudUiActionId::CreateAccount,
                        "Create new Sync Account",
                        self.provider_available(),
                        "This build has no Aerobag Cloud server configured.",
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
            provider_card: self.current_provider().ok().map(|_| self.provider_card()),
            overall_status: self.overall_status_panel(now_epoch_ms),
            next_refresh_epoch_ms: None,
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
        panel.time_facts = self.sync_time_facts(now_epoch_ms);
        panel
    }

    fn sync_time_facts(&self, now_epoch_ms: i64) -> Vec<UiCloudTimeFact> {
        let mut facts = Vec::new();
        if let Some(epoch_ms) = self.persistent.last_read_epoch_ms {
            facts.push(UiCloudTimeFact {
                label: "Last read from cloud".to_string(),
                value: crate::time_display::format_relative_time(
                    epoch_ms,
                    now_epoch_ms,
                    crate::time_display::RelativeTimeStyle::Ago,
                    true,
                ),
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
                value: crate::time_display::format_relative_time(
                    epoch_ms,
                    now_epoch_ms,
                    crate::time_display::RelativeTimeStyle::Ago,
                    true,
                ),
            });
        }
        if let Some(epoch_ms) = self.persistent.last_write_epoch_ms {
            facts.push(UiCloudTimeFact {
                label: "Last write to cloud from this device".to_string(),
                value: crate::time_display::format_relative_time(
                    epoch_ms,
                    now_epoch_ms,
                    crate::time_display::RelativeTimeStyle::Ago,
                    true,
                ),
            });
        }
        facts
    }

    pub fn status_summary(&self, _now_epoch_ms: i64) -> CloudStatusSummary {
        let linked = self
            .persistent
            .account
            .as_ref()
            .is_some_and(|account| account.tip.is_some());
        let provider = self.current_provider().ok();
        let mut facts = Vec::new();
        if let Some(provider) = provider {
            facts.push(CloudStatusFact {
                label: "Provider".to_string(),
                value: provider.label().to_string(),
                time_epoch_ms: None,
            });
        }
        facts.push(CloudStatusFact {
            label: "Sync Account".to_string(),
            value: if linked { "Linked" } else { "Not linked" }.to_string(),
            time_epoch_ms: None,
        });
        if let Some(account) = self.persistent.account.as_ref() {
            if let Some(tip) = account.tip.as_ref() {
                facts.push(CloudStatusFact {
                    label: "Generation".to_string(),
                    value: tip.generation.to_string(),
                    time_epoch_ms: None,
                });
            }
        }
        facts.push(CloudStatusFact {
            label: "Pending local records".to_string(),
            value: self.persistent.records.pending_keys.len().to_string(),
            time_epoch_ms: None,
        });
        if let Some(last_success) = self.persistent.last_success_epoch_ms {
            facts.push(CloudStatusFact {
                label: "Last provider success".to_string(),
                value: format_epoch_ms_utc(last_success),
                time_epoch_ms: Some(last_success),
            });
        }

        if linked {
            if let Some(failure) = self.persistent.last_provider_failure.as_ref() {
                return CloudStatusSummary {
                    label: if failure.kind == CloudProviderErrorKind::Transient {
                        "OFFLINE"
                    } else {
                        "FAILED"
                    }
                    .to_string(),
                    severity: if failure.kind == CloudProviderErrorKind::Transient {
                        UiStatusSeverity::Info
                    } else {
                        UiStatusSeverity::Caution
                    },
                    detail: failure.detail.clone(),
                    facts,
                };
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
            label: "NOT SET UP".to_string(),
            severity: UiStatusSeverity::Info,
            detail: "This device is not linked to a Sync Account.".to_string(),
            facts,
        }
    }

    fn provider_card(&self) -> UiCloudPanel {
        let server = self
            .persistent
            .account
            .as_ref()
            .and_then(|account| account.acs.as_ref())
            .map(|acs| acs.base_url.as_str())
            .or(self.acs_default_base_url.as_deref())
            .unwrap_or("No server configured");
        cloud_panel(
            "provider",
            CloudProviderKind::AerobagCloud.label(),
            UiCloudPanelState::Complete,
            Some(&format!("Configured for {server}")),
            Vec::new(),
            None,
        )
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
        let actions = if state == UiCloudPanelState::Active {
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
        } else {
            Vec::new()
        };
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

    pub fn status_record(&self, _now_epoch_ms: i64) -> Option<DataStatusRecord> {
        let linked = self
            .persistent
            .account
            .as_ref()
            .is_some_and(|account| account.tip.is_some());
        if !linked {
            return None;
        }
        let failure = self.persistent.last_provider_failure.as_ref()?;
        let transient = failure.kind == CloudProviderErrorKind::Transient;
        Some(DataStatusRecord::new(
            CLOUD_STATUS_ID,
            "CLOUD",
            Some(if transient { "OFFLINE" } else { "FAILED" }.to_string()),
            if transient {
                UiStatusSeverity::Info
            } else {
                UiStatusSeverity::Caution
            },
            !transient,
            failure.detail.clone(),
        ))
    }

    pub fn device_setup_code(&self) -> AppResult<String> {
        let account = self.account()?;
        if account.tip.is_none() {
            return Err(cloud_error("cloud account creation has not completed"));
        }
        if let Some(imported) = &account.imported_device_setup_code {
            return Ok(imported.clone());
        }
        Ok(encode_device_setup_code(&DeviceSetupCode {
            root_secret: account_secret(account)?,
            provider: {
                let acs = account
                    .acs
                    .as_ref()
                    .ok_or_else(|| cloud_error("Aerobag Cloud configuration is unavailable"))?;
                DeviceSetupProvider::AerobagCloud {
                    base_url: acs.base_url.clone(),
                    account_locator: URL_SAFE_NO_PAD
                        .decode(&acs.account_locator)
                        .map_err(|_| cloud_error("Aerobag Cloud account locator is invalid"))?
                        .try_into()
                        .map_err(|_| {
                            cloud_error("Aerobag Cloud account locator has the wrong size")
                        })?,
                }
            },
        }))
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

fn required_ui_field(fields: &[CloudUiFieldValue], field_id: CloudUiFieldId) -> AppResult<String> {
    let value = fields
        .iter()
        .find(|field| field.id == field_id)
        .map(|field| field.value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| cloud_error("Device Setup Code is required"))?;
    Ok(value.to_string())
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

fn operation_for_workflow(
    workflow: &CloudWorkflow,
    account: Option<&CloudAccount>,
) -> AppResult<CloudProviderOperation> {
    Ok(match workflow {
        CloudWorkflow::AcsCreateChallenge => CloudProviderOperation::AcsIssueAccountChallenge,
        CloudWorkflow::AcsCreateAccount { challenge } => {
            let account = account.ok_or_else(|| cloud_error("Aerobag Cloud account is missing"))?;
            let acs = account
                .acs
                .as_ref()
                .ok_or_else(|| cloud_error("Aerobag Cloud configuration is missing"))?;
            let identity = crate::cloud_acs::derive_identity(&account_secret(account)?)?;
            CloudProviderOperation::AcsCreateAccount {
                request: AcsCreateAccountRequest {
                    contract_id: product_contracts::ACS_CONTRACT_ID.to_string(),
                    account_locator: acs.account_locator.clone(),
                    signing_key_id: identity.signing_key_id,
                    signing_public_key_base64url: identity.signing_public_key_base64url,
                    creation_challenge: challenge.clone(),
                },
            }
        }
        CloudWorkflow::AcsCreatePage { staged, .. } => CloudProviderOperation::AcsCreateObject {
            request: AcsCreateObjectRequest {
                contract_id: product_contracts::ACS_CONTRACT_ID.to_string(),
                object_id: staged.page_id.clone(),
                value: acs_staged_page_value(staged)?,
            },
        },
        CloudWorkflow::AcsCommitRoot {
            staged,
            expected_revision,
            expected_root_hash,
        } => CloudProviderOperation::AcsCompareAndSwapRoot {
            request: AcsCompareAndSwapRootRequest {
                contract_id: product_contracts::ACS_CONTRACT_ID.to_string(),
                expected_revision: *expected_revision,
                expected_root_hash: expected_root_hash.clone(),
                replacement: acs_staged_root_value(staged)?,
            },
        },
        CloudWorkflow::AcsReadRoot { .. } => CloudProviderOperation::AcsReadRoot,
        CloudWorkflow::AcsReadPage { node, .. } => CloudProviderOperation::AcsReadObject {
            id: node.merkle_root_id.clone(),
        },
        CloudWorkflow::AcsCreateSseTicket => {
            let account = account.ok_or_else(|| cloud_error("Aerobag Cloud account is missing"))?;
            let acs = account
                .acs
                .as_ref()
                .ok_or_else(|| cloud_error("Aerobag Cloud configuration is missing"))?;
            CloudProviderOperation::AcsCreateSseTicket {
                request: AcsCreateSseTicketRequest {
                    contract_id: product_contracts::ACS_CONTRACT_ID.to_string(),
                    last_event_sequence: acs.last_event_sequence,
                },
            }
        }
    })
}

fn acs_staged_page_value(staged: &StagedPublication) -> AppResult<AcsEncryptedValue> {
    let bytes = URL_SAFE_NO_PAD
        .decode(&staged.page_bytes_base64)
        .map_err(|_| cloud_error("staged Aerobag Cloud page is not valid base64url"))?;
    let value = AcsEncryptedValue::from_ciphertext(&bytes, Vec::new());
    let hash = value
        .authenticated_hash(AcsEncryptedValueKind::Object, &staged.page_id)
        .map_err(cloud_error)?;
    if hash != staged.merkle_root_hash {
        return Err(cloud_error("staged Aerobag Cloud page hash changed"));
    }
    Ok(value)
}

fn acs_staged_root_value(staged: &StagedPublication) -> AppResult<AcsEncryptedValue> {
    let bytes = URL_SAFE_NO_PAD
        .decode(&staged.node_bytes_base64)
        .map_err(|_| cloud_error("staged Aerobag Cloud root is not valid base64url"))?;
    let value = AcsEncryptedValue::from_ciphertext(&bytes, vec![staged.page_id.clone()]);
    let hash = value
        .authenticated_hash(AcsEncryptedValueKind::Root, ACS_FIXED_ROOT_ID)
        .map_err(cloud_error)?;
    if hash != staged.node_hash {
        return Err(cloud_error("staged Aerobag Cloud root hash changed"));
    }
    Ok(value)
}

fn validate_acs_root_snapshot(root: &AcsRootSnapshot) -> AppResult<()> {
    if root.revision == 0 {
        return Err(cloud_error("Aerobag Cloud returned root revision zero"));
    }
    let hash = root
        .value
        .authenticated_hash(AcsEncryptedValueKind::Root, ACS_FIXED_ROOT_ID)
        .map_err(cloud_error)?;
    if hash != root.root_hash {
        return Err(cloud_error(format!(
            "Aerobag Cloud root hash mismatch: expected {}, got {hash}",
            root.root_hash
        )));
    }
    Ok(())
}

fn validate_acs_node(
    root: &AcsRootSnapshot,
    node: &CloudNode,
    current_tip: Option<&VerifiedTip>,
    purpose: ReadPurpose,
) -> AppResult<()> {
    if node.version != CLOUD_NODE_VERSION
        || root.value.child_object_ids.as_slice() != [node.merkle_root_id.as_str()]
    {
        return Err(cloud_error(
            "Aerobag Cloud root does not describe one valid state page",
        ));
    }
    if matches!(purpose, ReadPurpose::Link) {
        return Ok(());
    }
    let current_tip = current_tip
        .ok_or_else(|| cloud_error("Aerobag Cloud account has no previously verified root"))?;
    if node.generation <= current_tip.generation {
        return Err(cloud_error("Aerobag Cloud root generation did not advance"));
    }
    Ok(())
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
    let plan: FlightPlan =
        serde_json::from_value(record.value.clone()).map_err(cloud_json_error)?;
    Ok(StampedFlightPlan {
        plan: plan.normalized(),
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

fn inactivity_sleep_timeout_from_record(record: &CloudRecord) -> AppResult<InactivitySleepTimeout> {
    if record.schema_version != INACTIVITY_SLEEP_TIMEOUT_SCHEMA_VERSION {
        return Err(cloud_error(format!(
            "unsupported inactivity sleep timeout schema {}",
            record.schema_version
        )));
    }
    if record.modified_at_epoch_ms.is_none() {
        return Err(cloud_error(
            "inactivity sleep timeout has no user-mutation timestamp",
        ));
    }
    serde_json::from_value(record.value.clone()).map_err(cloud_json_error)
}

fn nexrad_acquisition_from_record(record: &CloudRecord) -> AppResult<NexradAcquisitionPreferences> {
    if record.schema_version != NEXRAD_ACQUISITION_SCHEMA_VERSION {
        return Err(cloud_error(format!(
            "unsupported NEXRAD acquisition schema {}",
            record.schema_version
        )));
    }
    if record.modified_at_epoch_ms.is_none() {
        return Err(cloud_error(
            "NEXRAD acquisition setting has no user-mutation timestamp",
        ));
    }
    serde_json::from_value(record.value.clone()).map_err(cloud_json_error)
}

fn debug_flag_record_key(flag_id: DebugFlagId) -> String {
    format!("{DEBUG_FLAG_RECORD_PREFIX}{}", debug_flag_id(flag_id))
}

fn debug_flag_from_record(record: &CloudRecord) -> AppResult<bool> {
    if record.schema_version != DEBUG_FLAG_SCHEMA_VERSION {
        return Err(cloud_error(format!(
            "unsupported debug flag schema {}",
            record.schema_version
        )));
    }
    if record.modified_at_epoch_ms.is_none() {
        return Err(cloud_error("debug flag has no user-mutation timestamp"));
    }
    serde_json::from_value(record.value.clone()).map_err(cloud_json_error)
}

fn aircraft_definition_from_record(
    expected_hash: &str,
    record: &CloudRecord,
) -> AppResult<product_contracts::AircraftDefinition> {
    if record.schema_version != product_contracts::AIRCRAFT_DEFINITION_SCHEMA_VERSION {
        return Err(cloud_error(format!(
            "unsupported aircraft definition schema {}",
            record.schema_version
        )));
    }
    if record.modified_at_epoch_ms.is_some() {
        return Err(cloud_error(
            "immutable aircraft definition has a mutation timestamp",
        ));
    }
    let definition: product_contracts::AircraftDefinition =
        serde_json::from_value(record.value.clone()).map_err(cloud_json_error)?;
    let actual_hash = definition.content_hash().map_err(cloud_error)?;
    if actual_hash != expected_hash {
        return Err(cloud_error(format!(
            "aircraft definition record hash mismatch: key has {expected_hash}, value has {actual_hash}"
        )));
    }
    Ok(definition)
}

fn aircraft_library_membership_from_record(
    record: &CloudRecord,
) -> AppResult<product_contracts::AircraftLibraryMembership> {
    if record.schema_version != AIRCRAFT_LIBRARY_SCHEMA_VERSION {
        return Err(cloud_error(format!(
            "unsupported aircraft library schema {}",
            record.schema_version
        )));
    }
    if record.modified_at_epoch_ms.is_none() {
        return Err(cloud_error(
            "aircraft library membership has no user-mutation timestamp",
        ));
    }
    serde_json::from_value(record.value.clone()).map_err(cloud_json_error)
}

fn validate_known_record(key: &str, record: &CloudRecord) -> AppResult<()> {
    if key == FLIGHT_PLAN_RECORD_KEY {
        flight_plan_from_record(record)?;
    } else if key == INACTIVITY_SLEEP_TIMEOUT_RECORD_KEY {
        inactivity_sleep_timeout_from_record(record)?;
    } else if key == NEXRAD_ACQUISITION_RECORD_KEY {
        nexrad_acquisition_from_record(record)?;
    } else if let Some(id) = key.strip_prefix(DEBUG_FLAG_RECORD_PREFIX) {
        // Older clients preserve unknown settings records for forward compatibility.
        if debug_flag_from_id(id).is_some() {
            debug_flag_from_record(record)?;
        }
    } else if key.starts_with(OFFLINE_PACKAGE_REGION_RECORD_PREFIX)
        || key.starts_with(OFFLINE_PACKAGE_PRODUCT_RECORD_PREFIX)
    {
        offline_package_selection_from_record(record)?;
    } else if let Some(hash) = key.strip_prefix(product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX) {
        aircraft_definition_from_record(hash, record)?;
    } else if let Some(hash) = key.strip_prefix(AIRCRAFT_LIBRARY_RECORD_PREFIX) {
        product_contracts::validate_aircraft_definition_hash(hash).map_err(cloud_error)?;
        aircraft_library_membership_from_record(record)?;
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
    let mut plan = plan.clone().normalized();
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

fn derive_acs_payload_key(secret: &[u8; 32]) -> AppResult<[u8; 32]> {
    let hkdf = Hkdf::<Sha256>::new(Some(ACS_KDF_SALT), secret);
    let mut key = [0_u8; 32];
    hkdf.expand(ACS_KDF_PAYLOAD_ENCRYPTION_LABEL, &mut key)
        .map_err(|_| cloud_error("Aerobag Cloud payload key derivation failed"))?;
    Ok(key)
}

fn account_tag(secret: &[u8; 32]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"aerobag-cloud-account-locator-v1");
    hash.update(secret);
    hex_bytes(&hash.finalize()[..16])
}

pub(crate) fn random_bytes<const N: usize>() -> AppResult<[u8; N]> {
    let mut bytes = [0_u8; N];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| cloud_error(format!("secure random generation failed: {error}")))?;
    Ok(bytes)
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

fn cloud_provider_failure_detail(
    provider: CloudProviderKind,
    _kind: CloudProviderErrorKind,
    fallback: &str,
    gate: Option<AcsRateLimitGate>,
    retry_after_ms: Option<u64>,
) -> String {
    if provider != CloudProviderKind::AerobagCloud {
        return fallback.to_string();
    }
    let retry = retry_after_ms.map(compact_retry_duration);
    match gate {
        Some(AcsRateLimitGate::AccountCreationNetwork) => format!(
            "This network has created several Sync Accounts recently. Try again{}.",
            retry
                .as_deref()
                .map(|duration| format!(" in {duration}"))
                .unwrap_or_default()
        ),
        Some(AcsRateLimitGate::AccountCreationGlobal) => format!(
            "Aerobag Cloud is temporarily limiting new Sync Accounts. Try again{}. Existing accounts are unaffected.",
            retry
                .as_deref()
                .map(|duration| format!(" in {duration}"))
                .unwrap_or_default()
        ),
        _ => fallback.to_string(),
    }
}

fn compact_retry_duration(retry_after_ms: u64) -> String {
    let total_minutes = retry_after_ms.div_ceil(60_000).max(1);
    let days = total_minutes / (24 * 60);
    let hours = (total_minutes % (24 * 60)) / 60;
    let minutes = total_minutes % 60;
    if days > 0 {
        if hours > 0 {
            format!("{days}d {hours}h")
        } else {
            format!("{days}d")
        }
    } else if hours > 0 {
        if minutes > 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{hours}h")
        }
    } else {
        format!("{minutes}m")
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

    fn bundled_private_aircraft() -> product_contracts::AircraftDefinition {
        serde_json::from_str(include_str!(
            "../../../../../product/preprocessor/preprocessor-cli/resources/aircraft/piper-pa46-310p.json"
        ))
        .expect("bundled PA46 definition")
    }

    fn execute_acs(
        provider: &mut crate::cloud_acs_memory::InMemoryAcsProvider,
        request: &CloudProviderRequest,
        account_locator: &str,
        now: i64,
    ) -> CloudHttpResponse {
        use crate::cloud_acs_memory::AcsMemoryDelivery;
        let (status_code, body) = match &request.operation {
            CloudProviderOperation::AcsIssueAccountChallenge => (
                200,
                serde_json::to_vec(&AcsCreationChallengeResponse {
                    contract_id: product_contracts::ACS_CONTRACT_ID.to_string(),
                    challenge: "test-challenge".to_string(),
                    expires_at_epoch_ms: now + 60_000,
                    server_time_epoch_ms: now,
                })
                .unwrap(),
            ),
            CloudProviderOperation::AcsCreateAccount { request } => {
                provider.create_account(&request.account_locator).unwrap();
                (
                    201,
                    serde_json::to_vec(&AcsCreateAccountResponse {
                        contract_id: product_contracts::ACS_CONTRACT_ID.to_string(),
                        account_locator: request.account_locator.clone(),
                        server_time_epoch_ms: now,
                        quota_class: "anonymous".to_string(),
                        quota_bytes: 1024 * 1024,
                    })
                    .unwrap(),
                )
            }
            CloudProviderOperation::AcsCreateObject { request } => (
                200,
                serde_json::to_vec(
                    &provider
                        .create_object(account_locator, request.clone(), now)
                        .unwrap(),
                )
                .unwrap(),
            ),
            CloudProviderOperation::AcsReadObject { id } => {
                match provider.read_object(account_locator, id).unwrap() {
                    Some(object) => (200, serde_json::to_vec(&object).unwrap()),
                    None => (404, Vec::new()),
                }
            }
            CloudProviderOperation::AcsReadRoot => match provider.root(account_locator).unwrap() {
                Some(root) => (200, serde_json::to_vec(&root).unwrap()),
                None => (404, Vec::new()),
            },
            CloudProviderOperation::AcsCompareAndSwapRoot { request } => {
                match provider
                    .compare_and_swap_root(account_locator, request.clone(), now)
                    .unwrap()
                {
                    AcsMemoryDelivery::Delivered(response) => (
                        if matches!(response, AcsCompareAndSwapRootResponse::Conflict { .. }) {
                            409
                        } else {
                            200
                        },
                        serde_json::to_vec(&response).unwrap(),
                    ),
                    AcsMemoryDelivery::LostAfterCommit => panic!("unexpected lost test response"),
                }
            }
            CloudProviderOperation::AcsCreateSseTicket { .. } => (
                200,
                serde_json::to_vec(&AcsCreateSseTicketResponse {
                    contract_id: product_contracts::ACS_CONTRACT_ID.to_string(),
                    ticket: "test-ticket".to_string(),
                    expires_at_epoch_ms: now + 60_000,
                    events_url: "/cloud/v1/events?ticket=test-ticket".to_string(),
                })
                .unwrap(),
            ),
        };
        CloudHttpResponse::Completed {
            status_code,
            body_base64: URL_SAFE_NO_PAD.encode(body),
        }
    }

    fn pump_acs(
        engine: &mut CloudEngine,
        provider: &mut crate::cloud_acs_memory::InMemoryAcsProvider,
        now: i64,
    ) -> Vec<FlightPlan> {
        let mut remote_plans = Vec::new();
        for _ in 0..32 {
            let Some(http_request) = engine.take_provider_request(now).unwrap() else {
                return remote_plans;
            };
            let semantic_request = engine
                .provider_request_in_flight
                .as_ref()
                .expect("request is in flight")
                .clone();
            let account_locator = engine
                .account()
                .unwrap()
                .acs
                .as_ref()
                .unwrap()
                .account_locator
                .clone();
            let response = execute_acs(provider, &semantic_request, &account_locator, now);
            let completion = engine
                .complete_provider_request(http_request.request_id, response, now)
                .unwrap();
            remote_plans.extend(completion.remote_flight_plan().unwrap());
        }
        panic!("ACS cloud test pump did not quiesce");
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

    fn configured_engine() -> CloudEngine {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        engine
            .set_acs_default_base_url(Some("https://cloud.example/cloud/".to_string()))
            .unwrap();
        engine
    }

    fn read_page_workflow(purpose: ReadPurpose) -> CloudWorkflow {
        let stale_page_id = "stale-page".to_string();
        CloudWorkflow::AcsReadPage {
            root: AcsRootSnapshot {
                revision: 7,
                root_hash: "stale-root-hash".to_string(),
                value: AcsEncryptedValue::from_ciphertext(
                    b"stale root",
                    vec![stale_page_id.clone()],
                ),
                updated_at_epoch_ms: 1,
            },
            node: CloudNode {
                version: CLOUD_NODE_VERSION,
                generation: 7,
                published_at_epoch_ms: Some(1),
                parent_node_id: None,
                parent_node_hash: None,
                merkle_root_id: stale_page_id,
                merkle_root_hash: "stale-page-hash".to_string(),
                next_slot_id: "next-slot".to_string(),
            },
            purpose,
        }
    }

    fn create_account(
        engine: &mut CloudEngine,
        provider: &mut crate::cloud_acs_memory::InMemoryAcsProvider,
        initial: &FlightPlan,
        now: i64,
    ) -> String {
        engine
            .perform_action_at(CloudAction::BeginCreateAccount, initial, now)
            .unwrap();
        engine
            .perform_action_at(CloudAction::CreateAccount, initial, now)
            .unwrap();
        assert!(pump_acs(engine, provider, now).is_empty());
        engine.device_setup_code().unwrap()
    }

    fn link_account(
        provider: &mut crate::cloud_acs_memory::InMemoryAcsProvider,
        setup_code: String,
        now: i64,
    ) -> (CloudEngine, Vec<FlightPlan>) {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        engine
            .perform_action_at(
                CloudAction::AcceptDeviceSetupCode { setup_code },
                &FlightPlan::default(),
                now,
            )
            .unwrap();
        let plans = pump_acs(&mut engine, provider, now);
        (engine, plans)
    }

    fn active_panel(engine: &CloudEngine) -> UiCloudPanel {
        engine
            .page_state(0)
            .sync_account_panels
            .into_iter()
            .find(|panel| panel.state != UiCloudPanelState::Complete)
            .expect("active cloud panel")
    }

    #[test]
    fn aerobag_cloud_creates_and_links_through_fixed_root_cas() {
        let initial = plan(&["KPAE", "KAPA"]);
        let mut provider = crate::cloud_acs_memory::InMemoryAcsProvider::default();
        let mut source = configured_engine();
        let setup_code = create_account(&mut source, &mut provider, &initial, 1_000);

        assert!(source.has_linked_account());
        assert_eq!(
            source
                .account()
                .unwrap()
                .acs
                .as_ref()
                .unwrap()
                .root_revision,
            1
        );

        let (target, remote) = link_account(&mut provider, setup_code, 2_000);
        assert_eq!(remote, vec![initial]);
        assert!(target.has_linked_account());
        assert_eq!(
            target
                .account()
                .unwrap()
                .acs
                .as_ref()
                .unwrap()
                .root_revision,
            1
        );
    }

    #[test]
    fn account_creation_ui_has_no_provider_or_authorization_step() {
        let initial = plan(&["KRNT", "KPAE"]);
        let mut engine = configured_engine();
        assert_eq!(active_panel(&engine).id, "get_started");

        engine
            .perform_ui_action(CloudUiActionId::BeginCreate, &[], &initial, 1)
            .unwrap();
        let panel = active_panel(&engine);
        assert_eq!(panel.id, "create_account");
        assert!(panel
            .actions
            .iter()
            .any(|action| action.id == CloudUiActionId::CreateAccount && action.enabled));
        assert_eq!(
            engine
                .page_state(1)
                .provider_card
                .and_then(|card| card.summary),
            Some("Configured for https://cloud.example/cloud/".to_string())
        );

        engine
            .perform_ui_action(CloudUiActionId::CreateAccount, &[], &initial, 1)
            .unwrap();
        assert_eq!(active_panel(&engine).state, UiCloudPanelState::Working);
    }

    #[test]
    fn unconfigured_build_disables_account_creation() {
        let initial = plan(&["KRNT"]);
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        engine
            .perform_ui_action(CloudUiActionId::BeginCreate, &[], &initial, 1)
            .unwrap();
        let create = active_panel(&engine)
            .actions
            .into_iter()
            .find(|action| action.id == CloudUiActionId::CreateAccount)
            .unwrap();
        assert!(!create.enabled);
        assert!(create
            .disabled_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("no Aerobag Cloud server")));
        assert!(engine
            .perform_ui_action(CloudUiActionId::CreateAccount, &[], &initial, 1)
            .is_err());
    }

    #[test]
    fn qr_scanner_capability_enables_a_typed_setup_completion_effect() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        engine
            .perform_action(CloudAction::BeginSetupFromDevice, &FlightPlan::default())
            .unwrap();

        let disabled = engine.page_state_with_qr_scanner(0, false);
        let scan = disabled.sync_account_panels[1]
            .actions
            .iter()
            .find(|action| action.id == CloudUiActionId::ScanSetupCode)
            .unwrap();
        assert!(!scan.enabled);
        assert!(scan.platform_effect.is_none());

        let enabled = engine.page_state_with_qr_scanner(0, true);
        let scan = enabled.sync_account_panels[1]
            .actions
            .iter()
            .find(|action| action.id == CloudUiActionId::ScanSetupCode)
            .unwrap();
        assert_eq!(
            scan.platform_effect,
            Some(CloudPlatformEffect::ScanQrCode {
                completion_action: CloudUiActionId::AcceptSetupCode,
                field_id: CloudUiFieldId::DeviceSetupCode,
            })
        );
    }

    #[test]
    fn flight_plan_and_orthogonal_settings_crossfill_between_devices() {
        let initial = plan(&["KRNT", "KPAE"]);
        let edited = plan(&["KRNT", "KSEA", "KPAE"]);
        let definition = bundled_private_aircraft();
        let hash = definition.content_hash().unwrap();
        let mut provider = crate::cloud_acs_memory::InMemoryAcsProvider::default();
        let mut first = configured_engine();
        let setup_code = create_account(&mut first, &mut provider, &initial, 10);
        let (mut second, remote) = link_account(&mut provider, setup_code, 20);
        assert_eq!(remote, vec![initial.clone()]);

        first
            .record_local_flight_plan_mutation(&initial, &edited, 100)
            .unwrap();
        first
            .record_local_inactivity_sleep_timeout(InactivitySleepTimeout::TwoHours, 101)
            .unwrap();
        let nexrad_preferences = NexradAcquisitionPreferences {
            coverage: crate::NexradCoverageMode::ViewportOnly,
            ..NexradAcquisitionPreferences::default()
        };
        first
            .record_local_nexrad_acquisition(nexrad_preferences, 102)
            .unwrap();
        first
            .record_local_debug_flag(DebugFlagId::BadAutopilot, true, 103)
            .unwrap();
        first.record_local_aircraft_definition(&definition).unwrap();
        first
            .record_local_aircraft_library_membership(
                &hash,
                product_contracts::AircraftLibraryMembership { included: true },
                103,
            )
            .unwrap();
        assert!(pump_acs(&mut first, &mut provider, 110).is_empty());

        second
            .perform_action(CloudAction::SyncNow, &FlightPlan::default())
            .unwrap();
        assert_eq!(pump_acs(&mut second, &mut provider, 120), vec![edited]);
        assert_eq!(
            second.inactivity_sleep_timeout().unwrap(),
            Some(InactivitySleepTimeout::TwoHours)
        );
        assert_eq!(
            second.nexrad_acquisition().unwrap(),
            Some(nexrad_preferences)
        );
        assert!(second
            .debug_flags()
            .unwrap()
            .contains(&(DebugFlagId::BadAutopilot, true)));
        assert_eq!(
            second.aircraft_definitions().unwrap().get(&hash),
            Some(&definition)
        );
        assert_eq!(
            second.aircraft_library_memberships().unwrap(),
            BTreeMap::from([(
                hash.clone(),
                product_contracts::AircraftLibraryMembership { included: true },
            )])
        );

        second
            .record_local_aircraft_library_membership(
                &hash,
                product_contracts::AircraftLibraryMembership { included: false },
                130,
            )
            .unwrap();
        assert!(pump_acs(&mut second, &mut provider, 140).is_empty());
        first
            .perform_action(CloudAction::SyncNow, &FlightPlan::default())
            .unwrap();
        assert!(pump_acs(&mut first, &mut provider, 150).is_empty());
        assert_eq!(
            first.aircraft_library_memberships().unwrap().get(&hash),
            Some(&product_contracts::AircraftLibraryMembership { included: false })
        );
        assert_eq!(
            first.aircraft_definitions().unwrap().get(&hash),
            Some(&definition)
        );
    }

    #[test]
    fn newer_remote_record_wins_a_publish_race() {
        let initial = plan(&["KRNT", "KPAE"]);
        let earlier_edit = plan(&["KRNT", "KSEA", "KPAE"]);
        let later_edit = plan(&["KRNT", "KSUS", "KPAE"]);
        let mut provider = crate::cloud_acs_memory::InMemoryAcsProvider::default();
        let mut first = configured_engine();
        let setup_code = create_account(&mut first, &mut provider, &initial, 10);
        let (mut second, _) = link_account(&mut provider, setup_code, 20);

        first
            .record_local_flight_plan_mutation(&initial, &earlier_edit, 100)
            .unwrap();
        second
            .record_local_flight_plan_mutation(&initial, &later_edit, 200)
            .unwrap();
        assert!(pump_acs(&mut second, &mut provider, 210).is_empty());
        assert_eq!(
            pump_acs(&mut first, &mut provider, 300),
            vec![later_edit.clone()]
        );
        assert_eq!(first.cached_flight_plan(), Some(later_edit));
        assert!(first.persistent.records.pending_keys.is_empty());
    }

    #[test]
    fn account_creation_rate_limit_messages_are_core_owned() {
        assert_eq!(
            cloud_provider_failure_detail(
                CloudProviderKind::AerobagCloud,
                CloudProviderErrorKind::Transient,
                "ignored server prose",
                Some(AcsRateLimitGate::AccountCreationNetwork),
                Some(28_800_000),
            ),
            "This network has created several Sync Accounts recently. Try again in 8h."
        );
        assert_eq!(
            cloud_provider_failure_detail(
                CloudProviderKind::AerobagCloud,
                CloudProviderErrorKind::Transient,
                "ignored server prose",
                Some(AcsRateLimitGate::AccountCreationGlobal),
                Some(8_640_000),
            ),
            "Aerobag Cloud is temporarily limiting new Sync Accounts. Try again in 2h 24m. Existing accounts are unaffected."
        );
    }

    #[test]
    fn aircraft_definition_record_rejects_a_content_hash_mismatch() {
        let definition = bundled_private_aircraft();
        let wrong_hash = "0".repeat(64);
        let record = CloudRecord {
            schema_version: product_contracts::AIRCRAFT_DEFINITION_SCHEMA_VERSION,
            modified_at_epoch_ms: None,
            value: serde_json::to_value(definition).unwrap(),
        };

        let error = validate_known_record(
            &product_contracts::aircraft_definition_key(&wrong_hash).unwrap(),
            &record,
        )
        .expect_err("mismatched hash must be rejected");
        assert!(error.message.contains("hash mismatch"));
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

    #[test]
    fn unconfigured_engine_has_no_provider_work() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        assert!(engine.take_provider_request(0).unwrap().is_none());
    }

    #[test]
    fn persisted_page_read_restarts_from_current_root() {
        let mut persistent = CloudPersistentState {
            workflow: Some(read_page_workflow(ReadPurpose::Poll)),
            ..CloudPersistentState::default()
        };

        let engine = CloudEngine::new(persistent.clone());
        assert_eq!(
            engine.persistent.workflow,
            Some(CloudWorkflow::AcsReadRoot {
                purpose: ReadPurpose::Poll
            })
        );

        persistent.workflow = Some(CloudWorkflow::AcsReadRoot {
            purpose: ReadPurpose::Link,
        });
        let engine = CloudEngine::new(persistent);
        assert_eq!(
            engine.persistent.workflow,
            Some(CloudWorkflow::AcsReadRoot {
                purpose: ReadPurpose::Link
            })
        );
    }

    #[test]
    fn invalid_provider_completion_commits_failed_status_and_stops_retrying() {
        let mut provider = crate::cloud_acs_memory::InMemoryAcsProvider::default();
        let mut engine = configured_engine();
        create_account(&mut engine, &mut provider, &plan(&["KRNT", "KPAE"]), 1_000);
        engine.persistent.workflow = Some(read_page_workflow(ReadPurpose::Poll));

        let request = engine.take_provider_request(2_000).unwrap().unwrap();
        assert!(matches!(
            engine
                .provider_request_in_flight
                .as_ref()
                .map(|request| &request.operation),
            Some(CloudProviderOperation::AcsReadObject { id }) if id == "stale-page"
        ));
        let completion = engine
            .complete_provider_request(
                request.request_id,
                CloudHttpResponse::Completed {
                    status_code: 404,
                    body_base64: String::new(),
                },
                2_001,
            )
            .expect("invalid provider data must commit a cloud failure");

        assert!(completion.changed_records.is_empty());
        assert!(engine.provider_request_in_flight.is_none());
        assert!(engine.persistent.workflow.is_none());
        assert!(engine.acs_event_stream_plan.is_none());
        assert_eq!(
            engine.persistent.last_provider_failure,
            Some(CloudProviderFailure {
                kind: CloudProviderErrorKind::Permanent,
                detail: "Aerobag Cloud state page stale-page is missing".to_string(),
            })
        );
        let summary = engine.status_summary(2_001);
        assert_eq!(summary.label, "FAILED");
        assert_eq!(summary.severity, UiStatusSeverity::Caution);
        assert_eq!(
            summary.detail,
            "Aerobag Cloud state page stale-page is missing"
        );
        let record = engine.status_record(2_001).expect("visible failure");
        assert_eq!(record.value.as_deref(), Some("FAILED"));
        assert_eq!(record.severity, UiStatusSeverity::Caution);
        assert!(record.drives_caution);
        assert!(engine.take_provider_request(3_000).unwrap().is_none());

        engine
            .perform_action(CloudAction::SyncNow, &FlightPlan::default())
            .unwrap();
        engine.take_provider_request(3_001).unwrap().unwrap();
        assert!(matches!(
            engine
                .provider_request_in_flight
                .as_ref()
                .map(|request| &request.operation),
            Some(CloudProviderOperation::AcsReadRoot)
        ));
    }
}
