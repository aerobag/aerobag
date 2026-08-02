// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::Deserialize;

use crate::{
    cloud::{
        CloudHttpHeader, CloudHttpMethod, CloudHttpRequest, CloudHttpResponse,
        CloudProviderErrorKind, CloudProviderKind, CloudProviderObject, CloudProviderOperation,
        CloudProviderRequest, CloudProviderResponse,
    },
    AppError, AppErrorKind, AppResult,
};

const DRIVE_API_ROOT: &str = "https://www.googleapis.com/drive/v3";
const DRIVE_UPLOAD_ROOT: &str = "https://www.googleapis.com/upload/drive/v3";
const CLOUD_OBJECT_MIME: &str = "application/vnd.aerobag.cloud-object";
pub(crate) const MAX_CLOUD_OBJECT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SMALL_RESPONSE_BYTES: u64 = 64 * 1024;

pub(crate) fn plan_request(request: &CloudProviderRequest) -> AppResult<CloudHttpRequest> {
    if request.provider != CloudProviderKind::GoogleDrive {
        return Err(protocol_error(format!(
            "no HTTP protocol is implemented for {}",
            request.provider.label()
        )));
    }
    let (method, url, headers, body_base64, max_response_bytes) = match &request.operation {
        CloudProviderOperation::AllocateIds { count } => (
            CloudHttpMethod::Get,
            format!(
                "{DRIVE_API_ROOT}/files/generateIds?count={count}&space=appDataFolder&type=files"
            ),
            json_headers(),
            None,
            MAX_SMALL_RESPONSE_BYTES,
        ),
        CloudProviderOperation::Read { id } => (
            CloudHttpMethod::Get,
            format!("{DRIVE_API_ROOT}/files/{}?alt=media", percent_encode(id)),
            Vec::new(),
            None,
            MAX_CLOUD_OBJECT_BYTES,
        ),
        CloudProviderOperation::CreateOnce {
            id,
            name,
            bytes_base64,
        } => {
            let object_bytes = URL_SAFE_NO_PAD.decode(bytes_base64).map_err(|error| {
                protocol_error(format!(
                    "cloud object payload is not valid base64url: {error}"
                ))
            })?;
            if object_bytes.len() as u64 > MAX_CLOUD_OBJECT_BYTES {
                return Err(protocol_error(format!(
                    "cloud object exceeds {MAX_CLOUD_OBJECT_BYTES} bytes"
                )));
            }
            let boundary = format!("aerobag_cloud_{:016x}", request.request_id);
            let metadata = serde_json::json!({
                "id": id,
                "name": name,
                "mimeType": CLOUD_OBJECT_MIME,
                "parents": ["appDataFolder"],
            });
            let metadata = serde_json::to_vec(&metadata)
                .map_err(|error| protocol_error(format!("encode Drive metadata: {error}")))?;
            let mut body = Vec::with_capacity(metadata.len() + object_bytes.len() + 256);
            body.extend_from_slice(
                format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n")
                    .as_bytes(),
            );
            body.extend_from_slice(&metadata);
            body.extend_from_slice(
                format!("\r\n--{boundary}\r\nContent-Type: {CLOUD_OBJECT_MIME}\r\n\r\n").as_bytes(),
            );
            body.extend_from_slice(&object_bytes);
            body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            (
                CloudHttpMethod::Post,
                format!("{DRIVE_UPLOAD_ROOT}/files?uploadType=multipart&fields=id"),
                vec![CloudHttpHeader {
                    name: "Content-Type".to_string(),
                    value: format!("multipart/related; boundary={boundary}"),
                }],
                Some(URL_SAFE_NO_PAD.encode(body)),
                MAX_SMALL_RESPONSE_BYTES,
            )
        }
        CloudProviderOperation::Delete { id } => (
            CloudHttpMethod::Delete,
            format!("{DRIVE_API_ROOT}/files/{}", percent_encode(id)),
            Vec::new(),
            None,
            MAX_SMALL_RESPONSE_BYTES,
        ),
        CloudProviderOperation::List { page_token } => {
            let mut url = format!(
                "{DRIVE_API_ROOT}/files?spaces=appDataFolder&pageSize=1000&q={}&fields={}",
                percent_encode(&format!(
                    "trashed = false and mimeType = '{CLOUD_OBJECT_MIME}'"
                )),
                percent_encode("nextPageToken,files(id,size,createdTime)"),
            );
            if let Some(page_token) = page_token {
                url.push_str("&pageToken=");
                url.push_str(&percent_encode(page_token));
            }
            (
                CloudHttpMethod::Get,
                url,
                json_headers(),
                None,
                MAX_CLOUD_OBJECT_BYTES,
            )
        }
        CloudProviderOperation::AcsIssueAccountChallenge
        | CloudProviderOperation::AcsCreateAccount { .. }
        | CloudProviderOperation::AcsCreateObject { .. }
        | CloudProviderOperation::AcsReadObject { .. }
        | CloudProviderOperation::AcsReadRoot
        | CloudProviderOperation::AcsCompareAndSwapRoot { .. }
        | CloudProviderOperation::AcsCreateSseTicket { .. } => {
            return Err(protocol_error(
                "Aerobag Cloud operation sent to the Google Drive adapter".to_string(),
            ));
        }
    };
    Ok(CloudHttpRequest {
        request_id: request.request_id,
        provider: request.provider,
        method,
        url,
        headers,
        body_base64,
        max_response_bytes,
    })
}

