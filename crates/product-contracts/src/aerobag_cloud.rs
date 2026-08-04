// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ACS_CONTRACT_ID: &str = "ACS1";
pub const ACS_API_PREFIX: &str = "/cloud/v1";
pub const ACS_STATUS_PATH: &str = "/cloud/v1/status";
pub const ACS_HEALTH_PATH: &str = "/cloud/v1/health";
pub const ACS_SIGNATURE_CLOCK_WINDOW_MS: i64 = 5 * 60 * 1_000;
pub const ACS_SSE_TICKET_TTL_MS: i64 = 2 * 60 * 1_000;
pub const ACS_FIXED_ROOT_ID: &str = "root";
pub const ACS_KDF_SALT: &[u8] = b"aerobag-cloud-account-v1";
pub const ACS_KDF_ALGORITHM: &str = "HKDF-SHA256";
pub const ACS_KDF_ACCOUNT_LOCATOR_LABEL: &[u8] = b"account-locator";
pub const ACS_KDF_PAYLOAD_ENCRYPTION_LABEL: &[u8] = b"payload-encryption";
pub const ACS_KDF_REQUEST_SIGNING_SEED_LABEL: &[u8] = b"request-signing-ed25519-seed";
pub const ACS_PAYLOAD_ENCRYPTION_ALGORITHM: &str = "ChaCha20-Poly1305";
pub const ACS_ACCOUNT_LOCATOR_BYTES: usize = 32;
pub const ACS_SIGNING_KEY_ID_BYTES: usize = 16;
pub const ACS_REQUEST_NONCE_BYTES: usize = 16;
pub const ACS_MAX_VISIBLE_CHILD_OBJECTS: usize = 4_096;
pub const ACS_AUTH_CONTRACT_HEADER: &str = "Aerobag-Contract";
pub const ACS_AUTH_ACCOUNT_HEADER: &str = "Aerobag-Account";
pub const ACS_AUTH_KEY_ID_HEADER: &str = "Aerobag-Key-Id";
pub const ACS_AUTH_ALGORITHM_HEADER: &str = "Aerobag-Signature-Algorithm";
pub const ACS_AUTH_TIMESTAMP_HEADER: &str = "Aerobag-Timestamp-Ms";
pub const ACS_AUTH_NONCE_HEADER: &str = "Aerobag-Nonce";
pub const ACS_AUTH_BODY_HASH_HEADER: &str = "Aerobag-Body-SHA256";
pub const ACS_AUTH_SIGNATURE_HEADER: &str = "Aerobag-Signature";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcsHttpMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl AcsHttpMethod {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcsSignatureAlgorithm {
    Ed25519,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcsEncryptedValueKind {
    Object,
    Root,
}

impl AcsEncryptedValueKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Root => "root",
        }
    }
}

/// Authentication fields transported as `Aerobag-*` HTTP headers.
///
/// `signature_base64url` signs [`AcsRequestAuthentication::signing_bytes`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsRequestAuthentication {
    pub contract_id: String,
    pub account_locator: String,
    pub signing_key_id: String,
    pub signature_algorithm: AcsSignatureAlgorithm,
    pub timestamp_epoch_ms: i64,
    pub nonce_base64url: String,
    pub body_sha256: String,
    pub signature_base64url: String,
}

