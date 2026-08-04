// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{convert::Infallible, fs, net::IpAddr, net::SocketAddr, path::PathBuf, time::Duration};

use async_stream::stream;
use axum::{
    body::{to_bytes, Body, Bytes},
    extract::{connect_info::ConnectInfo, Extension, OriginalUri, Path, Request, State},
    http::{header, HeaderMap, HeaderName, Method, StatusCode},
    middleware::{self, Next},
    response::{sse::Event, IntoResponse, Response, Sse},
    routing::{get, post},
    Router,
};
use base64::Engine as _;
use hmac::{Hmac, Mac as _};
use product_contracts::{
    acs_events_path, AcsCompareAndSwapRootRequest, AcsCompareAndSwapRootResponse,
    AcsCreateAccountRequest, AcsCreateObjectRequest, AcsCreateSseTicketRequest, AcsErrorCode,
    AcsErrorResponse, AcsHttpMethod, AcsSseEvent, ACS_CONTRACT_ID, ACS_HEALTH_PATH,
    ACS_STATUS_PATH, AEROBAG_SSE_TRANSPORT_POLICY,
};
use sha2::Sha256;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::cors::{Any, CorsLayer};

use crate::{
    auth::{
        source_network_pseudonym, verify_account_creation_request, verify_registered_request,
        SignedRequest,
    },
    store::{CloudStore, RootEventRecord, StoreError, StoreResult},
    AcsRuntimePolicy,
};

const CLIENT_ADDRESS_HEADER: &str = "aerobag-client-address";
const OPERATOR_STATUS_KDF_LABEL: &[u8] = b"aerobag-cloud-operator-status-v1";

const SERVER_SECRET_BYTES: usize = 32;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub listen: SocketAddr,
    pub server_secret_path: PathBuf,
    pub policy: AcsRuntimePolicy,
}

#[derive(Clone)]
struct AppState {
    store: CloudStore,
    server_secret: [u8; SERVER_SECRET_BYTES],
    policy: AcsRuntimePolicy,
}

#[derive(Clone)]
struct NetworkPseudonym(String);

pub fn server_router(
    store: CloudStore,
    server_secret: [u8; SERVER_SECRET_BYTES],
    policy: AcsRuntimePolicy,
) -> Router {
    let state = AppState {
        store,
        server_secret,
        policy: policy.clone(),
    };
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static("last-event-id"),
            HeaderName::from_static("aerobag-contract"),
            HeaderName::from_static("aerobag-account"),
            HeaderName::from_static("aerobag-key-id"),
            HeaderName::from_static("aerobag-signature-algorithm"),
            HeaderName::from_static("aerobag-timestamp-ms"),
            HeaderName::from_static("aerobag-nonce"),
            HeaderName::from_static("aerobag-body-sha256"),
            HeaderName::from_static("aerobag-signature"),
        ]);
    Router::new()
        .route(
            "/cloud/v1/account-challenges",
            post(issue_account_challenge),
        )
        .route("/cloud/v1/accounts", post(create_account))
        .route(
            "/cloud/v1/accounts/{account}/objects/{object}",
            get(read_object).put(create_object).delete(delete_object),
        )
        .route("/cloud/v1/accounts/{account}/objects", get(list_objects))
        .route(
            "/cloud/v1/accounts/{account}/root",
            get(read_root).put(compare_and_swap_root),
        )
        .route(
            "/cloud/v1/accounts/{account}/event-tickets",
            post(create_sse_ticket),
        )
        .route(acs_events_path(), get(events))
        .route(ACS_HEALTH_PATH, get(health))
        .route(ACS_STATUS_PATH, get(status))
        .fallback(unknown_route)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            bound_request_body,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            enforce_network_request_limit,
        ))
        .layer(ConcurrencyLimitLayer::new(
            policy.request.max_concurrent_requests as usize,
        ))
        .layer(cors)
        .with_state(state)
}

async fn unknown_route(State(state): State<AppState>) -> ApiError {
    state.store.increment_malformed_rejection();
    ApiError::not_found("ACS resource does not exist")
}

async fn method_not_allowed(State(state): State<AppState>) -> ApiError {
    state.store.increment_malformed_rejection();
    ApiError::invalid("HTTP method is not allowed for this ACS resource")
}

