// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

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
    AppError, AppErrorKind, AppResult, FlightPlan,
};

const CLOUD_PERSISTENCE_VERSION: u32 = 1;
const CLOUD_ENVELOPE_VERSION: u32 = 1;
const CLOUD_PAGE_VERSION: u32 = 1;
const CLOUD_NODE_VERSION: u32 = 1;
const CLOUD_PAIRING_VERSION: u32 = 1;
const FLIGHT_PLAN_RECORD_KEY: &str = "flight_plan/current";
const FLIGHT_PLAN_SCHEMA_VERSION: u32 = 1;
const CLOUD_POLL_INTERVAL_MS: i64 = 3_000;
const CLOUD_TRANSIENT_RETRY_MS: i64 = 5_000;
pub const CLOUD_STATUS_ID: &str = "cloud:provider";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CloudProviderKind {
    #[default]
    GoogleDrive,
}

impl CloudProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::GoogleDrive => "Google Drive",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CloudCredentialState {
    #[default]
    Disconnected,
    Connecting,
    Ready {
        #[serde(default)]
        expires_at_epoch_ms: Option<i64>,
    },
    NeedsUserAction {
        detail: String,
    },
    TransientFailure {
        detail: String,
    },
    Failed {
        detail: String,
    },
}

impl CloudCredentialState {
    pub fn is_ready(&self, now_epoch_ms: i64) -> bool {
        match self {
            Self::Ready {
                expires_at_epoch_ms,
            } => expires_at_epoch_ms.is_none_or(|expires| expires > now_epoch_ms),
            Self::TransientFailure { .. } => true,
            _ => false,
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Disconnected => "DISCONNECTED".to_string(),
            Self::Connecting => "CONNECTING".to_string(),
            Self::Ready { .. } => "CONNECTED".to_string(),
            Self::NeedsUserAction { .. } => "AUTHORIZATION REQUIRED".to_string(),
            Self::TransientFailure { .. } => "TEMPORARILY UNAVAILABLE".to_string(),
            Self::Failed { .. } => "FAILED".to_string(),
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::NeedsUserAction { detail }
            | Self::TransientFailure { detail }
            | Self::Failed { detail } => Some(detail),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudProviderErrorKind {
    Unauthorized,
    Transient,
    Permanent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum CloudProviderOperation {
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
    Delete {
        id: String,
    },
    List {
        #[serde(default)]
        page_token: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudProviderObject {
    pub id: String,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudProviderRequest {
    pub request_id: u64,
    pub provider: CloudProviderKind,
    pub operation: CloudProviderOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum CloudProviderResponse {
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_page_token: Option<String>,
    },
    Error {
        kind: CloudProviderErrorKind,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudAction {
    SelectProvider { provider: CloudProviderKind },
    CreateAccount,
    LinkExisting { pairing_token: String },
    RevealPairingToken,
    HidePairingToken,
    SyncNow,
    Disconnect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiCloudAction {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiCloudPageFact {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiCloudProviderOption {
    pub id: CloudProviderKind,
    pub label: String,
    pub selected: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiCloudPageState {
    pub title: String,
    pub provider_label: String,
    pub connection_label: String,
    pub account_label: String,
    pub summary: String,
    pub provider_options: Vec<UiCloudProviderOption>,
    pub facts: Vec<UiCloudPageFact>,
    pub actions: Vec<UiCloudAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairing_token: Option<String>,
    pub pairing_input_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CloudRecord {
    schema_version: u32,
    value: serde_json::Value,
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
struct PairingPayload {
    version: u32,
    provider: CloudProviderKind,
    genesis_id: String,
    root_secret_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct VerifiedTip {
    node_id: String,
    node_hash: String,
    generation: u64,
    merkle_root_id: String,
    merkle_root_hash: String,
    next_slot_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CloudAccount {
    provider: CloudProviderKind,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPersistentState {
    version: u32,
    provider: CloudProviderKind,
    #[serde(default)]
    account: Option<CloudAccount>,
    #[serde(default)]
    workflow: Option<CloudWorkflow>,
    #[serde(default)]
    cached_flight_plan: Option<FlightPlan>,
    #[serde(default)]
    pending_flight_plan: Option<FlightPlan>,
    #[serde(default)]
    pending_remote_flight_plan: Option<FlightPlan>,
    #[serde(default)]
    local_revision: u64,
    #[serde(default)]
    next_request_id: u64,
    #[serde(default)]
    last_success_epoch_ms: Option<i64>,
    #[serde(default)]
    last_poll_epoch_ms: Option<i64>,
    #[serde(default)]
    next_retry_epoch_ms: Option<i64>,
    #[serde(default)]
    force_poll: bool,
    #[serde(default)]
    last_error: Option<String>,
}

impl Default for CloudPersistentState {
    fn default() -> Self {
        Self {
            version: CLOUD_PERSISTENCE_VERSION,
            provider: CloudProviderKind::GoogleDrive,
            account: None,
            workflow: None,
            cached_flight_plan: None,
            pending_flight_plan: None,
            pending_remote_flight_plan: None,
            local_revision: 0,
            next_request_id: 1,
            last_success_epoch_ms: None,
            last_poll_epoch_ms: None,
            next_retry_epoch_ms: None,
            force_poll: false,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CloudEngine {
    persistent: CloudPersistentState,
    credential: CloudCredentialState,
    request_in_flight: Option<u64>,
    reveal_pairing_token: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CloudCompletion {
    pub remote_flight_plan: Option<FlightPlan>,
}

impl CloudEngine {
    pub fn new(persistent: CloudPersistentState) -> Self {
        Self {
            persistent,
            credential: CloudCredentialState::Disconnected,
            request_in_flight: None,
            reveal_pairing_token: false,
        }
    }

    pub fn persistent(&self) -> &CloudPersistentState {
        &self.persistent
    }

    pub fn cached_flight_plan(&self) -> Option<&FlightPlan> {
        self.persistent.cached_flight_plan.as_ref()
    }

    pub fn pending_remote_flight_plan(&self) -> Option<&FlightPlan> {
        self.persistent.pending_remote_flight_plan.as_ref()
    }

    pub fn clear_pending_remote_flight_plan(&mut self) {
        self.persistent.pending_remote_flight_plan = None;
    }

    pub fn set_credential_state(&mut self, state: CloudCredentialState) {
        self.credential = state;
        self.request_in_flight = None;
        if matches!(self.credential, CloudCredentialState::Ready { .. }) {
            self.persistent.next_retry_epoch_ms = None;
            self.persistent.last_error = None;
            self.persistent.force_poll = true;
        }
    }

    pub fn credential_state(&self) -> &CloudCredentialState {
        &self.credential
    }

    pub fn record_local_flight_plan(&mut self, plan: &FlightPlan) {
        let definition = cloud_flight_plan_definition(plan);
        if self.persistent.cached_flight_plan.as_ref() == Some(&definition) {
            return;
        }
        self.persistent.local_revision = self.persistent.local_revision.saturating_add(1);
        self.persistent.cached_flight_plan = Some(definition.clone());
        if self.persistent.account.is_some() {
            self.persistent.pending_flight_plan = Some(definition);
        }
    }

    pub fn perform_action(
        &mut self,
        action: CloudAction,
        current_plan: &FlightPlan,
    ) -> AppResult<()> {
        match action {
            CloudAction::SelectProvider { provider } => {
                if self.persistent.account.is_some() || self.persistent.workflow.is_some() {
                    return Err(cloud_error(
                        "disconnect the current cloud account before changing providers",
                    ));
                }
                self.persistent.provider = provider;
                self.persistent.last_error = None;
            }
            CloudAction::CreateAccount => {
                if self.persistent.account.is_some() || self.persistent.workflow.is_some() {
                    return Err(cloud_error(
                        "a cloud account is already linked or being created",
                    ));
                }
                let secret = random_bytes::<32>()?;
                self.persistent.account = Some(CloudAccount {
                    provider: self.persistent.provider,
                    root_secret_base64: URL_SAFE_NO_PAD.encode(secret),
                    genesis_id: String::new(),
                    tip: None,
                });
                let plan = cloud_flight_plan_definition(current_plan);
                self.persistent.cached_flight_plan = Some(plan.clone());
                self.persistent.pending_flight_plan = Some(plan);
                self.persistent.workflow = Some(CloudWorkflow::CreateAwaitIds);
                self.persistent.last_error = None;
            }
            CloudAction::LinkExisting { pairing_token } => {
                if self.persistent.workflow.is_some() {
                    return Err(cloud_error("cloud synchronization is already busy"));
                }
                let payload = decode_pairing_token(&pairing_token)?;
                self.persistent.provider = payload.provider;
                self.persistent.account = Some(CloudAccount {
                    provider: payload.provider,
                    root_secret_base64: payload.root_secret_base64,
                    genesis_id: payload.genesis_id.clone(),
                    tip: None,
                });
                self.persistent.pending_flight_plan = None;
                self.persistent.workflow = Some(CloudWorkflow::ReadNode {
                    node_id: payload.genesis_id,
                    purpose: ReadPurpose::Link,
                });
                self.persistent.last_error = None;
            }
            CloudAction::RevealPairingToken => {
                if self
                    .persistent
                    .account
                    .as_ref()
                    .is_some_and(|account| !account.genesis_id.is_empty() && account.tip.is_some())
                {
                    self.reveal_pairing_token = true;
                }
            }
            CloudAction::HidePairingToken => self.reveal_pairing_token = false,
            CloudAction::SyncNow => self.persistent.force_poll = true,
            CloudAction::Disconnect => {
                self.credential = CloudCredentialState::Disconnected;
                self.request_in_flight = None;
            }
        }
        Ok(())
    }

    pub fn take_provider_request(
        &mut self,
        now_epoch_ms: i64,
    ) -> AppResult<Option<CloudProviderRequest>> {
        if self.request_in_flight.is_some()
            || !self.credential.is_ready(now_epoch_ms)
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
        self.request_in_flight = Some(request_id);
        Ok(Some(CloudProviderRequest {
            request_id,
            provider: self.persistent.provider,
            operation,
        }))
    }

    pub fn complete_provider_request(
        &mut self,
        request_id: u64,
        response: CloudProviderResponse,
        now_epoch_ms: i64,
    ) -> AppResult<CloudCompletion> {
        if self.request_in_flight != Some(request_id) {
            return Err(cloud_error(format!(
                "cloud provider response {request_id} does not match in-flight request {:?}",
                self.request_in_flight
            )));
        }
        self.request_in_flight = None;
        if let CloudProviderResponse::Error { kind, detail } = response {
            self.persistent.last_error = Some(detail.clone());
            match kind {
                CloudProviderErrorKind::Unauthorized => {
                    self.credential = CloudCredentialState::NeedsUserAction { detail };
                }
                CloudProviderErrorKind::Transient => {
                    self.credential = CloudCredentialState::TransientFailure { detail };
                    self.persistent.next_retry_epoch_ms =
                        Some(now_epoch_ms.saturating_add(CLOUD_TRANSIENT_RETRY_MS));
                }
                CloudProviderErrorKind::Permanent => {
                    self.credential = CloudCredentialState::Failed { detail };
                }
            }
            return Ok(CloudCompletion::default());
        }

        self.persistent.last_error = None;
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
        if self.persistent.pending_flight_plan.is_some() {
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
                let account = self.account()?.clone();
                let staged = self.stage_publication(
                    PublicationPurpose::CreateAccount,
                    ids[0].clone(),
                    ids[1].clone(),
                    ids[2].clone(),
                    0,
                    None,
                )?;
                self.account_mut()?.genesis_id = staged.node_id.clone();
                self.persistent.workflow = Some(CloudWorkflow::CreatePage { staged });
                debug_assert_eq!(account.provider, self.persistent.provider);
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
                                "the pairing token's cloud account was not found",
                            ));
                        }
                        ReadPurpose::Poll | ReadPurpose::PublishRace => {
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
                let remote_plan = flight_plan_from_page(&page)?;
                self.account_mut()?.tip = Some(VerifiedTip {
                    node_id,
                    node_hash,
                    generation: node.generation,
                    merkle_root_id: node.merkle_root_id,
                    merkle_root_hash: node.merkle_root_hash,
                    next_slot_id: node.next_slot_id,
                });
                self.persistent.last_poll_epoch_ms = Some(now_epoch_ms);
                self.persistent.workflow = None;
                let should_adopt = matches!(purpose, ReadPurpose::Link)
                    || self.persistent.pending_flight_plan.is_none();
                if should_adopt {
                    self.persistent.cached_flight_plan = Some(remote_plan.clone());
                    return Ok(CloudCompletion {
                        remote_flight_plan: Some(remote_plan),
                    });
                }
            }
        }
        Ok(CloudCompletion::default())
    }

    fn stage_publication(
        &self,
        purpose: PublicationPurpose,
        node_id: String,
        page_id: String,
        next_slot_id: String,
        generation: u64,
        parent: Option<&VerifiedTip>,
    ) -> AppResult<StagedPublication> {
        let plan = self
            .persistent
            .pending_flight_plan
            .as_ref()
            .or(self.persistent.cached_flight_plan.as_ref())
            .ok_or_else(|| cloud_error("cloud publication has no flight plan record"))?;
        let page = page_for_flight_plan(plan)?;
        let page_bytes = self.encrypt(&page, "merkle_page")?;
        let page_hash = sha256_hex(&page_bytes);
        let node = CloudNode {
            version: CLOUD_NODE_VERSION,
            generation,
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
            local_revision: self.persistent.local_revision,
        })
    }

    fn finish_publication(&mut self, staged: StagedPublication) -> AppResult<()> {
        self.account_mut()?.tip = Some(VerifiedTip {
            node_id: staged.node_id,
            node_hash: staged.node_hash,
            generation: staged.generation,
            merkle_root_id: staged.page_id,
            merkle_root_hash: staged.merkle_root_hash,
            next_slot_id: staged.next_slot_id,
        });
        if self.persistent.local_revision == staged.local_revision {
            self.persistent.pending_flight_plan = None;
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
                        "pairing token did not resolve to a valid genesis node",
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

    pub fn set_pending_remote_flight_plan(&mut self, plan: FlightPlan) {
        self.persistent.pending_remote_flight_plan = Some(plan);
    }

    pub fn page_state(&self) -> UiCloudPageState {
        let linked = self
            .persistent
            .account
            .as_ref()
            .is_some_and(|account| !account.genesis_id.is_empty() && account.tip.is_some());
        let busy = self.persistent.workflow.is_some() || self.request_in_flight.is_some();
        let ready = matches!(self.credential, CloudCredentialState::Ready { .. });
        let pairing_token = (linked && self.reveal_pairing_token)
            .then(|| self.pairing_token())
            .transpose()
            .ok()
            .flatten();
        let generation = self
            .persistent
            .account
            .as_ref()
            .and_then(|account| account.tip.as_ref())
            .map(|tip| tip.generation.to_string())
            .unwrap_or_else(|| "-".to_string());
        let outbox_count = usize::from(self.persistent.pending_flight_plan.is_some());
        let mut facts = vec![
            UiCloudPageFact {
                label: "Cloud generation".to_string(),
                value: generation,
            },
            UiCloudPageFact {
                label: "Pending local records".to_string(),
                value: outbox_count.to_string(),
            },
        ];
        if let Some(last_success) = self.persistent.last_success_epoch_ms {
            facts.push(UiCloudPageFact {
                label: "Last provider success".to_string(),
                value: format_epoch_ms_utc(last_success),
            });
        }
        let mut summary = if ready && !linked {
            "Google Drive is connected. Create a new Aerobag cloud account, or link an existing one with its pairing token."
                .to_string()
        } else {
            self.credential
                .detail()
                .unwrap_or(if linked {
                    "Aerobag cloud account is linked."
                } else {
                    "Connect Google Drive, then create or link an Aerobag cloud account."
                })
                .to_string()
        };
        if busy {
            summary = "Cloud synchronization is in progress.".to_string();
        }
        let action = |id: &str, label: &str, enabled: bool, reason: &str| UiCloudAction {
            id: id.to_string(),
            label: label.to_string(),
            enabled,
            disabled_reason: (!enabled).then(|| reason.to_string()),
        };
        UiCloudPageState {
            title: "Cloud".to_string(),
            provider_label: self.persistent.provider.label().to_string(),
            connection_label: self.credential.label(),
            account_label: if linked {
                "LINKED".to_string()
            } else {
                "NOT LINKED".to_string()
            },
            summary,
            provider_options: vec![UiCloudProviderOption {
                id: CloudProviderKind::GoogleDrive,
                label: CloudProviderKind::GoogleDrive.label().to_string(),
                selected: self.persistent.provider == CloudProviderKind::GoogleDrive,
                enabled: !linked && !busy,
            }],
            facts,
            actions: vec![
                action(
                    "connect",
                    if ready {
                        "Reconnect Google Drive"
                    } else {
                        "Connect Google Drive"
                    },
                    !busy,
                    "Cloud synchronization is busy.",
                ),
                action(
                    "create_account",
                    "Create Aerobag account",
                    ready && !linked && !busy,
                    "Connect Google Drive and disconnect any existing account first.",
                ),
                action(
                    "link_existing",
                    "Link Aerobag account",
                    ready && !linked && !busy,
                    "Connect Google Drive and disconnect any existing account first.",
                ),
                action(
                    if self.reveal_pairing_token {
                        "hide_pairing"
                    } else {
                        "reveal_pairing"
                    },
                    if self.reveal_pairing_token {
                        "Hide pairing token"
                    } else {
                        "Pair another device"
                    },
                    linked && !busy,
                    "Create or link a cloud account first.",
                ),
                action(
                    "sync_now",
                    "Sync now",
                    ready && linked && !busy,
                    "Connect Google Drive and finish the current synchronization first.",
                ),
                action(
                    "disconnect",
                    "Disconnect Google Drive",
                    !matches!(self.credential, CloudCredentialState::Disconnected),
                    "Google Drive is already disconnected.",
                ),
            ],
            pairing_token,
            pairing_input_enabled: ready && !linked && !busy,
        }
    }

    pub fn status_record(&self) -> Option<DataStatusRecord> {
        let linked = self.persistent.account.is_some();
        match &self.credential {
            CloudCredentialState::NeedsUserAction { detail } if linked => {
                Some(DataStatusRecord::new(
                    CLOUD_STATUS_ID,
                    "CLOUD",
                    Some("AUTH".to_string()),
                    UiStatusSeverity::Caution,
                    true,
                    detail.clone(),
                ))
            }
            CloudCredentialState::Failed { detail } if linked => Some(DataStatusRecord::new(
                CLOUD_STATUS_ID,
                "CLOUD",
                Some("FAILED".to_string()),
                UiStatusSeverity::Caution,
                true,
                detail.clone(),
            )),
            CloudCredentialState::Disconnected if linked => Some(DataStatusRecord::new(
                CLOUD_STATUS_ID,
                "CLOUD",
                Some("AUTH".to_string()),
                UiStatusSeverity::Caution,
                true,
                "Cloud synchronization requires Google Drive authorization.".to_string(),
            )),
            CloudCredentialState::TransientFailure { detail } if linked => {
                Some(DataStatusRecord::new(
                    CLOUD_STATUS_ID,
                    "CLOUD",
                    Some("OFFLINE".to_string()),
                    UiStatusSeverity::Info,
                    false,
                    detail.clone(),
                ))
            }
            _ => None,
        }
    }

    pub fn pairing_token(&self) -> AppResult<String> {
        let account = self.account()?;
        if account.genesis_id.is_empty() || account.tip.is_none() {
            return Err(cloud_error("cloud account creation has not completed"));
        }
        encode_pairing_token(&PairingPayload {
            version: CLOUD_PAIRING_VERSION,
            provider: account.provider,
            genesis_id: account.genesis_id.clone(),
            root_secret_base64: account.root_secret_base64.clone(),
        })
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

fn page_for_flight_plan(plan: &FlightPlan) -> AppResult<CloudPage> {
    let mut records = BTreeMap::new();
    records.insert(
        FLIGHT_PLAN_RECORD_KEY.to_string(),
        CloudRecord {
            schema_version: FLIGHT_PLAN_SCHEMA_VERSION,
            value: serde_json::to_value(plan).map_err(cloud_json_error)?,
        },
    );
    Ok(CloudPage {
        version: CLOUD_PAGE_VERSION,
        records,
    })
}

fn flight_plan_from_page(page: &CloudPage) -> AppResult<FlightPlan> {
    if page.version != CLOUD_PAGE_VERSION {
        return Err(cloud_error(format!(
            "unsupported cloud page version {}",
            page.version
        )));
    }
    let record = page
        .records
        .get(FLIGHT_PLAN_RECORD_KEY)
        .ok_or_else(|| cloud_error("cloud page has no flight-plan record"))?;
    if record.schema_version != FLIGHT_PLAN_SCHEMA_VERSION {
        return Err(cloud_error(format!(
            "unsupported cloud flight-plan schema {}",
            record.schema_version
        )));
    }
    serde_json::from_value(record.value.clone()).map_err(cloud_json_error)
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

fn encode_pairing_token(payload: &PairingPayload) -> AppResult<String> {
    let bytes = serde_json::to_vec(payload).map_err(cloud_json_error)?;
    let body = URL_SAFE_NO_PAD.encode(&bytes);
    let checksum = &sha256_hex(&bytes)[..16];
    Ok(format!("AB1.{body}.{checksum}"))
}

fn decode_pairing_token(token: &str) -> AppResult<PairingPayload> {
    let parts = token.trim().split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0] != "AB1" {
        return Err(cloud_error("pairing token has an unsupported format"));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|_| cloud_error("pairing token is not valid base64"))?;
    if &sha256_hex(&bytes)[..16] != parts[2] {
        return Err(cloud_error("pairing token checksum does not match"));
    }
    let payload: PairingPayload = serde_json::from_slice(&bytes).map_err(cloud_json_error)?;
    if payload.version != CLOUD_PAIRING_VERSION
        || payload.genesis_id.trim().is_empty()
        || URL_SAFE_NO_PAD
            .decode(&payload.root_secret_base64)
            .ok()
            .is_none_or(|bytes| bytes.len() != 32)
    {
        return Err(cloud_error("pairing token content is invalid"));
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

    fn pump(engine: &mut CloudEngine, provider: &mut MemoryProvider, now: i64) -> Vec<FlightPlan> {
        let mut remote_plans = Vec::new();
        for _ in 0..32 {
            let Some(request) = engine.take_provider_request(now).unwrap() else {
                return remote_plans;
            };
            let response = provider.execute(&request);
            let completion = engine
                .complete_provider_request(request.request_id, response, now)
                .unwrap();
            remote_plans.extend(completion.remote_flight_plan);
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
        engine.set_credential_state(CloudCredentialState::Ready {
            expires_at_epoch_ms: Some(100_000),
        });
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
        engine
            .complete_provider_request(request.request_id, response, now)
            .unwrap()
    }

    fn create_account(engine: &mut CloudEngine, initial: &FlightPlan) -> String {
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
        engine.pairing_token().unwrap()
    }

    #[test]
    fn account_creation_emits_encrypted_create_once_chain_and_pairing_token() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        let pairing = create_account(&mut engine, &plan(&["KRNT", "KPAE"]));
        assert!(pairing.starts_with("AB1."));
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
        assert!(engine.persistent.pending_flight_plan.is_none());
        assert!(!pairing.contains("KRNT"));
    }

    #[test]
    fn authorization_failure_drives_caution_but_network_failure_does_not() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        create_account(&mut engine, &plan(&["KRNT", "KPAE"]));
        engine.set_credential_state(CloudCredentialState::NeedsUserAction {
            detail: "Authorize Google Drive again.".into(),
        });
        let auth = engine.status_record().unwrap();
        assert_eq!(auth.severity, UiStatusSeverity::Caution);
        assert!(auth.drives_caution);

        engine.set_credential_state(CloudCredentialState::TransientFailure {
            detail: "Network is unavailable.".into(),
        });
        let network = engine.status_record().unwrap();
        assert_eq!(network.severity, UiStatusSeverity::Info);
        assert!(!network.drives_caution);
    }

    #[test]
    fn local_plan_mutation_becomes_durable_outbox_work() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        create_account(&mut engine, &plan(&["KRNT", "KPAE"]));
        engine.record_local_flight_plan(&plan(&["KRNT", "KSEA", "KPAE"]));
        let request = engine.take_provider_request(10).unwrap().unwrap();
        assert_eq!(
            request.operation,
            CloudProviderOperation::AllocateIds { count: 2 }
        );
        assert!(engine.persistent.pending_flight_plan.is_some());
    }

    #[test]
    fn pairing_token_checksum_rejects_typo() {
        let mut engine = CloudEngine::new(CloudPersistentState::default());
        let mut pairing = create_account(&mut engine, &plan(&["KRNT", "KPAE"]));
        pairing.push('x');
        assert!(decode_pairing_token(&pairing).is_err());
    }

    #[test]
    fn two_independent_clients_crossfill_flight_plan_through_provider_protocol() {
        let initial = plan(&["KRNT", "KPAE"]);
        let changed = plan(&["KRNT", "KSEA", "KPAE"]);
        let mut provider = MemoryProvider::default();
        let mut first = CloudEngine::new(CloudPersistentState::default());
        ready(&mut first);
        first
            .perform_action(CloudAction::CreateAccount, &initial)
            .unwrap();
        assert!(pump(&mut first, &mut provider, 10).is_empty());
        let pairing_token = first.pairing_token().unwrap();

        let mut second = CloudEngine::new(CloudPersistentState::default());
        ready(&mut second);
        second
            .perform_action(
                CloudAction::LinkExisting { pairing_token },
                &FlightPlan::default(),
            )
            .unwrap();
        assert_eq!(pump(&mut second, &mut provider, 20), vec![initial]);

        first.record_local_flight_plan(&changed);
        assert!(pump(&mut first, &mut provider, 30).is_empty());
        second
            .perform_action(CloudAction::SyncNow, &FlightPlan::default())
            .unwrap();
        assert_eq!(pump(&mut second, &mut provider, 40), vec![changed]);
    }
}
