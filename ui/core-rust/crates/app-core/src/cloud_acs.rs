// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signer as _, SigningKey};
use hkdf::Hkdf;
use product_contracts::{
    acs_canonical_request_target, acs_creation_challenges_path, acs_object_path, acs_root_path,
    acs_sse_ticket_path, AcsCompareAndSwapRootResponse, AcsCreateAccountResponse,
    AcsCreateObjectOutcome, AcsCreateSseTicketResponse, AcsCreationChallengeResponse, AcsErrorCode,
    AcsErrorResponse, AcsHttpMethod, AcsObjectSnapshot, AcsRequestAuthentication, AcsRootSnapshot,
    AcsSignatureAlgorithm, ACS_AUTH_ACCOUNT_HEADER, ACS_AUTH_ALGORITHM_HEADER,
    ACS_AUTH_BODY_HASH_HEADER, ACS_AUTH_CONTRACT_HEADER, ACS_AUTH_KEY_ID_HEADER,
    ACS_AUTH_NONCE_HEADER, ACS_AUTH_SIGNATURE_HEADER, ACS_AUTH_TIMESTAMP_HEADER, ACS_CONTRACT_ID,
    ACS_KDF_ACCOUNT_LOCATOR_LABEL, ACS_KDF_REQUEST_SIGNING_SEED_LABEL, ACS_KDF_SALT,
    ACS_REQUEST_NONCE_BYTES, ACS_SIGNING_KEY_ID_BYTES,
};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::{
    cloud::{
        CloudHttpHeader, CloudHttpMethod, CloudHttpRequest, CloudHttpResponse,
        CloudProviderErrorKind, CloudProviderOperation, CloudProviderRequest,
        CloudProviderResponse,
    },
    AppError, AppErrorKind, AppResult,
};

const MAX_SMALL_RESPONSE_BYTES: u64 = 64 * 1024;
const MAX_OBJECT_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const ACS_BASE_SUFFIX: &str = "/cloud/";

pub(crate) struct AcsIdentity {
    pub account_locator: String,
    pub signing_public_key_base64url: String,
    pub signing_key_id: String,
}

pub(crate) fn validate_base_url(value: &str) -> AppResult<String> {
    let value = value.trim();
    if !(value.starts_with("http://") || value.starts_with("https://"))
        || !value.ends_with(ACS_BASE_SUFFIX)
        || value.contains(['\r', '\n', '?', '#'])
    {
        return Err(protocol_error(
            "Aerobag Cloud URL must be an HTTP(S) URL ending in /cloud/",
        ));
    }
    Ok(value.to_string())
}

pub(crate) fn resolve_event_url(base_url: &str, events_url: &str) -> AppResult<String> {
    let base_url = validate_base_url(base_url)?;
    if events_url.contains(['\r', '\n', '#']) {
        return Err(protocol_error("Aerobag Cloud event URL is invalid"));
    }
    let (path, query) = events_url.split_once('?').unwrap_or((events_url, ""));
    let mut url = url_for_path(&base_url, path)?;
    if !query.is_empty() {
        url.push('?');
        url.push_str(query);
    }
    Ok(url)
}

pub(crate) fn derive_identity(root_secret: &[u8; 32]) -> AppResult<AcsIdentity> {
    let account_locator = derive(root_secret, ACS_KDF_ACCOUNT_LOCATOR_LABEL)?;
    let signing_seed = derive(root_secret, ACS_KDF_REQUEST_SIGNING_SEED_LABEL)?;
    let signing_key = SigningKey::from_bytes(&signing_seed);
    let public_key = signing_key.verifying_key().to_bytes();
    let key_hash = Sha256::digest(public_key);
    Ok(AcsIdentity {
        account_locator: URL_SAFE_NO_PAD.encode(account_locator),
        signing_public_key_base64url: URL_SAFE_NO_PAD.encode(public_key),
        signing_key_id: URL_SAFE_NO_PAD.encode(&key_hash[..ACS_SIGNING_KEY_ID_BYTES]),
    })
}