async fn enforce_network_request_limit(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    if request.uri().to_string().len() > state.policy.request.max_target_bytes as usize {
        state.store.increment_malformed_rejection();
        return ApiError::invalid("request target exceeds the ACS limit").into_response();
    }
    let Some(source) = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .copied()
    else {
        return ApiError::internal("request source address is unavailable").into_response();
    };
    let client_ip = match effective_client_ip(
        source.0.ip(),
        request.headers(),
        &state.policy.trusted_proxy_ips,
    ) {
        Ok(client_ip) => client_ip,
        Err(error) => {
            state.store.increment_malformed_rejection();
            return error.into_response();
        }
    };
    let network = source_network_pseudonym(&state.server_secret, client_ip);
    let checked_network = network.clone();
    if let Err(error) = blocking(state.store, move |store| {
        store.check_network_operation(&checked_network, now_epoch_ms())
    })
    .await
    {
        return ApiError::from(error).into_response();
    }
    request.extensions_mut().insert(NetworkPseudonym(network));
    next.run(request).await
}

async fn bound_request_body(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let (parts, body) = request.into_parts();
    match to_bytes(body, state.policy.request.max_body_bytes as usize).await {
        Ok(bytes) => {
            next.run(Request::from_parts(parts, Body::from(bytes)))
                .await
        }
        Err(_) => {
            state.store.increment_malformed_rejection();
            ApiError::payload_too_large().into_response()
        }
    }
}

pub async fn run_server(store: CloudStore, config: ServerConfig) -> anyhow::Result<()> {
    let server_secret = load_server_secret(&config.server_secret_path)?;
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    eprintln!(
        "Aerobag Cloud Server listening on {}",
        listener.local_addr()?
    );
    let gc_store = store.clone();
    let gc_interval = Duration::from_secs(config.policy.garbage_collection.interval_seconds);
    let gc_grace_ms = i64::try_from(
        config
            .policy
            .garbage_collection
            .orphan_grace_seconds
            .saturating_mul(1_000),
    )
    .unwrap_or(i64::MAX);
    let gc_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(gc_interval);
        loop {
            interval.tick().await;
            let store = gc_store.clone();
            match blocking(store, move |store| store.run_gc(now_epoch_ms(), gc_grace_ms)).await {
                Ok(report) => eprintln!(
                    "ACS garbage collection marked={} deleted_objects={} deleted_blob_files={} deleted_bytes={} database_pause_ms={} elapsed_ms={}",
                    report.marked_objects,
                    report.deleted_objects,
                    report.deleted_blob_files,
                    report.deleted_ciphertext_bytes,
                    report.database_pause_ms,
                    report.total_elapsed_ms,
                ),
                Err(error) => eprintln!("ACS garbage collection failed: {error}"),
            }
        }
    });
    let result = axum::serve(
        listener,
        server_router(store, server_secret, config.policy)
            .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await;
    gc_task.abort();
    result?;
    Ok(())
}

fn effective_client_ip(
    peer_ip: IpAddr,
    headers: &HeaderMap,
    trusted_proxy_ips: &[IpAddr],
) -> Result<IpAddr, ApiError> {
    if !trusted_proxy_ips.contains(&peer_ip) {
        return Ok(peer_ip);
    }
    let Some(value) = headers.get(CLIENT_ADDRESS_HEADER) else {
        return Ok(peer_ip);
    };
    value
        .to_str()
        .ok()
        .and_then(|value| value.parse::<IpAddr>().ok())
        .ok_or_else(|| ApiError::invalid("trusted proxy supplied an invalid client address"))
}

fn load_server_secret(path: &PathBuf) -> anyhow::Result<[u8; SERVER_SECRET_BYTES]> {
    let bytes = fs::read(path)
        .map_err(|error| anyhow::anyhow!("read ACS server secret {}: {error}", path.display()))?;
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        anyhow::anyhow!(
            "ACS server secret {} must be exactly {SERVER_SECRET_BYTES} bytes, got {}",
            path.display(),
            bytes.len()
        )
    })
}

async fn issue_account_challenge(
    State(state): State<AppState>,
    Extension(network): Extension<NetworkPseudonym>,
) -> Result<Response, ApiError> {
    let response = blocking(state.store.clone(), move |store| {
        store.issue_creation_challenge(&network.0, now_epoch_ms())
    })
    .await?;
    json(StatusCode::OK, &response)
}

async fn create_account(
    State(state): State<AppState>,
    Extension(network): Extension<NetworkPseudonym>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: AcsCreateAccountRequest = decode_json(&state.store, &body)?;
    require_contract(&state.store, &request.contract_id)?;
    let path = uri.path().to_string();
    let body = body.to_vec();
    let response = blocking(state.store, move |store| {
        let now = now_epoch_ms();
        let public_key = verify_account_creation_request(
            &store,
            SignedRequest {
                headers: &headers,
                method: AcsHttpMethod::Post,
                path: &path,
                query: None,
                body: &body,
                now_epoch_ms: now,
            },
            &request,
        )?;
        store.create_account(&request, &public_key, &network.0, now)
    })
    .await?;
    json(StatusCode::CREATED, &response)
}

