// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs,
    net::SocketAddr,
    str::FromStr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, ensure, Context as _};
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{header, HeaderMap, HeaderName, Method, Request, StatusCode},
    Extension, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signer as _, SigningKey};
use hmac::{Hmac, Mac as _};
use http_body_util::BodyExt as _;
use product_contracts::{
    acs_canonical_request_target, AcsCompareAndSwapRootRequest, AcsCompareAndSwapRootResponse,
    AcsCreateAccountRequest, AcsCreateObjectRequest, AcsCreateSseTicketRequest,
    AcsCreateSseTicketResponse, AcsCreationChallengeResponse, AcsEncryptedValue, AcsErrorCode,
    AcsErrorResponse, AcsHttpMethod, AcsListObjectsResponse, AcsObjectSnapshot, AcsRateLimitGate,
    AcsRequestAuthentication, AcsRootSnapshot, AcsSignatureAlgorithm, AcsSseEvent,
    AcsStatusResponse, ACS_ACCOUNT_LOCATOR_BYTES, ACS_AUTH_ACCOUNT_HEADER,
    ACS_AUTH_ALGORITHM_HEADER, ACS_AUTH_BODY_HASH_HEADER, ACS_AUTH_CONTRACT_HEADER,
    ACS_AUTH_KEY_ID_HEADER, ACS_AUTH_NONCE_HEADER, ACS_AUTH_SIGNATURE_HEADER,
    ACS_AUTH_TIMESTAMP_HEADER, ACS_CONTRACT_ID, ACS_REQUEST_NONCE_BYTES, ACS_SIGNING_KEY_ID_BYTES,
    ACS_STATUS_PATH,
};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tempfile::TempDir;
use tokio::{sync::Semaphore, task::JoinSet, time::timeout};
use tower::ServiceExt as _;

use crate::{
    create_backup, server_router, verify_backup, AccountMode, AcsRuntimePolicy, BackupReport,
    CloudStore, GcReport, StoreConfig,
};

const SERVER_SECRET: [u8; 32] = [0x5a; 32];
const OPERATOR_STATUS_KDF_LABEL: &[u8] = b"aerobag-cloud-operator-status-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadProfile {
    Ci,
    Production,
}