pub(crate) fn plan_request(
    request: &CloudProviderRequest,
    base_url: &str,
    root_secret: &[u8; 32],
    account_locator: &str,
    now_epoch_ms: i64,
) -> AppResult<CloudHttpRequest> {
    let (method, path, body, signed, max_response_bytes) = match &request.operation {
        CloudProviderOperation::AcsIssueAccountChallenge => (
            AcsHttpMethod::Post,
            acs_creation_challenges_path().to_string(),
            Vec::new(),
            false,
            MAX_SMALL_RESPONSE_BYTES,
        ),
        CloudProviderOperation::AcsCreateAccount { request } => (
            AcsHttpMethod::Post,
            "/cloud/v1/accounts".to_string(),
            json_bytes(request)?,
            true,
            MAX_SMALL_RESPONSE_BYTES,
        ),
        CloudProviderOperation::AcsCreateObject { request } => (
            AcsHttpMethod::Put,
            acs_object_path(account_locator, &request.object_id),
            json_bytes(request)?,
            true,
            MAX_SMALL_RESPONSE_BYTES,
        ),
        CloudProviderOperation::AcsReadObject { id } => (
            AcsHttpMethod::Get,
            acs_object_path(account_locator, id),
            Vec::new(),
            true,
            MAX_OBJECT_RESPONSE_BYTES,
        ),
        CloudProviderOperation::AcsReadRoot => (
            AcsHttpMethod::Get,
            acs_root_path(account_locator),
            Vec::new(),
            true,
            MAX_OBJECT_RESPONSE_BYTES,
        ),
        CloudProviderOperation::AcsCompareAndSwapRoot { request } => (
            AcsHttpMethod::Put,
            acs_root_path(account_locator),
            json_bytes(request)?,
            true,
            MAX_OBJECT_RESPONSE_BYTES,
        ),
        CloudProviderOperation::AcsCreateSseTicket { request } => (
            AcsHttpMethod::Post,
            acs_sse_ticket_path(account_locator),
            json_bytes(request)?,
            true,
            MAX_SMALL_RESPONSE_BYTES,
        ),
        _ => return Err(protocol_error("non-ACS operation sent to the ACS adapter")),
    };
    let base_url = validate_base_url(base_url)?;
    let url = url_for_path(&base_url, &path)?;
    let mut headers = if body.is_empty() {
        Vec::new()
    } else {
        vec![CloudHttpHeader {
            name: "Content-Type".to_string(),
            value: "application/json".to_string(),
        }]
    };
    if signed {
        headers.extend(signing_headers(
            root_secret,
            account_locator,
            method,
            &path,
            &body,
            now_epoch_ms,
        )?);
    }
    Ok(CloudHttpRequest {
        request_id: request.request_id,
        provider: request.provider,
        method: match method {
            AcsHttpMethod::Get => CloudHttpMethod::Get,
            AcsHttpMethod::Post => CloudHttpMethod::Post,
            AcsHttpMethod::Put => CloudHttpMethod::Put,
            AcsHttpMethod::Delete => CloudHttpMethod::Delete,
        },
        url,
        headers,
        body_base64: (!body.is_empty()).then(|| URL_SAFE_NO_PAD.encode(body)),
        max_response_bytes,
    })
}

pub(crate) fn parse_response(
    request: &CloudProviderRequest,
    response: CloudHttpResponse,
) -> CloudProviderResponse {
    let (status, body) = match decode_http_response(response) {
        Ok(value) => value,
        Err(response) => return response,
    };
    match &request.operation {
        CloudProviderOperation::AcsIssueAccountChallenge if status == 200 => {
            match decode_json::<AcsCreationChallengeResponse>(&body) {
                Ok(response) => CloudProviderResponse::AcsCreationChallenge { response },
                Err(error) => error,
            }
        }
        CloudProviderOperation::AcsCreateAccount { .. } if status == 201 => {
            match decode_json::<AcsCreateAccountResponse>(&body) {
                Ok(response) => CloudProviderResponse::AcsAccountCreated { response },
                Err(error) => error,
            }
        }
        CloudProviderOperation::AcsCreateObject { .. } if status == 200 => {
            match decode_json::<AcsCreateObjectOutcome>(&body) {
                Ok(AcsCreateObjectOutcome::Created) => CloudProviderResponse::Created,
                Ok(AcsCreateObjectOutcome::AlreadyExists) => CloudProviderResponse::AlreadyExists,
                Err(error) => error,
            }
        }
        CloudProviderOperation::AcsReadObject { .. } if status == 404 => {
            CloudProviderResponse::AcsObject { object: None }
        }
        CloudProviderOperation::AcsReadObject { .. } if status == 200 => {
            match decode_json::<AcsObjectSnapshot>(&body) {
                Ok(object) => CloudProviderResponse::AcsObject {
                    object: Some(object),
                },
                Err(error) => error,
            }
        }
        CloudProviderOperation::AcsReadRoot if status == 404 => {
            CloudProviderResponse::AcsRoot { root: None }
        }
        CloudProviderOperation::AcsReadRoot if status == 200 => {
            match decode_json::<AcsRootSnapshot>(&body) {
                Ok(root) => CloudProviderResponse::AcsRoot { root: Some(root) },
                Err(error) => error,
            }
        }
        CloudProviderOperation::AcsCompareAndSwapRoot { .. } if status == 200 || status == 409 => {
            match decode_json::<AcsCompareAndSwapRootResponse>(&body) {
                Ok(response) => CloudProviderResponse::AcsRootCas { response },
                Err(error) => error,
            }
        }
        CloudProviderOperation::AcsCreateSseTicket { .. } if status == 200 => {
            match decode_json::<AcsCreateSseTicketResponse>(&body) {
                Ok(response) => CloudProviderResponse::AcsSseTicket { response },
                Err(error) => error,
            }
        }
        _ => parse_http_error(status, &body),
    }
}