pub(crate) fn parse_response(
    request: &CloudProviderRequest,
    response: CloudHttpResponse,
) -> CloudProviderResponse {
    let (status, body) = match response {
        CloudHttpResponse::TransportError { detail } => {
            return provider_error(CloudProviderErrorKind::Transient, detail);
        }
        CloudHttpResponse::ResponseTooLarge { limit_bytes } => {
            return provider_error(
                CloudProviderErrorKind::Permanent,
                format!("cloud provider response exceeds {limit_bytes} bytes"),
            );
        }
        CloudHttpResponse::Completed {
            status_code,
            body_base64,
        } => match URL_SAFE_NO_PAD.decode(body_base64) {
            Ok(body) => (status_code, body),
            Err(error) => {
                return provider_error(
                    CloudProviderErrorKind::Permanent,
                    format!("cloud provider response is not valid base64url: {error}"),
                );
            }
        },
    };

    match &request.operation {
        CloudProviderOperation::AllocateIds { count } if is_success(status) => {
            #[derive(Deserialize)]
            struct GeneratedIds {
                ids: Option<Vec<String>>,
                space: Option<String>,
            }
            match serde_json::from_slice::<GeneratedIds>(&body) {
                Ok(payload)
                    if payload.space.as_deref() == Some("appDataFolder")
                        && payload.ids.as_ref().is_some_and(|ids| ids.len() == *count) =>
                {
                    CloudProviderResponse::AllocatedIds {
                        ids: payload.ids.expect("generated IDs checked above"),
                    }
                }
                Ok(_) => provider_error(
                    CloudProviderErrorKind::Permanent,
                    "Google Drive returned an invalid generated-ID response".to_string(),
                ),
                Err(error) => invalid_json("generated-ID", error),
            }
        }
        CloudProviderOperation::Read { .. } if status == 404 => {
            CloudProviderResponse::Read { bytes_base64: None }
        }
        CloudProviderOperation::Read { .. } if is_success(status) => {
            if body.len() as u64 > MAX_CLOUD_OBJECT_BYTES {
                provider_error(
                    CloudProviderErrorKind::Permanent,
                    format!("Google Drive cloud object exceeds {MAX_CLOUD_OBJECT_BYTES} bytes"),
                )
            } else {
                CloudProviderResponse::Read {
                    bytes_base64: Some(URL_SAFE_NO_PAD.encode(body)),
                }
            }
        }
        CloudProviderOperation::CreateOnce { .. } if status == 409 || status == 412 => {
            CloudProviderResponse::AlreadyExists
        }
        CloudProviderOperation::CreateOnce { .. } if is_success(status) => {
            CloudProviderResponse::Created
        }
        CloudProviderOperation::Delete { .. } if status == 404 => {
            CloudProviderResponse::Deleted { existed: false }
        }
        CloudProviderOperation::Delete { .. } if is_success(status) => {
            CloudProviderResponse::Deleted { existed: true }
        }
        CloudProviderOperation::List { .. } if is_success(status) => parse_list_response(&body),
        operation => http_error(status, &body, operation_label(operation)),
    }
}