impl FromStr for WorkloadProfile {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ci" => Ok(Self::Ci),
            "production" => Ok(Self::Production),
            _ => bail!("unsupported ACS workload profile {value:?}; expected ci or production"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct WorkloadDimensions {
    pub load_stages: u32,
    pub accounts_per_stage: u32,
    pub objects_per_account: u32,
    pub inline_object_bytes: u64,
    pub blob_objects_per_account: u32,
    pub blob_object_bytes: u64,
    pub read_rounds: u32,
    pub max_concurrency: u32,
    pub sse_connections: u32,
    pub request_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatencyReport {
    pub count: u64,
    pub elapsed_ms: f64,
    pub throughput_per_second: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StageReport {
    pub stage: u32,
    pub accounts_after_stage: u32,
    pub objects_after_stage: u64,
    pub create_accounts: LatencyReport,
    pub create_objects: LatencyReport,
    pub commit_roots: LatencyReport,
    pub read_objects: LatencyReport,
    pub list_objects: LatencyReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct FalloffReport {
    pub operation: String,
    pub first_stage_p95_ms: f64,
    pub last_stage_p95_ms: f64,
    pub p95_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckReport {
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusExpectation {
    pub metric_id: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusScenario {
    pub name: String,
    pub status: AcsStatusResponse,
    pub expected_pipeline_health: Vec<StatusExpectation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkloadReport {
    pub schema_version: u32,
    pub profile: WorkloadProfile,
    pub started_at_epoch_ms: i64,
    pub completed_at_epoch_ms: i64,
    pub total_elapsed_ms: u64,
    pub dimensions: WorkloadDimensions,
    pub stages: Vec<StageReport>,
    pub falloff: Vec<FalloffReport>,
    pub sse_open: LatencyReport,
    pub sse_delivery: LatencyReport,
    pub backup: BackupReport,
    pub backup_concurrent_writes: LatencyReport,
    pub garbage_collection: GcReport,
    pub gc_concurrent_reads: LatencyReport,
    pub checks: Vec<CheckReport>,
    pub status_scenarios: Vec<StatusScenario>,
    pub rss_bytes_at_start: Option<u64>,
    pub rss_bytes_at_end: Option<u64>,
    pub peak_rss_bytes: Option<u64>,
}

#[derive(Clone)]
struct WorkloadClient {
    router: Router,
    signing_key: Arc<SigningKey>,
    account_locator: String,
    nonce: Arc<AtomicU64>,
    request_timeout: Duration,
}

struct AccountState {
    client: WorkloadClient,
    object_ids: Vec<String>,
    root_revision: u64,
    root_hash: Option<String>,
}

struct OpenSse {
    account_index: usize,
    body: Body,
}

struct RawResponse {
    status: StatusCode,
    body: Vec<u8>,
    elapsed_us: u64,
}

pub async fn run_workload(
    profile: WorkloadProfile,
    policy: AcsRuntimePolicy,
) -> anyhow::Result<WorkloadReport> {
    let dimensions = dimensions(profile);
    let policy = workload_policy(profile, policy)?;
    let temp = TempDir::new().context("create ACS workload storage")?;
    let storage_root = temp.path().join("cloud-storage");
    let config = policy.store_config(storage_root);
    let store = CloudStore::open(config.clone())?;
    let base_router = server_router(store.clone(), SERVER_SECRET, policy.clone());
    let started_at_epoch_ms = now_epoch_ms();
    let started = Instant::now();
    let rss_bytes_at_start = process_memory_bytes("VmRSS:");
    let mut accounts = Vec::new();
    let mut stages = Vec::new();

    for stage in 0..dimensions.load_stages {
        stages.push(run_load_stage(stage, &base_router, &mut accounts, dimensions).await?);
    }

    let mut checks = Vec::new();
    exercise_quota_limit(&accounts[0], &policy).await?;
    checks.push(CheckReport {
        name: "account_quota".to_string(),
        detail: "an oversized create-once object was rejected with quota_exceeded".to_string(),
    });

    let (mut streams, sse_open, sse_delivery, sse_status) =
        exercise_sse_limits(&mut accounts, &store, dimensions.sse_connections).await?;
    checks.push(CheckReport {
        name: "sse_limit".to_string(),
        detail: format!(
            "held {} live streams and rejected the next stream at the global gate",
            dimensions.sse_connections
        ),
    });
    let mut sse_expectations = vec![StatusExpectation {
        metric_id: "aerobag_cloud.current_sse_connections".to_string(),
        severity: "critical".to_string(),
    }];
    if profile == WorkloadProfile::Ci {
        sse_expectations.push(StatusExpectation {
            metric_id: "aerobag_cloud.stored_bytes".to_string(),
            severity: "warning".to_string(),
        });
    }
    let mut status_scenarios = vec![StatusScenario {
        name: "sse_capacity".to_string(),
        status: sse_status,
        expected_pipeline_health: sse_expectations,
    }];
    streams.clear();
    tokio::task::yield_now().await;
    ensure!(
        metric_current(&store.status(now_epoch_ms())?, "current_sse_connections") == 0,
        "SSE guards did not release all connections"
    );

    let (backup, backup_concurrent_writes) = exercise_online_backup(&accounts, &config).await?;
    verify_backup(&backup.snapshot_path)?;
    checks.push(CheckReport {
        name: "online_backup".to_string(),
        detail: "backup verified while HTTP object writes continued".to_string(),
    });

    let (garbage_collection, gc_concurrent_reads) = exercise_gc(&accounts, &store).await?;
    ensure!(
        garbage_collection.deleted_objects > 0,
        "GC did not remove orphan objects"
    );
    checks.push(CheckReport {
        name: "garbage_collection".to_string(),
        detail: format!(
            "removed {} orphan objects while reachable-object reads completed",
            garbage_collection.deleted_objects
        ),
    });
    status_scenarios.push(StatusScenario {
        name: "maintenance_cost".to_string(),
        status: store.status(now_epoch_ms())?,
        expected_pipeline_health: [
            "gc_database_pause_ms",
            "gc_elapsed_ms",
            "backup_elapsed_ms",
            "backup_sqlite_snapshot_ms",
            "backup_wal_growth_bytes",
        ]
        .into_iter()
        .map(|metric| StatusExpectation {
            metric_id: format!("aerobag_cloud.{metric}"),
            severity: "ok".to_string(),
        })
        .collect(),
    });

    let read_only_status = exercise_read_only(&accounts[0], &store).await?;
    status_scenarios.push(StatusScenario {
        name: "operator_read_only".to_string(),
        status: read_only_status,
        expected_pipeline_health: vec![StatusExpectation {
            metric_id: "aerobag_cloud.mode".to_string(),
            severity: "warning".to_string(),
        }],
    });
    checks.push(CheckReport {
        name: "read_only_recovery".to_string(),
        detail:
            "writes were rejected, reads remained available, and checked resume restored writes"
                .to_string(),
    });

    exercise_operator_boundary(&base_router, dimensions.request_timeout_ms).await?;
    checks.push(CheckReport {
        name: "operator_boundary".to_string(),
        detail: "status was hidden remotely, authenticated locally, and rejected locally without a credential"
            .to_string(),
    });

    let disk_status = disk_pressure_scenario(&policy)?;
    status_scenarios.push(StatusScenario {
        name: "filesystem_pressure".to_string(),
        status: disk_status,
        expected_pipeline_health: vec![StatusExpectation {
            metric_id: "aerobag_cloud.filesystem_free_bytes".to_string(),
            severity: "critical".to_string(),
        }],
    });
    checks.push(CheckReport {
        name: "filesystem_pressure".to_string(),
        detail: "checked resume refused simulated insufficient free space; forced recovery remained explicit"
            .to_string(),
    });

    let global_storage_status =
        global_storage_scenario(&policy, dimensions.request_timeout_ms).await?;
    status_scenarios.push(StatusScenario {
        name: "global_storage_pressure".to_string(),
        status: global_storage_status,
        expected_pipeline_health: vec![StatusExpectation {
            metric_id: "aerobag_cloud.mode".to_string(),
            severity: "warning".to_string(),
        }],
    });
    checks.push(CheckReport {
        name: "global_storage_pressure".to_string(),
        detail: "crossing the global storage ceiling atomically moved ACS into read-only mode"
            .to_string(),
    });

    exercise_egress_limit(&policy, dimensions.request_timeout_ms).await?;
    checks.push(CheckReport {
        name: "egress_limit".to_string(),
        detail:
            "a read larger than the account egress bucket was rejected at the typed egress gate"
                .to_string(),
    });

    let falloff = stage_falloff(&stages);
    let report = WorkloadReport {
        schema_version: 1,
        profile,
        started_at_epoch_ms,
        completed_at_epoch_ms: now_epoch_ms(),
        total_elapsed_ms: elapsed_ms(started),
        dimensions,
        stages,
        falloff,
        sse_open,
        sse_delivery,
        backup,
        backup_concurrent_writes,
        garbage_collection,
        gc_concurrent_reads,
        checks,
        status_scenarios,
        rss_bytes_at_start,
        rss_bytes_at_end: process_memory_bytes("VmRSS:"),
        peak_rss_bytes: process_memory_bytes("VmHWM:"),
    };
    enforce_profile_bounds(&report)?;
    Ok(report)
}

fn dimensions(profile: WorkloadProfile) -> WorkloadDimensions {
    match profile {
        WorkloadProfile::Ci => WorkloadDimensions {
            load_stages: 2,
            accounts_per_stage: 2,
            objects_per_account: 12,
            inline_object_bytes: 1_024,
            blob_objects_per_account: 1,
            blob_object_bytes: 24 * 1_024,
            read_rounds: 1,
            max_concurrency: 8,
            sse_connections: 8,
            request_timeout_ms: 5_000,
        },
        WorkloadProfile::Production => WorkloadDimensions {
            load_stages: 4,
            accounts_per_stage: 8,
            objects_per_account: 48,
            inline_object_bytes: 6 * 1_024,
            blob_objects_per_account: 3,
            blob_object_bytes: 160 * 1_024,
            read_rounds: 2,
            max_concurrency: 32,
            sse_connections: 128,
            request_timeout_ms: 30_000,
        },
    }
}

fn workload_policy(
    profile: WorkloadProfile,
    mut policy: AcsRuntimePolicy,
) -> anyhow::Result<AcsRuntimePolicy> {
    if profile == WorkloadProfile::Ci {
        policy.request.max_body_bytes = 512 * 1_024;
        policy.request.max_concurrent_requests = 16;
        policy.storage.anonymous_account_quota_bytes = 128 * 1_024;
        policy.storage.anonymous_account_object_limit = 64;
        policy.storage.global_storage_limit_bytes = 1024 * 1_024;
        policy.storage.inline_ciphertext_threshold_bytes = 8 * 1_024;
        policy.storage.retained_sse_events = 32;
        policy.storage.write_resume_headroom_bytes = 64 * 1_024;
        policy.storage.write_resume_min_filesystem_free_bytes = 1;
        policy.rate_limits.account_creation_per_network.capacity = 2;
        policy
            .rate_limits
            .account_creation_per_network
            .refill_amount = 2;
        policy.rate_limits.account_creation_global.capacity = 16;
        policy.rate_limits.account_creation_global.refill_amount = 16;
        policy.rate_limits.operations_per_network.capacity = 10_000;
        policy.rate_limits.operations_per_network.refill_amount = 10_000;
        policy.rate_limits.operations_per_account.capacity = 10_000;
        policy.rate_limits.operations_per_account.refill_amount = 10_000;
        policy.rate_limits.egress_bytes_per_account.capacity = 8 * 1024 * 1024;
        policy.rate_limits.egress_bytes_per_account.refill_amount = 8 * 1024 * 1024;
        policy.sse.max_connections_global = 8;
        policy.sse.max_connections_per_account = 3;
        policy.sse.max_connections_per_network = 4;
        policy.monitoring.stored_bytes_warning = 64 * 1_024;
        policy.monitoring.stored_bytes_critical = 768 * 1_024;
        policy.monitoring.sse_connections_warning = 4;
        policy.monitoring.sse_connections_critical = 7;
        policy.backup.interval_seconds = 60;
        policy.monitoring.backup_age_seconds_warning = 120;
        policy.monitoring.backup_age_seconds_critical = 600;
    }
    policy.validate()?;
    Ok(policy)
}

async fn run_load_stage(
    stage: u32,
    base_router: &Router,
    accounts: &mut Vec<AccountState>,
    dimensions: WorkloadDimensions,
) -> anyhow::Result<StageReport> {
    let account_start = Instant::now();
    let first_index = accounts.len();
    let mut create_tasks = JoinSet::new();
    for offset in 0..dimensions.accounts_per_stage {
        let client = WorkloadClient::new(
            base_router,
            first_index + offset as usize,
            dimensions.request_timeout_ms,
        );
        create_tasks.spawn(async move {
            let latency = client.create_account().await?;
            Ok::<_, anyhow::Error>((client, latency))
        });
    }
    let mut clients = Vec::new();
    let mut create_latencies = Vec::new();
    while let Some(result) = create_tasks.join_next().await {
        let (client, latency) = result.context("account creation task panicked")??;
        clients.push(client);
        create_latencies.push(latency);
    }
    clients.sort_by(|left, right| left.account_locator.cmp(&right.account_locator));
    let create_accounts = LatencyReport::new(create_latencies, account_start.elapsed());

    let object_start = Instant::now();
    let semaphore = Arc::new(Semaphore::new(dimensions.max_concurrency as usize));
    let mut object_tasks = JoinSet::new();
    for (client_offset, client) in clients.iter().cloned().enumerate() {
        let account_index = first_index + client_offset;
        for object_index in 0..dimensions.objects_per_account {
            let client = client.clone();
            let permit = semaphore.clone();
            let size = if object_index < dimensions.blob_objects_per_account {
                dimensions.blob_object_bytes
            } else {
                dimensions.inline_object_bytes
            };
            object_tasks.spawn(async move {
                let _permit = permit.acquire_owned().await?;
                let object_id = format!("object-{object_index:04}");
                let byte = ((account_index as u64 * 37 + object_index as u64 * 17) % 251) as u8;
                let latency = client
                    .put_object(&object_id, vec![byte; size as usize])
                    .await?;
                Ok::<_, anyhow::Error>(latency)
            });
        }
    }
    let mut object_latencies = Vec::new();
    while let Some(result) = object_tasks.join_next().await {
        object_latencies.push(result.context("object creation task panicked")??);
    }
    let create_objects = LatencyReport::new(object_latencies, object_start.elapsed());

    let root_start = Instant::now();
    let mut root_tasks = JoinSet::new();
    for client in clients {
        let object_ids = (0..dimensions.objects_per_account)
            .map(|index| format!("object-{index:04}"))
            .collect::<Vec<_>>();
        let children = object_ids
            .iter()
            .enumerate()
            .filter(|(index, _)| index % 2 == 0)
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>();
        root_tasks.spawn(async move {
            let (root, latency) = client.commit_root(0, None, &children, b"root-v1").await?;
            Ok::<_, anyhow::Error>((client, object_ids, root, latency))
        });
    }
    let mut root_latencies = Vec::new();
    let mut new_accounts = Vec::new();
    while let Some(result) = root_tasks.join_next().await {
        let (client, object_ids, root, latency) = result.context("root task panicked")??;
        root_latencies.push(latency);
        new_accounts.push(AccountState {
            client,
            object_ids,
            root_revision: root.revision,
            root_hash: Some(root.root_hash),
        });
    }
    new_accounts.sort_by(|left, right| {
        left.client
            .account_locator
            .cmp(&right.client.account_locator)
    });
    let commit_roots = LatencyReport::new(root_latencies, root_start.elapsed());

    let read_start = Instant::now();
    let mut read_tasks = JoinSet::new();
    for account in &new_accounts {
        for _ in 0..dimensions.read_rounds {
            for object_id in &account.object_ids {
                let client = account.client.clone();
                let object_id = object_id.clone();
                let permit = semaphore.clone();
                read_tasks.spawn(async move {
                    let _permit = permit.acquire_owned().await?;
                    let (_, latency) = client.get_object(&object_id).await?;
                    Ok::<_, anyhow::Error>(latency)
                });
            }
        }
    }
    let mut read_latencies = Vec::new();
    while let Some(result) = read_tasks.join_next().await {
        read_latencies.push(result.context("object read task panicked")??);
    }
    let read_objects = LatencyReport::new(read_latencies, read_start.elapsed());

    let list_start = Instant::now();
    let mut list_tasks = JoinSet::new();
    for account in &new_accounts {
        let client = account.client.clone();
        list_tasks.spawn(async move {
            let (list, latency) = client.list_objects().await?;
            Ok::<_, anyhow::Error>((list, latency))
        });
    }
    let mut list_latencies = Vec::new();
    while let Some(result) = list_tasks.join_next().await {
        let (list, latency) = result.context("object list task panicked")??;
        ensure!(
            list.total_object_count == dimensions.objects_per_account as u64,
            "object listing returned {} rows, expected {}",
            list.total_object_count,
            dimensions.objects_per_account
        );
        list_latencies.push(latency);
    }
    let list_objects = LatencyReport::new(list_latencies, list_start.elapsed());

    accounts.extend(new_accounts);
    Ok(StageReport {
        stage,
        accounts_after_stage: accounts.len() as u32,
        objects_after_stage: accounts.len() as u64 * dimensions.objects_per_account as u64,
        create_accounts,
        create_objects,
        commit_roots,
        read_objects,
        list_objects,
    })
}

async fn exercise_quota_limit(
    account: &AccountState,
    policy: &AcsRuntimePolicy,
) -> anyhow::Result<()> {
    let error = account
        .client
        .put_object_error(
            "quota-probe",
            vec![0xa5; policy.storage.anonymous_account_quota_bytes as usize],
        )
        .await?;
    ensure!(
        error.code == AcsErrorCode::QuotaExceeded,
        "wrong quota error: {error:?}"
    );
    Ok(())
}

async fn exercise_sse_limits(
    accounts: &mut [AccountState],
    store: &CloudStore,
    connection_count: u32,
) -> anyhow::Result<(
    Vec<OpenSse>,
    LatencyReport,
    LatencyReport,
    AcsStatusResponse,
)> {
    let open_started = Instant::now();
    let mut streams = Vec::new();
    let mut open_latencies = Vec::new();
    for index in 0..connection_count as usize {
        let account_index = index % accounts.len();
        let (stream, latency) = accounts[account_index]
            .client
            .open_sse(account_index)
            .await?;
        streams.push(stream);
        open_latencies.push(latency);
    }
    let sse_open = LatencyReport::new(open_latencies, open_started.elapsed());
    let error = accounts[0].client.open_sse_error().await?;
    ensure!(
        error.code == AcsErrorCode::RateLimited
            && error.rate_limit_gate == Some(AcsRateLimitGate::GlobalSseConnections),
        "wrong SSE saturation error: {error:?}"
    );

    let delivery_started = Instant::now();
    let mut delivery_latencies = Vec::new();
    for (account_index, account) in accounts.iter_mut().enumerate() {
        let children = account
            .object_ids
            .iter()
            .enumerate()
            .filter(|(index, _)| index % 2 == 0)
            .map(|(_, id)| id.clone())
            .collect::<Vec<_>>();
        let committed_at = Instant::now();
        let (root, _) = account
            .client
            .commit_root(
                account.root_revision,
                account.root_hash.as_deref(),
                &children,
                b"root-v2",
            )
            .await?;
        account.root_revision = root.revision;
        account.root_hash = Some(root.root_hash);
        for stream in streams
            .iter_mut()
            .filter(|stream| stream.account_index == account_index)
        {
            let event = stream.next_event(Duration::from_secs(2)).await?;
            ensure!(
                matches!(
                    event,
                    AcsSseEvent::RootChanged { root_revision, .. } if root_revision == account.root_revision
                ),
                "SSE did not deliver the committed root: {event:?}"
            );
            delivery_latencies.push(committed_at.elapsed().as_micros() as u64);
        }
    }
    let sse_delivery = LatencyReport::new(delivery_latencies, delivery_started.elapsed());
    let status = store.status(now_epoch_ms())?;
    ensure!(
        metric_current(&status, "current_sse_connections") == connection_count as u64,
        "status did not report all held SSE connections"
    );
    Ok((streams, sse_open, sse_delivery, status))
}

async fn exercise_online_backup(
    accounts: &[AccountState],
    config: &StoreConfig,
) -> anyhow::Result<(BackupReport, LatencyReport)> {
    let backup_config = config.clone();
    let backup_time = now_epoch_ms();
    let backup_task =
        tokio::task::spawn_blocking(move || create_backup(&backup_config, backup_time));
    let write_started = Instant::now();
    let mut writes = JoinSet::new();
    for (index, account) in accounts.iter().enumerate() {
        let client = account.client.clone();
        writes.spawn(async move {
            client
                .put_object(&format!("backup-race-{index:04}"), vec![index as u8; 256])
                .await
        });
    }
    let mut latencies = Vec::new();
    while let Some(result) = writes.join_next().await {
        latencies.push(result.context("backup race write task panicked")??);
    }
    let backup = backup_task.await.context("backup task panicked")??;
    Ok((
        backup,
        LatencyReport::new(latencies, write_started.elapsed()),
    ))
}

async fn exercise_gc(
    accounts: &[AccountState],
    store: &CloudStore,
) -> anyhow::Result<(GcReport, LatencyReport)> {
    let gc_store = store.clone();
    let gc_task = tokio::task::spawn_blocking(move || gc_store.run_gc(now_epoch_ms() + 10_000, 0));
    let reads_started = Instant::now();
    let mut reads = JoinSet::new();
    for account in accounts {
        let client = account.client.clone();
        let kept = account.object_ids[0].clone();
        reads.spawn(async move {
            let (_, latency) = client.get_object(&kept).await?;
            Ok::<_, anyhow::Error>(latency)
        });
    }
    let mut latencies = Vec::new();
    while let Some(result) = reads.join_next().await {
        latencies.push(result.context("GC race read task panicked")??);
    }
    let report = gc_task.await.context("GC task panicked")??;
    let deleted = accounts[0]
        .client
        .get_object_error(&accounts[0].object_ids[1])
        .await?;
    ensure!(
        deleted.code == AcsErrorCode::NotFound,
        "GC left an orphan reachable"
    );
    Ok((
        report,
        LatencyReport::new(latencies, reads_started.elapsed()),
    ))
}

async fn exercise_read_only(
    account: &AccountState,
    store: &CloudStore,
) -> anyhow::Result<AcsStatusResponse> {
    store.set_service_mode(AccountMode::ReadOnly, now_epoch_ms())?;
    let error = account
        .client
        .put_object_error("read-only-probe", vec![1; 16])
        .await?;
    ensure!(
        error.code == AcsErrorCode::ReadOnly,
        "write bypassed read-only mode"
    );
    account.client.get_object(&account.object_ids[0]).await?;
    let status = store.status(now_epoch_ms())?;
    let resumed = store.resume_writes(now_epoch_ms())?;
    ensure!(
        resumed.resumed && !resumed.forced,
        "checked resume did not restore writes"
    );
    account
        .client
        .put_object("read-only-recovered", vec![2; 16])
        .await?;
    Ok(status)
}

async fn exercise_operator_boundary(
    base_router: &Router,
    request_timeout_ms: u64,
) -> anyhow::Result<()> {
    let remote = router_for_peer(base_router, "192.0.2.240:12000")?;
    let local = router_for_peer(base_router, "127.0.0.1:12001")?;
    let remote_response = dispatch(
        &remote,
        Request::get(ACS_STATUS_PATH).body(Body::empty())?,
        request_timeout_ms,
    )
    .await?;
    ensure!(
        remote_response.status == StatusCode::NOT_FOUND,
        "remote status was exposed"
    );
    let local_response = dispatch(
        &local,
        Request::get(ACS_STATUS_PATH).body(Body::empty())?,
        request_timeout_ms,
    )
    .await?;
    ensure!(
        local_response.status == StatusCode::UNAUTHORIZED,
        "local status did not require authentication"
    );
    let mut request = Request::get(ACS_STATUS_PATH).body(Body::empty())?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, operator_authorization()?);
    let authorized = dispatch(&local, request, request_timeout_ms).await?;
    ensure!(
        authorized.status == StatusCode::OK,
        "operator status authorization failed"
    );
    Ok(())
}

fn disk_pressure_scenario(policy: &AcsRuntimePolicy) -> anyhow::Result<AcsStatusResponse> {
    let temp = TempDir::new()?;
    let mut config = policy.store_config(temp.path().join("cloud-storage"));
    fs::create_dir_all(&config.storage_root)?;
    let free = fs2::available_space(&config.storage_root)?;
    config.filesystem_free_bytes_warning = free.saturating_add(2);
    config.filesystem_free_bytes_critical = free.saturating_add(1);
    config.write_resume_min_filesystem_free_bytes = free.saturating_add(1);
    let store = CloudStore::open(config)?;
    store.set_service_mode(AccountMode::ReadOnly, now_epoch_ms())?;
    ensure!(
        store.resume_writes(now_epoch_ms()).is_err(),
        "disk pressure did not block resume"
    );
    let status = store.status(now_epoch_ms())?;
    let forced = store.force_resume_writes("workload disk-pressure recovery", now_epoch_ms())?;
    ensure!(
        forced.forced,
        "forced disk-pressure recovery was not audited as forced"
    );
    Ok(status)
}

async fn global_storage_scenario(
    policy: &AcsRuntimePolicy,
    request_timeout_ms: u64,
) -> anyhow::Result<AcsStatusResponse> {
    let temp = TempDir::new()?;
    let mut scenario_policy = policy.clone();
    scenario_policy.storage.anonymous_account_quota_bytes = 64 * 1_024;
    scenario_policy.storage.global_storage_limit_bytes = 64 * 1_024;
    scenario_policy.storage.inline_ciphertext_threshold_bytes = 8 * 1_024;
    scenario_policy.storage.write_resume_headroom_bytes = 1_024;
    scenario_policy
        .storage
        .write_resume_min_filesystem_free_bytes = 1;
    scenario_policy.monitoring.stored_bytes_warning = 32 * 1_024;
    scenario_policy.monitoring.stored_bytes_critical = 48 * 1_024;
    scenario_policy.validate()?;
    let store = CloudStore::open(scenario_policy.store_config(temp.path().join("cloud-storage")))?;
    let router = server_router(store.clone(), SERVER_SECRET, scenario_policy);
    let first = WorkloadClient::new(&router, 220, request_timeout_ms);
    let second = WorkloadClient::new(&router, 222, request_timeout_ms);
    first.create_account().await?;
    second.create_account().await?;
    first
        .put_object("fills-storage", vec![3; 40 * 1_024])
        .await?;
    let error = second
        .put_object_error("crosses-storage", vec![4; 40 * 1_024])
        .await?;
    ensure!(
        error.code == AcsErrorCode::ReadOnly,
        "global storage did not enter read-only"
    );
    let status = store.status(now_epoch_ms())?;
    store.force_resume_writes("workload global-storage recovery", now_epoch_ms())?;
    Ok(status)
}

async fn exercise_egress_limit(
    policy: &AcsRuntimePolicy,
    request_timeout_ms: u64,
) -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let mut scenario_policy = policy.clone();
    scenario_policy
        .rate_limits
        .egress_bytes_per_account
        .capacity = 1_024;
    scenario_policy
        .rate_limits
        .egress_bytes_per_account
        .refill_amount = 1_024;
    scenario_policy.validate()?;
    let store = CloudStore::open(scenario_policy.store_config(temp.path().join("cloud-storage")))?;
    let router = server_router(store, SERVER_SECRET, scenario_policy);
    let client = WorkloadClient::new(&router, 221, request_timeout_ms);
    client.create_account().await?;
    client.put_object("egress-probe", vec![5; 2_048]).await?;
    let error = client.get_object_error("egress-probe").await?;
    ensure!(
        error.code == AcsErrorCode::RateLimited
            && error.rate_limit_gate == Some(AcsRateLimitGate::AccountEgress),
        "wrong egress error: {error:?}"
    );
    Ok(())
}

impl WorkloadClient {
    fn new(base_router: &Router, index: usize, request_timeout_ms: u64) -> Self {
        let seed: [u8; 32] =
            Sha256::digest(format!("aerobag-workload-signing-key-{index}").as_bytes()).into();
        let signing_key = SigningKey::from_bytes(&seed);
        let account = Sha256::digest(format!("aerobag-workload-account-{index}").as_bytes());
        let third = 2 + ((index / 250) % 250) as u8;
        let fourth = 1 + (index % 250) as u8;
        let peer = format!("192.0.{third}.{fourth}:{}", 10_000 + index);
        Self {
            router: router_for_peer(base_router, &peer).expect("workload peer is valid"),
            signing_key: Arc::new(signing_key),
            account_locator: URL_SAFE_NO_PAD.encode(&account[..ACS_ACCOUNT_LOCATOR_BYTES]),
            nonce: Arc::new(AtomicU64::new(1)),
            request_timeout: Duration::from_millis(request_timeout_ms),
        }
    }

    async fn create_account(&self) -> anyhow::Result<u64> {
        let challenge = dispatch(
            &self.router,
            Request::post("/cloud/v1/account-challenges").body(Body::empty())?,
            self.request_timeout.as_millis() as u64,
        )
        .await?;
        ensure_status(&challenge, StatusCode::OK)?;
        let challenge_elapsed_us = challenge.elapsed_us;
        let challenge: AcsCreationChallengeResponse = serde_json::from_slice(&challenge.body)?;
        let key_id = URL_SAFE_NO_PAD.encode(
            &Sha256::digest(self.signing_key.verifying_key().as_bytes())
                [..ACS_SIGNING_KEY_ID_BYTES],
        );
        let request = AcsCreateAccountRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            account_locator: self.account_locator.clone(),
            signing_key_id: key_id,
            signing_public_key_base64url: URL_SAFE_NO_PAD
                .encode(self.signing_key.verifying_key().as_bytes()),
            creation_challenge: challenge.challenge,
        };
        let response = self
            .signed_json(AcsHttpMethod::Post, "/cloud/v1/accounts", &request)
            .await?;
        ensure_status(&response, StatusCode::CREATED)?;
        Ok(challenge_elapsed_us.saturating_add(response.elapsed_us))
    }

    async fn put_object(&self, object_id: &str, ciphertext: Vec<u8>) -> anyhow::Result<u64> {
        let target = format!(
            "/cloud/v1/accounts/{}/objects/{object_id}",
            self.account_locator
        );
        let request = AcsCreateObjectRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            object_id: object_id.to_string(),
            value: AcsEncryptedValue::from_ciphertext(&ciphertext, vec![]),
        };
        let response = self
            .signed_json(AcsHttpMethod::Put, &target, &request)
            .await?;
        ensure_status(&response, StatusCode::OK)?;
        Ok(response.elapsed_us)
    }

    async fn put_object_error(
        &self,
        object_id: &str,
        ciphertext: Vec<u8>,
    ) -> anyhow::Result<AcsErrorResponse> {
        let target = format!(
            "/cloud/v1/accounts/{}/objects/{object_id}",
            self.account_locator
        );
        let request = AcsCreateObjectRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            object_id: object_id.to_string(),
            value: AcsEncryptedValue::from_ciphertext(&ciphertext, vec![]),
        };
        error_response(
            self.signed_json(AcsHttpMethod::Put, &target, &request)
                .await?,
        )
    }

    async fn get_object(&self, object_id: &str) -> anyhow::Result<(AcsObjectSnapshot, u64)> {
        let target = format!(
            "/cloud/v1/accounts/{}/objects/{object_id}",
            self.account_locator
        );
        let response = self.signed_empty(AcsHttpMethod::Get, &target).await?;
        ensure_status(&response, StatusCode::OK)?;
        Ok((serde_json::from_slice(&response.body)?, response.elapsed_us))
    }

    async fn get_object_error(&self, object_id: &str) -> anyhow::Result<AcsErrorResponse> {
        let target = format!(
            "/cloud/v1/accounts/{}/objects/{object_id}",
            self.account_locator
        );
        error_response(self.signed_empty(AcsHttpMethod::Get, &target).await?)
    }

    async fn list_objects(&self) -> anyhow::Result<(AcsListObjectsResponse, u64)> {
        let target = format!(
            "/cloud/v1/accounts/{}/objects?limit=100",
            self.account_locator
        );
        let response = self.signed_empty(AcsHttpMethod::Get, &target).await?;
        ensure_status(&response, StatusCode::OK)?;
        Ok((serde_json::from_slice(&response.body)?, response.elapsed_us))
    }

    async fn commit_root(
        &self,
        expected_revision: u64,
        expected_root_hash: Option<&str>,
        children: &[String],
        ciphertext: &[u8],
    ) -> anyhow::Result<(AcsRootSnapshot, u64)> {
        let target = format!("/cloud/v1/accounts/{}/root", self.account_locator);
        let request = AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision,
            expected_root_hash: expected_root_hash.map(str::to_string),
            replacement: AcsEncryptedValue::from_ciphertext(ciphertext, children.to_vec()),
        };
        let response = self
            .signed_json(AcsHttpMethod::Put, &target, &request)
            .await?;
        ensure_status(&response, StatusCode::OK)?;
        let parsed: AcsCompareAndSwapRootResponse = serde_json::from_slice(&response.body)?;
        match parsed {
            AcsCompareAndSwapRootResponse::Committed { root } => Ok((root, response.elapsed_us)),
            AcsCompareAndSwapRootResponse::Conflict { .. } => {
                bail!("workload root CAS unexpectedly conflicted")
            }
        }
    }

    async fn open_sse(&self, account_index: usize) -> anyhow::Result<(OpenSse, u64)> {
        let (mut response, elapsed_us) = self.open_sse_response().await?;
        ensure!(
            response.status() == StatusCode::OK,
            "SSE open failed: {}",
            response.status()
        );
        let started = Instant::now();
        let event = next_sse_event(response.body_mut(), self.request_timeout).await?;
        ensure!(
            matches!(event, AcsSseEvent::Ready { .. }),
            "SSE did not start ready: {event:?}"
        );
        Ok((
            OpenSse {
                account_index,
                body: response.into_body(),
            },
            elapsed_us.saturating_add(started.elapsed().as_micros() as u64),
        ))
    }

    async fn open_sse_error(&self) -> anyhow::Result<AcsErrorResponse> {
        let (response, _) = self.open_sse_response().await?;
        let status = response.status();
        let bytes = response.into_body().collect().await?.to_bytes().to_vec();
        error_response(RawResponse {
            status,
            body: bytes,
            elapsed_us: 0,
        })
    }

    async fn open_sse_response(&self) -> anyhow::Result<(axum::response::Response, u64)> {
        let ticket_target = format!("/cloud/v1/accounts/{}/event-tickets", self.account_locator);
        let ticket_request = AcsCreateSseTicketRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            last_event_sequence: None,
        };
        let ticket_response = self
            .signed_json(AcsHttpMethod::Post, &ticket_target, &ticket_request)
            .await?;
        ensure_status(&ticket_response, StatusCode::OK)?;
        let ticket: AcsCreateSseTicketResponse = serde_json::from_slice(&ticket_response.body)?;
        let request = Request::get(ticket.events_url).body(Body::empty())?;
        let started = Instant::now();
        let response = timeout(self.request_timeout, self.router.clone().oneshot(request))
            .await
            .context("SSE open timed out")??;
        Ok((
            response,
            ticket_response
                .elapsed_us
                .saturating_add(started.elapsed().as_micros() as u64),
        ))
    }

    async fn signed_json<T: Serialize>(
        &self,
        method: AcsHttpMethod,
        target: &str,
        value: &T,
    ) -> anyhow::Result<RawResponse> {
        let body = serde_json::to_vec(value)?;
        self.signed(method, target, body).await
    }

    async fn signed_empty(
        &self,
        method: AcsHttpMethod,
        target: &str,
    ) -> anyhow::Result<RawResponse> {
        self.signed(method, target, Vec::new()).await
    }

    async fn signed(
        &self,
        method: AcsHttpMethod,
        target: &str,
        body: Vec<u8>,
    ) -> anyhow::Result<RawResponse> {
        let (path, query) = target
            .split_once('?')
            .map_or((target, None), |(path, query)| (path, Some(query)));
        let canonical_target =
            acs_canonical_request_target(path, query).map_err(|error| anyhow::anyhow!(error))?;
        let counter = self.nonce.fetch_add(1, Ordering::Relaxed);
        let mut nonce = [0_u8; ACS_REQUEST_NONCE_BYTES];
        nonce[8..].copy_from_slice(&counter.to_be_bytes());
        let key_id = URL_SAFE_NO_PAD.encode(
            &Sha256::digest(self.signing_key.verifying_key().as_bytes())
                [..ACS_SIGNING_KEY_ID_BYTES],
        );
        let mut authentication = AcsRequestAuthentication {
            contract_id: ACS_CONTRACT_ID.to_string(),
            account_locator: self.account_locator.clone(),
            signing_key_id: key_id,
            signature_algorithm: AcsSignatureAlgorithm::Ed25519,
            timestamp_epoch_ms: now_epoch_ms(),
            nonce_base64url: URL_SAFE_NO_PAD.encode(nonce),
            body_sha256: hex_bytes(&Sha256::digest(&body)),
            signature_base64url: String::new(),
        };
        authentication.signature_base64url = URL_SAFE_NO_PAD.encode(
            self.signing_key
                .sign(
                    &authentication
                        .signing_bytes(method, &canonical_target)
                        .map_err(|error| anyhow::anyhow!(error))?,
                )
                .to_bytes(),
        );
        let mut request = Request::builder()
            .method(http_method(method))
            .uri(target)
            .body(Body::from(body))?;
        *request.headers_mut() = authentication_headers(&authentication)?;
        dispatch(
            &self.router,
            request,
            self.request_timeout.as_millis() as u64,
        )
        .await
    }
}

impl OpenSse {
    async fn next_event(&mut self, wait: Duration) -> anyhow::Result<AcsSseEvent> {
        next_sse_event(&mut self.body, wait).await
    }
}

impl LatencyReport {
    fn new(mut samples_us: Vec<u64>, elapsed: Duration) -> Self {
        samples_us.sort_unstable();
        let count = samples_us.len() as u64;
        let elapsed_seconds = elapsed.as_secs_f64();
        Self {
            count,
            elapsed_ms: elapsed.as_secs_f64() * 1_000.0,
            throughput_per_second: if elapsed_seconds > 0.0 {
                count as f64 / elapsed_seconds
            } else {
                0.0
            },
            p50_ms: percentile_ms(&samples_us, 50),
            p95_ms: percentile_ms(&samples_us, 95),
            p99_ms: percentile_ms(&samples_us, 99),
            max_ms: samples_us.last().copied().unwrap_or(0) as f64 / 1_000.0,
        }
    }
}

async fn dispatch(
    router: &Router,
    request: Request<Body>,
    request_timeout_ms: u64,
) -> anyhow::Result<RawResponse> {
    let started = Instant::now();
    let response = timeout(
        Duration::from_millis(request_timeout_ms),
        router.clone().oneshot(request),
    )
    .await
    .context("ACS workload request timed out")??;
    let status = response.status();
    let body = response.into_body().collect().await?.to_bytes().to_vec();
    Ok(RawResponse {
        status,
        body,
        elapsed_us: started.elapsed().as_micros() as u64,
    })
}

async fn next_sse_event(body: &mut Body, wait: Duration) -> anyhow::Result<AcsSseEvent> {
    let frame = timeout(wait, body.frame())
        .await
        .context("timed out waiting for ACS SSE event")?
        .context("ACS SSE stream ended")??;
    let bytes = frame
        .into_data()
        .map_err(|_| anyhow::anyhow!("ACS SSE emitted a non-data frame"))?;
    let text = std::str::from_utf8(&bytes)?;
    let data = text
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .context("ACS SSE frame has no data field")?;
    Ok(serde_json::from_str(data)?)
}

fn authentication_headers(authentication: &AcsRequestAuthentication) -> anyhow::Result<HeaderMap> {
    let timestamp = authentication.timestamp_epoch_ms.to_string();
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
            HeaderName::from_bytes(name.as_bytes())?,
            header::HeaderValue::from_str(value)?,
        );
    }
    Ok(headers)
}

