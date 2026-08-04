// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::net::IpAddr;

use axum::http;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use hmac::{Hmac, Mac as _};
use product_contracts::{
    acs_canonical_request_target, AcsCreateAccountRequest, AcsErrorCode, AcsHttpMethod,
    AcsRequestAuthentication, AcsSignatureAlgorithm, ACS_ACCOUNT_LOCATOR_BYTES,
    ACS_AUTH_ACCOUNT_HEADER, ACS_AUTH_ALGORITHM_HEADER, ACS_AUTH_BODY_HASH_HEADER,
    ACS_AUTH_CONTRACT_HEADER, ACS_AUTH_KEY_ID_HEADER, ACS_AUTH_NONCE_HEADER,
    ACS_AUTH_SIGNATURE_HEADER, ACS_AUTH_TIMESTAMP_HEADER, ACS_CONTRACT_ID, ACS_REQUEST_NONCE_BYTES,
    ACS_SIGNATURE_CLOCK_WINDOW_MS, ACS_SIGNING_KEY_ID_BYTES,
};
use sha2::{Digest as _, Sha256};

use crate::store::{CloudStore, StoreError, StoreResult};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Copy)]
pub(crate) struct SignedRequest<'a> {
    pub headers: &'a http::HeaderMap,
    pub method: AcsHttpMethod,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub body: &'a [u8],
    pub now_epoch_ms: i64,
}

pub(crate) fn source_network_pseudonym(server_secret: &[u8; 32], address: IpAddr) -> String {
    let mut hmac =
        HmacSha256::new_from_slice(server_secret).expect("HMAC accepts a fixed-size server secret");
    hmac.update(b"aerobag-cloud-source-network-v1\0");
    match address {
        IpAddr::V4(address) => hmac.update(&address.octets()),
        IpAddr::V6(address) => hmac.update(&address.octets()),
    }
    hex_bytes(&hmac.finalize().into_bytes()[..16])
}

pub(crate) fn verify_registered_request(
    store: &CloudStore,
    request: SignedRequest<'_>,
    path_account_locator: &str,
    write: bool,
) -> StoreResult<()> {
    let authentication = parse_authentication(request.headers)?;
    if authentication.account_locator != path_account_locator {
        return reject(store, false, "signed account does not match request path");
    }
    let registered = store.account_authentication(path_account_locator, write)?;
    if authentication.signing_key_id != registered.signing_key_id {
        return reject(
            store,
            false,
            "request signing key is not registered for this account",
        );
    }
    verify_authentication(
        store,
        &authentication,
        request,
        &registered.signing_public_key,
    )?;
    store.check_account_operation(path_account_locator, request.now_epoch_ms)?;
    store.consume_nonce(
        path_account_locator,
        &authentication.signing_key_id,
        &authentication.nonce_base64url,
        request.now_epoch_ms + ACS_SIGNATURE_CLOCK_WINDOW_MS,
        request.now_epoch_ms,
    )
}

pub(crate) fn verify_account_creation_request(
    store: &CloudStore,
    signed_request: SignedRequest<'_>,
    request: &AcsCreateAccountRequest,
) -> StoreResult<[u8; 32]> {
    validate_account_locator(&request.account_locator)?;
    let authentication = parse_authentication(signed_request.headers)?;
    if authentication.account_locator != request.account_locator
        || authentication.signing_key_id != request.signing_key_id
    {
        return reject(
            store,
            false,
            "signed identity does not match account request",
        );
    }
    let public_key: [u8; 32] = decode_exact(
        &request.signing_public_key_base64url,
        32,
        "signing public key",
    )?
    .try_into()
    .expect("validated length");
    let expected_key_id =
        URL_SAFE_NO_PAD.encode(&Sha256::digest(public_key)[..ACS_SIGNING_KEY_ID_BYTES]);
    if request.signing_key_id != expected_key_id {
        return reject(store, false, "signing key ID does not match public key");
    }
    verify_authentication(store, &authentication, signed_request, &public_key)?;
    store.consume_nonce(
        &request.account_locator,
        &request.signing_key_id,
        &authentication.nonce_base64url,
        signed_request.now_epoch_ms + ACS_SIGNATURE_CLOCK_WINDOW_MS,
        signed_request.now_epoch_ms,
    )?;
    Ok(public_key)
}