fn signing_headers(
    root_secret: &[u8; 32],
    account_locator: &str,
    method: AcsHttpMethod,
    path: &str,
    body: &[u8],
    now_epoch_ms: i64,
) -> AppResult<Vec<CloudHttpHeader>> {
    let identity = derive_identity(root_secret)?;
    if identity.account_locator != account_locator {
        return Err(protocol_error(
            "Aerobag Cloud account locator does not match its root secret",
        ));
    }
    let nonce = crate::cloud::random_bytes::<ACS_REQUEST_NONCE_BYTES>()?;
    let mut authentication = AcsRequestAuthentication {
        contract_id: ACS_CONTRACT_ID.to_string(),
        account_locator: account_locator.to_string(),
        signing_key_id: identity.signing_key_id,
        signature_algorithm: AcsSignatureAlgorithm::Ed25519,
        timestamp_epoch_ms: now_epoch_ms,
        nonce_base64url: URL_SAFE_NO_PAD.encode(nonce),
        body_sha256: hex_bytes(&Sha256::digest(body)),
        signature_base64url: String::new(),
    };
    let target = acs_canonical_request_target(path, None).map_err(protocol_error)?;
    let signing_seed = derive(root_secret, ACS_KDF_REQUEST_SIGNING_SEED_LABEL)?;
    let signature = SigningKey::from_bytes(&signing_seed)
        .sign(
            &authentication
                .signing_bytes(method, &target)
                .map_err(protocol_error)?,
        )
        .to_bytes();
    authentication.signature_base64url = URL_SAFE_NO_PAD.encode(signature);
    Ok([
        (ACS_AUTH_CONTRACT_HEADER, authentication.contract_id),
        (ACS_AUTH_ACCOUNT_HEADER, authentication.account_locator),
        (ACS_AUTH_KEY_ID_HEADER, authentication.signing_key_id),
        (ACS_AUTH_ALGORITHM_HEADER, "ed25519".to_string()),
        (
            ACS_AUTH_TIMESTAMP_HEADER,
            authentication.timestamp_epoch_ms.to_string(),
        ),
        (ACS_AUTH_NONCE_HEADER, authentication.nonce_base64url),
        (ACS_AUTH_BODY_HASH_HEADER, authentication.body_sha256),
        (
            ACS_AUTH_SIGNATURE_HEADER,
            authentication.signature_base64url,
        ),
    ]
    .into_iter()
    .map(|(name, value)| CloudHttpHeader {
        name: name.to_string(),
        value,
    })
    .collect())
}

fn derive(root_secret: &[u8; 32], label: &[u8]) -> AppResult<[u8; 32]> {
    let mut output = [0_u8; 32];
    Hkdf::<Sha256>::new(Some(ACS_KDF_SALT), root_secret)
        .expand(label, &mut output)
        .map_err(|_| protocol_error("Aerobag Cloud key derivation failed"))?;
    Ok(output)
}

fn url_for_path(base_url: &str, path: &str) -> AppResult<String> {
    let relative = path
        .strip_prefix(ACS_BASE_SUFFIX)
        .ok_or_else(|| protocol_error("ACS request path is outside /cloud/"))?;
    Ok(format!("{base_url}{relative}"))
}

fn json_bytes(value: &impl serde::Serialize) -> AppResult<Vec<u8>> {
    serde_json::to_vec(value)
        .map_err(|error| protocol_error(format!("encode ACS request: {error}")))
}