fn operator_authorization() -> anyhow::Result<header::HeaderValue> {
    let mut mac = Hmac::<Sha256>::new_from_slice(&SERVER_SECRET)?;
    mac.update(OPERATOR_STATUS_KDF_LABEL);
    let token = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Ok(header::HeaderValue::from_str(&format!("Bearer {token}"))?)
}

fn router_for_peer(router: &Router, peer: &str) -> anyhow::Result<Router> {
    Ok(router
        .clone()
        .layer(Extension(ConnectInfo(peer.parse::<SocketAddr>()?))))
}

fn ensure_status(response: &RawResponse, expected: StatusCode) -> anyhow::Result<()> {
    if response.status == expected {
        return Ok(());
    }
    bail!(
        "ACS returned {}, expected {}: {}",
        response.status,
        expected,
        String::from_utf8_lossy(&response.body)
    )
}

fn error_response(response: RawResponse) -> anyhow::Result<AcsErrorResponse> {
    ensure!(
        !response.status.is_success(),
        "ACS request unexpectedly succeeded with {}",
        response.status
    );
    Ok(serde_json::from_slice(&response.body)?)
}

fn http_method(method: AcsHttpMethod) -> Method {
    match method {
        AcsHttpMethod::Get => Method::GET,
        AcsHttpMethod::Post => Method::POST,
        AcsHttpMethod::Put => Method::PUT,
    }
}