impl AcsRequestAuthentication {
    pub fn signing_bytes(
        &self,
        method: AcsHttpMethod,
        canonical_request_target: &str,
    ) -> Result<Vec<u8>, &'static str> {
        if self.contract_id != ACS_CONTRACT_ID {
            return Err("unsupported ACS contract");
        }
        if !canonical_request_target.starts_with(ACS_API_PREFIX)
            || canonical_request_target.contains(['\r', '\n'])
        {
            return Err("invalid ACS request target");
        }
        for value in [
            self.account_locator.as_str(),
            self.signing_key_id.as_str(),
            self.nonce_base64url.as_str(),
            self.body_sha256.as_str(),
        ] {
            if value.is_empty() || value.contains(['\r', '\n']) {
                return Err("invalid ACS signature field");
            }
        }
        Ok(format!(
            "{ACS_CONTRACT_ID}\n{}\n{canonical_request_target}\n{}\n{}\n{}\n{}\n{}\n",
            method.as_str(),
            self.account_locator,
            self.signing_key_id,
            self.timestamp_epoch_ms,
            self.nonce_base64url,
            self.body_sha256,
        )
        .into_bytes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsCreateAccountRequest {
    pub contract_id: String,
    pub account_locator: String,
    pub signing_key_id: String,
    pub signing_public_key_base64url: String,
    pub creation_challenge: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsCreationChallengeResponse {
    pub contract_id: String,
    pub challenge: String,
    pub expires_at_epoch_ms: i64,
    pub server_time_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsCreateAccountResponse {
    pub contract_id: String,
    pub account_locator: String,
    pub server_time_epoch_ms: i64,
    pub quota_class: String,
    pub quota_bytes: u64,
}

/// Opaque encrypted bytes plus the visible Merkle edges authenticated by the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsEncryptedValue {
    pub ciphertext_base64url: String,
    pub ciphertext_sha256: String,
    pub child_object_ids: Vec<String>,
}

impl AcsEncryptedValue {
    pub fn from_ciphertext(ciphertext: &[u8], mut child_object_ids: Vec<String>) -> Self {
        child_object_ids.sort();
        child_object_ids.dedup();
        Self {
            ciphertext_base64url: URL_SAFE_NO_PAD.encode(ciphertext),
            ciphertext_sha256: sha256_hex(ciphertext),
            child_object_ids,
        }
    }

    pub fn ciphertext(&self) -> Result<Vec<u8>, &'static str> {
        URL_SAFE_NO_PAD
            .decode(&self.ciphertext_base64url)
            .map_err(|_| "ciphertext is not valid base64url")
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        validate_child_object_ids(&self.child_object_ids)?;
        let ciphertext = self.ciphertext()?;
        if sha256_hex(&ciphertext) != self.ciphertext_sha256 {
            return Err("ciphertext hash mismatch");
        }
        Ok(())
    }

    /// Canonical AEAD associated data. Encryption and decryption must both use it.
    pub fn associated_data(
        &self,
        kind: AcsEncryptedValueKind,
        value_id: &str,
    ) -> Result<Vec<u8>, &'static str> {
        acs_encrypted_value_associated_data(kind, value_id, &self.child_object_ids)
    }

    pub fn authenticated_hash(
        &self,
        kind: AcsEncryptedValueKind,
        value_id: &str,
    ) -> Result<String, &'static str> {
        self.validate()?;
        let mut hash = Sha256::new();
        hash.update(self.associated_data(kind, value_id)?);
        hash.update(self.ciphertext_sha256.as_bytes());
        Ok(hex_bytes(&hash.finalize()))
    }
}

/// Builds the AEAD associated data before ciphertext exists.
pub fn acs_encrypted_value_associated_data(
    kind: AcsEncryptedValueKind,
    value_id: &str,
    child_object_ids: &[String],
) -> Result<Vec<u8>, &'static str> {
    validate_child_object_ids(child_object_ids)?;
    if value_id.is_empty() || value_id.contains(['\r', '\n']) {
        return Err("invalid encrypted value ID");
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"aerobag-cloud-encrypted-value-v1\0");
    bytes.extend_from_slice(kind.as_str().as_bytes());
    bytes.push(0);
    bytes.extend_from_slice((value_id.len() as u32).to_be_bytes().as_slice());
    bytes.extend_from_slice(value_id.as_bytes());
    bytes.extend_from_slice((child_object_ids.len() as u32).to_be_bytes().as_slice());
    for child in child_object_ids {
        bytes.extend_from_slice((child.len() as u32).to_be_bytes().as_slice());
        bytes.extend_from_slice(child.as_bytes());
    }
    Ok(bytes)
}