fn decode_http_response(
    response: CloudHttpResponse,
) -> Result<(u16, Vec<u8>), CloudProviderResponse> {
    match response {
        CloudHttpResponse::TransportError { detail } => Err(provider_error(
            CloudProviderErrorKind::Transient,
            format!("Aerobag Cloud transport failed: {detail}"),
        )),
        CloudHttpResponse::ResponseTooLarge { limit_bytes } => Err(provider_error(
            CloudProviderErrorKind::Permanent,
            format!("Aerobag Cloud response exceeds {limit_bytes} bytes"),
        )),
        CloudHttpResponse::Completed {
            status_code,
            body_base64,
        } => URL_SAFE_NO_PAD.decode(body_base64).map_or_else(
            |_| {
                Err(provider_error(
                    CloudProviderErrorKind::Permanent,
                    "Aerobag Cloud response is not valid base64url".to_string(),
                ))
            },
            |body| Ok((status_code, body)),
        ),
    }
}

fn decode_json<T: DeserializeOwned>(body: &[u8]) -> Result<T, CloudProviderResponse> {
    match serde_json::from_slice::<T>(body) {
        Ok(value) => Ok(value),
        Err(error) => Err(provider_error(
            CloudProviderErrorKind::Permanent,
            format!("Aerobag Cloud returned invalid JSON: {error}"),
        )),
    }
}

fn parse_http_error(status: u16, body: &[u8]) -> CloudProviderResponse {
    let parsed = serde_json::from_slice::<AcsErrorResponse>(body).ok();
    let detail = parsed
        .as_ref()
        .map(|error| error.message.clone())
        .unwrap_or_else(|| format!("Aerobag Cloud request failed with HTTP {status}"));
    let kind = match parsed.as_ref().map(|error| error.code) {
        Some(AcsErrorCode::RequestExpired | AcsErrorCode::RateLimited | AcsErrorCode::ReadOnly) => {
            CloudProviderErrorKind::Transient
        }
        Some(AcsErrorCode::Unauthorized | AcsErrorCode::ReplayDetected) => {
            CloudProviderErrorKind::Unauthorized
        }
        None if status == 408 || status == 429 || status >= 500 => {
            CloudProviderErrorKind::Transient
        }
        _ => CloudProviderErrorKind::Permanent,
    };
    CloudProviderResponse::Error {
        kind,
        detail,
        retry_after_ms: parsed.as_ref().and_then(|error| error.retry_after_ms),
        rate_limit_gate: parsed.and_then(|error| error.rate_limit_gate),
    }
}

fn provider_error(kind: CloudProviderErrorKind, detail: String) -> CloudProviderResponse {
    CloudProviderResponse::Error {
        kind,
        detail,
        retry_after_ms: None,
        rate_limit_gate: None,
    }
}

fn protocol_error(message: impl Into<String>) -> AppError {
    AppError {
        kind: AppErrorKind::InvalidCatalog,
        message: message.into(),
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_derivation_matches_contract_sizes_and_url_is_strict() {
        let identity = derive_identity(&[0x42; 32]).unwrap();
        assert_eq!(
            URL_SAFE_NO_PAD
                .decode(identity.account_locator)
                .unwrap()
                .len(),
            product_contracts::ACS_ACCOUNT_LOCATOR_BYTES
        );
        assert_eq!(
            URL_SAFE_NO_PAD
                .decode(identity.signing_public_key_base64url)
                .unwrap()
                .len(),
            32
        );
        assert_eq!(
            URL_SAFE_NO_PAD
                .decode(identity.signing_key_id)
                .unwrap()
                .len(),
            ACS_SIGNING_KEY_ID_BYTES
        );
        assert_eq!(
            validate_base_url("https://aerobag.org/cloud/").unwrap(),
            "https://aerobag.org/cloud/"
        );
        assert!(validate_base_url("https://aerobag.org/").is_err());
    }

    #[test]
    fn typed_rate_limit_survives_the_http_boundary() {
        let body = serde_json::to_vec(&AcsErrorResponse {
            contract_id: ACS_CONTRACT_ID.to_string(),
            request_id: "request".to_string(),
            code: AcsErrorCode::RateLimited,
            message: "server wording is not the UI contract".to_string(),
            retry_after_ms: Some(28_800_000),
            rate_limit_gate: Some(product_contracts::AcsRateLimitGate::AccountCreationNetwork),
        })
        .unwrap();
        assert!(matches!(
            parse_http_error(429, &body),
            CloudProviderResponse::Error {
                kind: CloudProviderErrorKind::Transient,
                retry_after_ms: Some(28_800_000),
                rate_limit_gate: Some(product_contracts::AcsRateLimitGate::AccountCreationNetwork),
                ..
            }
        ));
    }
}