fn percentile_ms(samples_us: &[u64], percentile: usize) -> f64 {
    if samples_us.is_empty() {
        return 0.0;
    }
    let index = (samples_us.len() * percentile)
        .div_ceil(100)
        .saturating_sub(1);
    samples_us[index] as f64 / 1_000.0
}

fn stage_falloff(stages: &[StageReport]) -> Vec<FalloffReport> {
    let Some(first) = stages.first() else {
        return Vec::new();
    };
    let last = stages.last().expect("first stage exists");
    [
        (
            "create_objects",
            &first.create_objects,
            &last.create_objects,
        ),
        ("read_objects", &first.read_objects, &last.read_objects),
        ("list_objects", &first.list_objects, &last.list_objects),
        ("commit_roots", &first.commit_roots, &last.commit_roots),
    ]
    .into_iter()
    .map(|(operation, first, last)| FalloffReport {
        operation: operation.to_string(),
        first_stage_p95_ms: first.p95_ms,
        last_stage_p95_ms: last.p95_ms,
        p95_ratio: if first.p95_ms > 0.0 {
            last.p95_ms / first.p95_ms
        } else {
            0.0
        },
    })
    .collect()
}

fn enforce_profile_bounds(report: &WorkloadReport) -> anyhow::Result<()> {
    if report.profile != WorkloadProfile::Ci {
        return Ok(());
    }
    ensure!(
        report.total_elapsed_ms < 60_000,
        "CI workload exceeded 60 seconds"
    );
    for latency in report
        .stages
        .iter()
        .flat_map(|stage| {
            [
                &stage.create_accounts,
                &stage.create_objects,
                &stage.commit_roots,
                &stage.read_objects,
                &stage.list_objects,
            ]
        })
        .chain([
            &report.sse_open,
            &report.sse_delivery,
            &report.backup_concurrent_writes,
            &report.gc_concurrent_reads,
        ])
    {
        ensure!(
            latency.p99_ms < 5_000.0,
            "CI operation p99 exceeded five seconds"
        );
    }
    for falloff in &report.falloff {
        ensure!(
            falloff.p95_ratio < 10.0 || falloff.last_stage_p95_ms < 250.0,
            "CI {} p95 degraded from {:.1} ms to {:.1} ms",
            falloff.operation,
            falloff.first_stage_p95_ms,
            falloff.last_stage_p95_ms
        );
    }
    ensure!(
        report.backup.elapsed_ms < 5_000,
        "CI backup exceeded five seconds"
    );
    ensure!(
        report.garbage_collection.total_elapsed_ms < 5_000,
        "CI garbage collection exceeded five seconds"
    );
    Ok(())
}

fn metric_current(status: &AcsStatusResponse, id: &str) -> u64 {
    status
        .metrics
        .iter()
        .find(|metric| metric.id == id)
        .map(|metric| metric.current)
        .unwrap_or(0)
}

fn process_memory_bytes(field: &str) -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with(field))?;
    let kib = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    Some(kib.saturating_mul(1_024))
}

fn now_epoch_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workload_profiles_are_explicit() {
        assert_eq!(
            "ci".parse::<WorkloadProfile>().unwrap(),
            WorkloadProfile::Ci
        );
        assert_eq!(
            "production".parse::<WorkloadProfile>().unwrap(),
            WorkloadProfile::Production
        );
        assert!("surprise".parse::<WorkloadProfile>().is_err());
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        assert_eq!(percentile_ms(&[1_000, 2_000, 3_000, 4_000], 50), 2.0);
        assert_eq!(percentile_ms(&[1_000, 2_000, 3_000, 4_000], 95), 4.0);
    }
}