fn parse_list_response(body: &[u8]) -> CloudProviderResponse {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ListPayload {
        #[serde(default)]
        files: Vec<ListFile>,
        next_page_token: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ListFile {
        id: Option<String>,
        size: Option<String>,
        created_time: Option<String>,
    }
    let payload = match serde_json::from_slice::<ListPayload>(body) {
        Ok(payload) => payload,
        Err(error) => return invalid_json("object-list", error),
    };
    let mut objects = Vec::with_capacity(payload.files.len());
    for file in payload.files {
        let Some(id) = file.id.filter(|id| !id.is_empty()) else {
            return provider_error(
                CloudProviderErrorKind::Permanent,
                "Google Drive returned cloud object metadata without an ID".to_string(),
            );
        };
        let Some(size_bytes) = file.size.and_then(|size| size.parse::<u64>().ok()) else {
            return provider_error(
                CloudProviderErrorKind::Permanent,
                format!("Google Drive returned invalid size metadata for {id}"),
            );
        };
        objects.push(CloudProviderObject {
            id,
            size_bytes,
            created_at: file.created_time,
        });
    }
    CloudProviderResponse::Listed {
        objects,
        next_page_token: payload.next_page_token,
    }
}

fn json_headers() -> Vec<CloudHttpHeader> {
    vec![CloudHttpHeader {
        name: "Accept".to_string(),
        value: "application/json".to_string(),
    }]
}

fn http_error(status: u16, body: &[u8], operation: &str) -> CloudProviderResponse {
    let detail_body = String::from_utf8_lossy(body)
        .trim()
        .chars()
        .take(500)
        .collect::<String>();
    let detail = format!(
        "{operation} failed: HTTP {status}{}",
        if detail_body.is_empty() {
            String::new()
        } else {
            format!(" {detail_body}")
        }
    );
    let kind = match status {
        401 | 403 => CloudProviderErrorKind::Unauthorized,
        408 | 425 | 429 | 500..=599 => CloudProviderErrorKind::Transient,
        _ => CloudProviderErrorKind::Permanent,
    };
    provider_error(kind, detail)
}

fn invalid_json(context: &str, error: serde_json::Error) -> CloudProviderResponse {
    provider_error(
        CloudProviderErrorKind::Permanent,
        format!("Google Drive returned invalid {context} JSON: {error}"),
    )
}

fn provider_error(kind: CloudProviderErrorKind, detail: String) -> CloudProviderResponse {
    CloudProviderResponse::Error { kind, detail }
}

fn operation_label(operation: &CloudProviderOperation) -> &'static str {
    match operation {
        CloudProviderOperation::AllocateIds { .. } => "allocate Google Drive object IDs",
        CloudProviderOperation::Read { .. } => "read Google Drive cloud object",
        CloudProviderOperation::CreateOnce { .. } => "create Google Drive cloud object",
        CloudProviderOperation::Delete { .. } => "delete Google Drive cloud object",
        CloudProviderOperation::List { .. } => "list Google Drive cloud objects",
        CloudProviderOperation::AcsIssueAccountChallenge
        | CloudProviderOperation::AcsCreateAccount { .. }
        | CloudProviderOperation::AcsCreateObject { .. }
        | CloudProviderOperation::AcsReadObject { .. }
        | CloudProviderOperation::AcsReadRoot
        | CloudProviderOperation::AcsCompareAndSwapRoot { .. }
        | CloudProviderOperation::AcsCreateSseTicket { .. } => "invalid ACS operation",
    }
}

fn is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

fn percent_encode(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[(byte >> 4) as usize]));
            encoded.push(char::from(HEX[(byte & 0x0f) as usize]));
        }
    }
    encoded
}

fn protocol_error(message: String) -> AppError {
    AppError {
        kind: AppErrorKind::Internal,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(operation: CloudProviderOperation) -> CloudProviderRequest {
        CloudProviderRequest {
            request_id: 7,
            provider: CloudProviderKind::GoogleDrive,
            operation,
        }
    }

    fn completed(status_code: u16, bytes: &[u8]) -> CloudHttpResponse {
        CloudHttpResponse::Completed {
            status_code,
            body_base64: URL_SAFE_NO_PAD.encode(bytes),
        }
    }

    #[test]
    fn plans_paginated_appdata_listing() {
        let planned = plan_request(&request(CloudProviderOperation::List {
            page_token: Some("next page".to_string()),
        }))
        .unwrap();
        assert_eq!(planned.method, CloudHttpMethod::Get);
        assert!(planned.url.contains("spaces=appDataFolder"));
        assert!(planned.url.contains("pageToken=next%20page"));
        assert!(!planned.url.contains(' '));
    }

    #[test]
    fn parses_paginated_appdata_listing() {
        let response = parse_response(
            &request(CloudProviderOperation::List { page_token: None }),
            completed(
                200,
                br#"{"files":[{"id":"object-1","size":"37","createdTime":"2026-07-31T12:00:00Z"}],"nextPageToken":"next page"}"#,
            ),
        );
        assert_eq!(
            response,
            CloudProviderResponse::Listed {
                objects: vec![CloudProviderObject {
                    id: "object-1".to_string(),
                    size_bytes: 37,
                    created_at: Some("2026-07-31T12:00:00Z".to_string()),
                }],
                next_page_token: Some("next page".to_string()),
            }
        );
    }

    #[test]
    fn maps_create_contention_and_missing_delete() {
        let create = request(CloudProviderOperation::CreateOnce {
            id: "occupied".to_string(),
            name: "state".to_string(),
            bytes_base64: "AA".to_string(),
        });
        assert_eq!(
            parse_response(&create, completed(409, b"")),
            CloudProviderResponse::AlreadyExists
        );
        let delete = request(CloudProviderOperation::Delete {
            id: "missing".to_string(),
        });
        assert_eq!(
            parse_response(&delete, completed(404, b"")),
            CloudProviderResponse::Deleted { existed: false }
        );
    }

    #[test]
    fn classifies_http_and_transport_failures() {
        let read = request(CloudProviderOperation::Read {
            id: "state".to_string(),
        });
        assert!(matches!(
            parse_response(&read, completed(401, b"expired")),
            CloudProviderResponse::Error {
                kind: CloudProviderErrorKind::Unauthorized,
                ..
            }
        ));
        assert!(matches!(
            parse_response(
                &read,
                CloudHttpResponse::TransportError {
                    detail: "offline".to_string(),
                }
            ),
            CloudProviderResponse::Error {
                kind: CloudProviderErrorKind::Transient,
                ..
            }
        ));
    }
}