fn validate_child_object_ids(child_object_ids: &[String]) -> Result<(), &'static str> {
    if child_object_ids.len() > ACS_MAX_VISIBLE_CHILD_OBJECTS {
        return Err("encrypted value has too many visible child objects");
    }
    if child_object_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("child object IDs must be sorted and unique");
    }
    if child_object_ids
        .iter()
        .any(|child| child.is_empty() || child.contains(['\r', '\n']))
    {
        return Err("invalid child object ID");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsCreateObjectRequest {
    pub contract_id: String,
    pub object_id: String,
    pub value: AcsEncryptedValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcsCreateObjectOutcome {
    Created,
    AlreadyExists,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsObjectSnapshot {
    pub object_id: String,
    pub value: AcsEncryptedValue,
    pub created_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsObjectSummary {
    pub object_id: String,
    pub authenticated_hash: String,
    pub ciphertext_bytes: u64,
    pub created_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsListObjectsRequest {
    pub contract_id: String,
    pub cursor: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsListObjectsResponse {
    pub contract_id: String,
    pub objects: Vec<AcsObjectSummary>,
    pub next_cursor: Option<String>,
    pub total_object_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsRootSnapshot {
    pub revision: u64,
    pub root_hash: String,
    pub value: AcsEncryptedValue,
    pub updated_at_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsCompareAndSwapRootRequest {
    pub contract_id: String,
    pub expected_revision: u64,
    pub expected_root_hash: Option<String>,
    pub replacement: AcsEncryptedValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AcsCompareAndSwapRootResponse {
    Committed {
        root: AcsRootSnapshot,
    },
    Conflict {
        current_revision: u64,
        current_root_hash: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsCreateSseTicketRequest {
    pub contract_id: String,
    pub last_event_sequence: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsCreateSseTicketResponse {
    pub contract_id: String,
    pub ticket: String,
    pub expires_at_epoch_ms: i64,
    pub events_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AcsSseEvent {
    Ready {
        sequence: u64,
        root_revision: u64,
        root_hash: Option<String>,
    },
    RootChanged {
        sequence: u64,
        root_revision: u64,
        root_hash: String,
    },
    Reset {
        sequence: u64,
        root_revision: u64,
        root_hash: Option<String>,
    },
    Heartbeat {
        sequence: u64,
        root_revision: u64,
        root_hash: Option<String>,
    },
}

impl AcsSseEvent {
    pub const fn sequence(&self) -> u64 {
        match self {
            Self::Ready { sequence, .. }
            | Self::RootChanged { sequence, .. }
            | Self::Reset { sequence, .. }
            | Self::Heartbeat { sequence, .. } => *sequence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcsServiceMode {
    Normal,
    ReadOnly,
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsStatusMetric {
    pub id: String,
    pub current: u64,
    pub peak: u64,
    pub warning_at: Option<u64>,
    pub critical_at: Option<u64>,
    pub hard_limit: Option<u64>,
    pub window_seconds: Option<u64>,
    pub rejected_in_window: u64,
    #[serde(default)]
    pub lower_is_worse: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsHealthResponse {
    pub contract_id: String,
    pub server_time_epoch_ms: i64,
    pub mode: AcsServiceMode,
    pub database_healthy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsStatusTopContributor {
    pub metric_id: String,
    pub opaque_subject: String,
    pub current: u64,
    pub window_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsStatusResponse {
    pub contract_id: String,
    pub started_at_epoch_ms: i64,
    pub server_time_epoch_ms: i64,
    pub mode: AcsServiceMode,
    pub schema_version: u32,
    pub database_healthy: bool,
    pub last_durable_read_epoch_ms: Option<i64>,
    pub last_durable_write_epoch_ms: Option<i64>,
    pub metrics: Vec<AcsStatusMetric>,
    pub top_contributors: Vec<AcsStatusTopContributor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcsErrorCode {
    InvalidRequest,
    Unauthorized,
    RequestExpired,
    ReplayDetected,
    NotFound,
    ObjectIdCollision,
    MissingChildObject,
    Conflict,
    QuotaExceeded,
    RateLimited,
    ReadOnly,
    AccountSuspended,
    PayloadTooLarge,
    ResponseTooLarge,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcsRateLimitGate {
    AccountCreationNetwork,
    AccountCreationGlobal,
    OutstandingCreationChallenges,
    NetworkOperations,
    AccountOperations,
    AccountEgress,
    GlobalSseConnections,
    AccountSseConnections,
    NetworkSseConnections,
}

impl AcsErrorCode {
    pub const fn http_status(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::Unauthorized | Self::RequestExpired | Self::ReplayDetected => 401,
            Self::NotFound => 404,
            Self::ObjectIdCollision | Self::MissingChildObject | Self::Conflict => 409,
            Self::QuotaExceeded | Self::AccountSuspended => 403,
            Self::PayloadTooLarge | Self::ResponseTooLarge => 413,
            Self::RateLimited => 429,
            Self::ReadOnly => 503,
            Self::Internal => 500,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcsErrorResponse {
    pub contract_id: String,
    pub request_id: String,
    pub code: AcsErrorCode,
    pub message: String,
    pub retry_after_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_gate: Option<AcsRateLimitGate>,
}

pub fn acs_account_path(account_locator: &str) -> String {
    format!("{ACS_API_PREFIX}/accounts/{account_locator}")
}

pub fn acs_creation_challenges_path() -> &'static str {
    "/cloud/v1/account-challenges"
}

pub fn acs_object_path(account_locator: &str, object_id: &str) -> String {
    format!("{}/objects/{object_id}", acs_account_path(account_locator))
}

pub fn acs_objects_path(account_locator: &str) -> String {
    format!("{}/objects", acs_account_path(account_locator))
}

pub fn acs_root_path(account_locator: &str) -> String {
    format!("{}/root", acs_account_path(account_locator))
}

pub fn acs_sse_ticket_path(account_locator: &str) -> String {
    format!("{}/event-tickets", acs_account_path(account_locator))
}

pub fn acs_events_path() -> &'static str {
    "/cloud/v1/events"
}

/// Returns the request target used by ACS signatures.
///
/// Callers pass the already percent-encoded path and raw encoded query. Sorting
/// encoded pairs makes client and server signatures independent of query order.
pub fn acs_canonical_request_target(
    path: &str,
    query: Option<&str>,
) -> Result<String, &'static str> {
    if !path.starts_with(ACS_API_PREFIX)
        || path.contains(['\r', '\n', '?', '#'])
        || query.is_some_and(|query| query.contains(['\r', '\n', '#']))
    {
        return Err("invalid ACS request target");
    }
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return Ok(path.to_string());
    };
    let mut pairs = query
        .split('&')
        .map(|pair| pair.split_once('=').unwrap_or((pair, "")))
        .collect::<Vec<_>>();
    pairs.sort_unstable();
    let query = pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    Ok(format!("{path}?{query}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_request_bytes_are_stable_and_bind_the_request() {
        let auth = AcsRequestAuthentication {
            contract_id: ACS_CONTRACT_ID.to_string(),
            account_locator: "acct".to_string(),
            signing_key_id: "key".to_string(),
            signature_algorithm: AcsSignatureAlgorithm::Ed25519,
            timestamp_epoch_ms: 123,
            nonce_base64url: "nonce".to_string(),
            body_sha256: "body".to_string(),
            signature_base64url: "signature".to_string(),
        };
        assert_eq!(
            String::from_utf8(
                auth.signing_bytes(AcsHttpMethod::Put, "/cloud/v1/accounts/acct/root")
                    .unwrap()
            )
            .unwrap(),
            "ACS1\nPUT\n/cloud/v1/accounts/acct/root\nacct\nkey\n123\nnonce\nbody\n"
        );
    }

    #[test]
    fn canonical_request_target_sorts_encoded_query_pairs() {
        assert_eq!(
            acs_canonical_request_target(
                "/cloud/v1/accounts/account/objects",
                Some("limit=10&cursor=a%2Fb&cursor=a%20b"),
            ),
            Ok("/cloud/v1/accounts/account/objects?cursor=a%20b&cursor=a%2Fb&limit=10".to_string())
        );
        assert!(acs_canonical_request_target("/cloud/v1/status?bad", None).is_err());
    }

    #[test]
    fn value_hash_authenticates_visible_tree_edges() {
        let value = AcsEncryptedValue::from_ciphertext(b"ciphertext", vec!["b".into(), "a".into()]);
        let mut changed = value.clone();
        changed.child_object_ids.push("c".to_string());
        assert_ne!(
            value
                .authenticated_hash(AcsEncryptedValueKind::Object, "object-a")
                .unwrap(),
            changed
                .authenticated_hash(AcsEncryptedValueKind::Object, "object-a")
                .unwrap()
        );
        assert_eq!(value.child_object_ids, vec!["a", "b"]);
        assert_ne!(
            value
                .associated_data(AcsEncryptedValueKind::Object, "object-a")
                .unwrap(),
            value
                .associated_data(AcsEncryptedValueKind::Object, "object-b")
                .unwrap()
        );
    }

    #[test]
    fn value_validation_rejects_noncanonical_or_corrupt_data() {
        let mut value = AcsEncryptedValue::from_ciphertext(b"ciphertext", vec!["a".into()]);
        value.child_object_ids.push("a".to_string());
        assert_eq!(
            value.validate(),
            Err("child object IDs must be sorted and unique")
        );

        let mut value = AcsEncryptedValue::from_ciphertext(b"ciphertext", Vec::new());
        value.ciphertext_sha256 = "wrong".to_string();
        assert_eq!(value.validate(), Err("ciphertext hash mismatch"));

        let value = AcsEncryptedValue::from_ciphertext(
            b"ciphertext",
            (0..=ACS_MAX_VISIBLE_CHILD_OBJECTS)
                .map(|index| format!("child-{index:05}"))
                .collect(),
        );
        assert_eq!(
            value.validate(),
            Err("encrypted value has too many visible child objects")
        );
    }

    #[test]
    fn root_cas_json_shape_is_stable() {
        let request = AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision: 7,
            expected_root_hash: Some("old-root".to_string()),
            replacement: AcsEncryptedValue {
                ciphertext_base64url: "Y2lwaGVydGV4dA".to_string(),
                ciphertext_sha256: "cipher-hash".to_string(),
                child_object_ids: vec!["page-a".to_string(), "page-b".to_string()],
            },
        };
        assert_eq!(
            serde_json::to_string(&request).unwrap(),
            r#"{"contract_id":"ACS1","expected_revision":7,"expected_root_hash":"old-root","replacement":{"ciphertext_base64url":"Y2lwaGVydGV4dA","ciphertext_sha256":"cipher-hash","child_object_ids":["page-a","page-b"]}}"#
        );
    }

    #[test]
    fn sse_and_error_json_shapes_are_stable() {
        assert_eq!(
            serde_json::to_string(&AcsSseEvent::RootChanged {
                sequence: 9,
                root_revision: 4,
                root_hash: "root-hash".to_string(),
            })
            .unwrap(),
            r#"{"kind":"root-changed","sequence":9,"root_revision":4,"root_hash":"root-hash"}"#
        );
        assert_eq!(
            serde_json::to_string(&AcsErrorResponse {
                contract_id: ACS_CONTRACT_ID.to_string(),
                request_id: "request-1".to_string(),
                code: AcsErrorCode::ReplayDetected,
                message: "request nonce was already used".to_string(),
                retry_after_ms: None,
                rate_limit_gate: None,
            })
            .unwrap(),
            r#"{"contract_id":"ACS1","request_id":"request-1","code":"replay_detected","message":"request nonce was already used","retry_after_ms":null}"#
        );
        assert_eq!(
            serde_json::to_string(&AcsErrorResponse {
                contract_id: ACS_CONTRACT_ID.to_string(),
                request_id: "request-2".to_string(),
                code: AcsErrorCode::RateLimited,
                message: "network bucket is empty".to_string(),
                retry_after_ms: Some(28_800_000),
                rate_limit_gate: Some(AcsRateLimitGate::AccountCreationNetwork),
            })
            .unwrap(),
            r#"{"contract_id":"ACS1","request_id":"request-2","code":"rate_limited","message":"network bucket is empty","retry_after_ms":28800000,"rate_limit_gate":"account_creation_network"}"#
        );
    }
}