async fn create_object(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    Path((account, object)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: AcsCreateObjectRequest = decode_json(&state.store, &body)?;
    require_contract(&state.store, &request.contract_id)?;
    if request.object_id != object {
        state.store.increment_malformed_rejection();
        return Err(ApiError::invalid("object ID does not match request path"));
    }
    let path = uri.path().to_string();
    let query = uri.query().map(str::to_string);
    let body = body.to_vec();
    let response = blocking(state.store, move |store| {
        let now = now_epoch_ms();
        verify_registered_request(
            &store,
            SignedRequest {
                headers: &headers,
                method: AcsHttpMethod::Put,
                path: &path,
                query: query.as_deref(),
                body: &body,
                now_epoch_ms: now,
            },
            &account,
            true,
        )?;
        store.create_object(&account, &object, &request.value, now)
    })
    .await?;
    json(StatusCode::OK, &response)
}

async fn read_object(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    Path((account, object)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let path = uri.path().to_string();
    let query = uri.query().map(str::to_string);
    let response = blocking(state.store, move |store| {
        verify_registered_request(
            &store,
            SignedRequest {
                headers: &headers,
                method: AcsHttpMethod::Get,
                path: &path,
                query: query.as_deref(),
                body: b"",
                now_epoch_ms: now_epoch_ms(),
            },
            &account,
            false,
        )?;
        store.read_object(&account, &object)
    })
    .await?;
    json(StatusCode::OK, &response)
}

async fn delete_object(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    Path((account, object)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let path = uri.path().to_string();
    let query = uri.query().map(str::to_string);
    blocking(state.store, move |store| {
        let now = now_epoch_ms();
        verify_registered_request(
            &store,
            SignedRequest {
                headers: &headers,
                method: AcsHttpMethod::Delete,
                path: &path,
                query: query.as_deref(),
                body: b"",
                now_epoch_ms: now,
            },
            &account,
            true,
        )?;
        store.delete_object(&account, &object, now)
    })
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

struct ListQuery {
    cursor: Option<String>,
    limit: Option<u32>,
}

async fn list_objects(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    Path(account): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let request = parse_list_query(&state.store, uri.query())?;
    let path = uri.path().to_string();
    let query = uri.query().map(str::to_string);
    let response = blocking(state.store, move |store| {
        verify_registered_request(
            &store,
            SignedRequest {
                headers: &headers,
                method: AcsHttpMethod::Get,
                path: &path,
                query: query.as_deref(),
                body: b"",
                now_epoch_ms: now_epoch_ms(),
            },
            &account,
            false,
        )?;
        store.list_objects(
            &account,
            request.cursor.as_deref(),
            request.limit.unwrap_or(100),
        )
    })
    .await?;
    json(StatusCode::OK, &response)
}

async fn read_root(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    Path(account): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let path = uri.path().to_string();
    let query = uri.query().map(str::to_string);
    let response = blocking(state.store, move |store| {
        verify_registered_request(
            &store,
            SignedRequest {
                headers: &headers,
                method: AcsHttpMethod::Get,
                path: &path,
                query: query.as_deref(),
                body: b"",
                now_epoch_ms: now_epoch_ms(),
            },
            &account,
            false,
        )?;
        store.read_root(&account)
    })
    .await?;
    json(StatusCode::OK, &response)
}

async fn compare_and_swap_root(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    Path(account): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: AcsCompareAndSwapRootRequest = decode_json(&state.store, &body)?;
    require_contract(&state.store, &request.contract_id)?;
    let path = uri.path().to_string();
    let query = uri.query().map(str::to_string);
    let body = body.to_vec();
    let response = blocking(state.store, move |store| {
        let now = now_epoch_ms();
        verify_registered_request(
            &store,
            SignedRequest {
                headers: &headers,
                method: AcsHttpMethod::Put,
                path: &path,
                query: query.as_deref(),
                body: &body,
                now_epoch_ms: now,
            },
            &account,
            true,
        )?;
        store.compare_and_swap_root(&account, &request, now)
    })
    .await?;
    let status = if matches!(response, AcsCompareAndSwapRootResponse::Conflict { .. }) {
        StatusCode::CONFLICT
    } else {
        StatusCode::OK
    };
    json(status, &response)
}

async fn create_sse_ticket(
    State(state): State<AppState>,
    Extension(network): Extension<NetworkPseudonym>,
    OriginalUri(uri): OriginalUri,
    Path(account): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let request: AcsCreateSseTicketRequest = decode_json(&state.store, &body)?;
    require_contract(&state.store, &request.contract_id)?;
    let path = uri.path().to_string();
    let query = uri.query().map(str::to_string);
    let body = body.to_vec();
    let response = blocking(state.store, move |store| {
        let now = now_epoch_ms();
        verify_registered_request(
            &store,
            SignedRequest {
                headers: &headers,
                method: AcsHttpMethod::Post,
                path: &path,
                query: query.as_deref(),
                body: &body,
                now_epoch_ms: now,
            },
            &account,
            false,
        )?;
        store.create_sse_ticket(&account, request.last_event_sequence, &network.0, now)
    })
    .await?;
    json(StatusCode::OK, &response)
}

struct EventQuery {
    ticket: String,
}

async fn events(
    State(state): State<AppState>,
    Extension(network): Extension<NetworkPseudonym>,
    OriginalUri(uri): OriginalUri,
) -> Result<Response, ApiError> {
    let request = parse_event_query(&state.store, uri.query())?;
    let ticket = request.ticket;
    let ticket_network = network.0.clone();
    let consumed = blocking(state.store.clone(), move |store| {
        store.consume_sse_ticket(&ticket, &ticket_network, now_epoch_ms())
    })
    .await?;
    let account = consumed.account_locator;
    let connection_account = account.clone();
    let connection_network = network.0.clone();
    blocking(state.store.clone(), move |store| {
        store.begin_sse_connection(&connection_account, &connection_network)
    })
    .await?;
    let guard = SseConnectionGuard {
        store: state.store.clone(),
        account: account.clone(),
        network: network.0,
    };
    let mut receiver = state.store.subscribe();
    let initial_account = account.clone();
    let initial = blocking(state.store.clone(), move |store| {
        store.initial_sse_events(&initial_account, consumed.last_event_sequence)
    })
    .await?;
    let stream = stream! {
        let _guard = guard;
        let mut last_sequence = 0_u64;
        for event in initial {
            last_sequence = last_sequence.max(event.sequence());
            yield Ok::<Event, Infallible>(sse_event(&event));
        }
        let mut heartbeat = tokio::time::interval(Duration::from_millis(
            AEROBAG_SSE_TRANSPORT_POLICY.heartbeat_interval_ms as u64,
        ));
        heartbeat.tick().await;
        loop {
            tokio::select! {
                received = receiver.recv() => match received {
                    Ok(RootEventRecord { account_locator, event }) => {
                        if account_locator == account && event.sequence() > last_sequence {
                            last_sequence = event.sequence();
                            yield Ok::<Event, Infallible>(sse_event(&event));
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let reset_account = account.clone();
                        let reset = blocking(state.store.clone(), move |store| {
                            let heartbeat = store.heartbeat_event(&reset_account)?;
                            Ok(match heartbeat {
                                AcsSseEvent::Heartbeat { sequence, root_revision, root_hash } => {
                                    AcsSseEvent::Reset { sequence, root_revision, root_hash }
                                }
                                _ => unreachable!("heartbeat_event returned another event kind"),
                            })
                        }).await;
                        match reset {
                            Ok(reset) => {
                                last_sequence = reset.sequence();
                                yield Ok::<Event, Infallible>(sse_event(&reset));
                            }
                            Err(_) => break,
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                _ = heartbeat.tick() => {
                    let heartbeat_account = account.clone();
                    match blocking(state.store.clone(), move |store| {
                        store.heartbeat_event(&heartbeat_account)
                    }).await {
                        Ok(event) => {
                            last_sequence = last_sequence.max(event.sequence());
                            yield Ok::<Event, Infallible>(sse_event(&event));
                        }
                        Err(_) => break,
                    }
                }
            }
        }
    };
    Ok(Sse::new(stream).into_response())
}

async fn status(
    ConnectInfo(source): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    if !source.ip().is_loopback() || headers.contains_key(CLIENT_ADDRESS_HEADER) {
        return Err(ApiError::not_found("ACS resource does not exist"));
    }
    verify_operator_status_authorization(&headers, &state.server_secret)?;
    let status = blocking(state.store, |store| store.status(now_epoch_ms())).await?;
    json(StatusCode::OK, &status)
}

fn verify_operator_status_authorization(
    headers: &HeaderMap,
    server_secret: &[u8; SERVER_SECRET_BYTES],
) -> Result<(), ApiError> {
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .and_then(|value| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(value)
                .ok()
        })
        .ok_or_else(|| ApiError::unauthorized("operator authorization is required"))?;
    let mut mac = Hmac::<Sha256>::new_from_slice(server_secret)
        .expect("HMAC-SHA256 accepts the fixed ACS server secret size");
    mac.update(OPERATOR_STATUS_KDF_LABEL);
    mac.verify_slice(&supplied)
        .map_err(|_| ApiError::unauthorized("operator authorization is invalid"))
}

async fn health(State(state): State<AppState>) -> Result<Response, ApiError> {
    let health = blocking(state.store, |store| store.health(now_epoch_ms())).await?;
    json(StatusCode::OK, &health)
}

struct SseConnectionGuard {
    store: CloudStore,
    account: String,
    network: String,
}

impl Drop for SseConnectionGuard {
    fn drop(&mut self) {
        self.store.end_sse_connection(&self.account, &self.network);
    }
}

fn sse_event(event: &AcsSseEvent) -> Event {
    let kind = match event {
        AcsSseEvent::Ready { .. } => "ready",
        AcsSseEvent::RootChanged { .. } => "root-changed",
        AcsSseEvent::Reset { .. } => "reset",
        AcsSseEvent::Heartbeat { .. } => "heartbeat",
    };
    Event::default()
        .event(kind)
        .id(event.sequence().to_string())
        .json_data(event)
        .expect("ACS SSE events always serialize")
}

fn require_contract(store: &CloudStore, contract_id: &str) -> Result<(), ApiError> {
    if contract_id == ACS_CONTRACT_ID {
        Ok(())
    } else {
        store.increment_malformed_rejection();
        Err(ApiError::invalid("unsupported ACS contract"))
    }
}

fn decode_json<T: serde::de::DeserializeOwned>(
    store: &CloudStore,
    body: &[u8],
) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|error| {
        store.increment_malformed_rejection();
        ApiError::invalid(format!("malformed JSON request: {error}"))
    })
}

fn parse_list_query(store: &CloudStore, query: Option<&str>) -> Result<ListQuery, ApiError> {
    let mut cursor = None;
    let mut limit = None;
    for (key, value) in query_pairs(store, query)? {
        match key {
            "cursor" if cursor.is_none() && valid_query_token(value) => {
                cursor = Some(value.to_string());
            }
            "limit" if limit.is_none() => {
                limit = Some(value.parse::<u32>().map_err(|_| {
                    store.increment_malformed_rejection();
                    ApiError::invalid("object list limit is invalid")
                })?);
            }
            "cursor" | "limit" => {
                store.increment_malformed_rejection();
                return Err(ApiError::invalid("duplicate or invalid object list query"));
            }
            _ => {
                store.increment_malformed_rejection();
                return Err(ApiError::invalid("unknown object list query parameter"));
            }
        }
    }
    Ok(ListQuery { cursor, limit })
}

fn parse_event_query(store: &CloudStore, query: Option<&str>) -> Result<EventQuery, ApiError> {
    let pairs = query_pairs(store, query)?;
    if pairs.len() != 1 || pairs[0].0 != "ticket" || !valid_query_token(pairs[0].1) {
        store.increment_malformed_rejection();
        return Err(ApiError::invalid(
            "SSE request requires exactly one valid ticket",
        ));
    }
    Ok(EventQuery {
        ticket: pairs[0].1.to_string(),
    })
}

fn query_pairs<'a>(
    store: &CloudStore,
    query: Option<&'a str>,
) -> Result<Vec<(&'a str, &'a str)>, ApiError> {
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return Ok(Vec::new());
    };
    query
        .split('&')
        .map(|pair| {
            pair.split_once('=').ok_or_else(|| {
                store.increment_malformed_rejection();
                ApiError::invalid("query parameter is malformed")
            })
        })
        .collect()
}

fn valid_query_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn json(status: StatusCode, value: &impl serde::Serialize) -> Result<Response, ApiError> {
    let body = serde_json::to_vec(value)
        .map_err(|error| ApiError::internal(format!("encode JSON response: {error}")))?;
    Ok((status, [(header::CONTENT_TYPE, "application/json")], body).into_response())
}

async fn blocking<T, F>(store: CloudStore, operation: F) -> StoreResult<T>
where
    T: Send + 'static,
    F: FnOnce(CloudStore) -> StoreResult<T> + Send + 'static,
{
    tokio::task::spawn_blocking(move || operation(store))
        .await
        .map_err(|error| {
            StoreError::new(
                AcsErrorCode::Internal,
                format!("cloud worker failed: {error}"),
            )
        })?
}

#[derive(Debug)]
struct ApiError {
    error: StoreError,
    request_id: String,
}

impl ApiError {
    fn invalid(message: impl Into<String>) -> Self {
        Self::from(StoreError::new(AcsErrorCode::InvalidRequest, message))
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::from(StoreError::new(AcsErrorCode::Internal, message))
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::from(StoreError::new(AcsErrorCode::NotFound, message))
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self::from(StoreError::new(AcsErrorCode::Unauthorized, message))
    }

    fn payload_too_large() -> Self {
        Self::from(StoreError::new(
            AcsErrorCode::PayloadTooLarge,
            "request body exceeds the ACS limit",
        ))
    }
}

impl From<StoreError> for ApiError {
    fn from(error: StoreError) -> Self {
        let mut random = [0_u8; 12];
        let request_id = match getrandom::fill(&mut random) {
            Ok(()) => base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random),
            Err(_) => "request-id-unavailable".to_string(),
        };
        Self { error, request_id }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.error.code.http_status())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let retry_after_ms = self.error.retry_after_ms;
        let rate_limit_gate = self.error.rate_limit_gate;
        let response = AcsErrorResponse {
            contract_id: ACS_CONTRACT_ID.to_string(),
            request_id: self.request_id,
            code: self.error.code,
            message: self.error.message,
            retry_after_ms,
            rate_limit_gate,
        };
        let mut response = json(status, &response)
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
        if let Some(retry_after_ms) = retry_after_ms {
            if let Ok(value) =
                header::HeaderValue::from_str(&retry_after_ms.div_ceil(1_000).to_string())
            {
                response.headers_mut().insert(header::RETRY_AFTER, value);
            }
        }
        response
    }
}

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use ed25519_dalek::{Signer as _, SigningKey};
    use http_body_util::BodyExt as _;
    use product_contracts::{
        AcsCompareAndSwapRootRequest, AcsCreateAccountRequest, AcsCreateObjectRequest,
        AcsCreateSseTicketRequest, AcsCreateSseTicketResponse, AcsCreationChallengeResponse,
        AcsEncryptedValue, AcsErrorResponse, AcsRequestAuthentication, AcsSignatureAlgorithm,
        ACS_ACCOUNT_LOCATOR_BYTES, ACS_AUTH_ACCOUNT_HEADER, ACS_AUTH_ALGORITHM_HEADER,
        ACS_AUTH_BODY_HASH_HEADER, ACS_AUTH_CONTRACT_HEADER, ACS_AUTH_KEY_ID_HEADER,
        ACS_AUTH_NONCE_HEADER, ACS_AUTH_SIGNATURE_HEADER, ACS_AUTH_TIMESTAMP_HEADER,
        ACS_REQUEST_NONCE_BYTES, ACS_SIGNING_KEY_ID_BYTES,
    };
    use sha2::{Digest as _, Sha256};
    use tempfile::TempDir;
    use tower::ServiceExt as _;

    fn test_policy() -> AcsRuntimePolicy {
        crate::policy::checked_in_test_policy()
    }

    fn test_router(store: CloudStore) -> Router {
        test_router_with_peer(store, "192.0.2.10:1234")
    }

    fn test_router_with_peer(store: CloudStore, peer: &str) -> Router {
        server_router(store, [0x5a; SERVER_SECRET_BYTES], test_policy()).layer(axum::Extension(
            ConnectInfo(peer.parse::<SocketAddr>().unwrap()),
        ))
    }

    fn signed_headers(
        signing_key: &SigningKey,
        account_locator: &str,
        method: AcsHttpMethod,
        target: &str,
        body: &[u8],
        nonce: u8,
    ) -> HeaderMap {
        let key_id = URL_SAFE_NO_PAD.encode(
            &Sha256::digest(signing_key.verifying_key().as_bytes())[..ACS_SIGNING_KEY_ID_BYTES],
        );
        let now = now_epoch_ms();
        let mut authentication = AcsRequestAuthentication {
            contract_id: ACS_CONTRACT_ID.to_string(),
            account_locator: account_locator.to_string(),
            signing_key_id: key_id,
            signature_algorithm: AcsSignatureAlgorithm::Ed25519,
            timestamp_epoch_ms: now,
            nonce_base64url: URL_SAFE_NO_PAD.encode([nonce; ACS_REQUEST_NONCE_BYTES]),
            body_sha256: hex_bytes(&Sha256::digest(body)),
            signature_base64url: String::new(),
        };
        authentication.signature_base64url = URL_SAFE_NO_PAD.encode(
            signing_key
                .sign(&authentication.signing_bytes(method, target).unwrap())
                .to_bytes(),
        );
        let timestamp = now.to_string();
        let mut headers = HeaderMap::new();
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
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                header::HeaderValue::from_str(value).unwrap(),
            );
        }
        headers
    }

    fn hex_bytes(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn operator_authorization() -> header::HeaderValue {
        let mut mac = Hmac::<Sha256>::new_from_slice(&[0x5a; SERVER_SECRET_BYTES]).unwrap();
        mac.update(OPERATOR_STATUS_KDF_LABEL);
        let token = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        header::HeaderValue::from_str(&format!("Bearer {token}")).unwrap()
    }

    #[test]
    fn operator_authorization_derivation_matches_pipeline_health() {
        assert_eq!(
            operator_authorization(),
            "Bearer oAvfo7uXmJVexL5TLb2Uwt5nQZ7smFsvuqkN6YXikFg"
        );
    }

    #[tokio::test]
    async fn status_is_bounded_json_and_challenge_knows_source_without_storing_ip() {
        let root = TempDir::new().unwrap();
        let store = CloudStore::open(crate::StoreConfig::for_test_data_root(
            root.path().to_path_buf(),
        ))
        .unwrap();
        let router = test_router(store.clone());
        let challenge = router
            .clone()
            .oneshot(
                Request::post("/cloud/v1/account-challenges")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(challenge.status(), StatusCode::OK);
        let mut status_request = Request::get(ACS_STATUS_PATH).body(Body::empty()).unwrap();
        status_request
            .headers_mut()
            .insert(header::AUTHORIZATION, operator_authorization());
        let status = test_router_with_peer(store.clone(), "127.0.0.1:1234")
            .oneshot(status_request)
            .await
            .unwrap();
        assert_eq!(status.status(), StatusCode::OK);
        let body = status.into_body().collect().await.unwrap().to_bytes();
        assert!(body.len() < 64 * 1024);
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(!text.contains("192.0.2.10"));

        let unauthorized = test_router_with_peer(store.clone(), "127.0.0.1:1234")
            .oneshot(Request::get(ACS_STATUS_PATH).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let mut proxied_status = Request::get(ACS_STATUS_PATH).body(Body::empty()).unwrap();
        proxied_status
            .headers_mut()
            .insert(CLIENT_ADDRESS_HEADER, "198.51.100.24".parse().unwrap());
        let response = test_router_with_peer(store, "127.0.0.1:1234")
            .oneshot(proxied_status)
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let health = test_router(
            CloudStore::open(crate::StoreConfig::for_test_data_root(
                root.path().join("health"),
            ))
            .unwrap(),
        )
        .oneshot(Request::get(ACS_HEALTH_PATH).body(Body::empty()).unwrap())
        .await
        .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
        let health_text = String::from_utf8(
            health
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(!health_text.contains("top_contributors"));
        assert!(!health_text.contains("metrics"));
    }

    #[test]
    fn client_address_header_is_honored_only_from_a_trusted_proxy() {
        let trusted: IpAddr = "127.0.0.1".parse().unwrap();
        let client: IpAddr = "198.51.100.24".parse().unwrap();
        let attacker: IpAddr = "203.0.113.9".parse().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(CLIENT_ADDRESS_HEADER, "198.51.100.24".parse().unwrap());

        assert_eq!(
            effective_client_ip(trusted, &headers, &[trusted]).unwrap(),
            client
        );
        assert_eq!(
            effective_client_ip(attacker, &headers, &[trusted]).unwrap(),
            attacker
        );
        assert_eq!(
            effective_client_ip(trusted, &HeaderMap::new(), &[trusted]).unwrap(),
            trusted
        );
        headers.insert(CLIENT_ADDRESS_HEADER, "not-an-ip".parse().unwrap());
        assert!(effective_client_ip(trusted, &headers, &[trusted]).is_err());
    }

    #[tokio::test]
    async fn signed_account_creation_and_replay_rejection_cross_the_http_boundary() {
        let root = TempDir::new().unwrap();
        let store = CloudStore::open(crate::StoreConfig::for_test_data_root(
            root.path().to_path_buf(),
        ))
        .unwrap();
        let router = test_router(store);
        let challenge = router
            .clone()
            .oneshot(
                Request::post("/cloud/v1/account-challenges")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let challenge: AcsCreationChallengeResponse =
            serde_json::from_slice(&challenge.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
        let account = URL_SAFE_NO_PAD.encode([7_u8; ACS_ACCOUNT_LOCATOR_BYTES]);
        let key_id = URL_SAFE_NO_PAD.encode(
            &Sha256::digest(signing_key.verifying_key().as_bytes())[..ACS_SIGNING_KEY_ID_BYTES],
        );
        let create = AcsCreateAccountRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            account_locator: account.clone(),
            signing_key_id: key_id,
            signing_public_key_base64url: URL_SAFE_NO_PAD
                .encode(signing_key.verifying_key().as_bytes()),
            creation_challenge: challenge.challenge,
        };
        let body = serde_json::to_vec(&create).unwrap();
        let mut request = Request::post("/cloud/v1/accounts")
            .body(Body::from(body.clone()))
            .unwrap();
        *request.headers_mut() = signed_headers(
            &signing_key,
            &account,
            AcsHttpMethod::Post,
            "/cloud/v1/accounts",
            &body,
            1,
        );
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let target = format!("/cloud/v1/accounts/{account}/root");
        let headers = signed_headers(&signing_key, &account, AcsHttpMethod::Get, &target, b"", 2);
        for expected in [StatusCode::NOT_FOUND, StatusCode::UNAUTHORIZED] {
            let mut request = Request::get(&target).body(Body::empty()).unwrap();
            *request.headers_mut() = headers.clone();
            let response = router.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), expected);
            let error: AcsErrorResponse =
                serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            if expected == StatusCode::UNAUTHORIZED {
                assert_eq!(error.code, AcsErrorCode::ReplayDetected);
            }
        }

        let object_target = format!("/cloud/v1/accounts/{account}/objects/page");
        let object_request = AcsCreateObjectRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            object_id: "page".to_string(),
            value: AcsEncryptedValue::from_ciphertext(b"page", vec![]),
        };
        let object_body = serde_json::to_vec(&object_request).unwrap();
        let mut request = Request::put(&object_target)
            .body(Body::from(object_body.clone()))
            .unwrap();
        *request.headers_mut() = signed_headers(
            &signing_key,
            &account,
            AcsHttpMethod::Put,
            &object_target,
            &object_body,
            3,
        );
        assert_eq!(
            router.clone().oneshot(request).await.unwrap().status(),
            StatusCode::OK
        );

        let root_request = AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision: 0,
            expected_root_hash: None,
            replacement: AcsEncryptedValue::from_ciphertext(b"root", vec!["page".to_string()]),
        };
        let root_body = serde_json::to_vec(&root_request).unwrap();
        let mut request = Request::put(&target)
            .body(Body::from(root_body.clone()))
            .unwrap();
        *request.headers_mut() = signed_headers(
            &signing_key,
            &account,
            AcsHttpMethod::Put,
            &target,
            &root_body,
            4,
        );
        assert_eq!(
            router.clone().oneshot(request).await.unwrap().status(),
            StatusCode::OK
        );

        let ticket_target = format!("/cloud/v1/accounts/{account}/event-tickets");
        let ticket_request = AcsCreateSseTicketRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            last_event_sequence: None,
        };
        let ticket_body = serde_json::to_vec(&ticket_request).unwrap();
        let mut request = Request::post(&ticket_target)
            .body(Body::from(ticket_body.clone()))
            .unwrap();
        *request.headers_mut() = signed_headers(
            &signing_key,
            &account,
            AcsHttpMethod::Post,
            &ticket_target,
            &ticket_body,
            5,
        );
        let response = router.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let ticket: AcsCreateSseTicketResponse =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        let response = router
            .oneshot(Request::get(ticket.events_url).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let frame = tokio::time::timeout(Duration::from_secs(1), response.into_body().frame())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let event = String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap();
        assert!(event.contains("event: ready"));
        assert!(event.contains("\"root_revision\":1"));
    }

    #[tokio::test]
    async fn malformed_and_oversized_requests_return_typed_errors() {
        let root = TempDir::new().unwrap();
        let store = CloudStore::open(crate::StoreConfig::for_test_data_root(
            root.path().to_path_buf(),
        ))
        .unwrap();
        let router = test_router(store);
        for (body, expected) in [
            (Body::from("{"), StatusCode::BAD_REQUEST),
            (
                Body::from(vec![
                    0_u8;
                    test_policy().request.max_body_bytes as usize + 1
                ]),
                StatusCode::PAYLOAD_TOO_LARGE,
            ),
        ] {
            let response = router
                .clone()
                .oneshot(Request::post("/cloud/v1/accounts").body(body).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            let error: AcsErrorResponse =
                serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                    .unwrap();
            assert_eq!(error.code.http_status(), expected.as_u16());
        }
    }

    #[tokio::test]
    async fn rate_limit_response_identifies_gate_and_exact_retry_delay() {
        let response = ApiError::from(StoreError {
            code: AcsErrorCode::RateLimited,
            message: "network creation bucket is empty".to_string(),
            retry_after_ms: Some(28_800_001),
            rate_limit_gate: Some(product_contracts::AcsRateLimitGate::AccountCreationNetwork),
        })
        .into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "28801");
        let error: AcsErrorResponse =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(error.retry_after_ms, Some(28_800_001));
        assert_eq!(
            error.rate_limit_gate,
            Some(product_contracts::AcsRateLimitGate::AccountCreationNetwork)
        );
    }
}