fn verify_authentication(
    store: &CloudStore,
    authentication: &AcsRequestAuthentication,
    request: SignedRequest<'_>,
    public_key: &[u8; 32],
) -> StoreResult<()> {
    if authentication.contract_id != ACS_CONTRACT_ID
        || authentication.signature_algorithm != AcsSignatureAlgorithm::Ed25519
    {
        return reject(store, false, "unsupported request authentication contract");
    }
    validate_account_locator(&authentication.account_locator)?;
    decode_exact(
        &authentication.signing_key_id,
        ACS_SIGNING_KEY_ID_BYTES,
        "signing key ID",
    )?;
    if request
        .now_epoch_ms
        .abs_diff(authentication.timestamp_epoch_ms)
        > ACS_SIGNATURE_CLOCK_WINDOW_MS as u64
    {
        store.increment_authentication_rejection(false);
        return Err(StoreError::new(
            AcsErrorCode::RequestExpired,
            "signed request timestamp is outside the accepted clock window",
        ));
    }
    decode_exact(
        &authentication.nonce_base64url,
        ACS_REQUEST_NONCE_BYTES,
        "request nonce",
    )?;
    let body_hash = hex_bytes(&Sha256::digest(request.body));
    if authentication.body_sha256 != body_hash {
        return reject(store, false, "signed request body hash does not match body");
    }
    let canonical_target = acs_canonical_request_target(request.path, request.query)
        .map_err(|message| StoreError::new(AcsErrorCode::InvalidRequest, message))?;
    let signing_bytes = authentication
        .signing_bytes(request.method, &canonical_target)
        .map_err(|message| StoreError::new(AcsErrorCode::InvalidRequest, message))?;
    let signature_bytes = decode_exact(&authentication.signature_base64url, 64, "signature")?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| StoreError::new(AcsErrorCode::Unauthorized, "signature is malformed"))?;
    let verifying_key = VerifyingKey::from_bytes(public_key).map_err(|_| {
        StoreError::new(AcsErrorCode::Unauthorized, "signing public key is invalid")
    })?;
    verifying_key
        .verify(&signing_bytes, &signature)
        .map_err(|_| {
            store.increment_authentication_rejection(false);
            StoreError::new(AcsErrorCode::Unauthorized, "request signature is invalid")
        })
}

fn parse_authentication(headers: &http::HeaderMap) -> StoreResult<AcsRequestAuthentication> {
    let algorithm = required_header(headers, ACS_AUTH_ALGORITHM_HEADER)?;
    if algorithm != "ed25519" {
        return Err(StoreError::new(
            AcsErrorCode::Unauthorized,
            "unsupported signature algorithm",
        ));
    }
    Ok(AcsRequestAuthentication {
        contract_id: required_header(headers, ACS_AUTH_CONTRACT_HEADER)?.to_string(),
        account_locator: required_header(headers, ACS_AUTH_ACCOUNT_HEADER)?.to_string(),
        signing_key_id: required_header(headers, ACS_AUTH_KEY_ID_HEADER)?.to_string(),
        signature_algorithm: AcsSignatureAlgorithm::Ed25519,
        timestamp_epoch_ms: required_header(headers, ACS_AUTH_TIMESTAMP_HEADER)?
            .parse()
            .map_err(|_| {
                StoreError::new(AcsErrorCode::Unauthorized, "invalid request timestamp")
            })?,
        nonce_base64url: required_header(headers, ACS_AUTH_NONCE_HEADER)?.to_string(),
        body_sha256: required_header(headers, ACS_AUTH_BODY_HASH_HEADER)?.to_string(),
        signature_base64url: required_header(headers, ACS_AUTH_SIGNATURE_HEADER)?.to_string(),
    })
}

fn required_header<'a>(headers: &'a http::HeaderMap, name: &str) -> StoreResult<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            StoreError::new(
                AcsErrorCode::Unauthorized,
                format!("required authentication header {name} is missing or invalid"),
            )
        })
}

fn validate_account_locator(account_locator: &str) -> StoreResult<()> {
    decode_exact(
        account_locator,
        ACS_ACCOUNT_LOCATOR_BYTES,
        "account locator",
    )
    .map(|_| ())
}

fn decode_exact(value: &str, length: usize, label: &str) -> StoreResult<Vec<u8>> {
    let maximum_encoded_length = length.div_ceil(3) * 4;
    if value.len() > maximum_encoded_length {
        return Err(StoreError::new(
            AcsErrorCode::Unauthorized,
            format!("{label} has the wrong length"),
        ));
    }
    let bytes = URL_SAFE_NO_PAD.decode(value).map_err(|_| {
        StoreError::new(
            AcsErrorCode::Unauthorized,
            format!("{label} is not valid base64url"),
        )
    })?;
    if bytes.len() != length {
        return Err(StoreError::new(
            AcsErrorCode::Unauthorized,
            format!("{label} has the wrong length"),
        ));
    }
    Ok(bytes)
}

fn reject<T>(store: &CloudStore, replay: bool, message: &str) -> StoreResult<T> {
    store.increment_authentication_rejection(replay);
    Err(StoreError::new(AcsErrorCode::Unauthorized, message))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer as _, SigningKey};
    use product_contracts::{AcsRequestAuthentication, ACS_AUTH_SIGNATURE_HEADER};
    use tempfile::TempDir;

    fn signed_headers(
        signing_key: &SigningKey,
        account_locator: &str,
        method: AcsHttpMethod,
        path: &str,
        body: &[u8],
        now: i64,
    ) -> http::HeaderMap {
        let key_id = URL_SAFE_NO_PAD.encode(
            &Sha256::digest(signing_key.verifying_key().as_bytes())[..ACS_SIGNING_KEY_ID_BYTES],
        );
        let mut authentication = AcsRequestAuthentication {
            contract_id: ACS_CONTRACT_ID.to_string(),
            account_locator: account_locator.to_string(),
            signing_key_id: key_id,
            signature_algorithm: AcsSignatureAlgorithm::Ed25519,
            timestamp_epoch_ms: now,
            nonce_base64url: URL_SAFE_NO_PAD.encode([3_u8; ACS_REQUEST_NONCE_BYTES]),
            body_sha256: hex_bytes(&Sha256::digest(body)),
            signature_base64url: String::new(),
        };
        authentication.signature_base64url = URL_SAFE_NO_PAD.encode(
            signing_key
                .sign(&authentication.signing_bytes(method, path).unwrap())
                .to_bytes(),
        );
        let timestamp = authentication.timestamp_epoch_ms.to_string();
        let mut headers = http::HeaderMap::new();
        for (name, value) in [
            (
                ACS_AUTH_CONTRACT_HEADER,
                authentication.contract_id.as_str(),
            ),
            (
                ACS_AUTH_ACCOUNT_HEADER,
                authentication.account_locator.as_str(),
            ),
            (
                ACS_AUTH_KEY_ID_HEADER,
                authentication.signing_key_id.as_str(),
            ),
            (ACS_AUTH_ALGORITHM_HEADER, "ed25519"),
            (ACS_AUTH_TIMESTAMP_HEADER, timestamp.as_str()),
            (
                ACS_AUTH_NONCE_HEADER,
                authentication.nonce_base64url.as_str(),
            ),
            (
                ACS_AUTH_BODY_HASH_HEADER,
                authentication.body_sha256.as_str(),
            ),
            (
                ACS_AUTH_SIGNATURE_HEADER,
                authentication.signature_base64url.as_str(),
            ),
        ] {
            headers.insert(
                http::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
                http::HeaderValue::from_str(value).unwrap(),
            );
        }
        headers
    }

    #[test]
    fn registered_signature_is_verified_and_nonce_cannot_replay() {
        let root = TempDir::new().unwrap();
        let store = CloudStore::open(crate::StoreConfig::for_test_data_root(
            root.path().to_path_buf(),
        ))
        .unwrap();
        let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
        let account_locator = URL_SAFE_NO_PAD.encode([5_u8; ACS_ACCOUNT_LOCATOR_BYTES]);
        let challenge = store.issue_creation_challenge("network", 1).unwrap();
        let key_id = URL_SAFE_NO_PAD.encode(
            &Sha256::digest(signing_key.verifying_key().as_bytes())[..ACS_SIGNING_KEY_ID_BYTES],
        );
        store
            .create_account(
                &AcsCreateAccountRequest {
                    contract_id: ACS_CONTRACT_ID.to_string(),
                    account_locator: account_locator.clone(),
                    signing_key_id: key_id,
                    signing_public_key_base64url: URL_SAFE_NO_PAD
                        .encode(signing_key.verifying_key().as_bytes()),
                    creation_challenge: challenge.challenge,
                },
                signing_key.verifying_key().as_bytes(),
                "network",
                2,
            )
            .unwrap();
        let headers = signed_headers(
            &signing_key,
            &account_locator,
            AcsHttpMethod::Get,
            "/cloud/v1/accounts/account/root",
            b"",
            100,
        );
        assert!(verify_registered_request(
            &store,
            SignedRequest {
                headers: &headers,
                method: AcsHttpMethod::Get,
                path: "/cloud/v1/accounts/account/root",
                query: None,
                body: b"",
                now_epoch_ms: 100,
            },
            &account_locator,
            false,
        )
        .is_ok());
        assert_eq!(
            verify_registered_request(
                &store,
                SignedRequest {
                    headers: &headers,
                    method: AcsHttpMethod::Get,
                    path: "/cloud/v1/accounts/account/root",
                    query: None,
                    body: b"",
                    now_epoch_ms: 100,
                },
                &account_locator,
                false,
            )
            .unwrap_err()
            .code,
            AcsErrorCode::ReplayDetected
        );
    }
}
