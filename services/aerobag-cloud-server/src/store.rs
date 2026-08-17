// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::Write as _,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicI64, AtomicU64, Ordering},
        Arc, Condvar, Mutex, MutexGuard,
    },
    thread,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use product_contracts::{
    acs_events_path, AcsCompareAndSwapRootRequest, AcsCompareAndSwapRootResponse,
    AcsCreateAccountRequest, AcsCreateAccountResponse, AcsCreateObjectOutcome,
    AcsCreateSseTicketResponse, AcsCreationChallengeResponse, AcsEncryptedValue,
    AcsEncryptedValueKind, AcsErrorCode, AcsHealthResponse, AcsListObjectsResponse,
    AcsObjectSnapshot, AcsObjectSummary, AcsRateLimitGate, AcsRootSnapshot, AcsServiceMode,
    AcsSseEvent, AcsStatusMetric, AcsStatusResponse, AcsStatusTopContributor, ACS_CONTRACT_ID,
    ACS_FIXED_ROOT_ID, ACS_SSE_TICKET_TTL_MS,
};
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

use crate::StorageLayout;

pub(crate) const SCHEMA_VERSION: u32 = 3;
const CHALLENGE_TTL_MS: i64 = 5 * 60 * 1_000;
const OUTSTANDING_CHALLENGES_PER_NETWORK: u64 = 8;
const DEFAULT_LIST_LIMIT: u32 = 100;
const MAX_LIST_LIMIT: u32 = 500;
const RATE_WINDOW_MS: i64 = 60_000;
const DAY_MS: u64 = 24 * 60 * 60 * 1_000;
const MAX_TRACKED_NETWORK_WINDOWS: usize = 4_096;
const READ_CONNECTION_POOL_SIZE: usize = 8;
const OBJECT_STORAGE_ACCOUNTING_OVERHEAD_BYTES: u64 = 512;
const ROOT_STORAGE_ACCOUNTING_OVERHEAD_BYTES: u64 = 512;

#[derive(Debug, Clone)]
pub struct StoreConfig {
    pub storage_root: PathBuf,
    pub anonymous_quota_bytes: u64,
    pub anonymous_object_limit: u64,
    pub global_storage_limit_bytes: u64,
    pub inline_threshold_bytes: u64,
    pub event_retention: u64,
    pub global_operation_bucket: TokenBucketConfig,
    pub network_operation_bucket: TokenBucketConfig,
    pub account_operation_bucket: TokenBucketConfig,
    pub global_ingress_bucket: TokenBucketConfig,
    pub network_ingress_bucket: TokenBucketConfig,
    pub account_ingress_bucket: TokenBucketConfig,
    pub global_egress_bucket: TokenBucketConfig,
    pub network_egress_bucket: TokenBucketConfig,
    pub account_egress_bucket: TokenBucketConfig,
    pub global_sse_limit: u64,
    pub account_sse_limit: u64,
    pub network_sse_limit: u64,
    pub creation_network_bucket: TokenBucketConfig,
    pub creation_global_bucket: TokenBucketConfig,
    pub stored_bytes_warning: u64,
    pub stored_bytes_critical: u64,
    pub filesystem_free_bytes_warning: u64,
    pub filesystem_free_bytes_critical: u64,
    pub sse_connections_warning: u64,
    pub sse_connections_critical: u64,
    pub gc_database_pause_ms_warning: u64,
    pub gc_database_pause_ms_critical: u64,
    pub gc_elapsed_ms_warning: u64,
    pub gc_elapsed_ms_critical: u64,
    pub write_resume_headroom_bytes: u64,
    pub write_resume_min_filesystem_free_bytes: u64,
    pub backup_interval_seconds: u64,
    pub backup_retained_snapshots: u64,
    pub backup_age_seconds_warning: u64,
    pub backup_age_seconds_critical: u64,
    pub backup_elapsed_ms_warning: u64,
    pub backup_elapsed_ms_critical: u64,
    pub backup_sqlite_snapshot_ms_warning: u64,
    pub backup_sqlite_snapshot_ms_critical: u64,
    pub backup_wal_growth_bytes_warning: u64,
    pub backup_wal_growth_bytes_critical: u64,
    pub backup_linked_blob_count_warning: u64,
    pub backup_linked_blob_count_critical: u64,
    pub backup_linked_blob_bytes_warning: u64,
    pub backup_linked_blob_bytes_critical: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenBucketConfig {
    pub capacity: u64,
    pub refill_amount: u64,
    pub refill_period_ms: u64,
}

impl TokenBucketConfig {
    pub const fn per_minute(capacity: u64, refill_amount: u64) -> Self {
        Self {
            capacity,
            refill_amount,
            refill_period_ms: RATE_WINDOW_MS as u64,
        }
    }

    pub const fn per_day(capacity: u64, refill_amount: u64) -> Self {
        Self {
            capacity,
            refill_amount,
            refill_period_ms: DAY_MS,
        }
    }

    fn validate(self, label: &str) -> StoreResult<()> {
        if self.capacity == 0 || self.refill_amount == 0 || self.refill_period_ms == 0 {
            return Err(StoreError::internal(format!(
                "{label} token bucket values must be positive"
            )));
        }
        let _ = self
            .capacity
            .checked_mul(self.refill_period_ms)
            .ok_or_else(|| StoreError::internal(format!("{label} token bucket is too large")))?;
        Ok(())
    }
}

impl StoreConfig {
    #[cfg(test)]
    pub(crate) fn for_test_data_root(storage_root: PathBuf) -> Self {
        crate::policy::checked_in_test_policy().store_config(storage_root)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountMode {
    Normal,
    ReadOnly,
    Suspended,
}

impl AccountMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::ReadOnly => "read_only",
            Self::Suspended => "suspended",
        }
    }

    fn parse(value: &str) -> StoreResult<Self> {
        match value {
            "normal" => Ok(Self::Normal),
            "read_only" => Ok(Self::ReadOnly),
            "suspended" => Ok(Self::Suspended),
            _ => Err(StoreError::internal("invalid persisted service mode")),
        }
    }

    fn service_mode(self) -> AcsServiceMode {
        match self {
            Self::Normal => AcsServiceMode::Normal,
            Self::ReadOnly => AcsServiceMode::ReadOnly,
            Self::Suspended => AcsServiceMode::Suspended,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AccountAuthentication {
    pub signing_key_id: String,
    pub signing_public_key: [u8; 32],
}

#[derive(Debug, Clone)]
pub(crate) struct ConsumedTicket {
    pub account_locator: String,
    pub last_event_sequence: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct RootEventRecord {
    pub account_locator: String,
    pub event: AcsSseEvent,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GcReport {
    pub marked_objects: u64,
    pub deleted_objects: u64,
    pub deleted_ciphertext_bytes: u64,
    pub deleted_blob_files: u64,
    /// Longest contiguous hold of the database writer during this run.
    pub database_pause_ms: u64,
    /// Sum of all database-writer holds during this run.
    pub database_work_ms: u64,
    pub total_elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResumeWritesReport {
    pub resumed: bool,
    pub forced: bool,
    pub stored_bytes: u64,
    pub storage_headroom_bytes: u64,
    pub filesystem_free_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
struct BackupStatus {
    runs: u64,
    failures: u64,
    first_required_at_epoch_ms: i64,
    last_completed_at_epoch_ms: Option<i64>,
    last_elapsed_ms: u64,
    peak_elapsed_ms: u64,
    last_sqlite_snapshot_ms: u64,
    peak_sqlite_snapshot_ms: u64,
    last_wal_growth_bytes: u64,
    peak_wal_growth_bytes: u64,
    last_linked_blob_count: u64,
    peak_linked_blob_count: u64,
    last_linked_blob_bytes: u64,
    peak_linked_blob_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct StoreError {
    pub code: AcsErrorCode,
    pub message: String,
    pub retry_after_ms: Option<u64>,
    pub rate_limit_gate: Option<AcsRateLimitGate>,
}

impl StoreError {
    pub(crate) fn new(code: AcsErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retry_after_ms: None,
            rate_limit_gate: None,
        }
    }

    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self::new(AcsErrorCode::Internal, message)
    }

    pub(crate) fn sqlite(error: rusqlite::Error) -> Self {
        Self::internal(format!("cloud database failure: {error}"))
    }

    pub(crate) fn io(context: &str, error: std::io::Error) -> Self {
        Self::internal(format!("{context}: {error}"))
    }
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StoreError {}

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Default)]
struct OperationCounters {
    reads: AtomicU64,
    creates: AtomicU64,
    root_cas: AtomicU64,
    lists: AtomicU64,
    ingress_bytes: AtomicU64,
    egress_bytes: AtomicU64,
    authentication_rejections: AtomicU64,
    replay_rejections: AtomicU64,
    quota_rejections: AtomicU64,
    rate_limit_rejections: AtomicU64,
    malformed_rejections: AtomicU64,
    account_creation_attempts: AtomicU64,
    account_creation_successes: AtomicU64,
    account_creation_rejections: AtomicU64,
    account_creation_network_rate_rejections: AtomicU64,
    account_creation_global_rate_rejections: AtomicU64,
    account_creation_other_rejections: AtomicU64,
    account_creation_idempotent_successes: AtomicU64,
    current_sse_connections: AtomicU64,
    peak_sse_connections: AtomicU64,
    gc_runs: AtomicU64,
    gc_last_database_pause_ms: AtomicU64,
    gc_peak_database_pause_ms: AtomicU64,
    gc_total_database_work_ms: AtomicU64,
    gc_last_elapsed_ms: AtomicU64,
    gc_peak_elapsed_ms: AtomicU64,
}

#[derive(Debug, Clone, Copy)]
enum CreationMetric {
    Attempt = 0,
    Success = 1,
    NetworkRateRejection = 2,
    GlobalRateRejection = 3,
    OtherRejection = 4,
}

const CREATION_METRIC_COUNT: usize = 5;

#[derive(Debug, Clone)]
struct CreationMinuteBucket {
    minute: i64,
    counts: [u64; CREATION_METRIC_COUNT],
}

#[derive(Default)]
struct RollingCreationMetrics {
    buckets: VecDeque<CreationMinuteBucket>,
}

impl RollingCreationMetrics {
    fn record(&mut self, metric: CreationMetric, now_epoch_ms: i64) {
        let minute = now_epoch_ms.div_euclid(60_000);
        if self
            .buckets
            .back()
            .is_none_or(|bucket| bucket.minute != minute)
        {
            self.buckets.push_back(CreationMinuteBucket {
                minute,
                counts: [0; CREATION_METRIC_COUNT],
            });
        }
        if let Some(bucket) = self.buckets.back_mut() {
            bucket.counts[metric as usize] += 1;
        }
        while self
            .buckets
            .front()
            .is_some_and(|bucket| minute.saturating_sub(bucket.minute) >= 60)
        {
            self.buckets.pop_front();
        }
    }

    fn count(&self, metric: CreationMetric, now_epoch_ms: i64, minutes: i64) -> u64 {
        let current_minute = now_epoch_ms.div_euclid(60_000);
        self.buckets
            .iter()
            .filter(|bucket| current_minute.saturating_sub(bucket.minute) < minutes)
            .map(|bucket| bucket.counts[metric as usize])
            .sum()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct TokenBucket {
    available_quanta: u128,
    updated_at_epoch_ms: i64,
    initialized: bool,
}

impl TokenBucket {
    fn refill(&mut self, config: TokenBucketConfig, now_epoch_ms: i64) {
        let full = u128::from(config.capacity) * u128::from(config.refill_period_ms);
        if !self.initialized {
            self.available_quanta = full;
            self.updated_at_epoch_ms = now_epoch_ms;
            self.initialized = true;
            return;
        }
        let elapsed_ms = now_epoch_ms.saturating_sub(self.updated_at_epoch_ms).max(0) as u128;
        self.available_quanta = self
            .available_quanta
            .saturating_add(elapsed_ms * u128::from(config.refill_amount))
            .min(full);
        self.updated_at_epoch_ms = now_epoch_ms;
    }

    fn reserve(
        &mut self,
        amount: u64,
        config: TokenBucketConfig,
        now_epoch_ms: i64,
    ) -> Result<(), u64> {
        self.refill(config, now_epoch_ms);
        let required = u128::from(amount) * u128::from(config.refill_period_ms);
        if self.available_quanta < required {
            let deficit = required - self.available_quanta;
            let retry_after_ms = deficit.div_ceil(u128::from(config.refill_amount));
            return Err(retry_after_ms.try_into().unwrap_or(u64::MAX));
        }
        self.available_quanta -= required;
        Ok(())
    }

    fn consumed(&self, config: TokenBucketConfig, now_epoch_ms: i64) -> u64 {
        let mut current = *self;
        current.refill(config, now_epoch_ms);
        let full = u128::from(config.capacity) * u128::from(config.refill_period_ms);
        ((full - current.available_quanta) / u128::from(config.refill_period_ms))
            .try_into()
            .unwrap_or(u64::MAX)
    }

    fn is_full(&self, config: TokenBucketConfig, now_epoch_ms: i64) -> bool {
        self.consumed(config, now_epoch_ms) == 0
    }
}

#[derive(Default)]
struct RuntimeLimits {
    global_operations: TokenBucket,
    network_operations: HashMap<String, TokenBucket>,
    account_operations: HashMap<String, TokenBucket>,
    global_ingress_bytes: TokenBucket,
    network_ingress_bytes: HashMap<String, TokenBucket>,
    account_ingress_bytes: HashMap<String, TokenBucket>,
    global_egress_bytes: TokenBucket,
    network_egress_bytes: HashMap<String, TokenBucket>,
    account_egress_bytes: HashMap<String, TokenBucket>,
    account_sse: HashMap<String, u64>,
    network_sse: HashMap<String, u64>,
}

struct ReadConnectionPool {
    available: Mutex<Vec<Connection>>,
    ready: Condvar,
}

impl ReadConnectionPool {
    fn open(database_path: &Path) -> StoreResult<Self> {
        let mut available = Vec::with_capacity(READ_CONNECTION_POOL_SIZE);
        for _ in 0..READ_CONNECTION_POOL_SIZE {
            let connection =
                Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .map_err(StoreError::sqlite)?;
            configure_read_database(&connection)?;
            available.push(connection);
        }
        Ok(Self {
            available: Mutex::new(available),
            ready: Condvar::new(),
        })
    }

    fn acquire(&self) -> StoreResult<ReadConnection<'_>> {
        let mut available = self
            .available
            .lock()
            .map_err(|_| StoreError::internal("cloud reader pool mutex is poisoned"))?;
        loop {
            if let Some(connection) = available.pop() {
                return Ok(ReadConnection {
                    pool: self,
                    connection: Some(connection),
                });
            }
            available = self
                .ready
                .wait(available)
                .map_err(|_| StoreError::internal("cloud reader pool mutex is poisoned"))?;
        }
    }
}

struct ReadConnection<'a> {
    pool: &'a ReadConnectionPool,
    connection: Option<Connection>,
}

impl Deref for ReadConnection<'_> {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.connection
            .as_ref()
            .expect("pooled reader must hold a connection")
    }
}

impl Drop for ReadConnection<'_> {
    fn drop(&mut self) {
        let Some(connection) = self.connection.take() else {
            return;
        };
        if let Ok(mut available) = self.pool.available.lock() {
            available.push(connection);
            self.pool.ready.notify_one();
        }
    }
}

struct StoreInner {
    config: StoreConfig,
    layout: StorageLayout,
    database_path: PathBuf,
    blob_root: PathBuf,
    readers: ReadConnectionPool,
    connection: Mutex<Connection>,
    started_at_epoch_ms: i64,
    last_durable_read_epoch_ms: AtomicI64,
    last_durable_write_epoch_ms: AtomicI64,
    events: broadcast::Sender<RootEventRecord>,
    counters: OperationCounters,
    creation_metrics: Mutex<RollingCreationMetrics>,
    limits: Mutex<RuntimeLimits>,
}

#[derive(Clone)]
pub struct CloudStore {
    inner: Arc<StoreInner>,
}

impl CloudStore {
    pub fn open(config: StoreConfig) -> StoreResult<Self> {
        config
            .global_operation_bucket
            .validate("global operation")?;
        config
            .network_operation_bucket
            .validate("network operation")?;
        config
            .account_operation_bucket
            .validate("account operation")?;
        config.global_ingress_bucket.validate("global ingress")?;
        config.network_ingress_bucket.validate("network ingress")?;
        config.account_ingress_bucket.validate("account ingress")?;
        config.global_egress_bucket.validate("global egress")?;
        config.network_egress_bucket.validate("network egress")?;
        config.account_egress_bucket.validate("account egress")?;
        config
            .creation_network_bucket
            .validate("network account creation")?;
        config
            .creation_global_bucket
            .validate("global account creation")?;
        let layout = StorageLayout::new(config.storage_root.clone());
        layout.ensure()?;
        let blob_root = layout.blob_root();
        let database_path = layout.database_path();
        let mut connection = Connection::open(&database_path).map_err(StoreError::sqlite)?;
        configure_database(&connection)?;
        initialize_schema(&connection)?;
        repair_storage_accounting(&mut connection, config.global_storage_limit_bytes)?;
        let readers = ReadConnectionPool::open(&database_path)?;
        let (events, _) = broadcast::channel(512);
        let store = Self {
            inner: Arc::new(StoreInner {
                config,
                layout,
                database_path,
                blob_root,
                readers,
                connection: Mutex::new(connection),
                started_at_epoch_ms: now_epoch_ms(),
                last_durable_read_epoch_ms: AtomicI64::new(0),
                last_durable_write_epoch_ms: AtomicI64::new(0),
                events,
                counters: OperationCounters::default(),
                creation_metrics: Mutex::new(RollingCreationMetrics::default()),
                limits: Mutex::new(RuntimeLimits::default()),
            }),
        };
        store.reconcile_pending_blobs()?;
        Ok(store)
    }

    fn connection(&self) -> StoreResult<MutexGuard<'_, Connection>> {
        self.inner
            .connection
            .lock()
            .map_err(|_| StoreError::internal("cloud database mutex is poisoned"))
    }

    fn read_connection(&self) -> StoreResult<ReadConnection<'_>> {
        self.inner.readers.acquire()
    }

    pub(crate) fn increment_authentication_rejection(&self, replay: bool) {
        self.inner
            .counters
            .authentication_rejections
            .fetch_add(1, Ordering::Relaxed);
        if replay {
            self.inner
                .counters
                .replay_rejections
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn increment_malformed_rejection(&self) {
        self.inner
            .counters
            .malformed_rejections
            .fetch_add(1, Ordering::Relaxed);
    }

    fn record_creation_metric(&self, metric: CreationMetric, now_epoch_ms: i64) {
        if let Ok(mut metrics) = self.inner.creation_metrics.lock() {
            metrics.record(metric, now_epoch_ms);
        }
    }

    fn record_other_creation_rejection(&self, now_epoch_ms: i64) {
        self.inner
            .counters
            .account_creation_rejections
            .fetch_add(1, Ordering::Relaxed);
        self.inner
            .counters
            .account_creation_other_rejections
            .fetch_add(1, Ordering::Relaxed);
        self.record_creation_metric(CreationMetric::OtherRejection, now_epoch_ms);
    }

    fn ensure_new_storage_write(&self, now_epoch_ms: i64) -> StoreResult<()> {
        let connection = self.connection()?;
        let mode = AccountMode::parse(
            &connection
                .query_row(
                    "SELECT mode FROM service_state WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(StoreError::sqlite)?,
        )?;
        enforce_mode(mode, true, "service")?;
        let filesystem_free_bytes = fs2::available_space(self.inner.layout.root())
            .map_err(|error| StoreError::io("measure cloud filesystem free space", error))?;
        if filesystem_free_bytes >= self.inner.config.filesystem_free_bytes_critical {
            return Ok(());
        }
        connection
            .execute(
                "UPDATE service_state SET mode = 'read_only' WHERE singleton = 1",
                [],
            )
            .map_err(StoreError::sqlite)?;
        self.note_durable_write(now_epoch_ms);
        Err(StoreError::new(
            AcsErrorCode::ReadOnly,
            "cloud service stopped writes before filesystem exhaustion",
        ))
    }

    pub(crate) fn check_network_operation(
        &self,
        network_pseudonym: &str,
        now_epoch_ms: i64,
    ) -> StoreResult<()> {
        let mut limits = self
            .inner
            .limits
            .lock()
            .map_err(|_| StoreError::internal("cloud limit mutex is poisoned"))?;
        let mut global = limits.global_operations;
        if let Err(retry_after_ms) =
            global.reserve(1, self.inner.config.global_operation_bucket, now_epoch_ms)
        {
            return self.rate_limited(
                AcsRateLimitGate::GlobalOperations,
                "global cloud request rate is exceeded",
                retry_after_ms,
            );
        }
        if limits.network_operations.len() >= MAX_TRACKED_NETWORK_WINDOWS
            && !limits.network_operations.contains_key(network_pseudonym)
        {
            let config = self.inner.config.network_operation_bucket;
            limits
                .network_operations
                .retain(|_, bucket| !bucket.is_full(config, now_epoch_ms));
        }
        if limits.network_operations.len() >= MAX_TRACKED_NETWORK_WINDOWS
            && !limits.network_operations.contains_key(network_pseudonym)
        {
            return self.rate_limited(
                AcsRateLimitGate::NetworkOperations,
                "too many distinct source networks",
                RATE_WINDOW_MS as u64,
            );
        }
        let config = self.inner.config.network_operation_bucket;
        let mut network = limits
            .network_operations
            .get(network_pseudonym)
            .copied()
            .unwrap_or_default();
        if let Err(retry_after_ms) = network.reserve(1, config, now_epoch_ms) {
            return self.rate_limited(
                AcsRateLimitGate::NetworkOperations,
                "source network request rate is exceeded",
                retry_after_ms,
            );
        }
        limits.global_operations = global;
        limits
            .network_operations
            .insert(network_pseudonym.to_string(), network);
        Ok(())
    }

    pub(crate) fn check_account_operation(
        &self,
        account_locator: &str,
        now_epoch_ms: i64,
    ) -> StoreResult<()> {
        let mut limits = self
            .inner
            .limits
            .lock()
            .map_err(|_| StoreError::internal("cloud limit mutex is poisoned"))?;
        let config = self.inner.config.account_operation_bucket;
        if let Err(retry_after_ms) = limits
            .account_operations
            .entry(account_locator.to_string())
            .or_default()
            .reserve(1, config, now_epoch_ms)
        {
            return self.rate_limited(
                AcsRateLimitGate::AccountOperations,
                "cloud account request rate is exceeded",
                retry_after_ms,
            );
        }
        Ok(())
    }

    pub(crate) fn admit_network_ingress(
        &self,
        network_pseudonym: &str,
        bytes: u64,
        now_epoch_ms: i64,
    ) -> StoreResult<()> {
        if bytes == 0 {
            return Ok(());
        }
        let mut limits = self
            .inner
            .limits
            .lock()
            .map_err(|_| StoreError::internal("cloud limit mutex is poisoned"))?;
        if limits.network_ingress_bytes.len() >= MAX_TRACKED_NETWORK_WINDOWS
            && !limits.network_ingress_bytes.contains_key(network_pseudonym)
        {
            let config = self.inner.config.network_ingress_bucket;
            limits
                .network_ingress_bytes
                .retain(|_, bucket| !bucket.is_full(config, now_epoch_ms));
        }
        if limits.network_ingress_bytes.len() >= MAX_TRACKED_NETWORK_WINDOWS
            && !limits.network_ingress_bytes.contains_key(network_pseudonym)
        {
            return self.rate_limited(
                AcsRateLimitGate::NetworkIngress,
                "too many distinct cloud ingress source networks",
                RATE_WINDOW_MS as u64,
            );
        }
        let mut global = limits.global_ingress_bytes;
        if let Err(retry_after_ms) =
            global.reserve(bytes, self.inner.config.global_ingress_bucket, now_epoch_ms)
        {
            return self.rate_limited(
                AcsRateLimitGate::GlobalIngress,
                "global cloud ingress rate is exceeded",
                retry_after_ms,
            );
        }
        let mut network = limits
            .network_ingress_bytes
            .get(network_pseudonym)
            .copied()
            .unwrap_or_default();
        if let Err(retry_after_ms) = network.reserve(
            bytes,
            self.inner.config.network_ingress_bucket,
            now_epoch_ms,
        ) {
            return self.rate_limited(
                AcsRateLimitGate::NetworkIngress,
                "source network cloud ingress rate is exceeded",
                retry_after_ms,
            );
        }
        limits.global_ingress_bytes = global;
        limits
            .network_ingress_bytes
            .insert(network_pseudonym.to_string(), network);
        self.inner
            .counters
            .ingress_bytes
            .fetch_add(bytes, Ordering::Relaxed);
        Ok(())
    }

    pub(crate) fn admit_account_ingress(
        &self,
        account_locator: &str,
        bytes: u64,
        now_epoch_ms: i64,
    ) -> StoreResult<()> {
        if bytes == 0 {
            return Ok(());
        }
        let mut limits = self
            .inner
            .limits
            .lock()
            .map_err(|_| StoreError::internal("cloud limit mutex is poisoned"))?;
        let config = self.inner.config.account_ingress_bucket;
        if let Err(retry_after_ms) = limits
            .account_ingress_bytes
            .entry(account_locator.to_string())
            .or_default()
            .reserve(bytes, config, now_epoch_ms)
        {
            return self.rate_limited(
                AcsRateLimitGate::AccountIngress,
                "cloud account ingress rate is exceeded",
                retry_after_ms,
            );
        }
        Ok(())
    }

    pub(crate) fn admit_egress(
        &self,
        account_locator: &str,
        network_pseudonym: &str,
        bytes: u64,
        now_epoch_ms: i64,
    ) -> StoreResult<()> {
        let mut limits = self
            .inner
            .limits
            .lock()
            .map_err(|_| StoreError::internal("cloud limit mutex is poisoned"))?;
        if limits.network_egress_bytes.len() >= MAX_TRACKED_NETWORK_WINDOWS
            && !limits.network_egress_bytes.contains_key(network_pseudonym)
        {
            let config = self.inner.config.network_egress_bucket;
            limits
                .network_egress_bytes
                .retain(|_, bucket| !bucket.is_full(config, now_epoch_ms));
        }
        if limits.network_egress_bytes.len() >= MAX_TRACKED_NETWORK_WINDOWS
            && !limits.network_egress_bytes.contains_key(network_pseudonym)
        {
            return self.rate_limited(
                AcsRateLimitGate::NetworkEgress,
                "too many distinct cloud egress source networks",
                RATE_WINDOW_MS as u64,
            );
        }
        let mut global = limits.global_egress_bytes;
        if let Err(retry_after_ms) =
            global.reserve(bytes, self.inner.config.global_egress_bucket, now_epoch_ms)
        {
            return self.rate_limited(
                AcsRateLimitGate::GlobalEgress,
                "global cloud egress rate is exceeded",
                retry_after_ms,
            );
        }
        let mut network = limits
            .network_egress_bytes
            .get(network_pseudonym)
            .copied()
            .unwrap_or_default();
        if let Err(retry_after_ms) =
            network.reserve(bytes, self.inner.config.network_egress_bucket, now_epoch_ms)
        {
            return self.rate_limited(
                AcsRateLimitGate::NetworkEgress,
                "source network cloud egress rate is exceeded",
                retry_after_ms,
            );
        }
        let config = self.inner.config.account_egress_bucket;
        let mut account = limits
            .account_egress_bytes
            .get(account_locator)
            .copied()
            .unwrap_or_default();
        if let Err(retry_after_ms) = account.reserve(bytes, config, now_epoch_ms) {
            return self.rate_limited(
                AcsRateLimitGate::AccountEgress,
                "cloud account egress rate is exceeded",
                retry_after_ms,
            );
        }
        limits.global_egress_bytes = global;
        limits
            .network_egress_bytes
            .insert(network_pseudonym.to_string(), network);
        limits
            .account_egress_bytes
            .insert(account_locator.to_string(), account);
        self.inner
            .counters
            .egress_bytes
            .fetch_add(bytes, Ordering::Relaxed);
        Ok(())
    }

    fn rate_limited<T>(
        &self,
        gate: AcsRateLimitGate,
        message: &str,
        retry_after_ms: u64,
    ) -> StoreResult<T> {
        self.inner
            .counters
            .rate_limit_rejections
            .fetch_add(1, Ordering::Relaxed);
        Err(StoreError {
            code: AcsErrorCode::RateLimited,
            message: message.to_string(),
            retry_after_ms: Some(retry_after_ms),
            rate_limit_gate: Some(gate),
        })
    }

    pub(crate) fn issue_creation_challenge(
        &self,
        network_pseudonym: &str,
        now_epoch_ms: i64,
    ) -> StoreResult<AcsCreationChallengeResponse> {
        self.ensure_new_storage_write(now_epoch_ms)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::sqlite)?;
        ensure_service_writeable(&transaction)?;
        transaction
            .execute(
                "DELETE FROM challenges WHERE expires_at_epoch_ms < ?1",
                [now_epoch_ms],
            )
            .map_err(StoreError::sqlite)?;
        let outstanding: u64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM challenges WHERE network_pseudonym = ?1 AND used = 0",
                [network_pseudonym],
                |row| row.get(0),
            )
            .map_err(StoreError::sqlite)?;
        if outstanding >= OUTSTANDING_CHALLENGES_PER_NETWORK {
            self.inner
                .counters
                .rate_limit_rejections
                .fetch_add(1, Ordering::Relaxed);
            return self.rate_limited(
                AcsRateLimitGate::OutstandingCreationChallenges,
                "too many outstanding account challenges",
                60_000,
            );
        }
        let challenge = random_token(32)?;
        let expires_at_epoch_ms = now_epoch_ms + CHALLENGE_TTL_MS;
        transaction
            .execute(
                "INSERT INTO challenges(challenge, network_pseudonym, issued_at_epoch_ms, expires_at_epoch_ms, used) VALUES (?1, ?2, ?3, ?4, 0)",
                params![challenge, network_pseudonym, now_epoch_ms, expires_at_epoch_ms],
            )
            .map_err(StoreError::sqlite)?;
        transaction.commit().map_err(StoreError::sqlite)?;
        self.note_durable_write(now_epoch_ms);
        Ok(AcsCreationChallengeResponse {
            contract_id: ACS_CONTRACT_ID.to_string(),
            challenge,
            expires_at_epoch_ms,
            server_time_epoch_ms: now_epoch_ms,
        })
    }

    pub(crate) fn create_account(
        &self,
        request: &AcsCreateAccountRequest,
        signing_public_key: &[u8; 32],
        network_pseudonym: &str,
        now_epoch_ms: i64,
    ) -> StoreResult<AcsCreateAccountResponse> {
        self.ensure_new_storage_write(now_epoch_ms)?;
        self.inner
            .counters
            .account_creation_attempts
            .fetch_add(1, Ordering::Relaxed);
        self.record_creation_metric(CreationMetric::Attempt, now_epoch_ms);
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::sqlite)?;
        ensure_service_writeable(&transaction)?;
        let challenge = transaction
            .query_row(
                "SELECT network_pseudonym, expires_at_epoch_ms, used FROM challenges WHERE challenge = ?1",
                [&request.creation_challenge],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, bool>(2)?)),
            )
            .optional()
            .map_err(StoreError::sqlite)?;
        let Some(challenge) = challenge else {
            self.record_other_creation_rejection(now_epoch_ms);
            return Err(StoreError::new(
                AcsErrorCode::Unauthorized,
                "account challenge is unknown",
            ));
        };
        if challenge.0 != network_pseudonym || challenge.1 < now_epoch_ms || challenge.2 {
            self.record_other_creation_rejection(now_epoch_ms);
            return Err(StoreError::new(
                AcsErrorCode::Unauthorized,
                "account challenge is expired, used, or belongs to another network",
            ));
        }
        let existing = transaction
            .query_row(
                "SELECT signing_key_id, signing_public_key, quota_class, quota_bytes FROM accounts WHERE account_locator = ?1",
                [&request.account_locator],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, String>(2)?, row.get::<_, u64>(3)?)),
            )
            .optional()
            .map_err(StoreError::sqlite)?;
        if let Some((key_id, public_key, quota_class, quota_bytes)) = existing {
            if key_id != request.signing_key_id || public_key != signing_public_key {
                self.record_other_creation_rejection(now_epoch_ms);
                return Err(StoreError::new(
                    AcsErrorCode::Conflict,
                    "account locator is already registered to another key",
                ));
            }
            transaction
                .execute(
                    "UPDATE challenges SET used = 1 WHERE challenge = ?1",
                    [&request.creation_challenge],
                )
                .map_err(StoreError::sqlite)?;
            transaction.commit().map_err(StoreError::sqlite)?;
            self.inner
                .counters
                .account_creation_idempotent_successes
                .fetch_add(1, Ordering::Relaxed);
            return Ok(AcsCreateAccountResponse {
                contract_id: ACS_CONTRACT_ID.to_string(),
                account_locator: request.account_locator.clone(),
                server_time_epoch_ms: now_epoch_ms,
                quota_class,
                quota_bytes,
            });
        }
        if let Err(retry_after_ms) = reserve_durable_token_bucket(
            &transaction,
            &format!("account_creation_network:{network_pseudonym}"),
            self.inner.config.creation_network_bucket,
            now_epoch_ms,
        )? {
            self.inner
                .counters
                .account_creation_rejections
                .fetch_add(1, Ordering::Relaxed);
            self.inner
                .counters
                .account_creation_network_rate_rejections
                .fetch_add(1, Ordering::Relaxed);
            self.inner
                .counters
                .rate_limit_rejections
                .fetch_add(1, Ordering::Relaxed);
            self.record_creation_metric(CreationMetric::NetworkRateRejection, now_epoch_ms);
            return Err(StoreError {
                code: AcsErrorCode::RateLimited,
                message: "source network account creation rate is exceeded".to_string(),
                retry_after_ms: Some(retry_after_ms),
                rate_limit_gate: Some(AcsRateLimitGate::AccountCreationNetwork),
            });
        }
        if let Err(retry_after_ms) = reserve_durable_token_bucket(
            &transaction,
            "account_creation_global",
            self.inner.config.creation_global_bucket,
            now_epoch_ms,
        )? {
            self.inner
                .counters
                .account_creation_rejections
                .fetch_add(1, Ordering::Relaxed);
            self.inner
                .counters
                .account_creation_global_rate_rejections
                .fetch_add(1, Ordering::Relaxed);
            self.inner
                .counters
                .rate_limit_rejections
                .fetch_add(1, Ordering::Relaxed);
            self.record_creation_metric(CreationMetric::GlobalRateRejection, now_epoch_ms);
            return Err(StoreError {
                code: AcsErrorCode::RateLimited,
                message: "global account creation rate is exceeded".to_string(),
                retry_after_ms: Some(retry_after_ms),
                rate_limit_gate: Some(AcsRateLimitGate::AccountCreationGlobal),
            });
        }
        transaction
            .execute(
                "INSERT INTO accounts(account_locator, signing_key_id, signing_public_key, mode, quota_class, quota_bytes, object_limit, stored_bytes, object_count, event_sequence, creation_network_pseudonym, created_at_epoch_ms, updated_at_epoch_ms) VALUES (?1, ?2, ?3, 'normal', 'anonymous', ?4, ?5, 0, 0, 0, ?6, ?7, ?7)",
                params![request.account_locator, request.signing_key_id, signing_public_key.as_slice(), self.inner.config.anonymous_quota_bytes, self.inner.config.anonymous_object_limit, network_pseudonym, now_epoch_ms],
            )
            .map_err(StoreError::sqlite)?;
        transaction
            .execute(
                "UPDATE challenges SET used = 1 WHERE challenge = ?1",
                [&request.creation_challenge],
            )
            .map_err(StoreError::sqlite)?;
        transaction.commit().map_err(StoreError::sqlite)?;
        self.inner
            .counters
            .account_creation_successes
            .fetch_add(1, Ordering::Relaxed);
        self.record_creation_metric(CreationMetric::Success, now_epoch_ms);
        self.note_durable_write(now_epoch_ms);
        Ok(AcsCreateAccountResponse {
            contract_id: ACS_CONTRACT_ID.to_string(),
            account_locator: request.account_locator.clone(),
            server_time_epoch_ms: now_epoch_ms,
            quota_class: "anonymous".to_string(),
            quota_bytes: self.inner.config.anonymous_quota_bytes,
        })
    }

    pub(crate) fn account_authentication(
        &self,
        account_locator: &str,
        write: bool,
    ) -> StoreResult<AccountAuthentication> {
        let connection = self.read_connection()?;
        let service_mode: String = connection
            .query_row(
                "SELECT mode FROM service_state WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(StoreError::sqlite)?;
        enforce_mode(AccountMode::parse(&service_mode)?, write, "service")?;
        let account = connection
            .query_row(
                "SELECT signing_key_id, signing_public_key, mode FROM accounts WHERE account_locator = ?1",
                [account_locator],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, String>(2)?)),
            )
            .optional()
            .map_err(StoreError::sqlite)?
            .ok_or_else(|| StoreError::new(AcsErrorCode::NotFound, "cloud account does not exist"))?;
        enforce_mode(AccountMode::parse(&account.2)?, write, "account")?;
        Ok(AccountAuthentication {
            signing_key_id: account.0,
            signing_public_key: account
                .1
                .try_into()
                .map_err(|_| StoreError::internal("stored signing public key has wrong size"))?,
        })
    }

    pub(crate) fn consume_nonce(
        &self,
        account_locator: &str,
        signing_key_id: &str,
        nonce: &str,
        expires_at_epoch_ms: i64,
        now_epoch_ms: i64,
    ) -> StoreResult<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "DELETE FROM request_nonces WHERE expires_at_epoch_ms < ?1",
                [now_epoch_ms],
            )
            .map_err(StoreError::sqlite)?;
        let inserted = connection.execute(
            "INSERT OR IGNORE INTO request_nonces(account_locator, signing_key_id, nonce, expires_at_epoch_ms) VALUES (?1, ?2, ?3, ?4)",
            params![account_locator, signing_key_id, nonce, expires_at_epoch_ms],
        ).map_err(StoreError::sqlite)?;
        if inserted == 0 {
            self.increment_authentication_rejection(true);
            return Err(StoreError::new(
                AcsErrorCode::ReplayDetected,
                "request nonce was already used",
            ));
        }
        Ok(())
    }

    pub(crate) fn create_object(
        &self,
        account_locator: &str,
        object_id: &str,
        value: &AcsEncryptedValue,
        now_epoch_ms: i64,
    ) -> StoreResult<AcsCreateObjectOutcome> {
        self.ensure_new_storage_write(now_epoch_ms)?;
        value
            .validate()
            .map_err(|error| StoreError::new(AcsErrorCode::InvalidRequest, error))?;
        validate_opaque_id(object_id, "object ID")?;
        let ciphertext = value
            .ciphertext()
            .map_err(|error| StoreError::new(AcsErrorCode::InvalidRequest, error))?;
        let ciphertext_bytes = ciphertext.len() as u64;
        let children_json = serde_json::to_string(&value.child_object_ids)
            .map_err(|error| StoreError::internal(format!("encode object children: {error}")))?;
        let storage_bytes = object_storage_bytes(object_id, ciphertext_bytes, &children_json);
        let authenticated_hash = value
            .authenticated_hash(AcsEncryptedValueKind::Object, object_id)
            .map_err(|error| StoreError::new(AcsErrorCode::InvalidRequest, error))?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::sqlite)?;
        ensure_account_writeable(&transaction, account_locator)?;
        if let Some(existing_hash) = transaction
            .query_row(
                "SELECT authenticated_hash FROM objects WHERE account_locator = ?1 AND object_id = ?2",
                params![account_locator, object_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(StoreError::sqlite)?
        {
            if existing_hash == authenticated_hash {
                return Ok(AcsCreateObjectOutcome::AlreadyExists);
            }
            return Err(StoreError::new(
                AcsErrorCode::ObjectIdCollision,
                "object ID already contains different authenticated data",
            ));
        }
        ensure_children_exist(&transaction, account_locator, &value.child_object_ids)?;
        if let Err(error) = reserve_account_usage(
            &transaction,
            account_locator,
            storage_bytes,
            self.inner.config.global_storage_limit_bytes,
        ) {
            if error.code == AcsErrorCode::QuotaExceeded {
                self.inner
                    .counters
                    .quota_rejections
                    .fetch_add(1, Ordering::Relaxed);
            }
            if error.code == AcsErrorCode::ReadOnly {
                transaction
                    .execute(
                        "UPDATE service_state SET mode = 'read_only' WHERE singleton = 1",
                        [],
                    )
                    .map_err(StoreError::sqlite)?;
                transaction.commit().map_err(StoreError::sqlite)?;
                self.note_durable_write(now_epoch_ms);
            }
            return Err(error);
        }
        if ciphertext_bytes <= self.inner.config.inline_threshold_bytes {
            transaction
                .execute(
                    "INSERT INTO objects(account_locator, object_id, authenticated_hash, ciphertext_sha256, ciphertext_bytes, children_json, state, inline_ciphertext, blob_storage_key, created_at_epoch_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ready', ?7, NULL, ?8)",
                    params![account_locator, object_id, authenticated_hash, value.ciphertext_sha256, ciphertext_bytes, children_json, ciphertext, now_epoch_ms],
                )
                .map_err(StoreError::sqlite)?;
            transaction.commit().map_err(StoreError::sqlite)?;
        } else {
            let blob_storage_key =
                blob_storage_key(account_locator, object_id, &value.ciphertext_sha256);
            transaction
                .execute(
                    "INSERT INTO objects(account_locator, object_id, authenticated_hash, ciphertext_sha256, ciphertext_bytes, children_json, state, inline_ciphertext, blob_storage_key, created_at_epoch_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', NULL, ?7, ?8)",
                    params![account_locator, object_id, authenticated_hash, value.ciphertext_sha256, ciphertext_bytes, children_json, blob_storage_key, now_epoch_ms],
                )
                .map_err(StoreError::sqlite)?;
            transaction.commit().map_err(StoreError::sqlite)?;
            drop(connection);
            if let Err(error) = self.write_blob(&blob_storage_key, &ciphertext) {
                self.remove_pending_object(account_locator, object_id, storage_bytes)?;
                return Err(error);
            }
            let connection = self.connection()?;
            connection
                .execute(
                    "UPDATE objects SET state = 'ready' WHERE account_locator = ?1 AND object_id = ?2 AND state = 'pending'",
                    params![account_locator, object_id],
                )
                .map_err(StoreError::sqlite)?;
        }
        self.inner.counters.creates.fetch_add(1, Ordering::Relaxed);
        self.note_durable_write(now_epoch_ms);
        Ok(AcsCreateObjectOutcome::Created)
    }

    pub(crate) fn read_object(
        &self,
        account_locator: &str,
        object_id: &str,
        network_pseudonym: &str,
    ) -> StoreResult<AcsObjectSnapshot> {
        validate_opaque_id(object_id, "object ID")?;
        let connection = self.read_connection()?;
        ensure_account_readable(&connection, account_locator)?;
        let row = connection
            .query_row(
                "SELECT ciphertext_sha256, ciphertext_bytes, children_json, inline_ciphertext, blob_storage_key, created_at_epoch_ms FROM objects WHERE account_locator = ?1 AND object_id = ?2 AND state = 'ready'",
                params![account_locator, object_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<Vec<u8>>>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, i64>(5)?)),
            )
            .optional()
            .map_err(StoreError::sqlite)?
            .ok_or_else(|| StoreError::new(AcsErrorCode::NotFound, "cloud object does not exist"))?;
        drop(connection);
        let ciphertext = self.read_placed_ciphertext(row.3, row.4.as_deref())?;
        if ciphertext.len() as u64 != row.1 || sha256_hex(&ciphertext) != row.0 {
            return Err(StoreError::internal("stored object ciphertext is corrupt"));
        }
        let child_object_ids = serde_json::from_str(&row.2)
            .map_err(|error| StoreError::internal(format!("decode object children: {error}")))?;
        let snapshot = AcsObjectSnapshot {
            object_id: object_id.to_string(),
            value: AcsEncryptedValue {
                ciphertext_base64url: URL_SAFE_NO_PAD.encode(ciphertext),
                ciphertext_sha256: row.0,
                child_object_ids,
            },
            created_at_epoch_ms: row.5,
        };
        let response_bytes = serialized_response_bytes(&snapshot)?;
        self.admit_egress(
            account_locator,
            network_pseudonym,
            response_bytes,
            now_epoch_ms(),
        )?;
        self.inner.counters.reads.fetch_add(1, Ordering::Relaxed);
        self.note_durable_read(now_epoch_ms());
        Ok(snapshot)
    }

    pub(crate) fn list_objects(
        &self,
        account_locator: &str,
        cursor: Option<&str>,
        limit: u32,
        network_pseudonym: &str,
    ) -> StoreResult<AcsListObjectsResponse> {
        if limit > MAX_LIST_LIMIT {
            return Err(StoreError::new(
                AcsErrorCode::InvalidRequest,
                format!("object list limit exceeds {MAX_LIST_LIMIT}"),
            ));
        }
        let limit = if limit == 0 {
            DEFAULT_LIST_LIMIT
        } else {
            limit
        };
        if let Some(cursor) = cursor {
            validate_opaque_id(cursor, "object list cursor")?;
        }
        let cursor = cursor.unwrap_or("");
        let connection = self.read_connection()?;
        ensure_account_readable(&connection, account_locator)?;
        let total_object_count = connection
            .query_row(
                "SELECT COUNT(*) FROM objects WHERE account_locator = ?1 AND state = 'ready'",
                [account_locator],
                |row| row.get::<_, u64>(0),
            )
            .map_err(StoreError::sqlite)?;
        let mut statement = connection
            .prepare(
                "SELECT object_id, authenticated_hash, ciphertext_bytes, created_at_epoch_ms FROM objects WHERE account_locator = ?1 AND state = 'ready' AND object_id > ?2 ORDER BY object_id LIMIT ?3",
            )
            .map_err(StoreError::sqlite)?;
        let objects = statement
            .query_map(params![account_locator, cursor, limit + 1], |row| {
                Ok(AcsObjectSummary {
                    object_id: row.get(0)?,
                    authenticated_hash: row.get(1)?,
                    ciphertext_bytes: row.get(2)?,
                    created_at_epoch_ms: row.get(3)?,
                })
            })
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        let mut objects = objects;
        let next_cursor = if objects.len() > limit as usize {
            objects.truncate(limit as usize);
            objects.last().map(|object| object.object_id.clone())
        } else {
            None
        };
        let response = AcsListObjectsResponse {
            contract_id: ACS_CONTRACT_ID.to_string(),
            objects,
            next_cursor,
            total_object_count,
        };
        let response_bytes = serialized_response_bytes(&response)?;
        self.admit_egress(
            account_locator,
            network_pseudonym,
            response_bytes,
            now_epoch_ms(),
        )?;
        self.inner.counters.lists.fetch_add(1, Ordering::Relaxed);
        self.note_durable_read(now_epoch_ms());
        Ok(response)
    }

    pub(crate) fn read_root(
        &self,
        account_locator: &str,
        network_pseudonym: &str,
    ) -> StoreResult<AcsRootSnapshot> {
        let connection = self.read_connection()?;
        ensure_account_readable(&connection, account_locator)?;
        let root = read_root_row(&connection, account_locator)?
            .ok_or_else(|| StoreError::new(AcsErrorCode::NotFound, "cloud account has no root"))?;
        drop(connection);
        let response_bytes = serialized_response_bytes(&root)?;
        self.admit_egress(
            account_locator,
            network_pseudonym,
            response_bytes,
            now_epoch_ms(),
        )?;
        self.inner.counters.reads.fetch_add(1, Ordering::Relaxed);
        self.note_durable_read(now_epoch_ms());
        Ok(root)
    }

    pub(crate) fn compare_and_swap_root(
        &self,
        account_locator: &str,
        request: &AcsCompareAndSwapRootRequest,
        network_pseudonym: &str,
        now_epoch_ms: i64,
    ) -> StoreResult<AcsCompareAndSwapRootResponse> {
        self.ensure_new_storage_write(now_epoch_ms)?;
        request
            .replacement
            .validate()
            .map_err(|error| StoreError::new(AcsErrorCode::InvalidRequest, error))?;
        let root_hash = request
            .replacement
            .authenticated_hash(AcsEncryptedValueKind::Root, ACS_FIXED_ROOT_ID)
            .map_err(|error| StoreError::new(AcsErrorCode::InvalidRequest, error))?;
        let ciphertext = request
            .replacement
            .ciphertext()
            .map_err(|error| StoreError::new(AcsErrorCode::InvalidRequest, error))?;
        let ciphertext_bytes = ciphertext.len() as u64;
        let children_json = serde_json::to_string(&request.replacement.child_object_ids)
            .map_err(|error| StoreError::internal(format!("encode root children: {error}")))?;
        let replacement_storage_bytes = root_storage_bytes(ciphertext_bytes, &children_json);
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::sqlite)?;
        ensure_account_writeable(&transaction, account_locator)?;
        let current = read_root_row(&transaction, account_locator)?;
        let current_revision = current.as_ref().map_or(0, |root| root.revision);
        let current_hash = current.as_ref().map(|root| root.root_hash.clone());
        if request.expected_revision != current_revision
            || request.expected_root_hash != current_hash
        {
            let response = AcsCompareAndSwapRootResponse::Conflict {
                current_revision,
                current_root_hash: current_hash,
            };
            let response_bytes = serialized_response_bytes(&response)?;
            self.admit_egress(
                account_locator,
                network_pseudonym,
                response_bytes,
                now_epoch_ms,
            )?;
            return Ok(response);
        }
        ensure_children_exist(
            &transaction,
            account_locator,
            &request.replacement.child_object_ids,
        )?;
        let previous_root_bytes = if let Some(root) = &current {
            let children_json =
                serde_json::to_string(&root.value.child_object_ids).map_err(|error| {
                    StoreError::internal(format!("encode stored root children: {error}"))
                })?;
            let ciphertext_bytes = root
                .value
                .ciphertext()
                .map_err(|error| {
                    StoreError::internal(format!("decode stored root ciphertext: {error}"))
                })?
                .len() as u64;
            root_storage_bytes(ciphertext_bytes, &children_json)
        } else {
            0
        };
        if let Err(error) = replace_account_root_usage(
            &transaction,
            account_locator,
            previous_root_bytes,
            replacement_storage_bytes,
            self.inner.config.global_storage_limit_bytes,
        ) {
            if error.code == AcsErrorCode::QuotaExceeded {
                self.inner
                    .counters
                    .quota_rejections
                    .fetch_add(1, Ordering::Relaxed);
            }
            if error.code == AcsErrorCode::ReadOnly {
                transaction
                    .execute(
                        "UPDATE service_state SET mode = 'read_only' WHERE singleton = 1",
                        [],
                    )
                    .map_err(StoreError::sqlite)?;
                transaction.commit().map_err(StoreError::sqlite)?;
                self.note_durable_write(now_epoch_ms);
            }
            return Err(error);
        }
        let revision = current_revision + 1;
        transaction
            .execute(
                "INSERT INTO roots(account_locator, revision, root_hash, ciphertext_sha256, ciphertext_bytes, ciphertext, children_json, updated_at_epoch_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(account_locator) DO UPDATE SET revision = excluded.revision, root_hash = excluded.root_hash, ciphertext_sha256 = excluded.ciphertext_sha256, ciphertext_bytes = excluded.ciphertext_bytes, ciphertext = excluded.ciphertext, children_json = excluded.children_json, updated_at_epoch_ms = excluded.updated_at_epoch_ms",
                params![account_locator, revision, root_hash, request.replacement.ciphertext_sha256, ciphertext_bytes, ciphertext, children_json, now_epoch_ms],
            )
            .map_err(StoreError::sqlite)?;
        let sequence: u64 = transaction
            .query_row(
                "UPDATE accounts SET event_sequence = event_sequence + 1, updated_at_epoch_ms = ?2 WHERE account_locator = ?1 RETURNING event_sequence",
                params![account_locator, now_epoch_ms],
                |row| row.get(0),
            )
            .map_err(StoreError::sqlite)?;
        transaction
            .execute(
                "INSERT INTO root_events(account_locator, sequence, root_revision, root_hash, created_at_epoch_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![account_locator, sequence, revision, root_hash, now_epoch_ms],
            )
            .map_err(StoreError::sqlite)?;
        transaction
            .execute(
                "DELETE FROM root_events WHERE account_locator = ?1 AND sequence <= ?2",
                params![
                    account_locator,
                    sequence.saturating_sub(self.inner.config.event_retention)
                ],
            )
            .map_err(StoreError::sqlite)?;
        let root = read_root_row(&transaction, account_locator)?
            .ok_or_else(|| StoreError::internal("committed root disappeared"))?;
        let response = AcsCompareAndSwapRootResponse::Committed { root };
        let response_bytes = serialized_response_bytes(&response)?;
        self.admit_egress(
            account_locator,
            network_pseudonym,
            response_bytes,
            now_epoch_ms,
        )?;
        transaction.commit().map_err(StoreError::sqlite)?;
        self.inner.counters.root_cas.fetch_add(1, Ordering::Relaxed);
        self.note_durable_write(now_epoch_ms);
        let event = AcsSseEvent::RootChanged {
            sequence,
            root_revision: revision,
            root_hash: root_hash.clone(),
        };
        let _ = self.inner.events.send(RootEventRecord {
            account_locator: account_locator.to_string(),
            event,
        });
        Ok(response)
    }

    pub(crate) fn create_sse_ticket(
        &self,
        account_locator: &str,
        last_event_sequence: Option<u64>,
        network_pseudonym: &str,
        now_epoch_ms: i64,
    ) -> StoreResult<AcsCreateSseTicketResponse> {
        let ticket = random_token(32)?;
        let ticket_hash = sha256_hex(ticket.as_bytes());
        let expires_at_epoch_ms = now_epoch_ms + ACS_SSE_TICKET_TTL_MS;
        let connection = self.connection()?;
        ensure_account_readable(&connection, account_locator)?;
        connection
            .execute(
                "DELETE FROM sse_tickets WHERE expires_at_epoch_ms < ?1",
                [now_epoch_ms],
            )
            .map_err(StoreError::sqlite)?;
        connection
            .execute(
                "INSERT INTO sse_tickets(ticket_hash, account_locator, network_pseudonym, last_event_sequence, expires_at_epoch_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![ticket_hash, account_locator, network_pseudonym, last_event_sequence, expires_at_epoch_ms],
            )
            .map_err(StoreError::sqlite)?;
        let events_url = format!("{}?ticket={ticket}", acs_events_path());
        Ok(AcsCreateSseTicketResponse {
            contract_id: ACS_CONTRACT_ID.to_string(),
            ticket,
            expires_at_epoch_ms,
            events_url,
        })
    }

    pub(crate) fn consume_sse_ticket(
        &self,
        ticket: &str,
        network_pseudonym: &str,
        now_epoch_ms: i64,
    ) -> StoreResult<ConsumedTicket> {
        let ticket_hash = sha256_hex(ticket.as_bytes());
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::sqlite)?;
        let consumed = transaction
            .query_row(
                "DELETE FROM sse_tickets WHERE ticket_hash = ?1 RETURNING account_locator, network_pseudonym, last_event_sequence, expires_at_epoch_ms",
                [ticket_hash],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<u64>>(2)?, row.get::<_, i64>(3)?)),
            )
            .optional()
            .map_err(StoreError::sqlite)?
            .ok_or_else(|| StoreError::new(AcsErrorCode::Unauthorized, "SSE ticket is invalid or already used"))?;
        transaction.commit().map_err(StoreError::sqlite)?;
        if consumed.1 != network_pseudonym || consumed.3 < now_epoch_ms {
            return Err(StoreError::new(
                AcsErrorCode::Unauthorized,
                "SSE ticket is expired or belongs to another network",
            ));
        }
        Ok(ConsumedTicket {
            account_locator: consumed.0,
            last_event_sequence: consumed.2,
        })
    }

    pub(crate) fn initial_sse_events(
        &self,
        account_locator: &str,
        cursor: Option<u64>,
    ) -> StoreResult<Vec<AcsSseEvent>> {
        let connection = self.read_connection()?;
        ensure_account_readable(&connection, account_locator)?;
        let (current_sequence, current_revision, current_hash) =
            current_event_state(&connection, account_locator)?;
        let Some(cursor) = cursor else {
            return Ok(vec![AcsSseEvent::Ready {
                sequence: current_sequence,
                root_revision: current_revision,
                root_hash: current_hash,
            }]);
        };
        if cursor > current_sequence {
            return Ok(vec![AcsSseEvent::Reset {
                sequence: current_sequence,
                root_revision: current_revision,
                root_hash: current_hash,
            }]);
        }
        let minimum_retained = connection
            .query_row(
                "SELECT MIN(sequence) FROM root_events WHERE account_locator = ?1",
                [account_locator],
                |row| row.get::<_, Option<u64>>(0),
            )
            .map_err(StoreError::sqlite)?;
        if minimum_retained
            .is_some_and(|minimum| cursor < minimum && !(cursor == 0 && minimum == 1))
        {
            return Ok(vec![AcsSseEvent::Reset {
                sequence: current_sequence,
                root_revision: current_revision,
                root_hash: current_hash,
            }]);
        }
        let cursor_state = event_state_at(&connection, account_locator, cursor)?;
        let mut events = vec![AcsSseEvent::Ready {
            sequence: cursor,
            root_revision: cursor_state.0,
            root_hash: cursor_state.1,
        }];
        let mut statement = connection
            .prepare("SELECT sequence, root_revision, root_hash FROM root_events WHERE account_locator = ?1 AND sequence > ?2 ORDER BY sequence")
            .map_err(StoreError::sqlite)?;
        events.extend(
            statement
                .query_map(params![account_locator, cursor], |row| {
                    Ok(AcsSseEvent::RootChanged {
                        sequence: row.get(0)?,
                        root_revision: row.get(1)?,
                        root_hash: row.get(2)?,
                    })
                })
                .map_err(StoreError::sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(StoreError::sqlite)?,
        );
        Ok(events)
    }

    pub(crate) fn heartbeat_event(&self, account_locator: &str) -> StoreResult<AcsSseEvent> {
        let connection = self.read_connection()?;
        let (sequence, root_revision, root_hash) =
            current_event_state(&connection, account_locator)?;
        Ok(AcsSseEvent::Heartbeat {
            sequence,
            root_revision,
            root_hash,
        })
    }

    pub(crate) fn subscribe(&self) -> broadcast::Receiver<RootEventRecord> {
        self.inner.events.subscribe()
    }

    pub(crate) fn begin_sse_connection(
        &self,
        account_locator: &str,
        network_pseudonym: &str,
    ) -> StoreResult<()> {
        let mut limits = self
            .inner
            .limits
            .lock()
            .map_err(|_| StoreError::internal("cloud limit mutex is poisoned"))?;
        let current_global = self
            .inner
            .counters
            .current_sse_connections
            .load(Ordering::Relaxed);
        let current_account = limits
            .account_sse
            .get(account_locator)
            .copied()
            .unwrap_or(0);
        let current_network = limits
            .network_sse
            .get(network_pseudonym)
            .copied()
            .unwrap_or(0);
        let rejected_gate = if current_global >= self.inner.config.global_sse_limit {
            Some(AcsRateLimitGate::GlobalSseConnections)
        } else if current_account >= self.inner.config.account_sse_limit {
            Some(AcsRateLimitGate::AccountSseConnections)
        } else if current_network >= self.inner.config.network_sse_limit {
            Some(AcsRateLimitGate::NetworkSseConnections)
        } else {
            None
        };
        if let Some(gate) = rejected_gate {
            drop(limits);
            return self.rate_limited(gate, "SSE connection limit is exceeded", 30_000);
        }
        *limits
            .account_sse
            .entry(account_locator.to_string())
            .or_default() += 1;
        *limits
            .network_sse
            .entry(network_pseudonym.to_string())
            .or_default() += 1;
        let current = self
            .inner
            .counters
            .current_sse_connections
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        self.inner
            .counters
            .peak_sse_connections
            .fetch_max(current, Ordering::Relaxed);
        Ok(())
    }

    pub(crate) fn end_sse_connection(&self, account_locator: &str, network_pseudonym: &str) {
        if let Ok(mut limits) = self.inner.limits.lock() {
            decrement_scope(&mut limits.account_sse, account_locator);
            decrement_scope(&mut limits.network_sse, network_pseudonym);
        }
        self.inner
            .counters
            .current_sse_connections
            .fetch_sub(1, Ordering::Relaxed);
    }

    pub fn set_service_mode(&self, mode: AccountMode, now_epoch_ms: i64) -> StoreResult<()> {
        let connection = self.connection()?;
        connection
            .execute(
                "UPDATE service_state SET mode = ?1 WHERE singleton = 1",
                [mode.as_str()],
            )
            .map_err(StoreError::sqlite)?;
        self.note_durable_write(now_epoch_ms);
        Ok(())
    }

    pub fn resume_writes(&self, now_epoch_ms: i64) -> StoreResult<ResumeWritesReport> {
        self.resume_writes_inner(false, "checked operator write resume", now_epoch_ms)
    }

    pub fn force_resume_writes(
        &self,
        reason: &str,
        now_epoch_ms: i64,
    ) -> StoreResult<ResumeWritesReport> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(StoreError::internal(
                "forced write resume requires a non-empty operator reason",
            ));
        }
        self.resume_writes_inner(true, reason, now_epoch_ms)
    }

    fn resume_writes_inner(
        &self,
        forced: bool,
        reason: &str,
        now_epoch_ms: i64,
    ) -> StoreResult<ResumeWritesReport> {
        let connection = self.connection()?;
        let mode = AccountMode::parse(
            &connection
                .query_row(
                    "SELECT mode FROM service_state WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(StoreError::sqlite)?,
        )?;
        if mode == AccountMode::Suspended {
            return Err(StoreError::internal(
                "suspended service cannot resume writes; set it read-only first",
            ));
        }
        let stored_bytes = connection
            .query_row(
                "SELECT COALESCE(SUM(stored_bytes), 0) FROM accounts",
                [],
                |row| row.get::<_, u64>(0),
            )
            .map_err(StoreError::sqlite)?;
        let storage_headroom_bytes = self
            .inner
            .config
            .global_storage_limit_bytes
            .saturating_sub(stored_bytes);
        let filesystem_free_bytes = fs2::available_space(self.inner.layout.root()).unwrap_or(0);
        if !forced {
            let integrity: String = connection
                .query_row("PRAGMA integrity_check", [], |row| row.get(0))
                .map_err(StoreError::sqlite)?;
            if integrity != "ok" {
                return Err(StoreError::internal(format!(
                    "write resume refused: SQLite integrity check failed: {integrity}"
                )));
            }
            if storage_headroom_bytes < self.inner.config.write_resume_headroom_bytes {
                return Err(StoreError::internal(format!(
                    "write resume refused: {storage_headroom_bytes} bytes of ACS quota headroom is below required {}",
                    self.inner.config.write_resume_headroom_bytes
                )));
            }
            if filesystem_free_bytes < self.inner.config.write_resume_min_filesystem_free_bytes {
                return Err(StoreError::internal(format!(
                    "write resume refused: {filesystem_free_bytes} filesystem bytes free is below required {}",
                    self.inner.config.write_resume_min_filesystem_free_bytes
                )));
            }
        }
        let resumed = mode != AccountMode::Normal;
        connection
            .execute(
                "UPDATE service_state SET mode = 'normal' WHERE singleton = 1",
                [],
            )
            .map_err(StoreError::sqlite)?;
        connection
            .execute(
                "INSERT INTO operator_audit(action, reason, created_at_epoch_ms) VALUES (?1, ?2, ?3)",
                params![
                    if forced {
                        "force_resume_writes"
                    } else {
                        "resume_writes"
                    },
                    reason,
                    now_epoch_ms
                ],
            )
            .map_err(StoreError::sqlite)?;
        self.note_durable_write(now_epoch_ms);
        Ok(ResumeWritesReport {
            resumed,
            forced,
            stored_bytes,
            storage_headroom_bytes,
            filesystem_free_bytes,
        })
    }

    pub fn set_account_mode(
        &self,
        account_locator: &str,
        mode: AccountMode,
        now_epoch_ms: i64,
    ) -> StoreResult<()> {
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE accounts SET mode = ?2, updated_at_epoch_ms = ?3 WHERE account_locator = ?1",
                params![account_locator, mode.as_str(), now_epoch_ms],
            )
            .map_err(StoreError::sqlite)?;
        if changed == 0 {
            return Err(StoreError::new(
                AcsErrorCode::NotFound,
                "cloud account does not exist",
            ));
        }
        self.note_durable_write(now_epoch_ms);
        Ok(())
    }

    pub fn set_account_quota(
        &self,
        account_locator: &str,
        quota_bytes: u64,
        now_epoch_ms: i64,
    ) -> StoreResult<()> {
        let connection = self.connection()?;
        let changed = connection
            .execute(
                "UPDATE accounts SET quota_bytes = ?2, quota_class = 'operator', updated_at_epoch_ms = ?3 WHERE account_locator = ?1 AND stored_bytes <= ?2",
                params![account_locator, quota_bytes, now_epoch_ms],
            )
            .map_err(StoreError::sqlite)?;
        if changed == 0 {
            return Err(StoreError::new(
                AcsErrorCode::Conflict,
                "account is missing or currently uses more than the requested quota",
            ));
        }
        self.note_durable_write(now_epoch_ms);
        Ok(())
    }

    pub fn delete_account(&self, account_locator: &str) -> StoreResult<()> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::sqlite)?;
        let changed = transaction
            .execute(
                "DELETE FROM accounts WHERE account_locator = ?1",
                [account_locator],
            )
            .map_err(StoreError::sqlite)?;
        if changed == 0 {
            return Err(StoreError::new(
                AcsErrorCode::NotFound,
                "cloud account does not exist",
            ));
        }
        transaction.commit().map_err(StoreError::sqlite)?;
        Ok(())
    }

    pub fn run_gc(&self, now_epoch_ms: i64, grace_ms: i64) -> StoreResult<GcReport> {
        let _reclamation_lock = self.inner.layout.acquire_reclamation_lock()?;
        let started = Instant::now();
        let mut report = self.run_gc_inner(now_epoch_ms, grace_ms)?;
        report.total_elapsed_ms = elapsed_ms(started);
        let counters = &self.inner.counters;
        counters.gc_runs.fetch_add(1, Ordering::Relaxed);
        counters
            .gc_last_database_pause_ms
            .store(report.database_pause_ms, Ordering::Relaxed);
        counters
            .gc_peak_database_pause_ms
            .fetch_max(report.database_pause_ms, Ordering::Relaxed);
        counters
            .gc_total_database_work_ms
            .fetch_add(report.database_work_ms, Ordering::Relaxed);
        counters
            .gc_last_elapsed_ms
            .store(report.total_elapsed_ms, Ordering::Relaxed);
        counters
            .gc_peak_elapsed_ms
            .fetch_max(report.total_elapsed_ms, Ordering::Relaxed);
        Ok(report)
    }

    fn run_gc_inner(&self, now_epoch_ms: i64, grace_ms: i64) -> StoreResult<GcReport> {
        self.run_gc_inner_with_progress(now_epoch_ms, grace_ms, |_| {})
    }

    fn run_gc_inner_with_progress(
        &self,
        now_epoch_ms: i64,
        grace_ms: i64,
        mut account_complete: impl FnMut(&str),
    ) -> StoreResult<GcReport> {
        let cutoff = now_epoch_ms - grace_ms;
        let accounts = {
            let connection = self.read_connection()?;
            query_strings(
                &connection,
                "SELECT account_locator FROM accounts ORDER BY account_locator",
                [],
            )?
        };
        let mut report = GcReport {
            marked_objects: 0,
            deleted_objects: 0,
            deleted_ciphertext_bytes: 0,
            deleted_blob_files: 0,
            database_pause_ms: 0,
            database_work_ms: 0,
            total_elapsed_ms: 0,
        };
        let mut blob_keys = Vec::new();
        for account in accounts {
            let mut connection = self.connection()?;
            let database_pause_started = Instant::now();
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(StoreError::sqlite)?;
            let object_rows = load_object_graph(&transaction, &account)?;
            let root_children = transaction
                .query_row(
                    "SELECT children_json FROM roots WHERE account_locator = ?1",
                    [&account],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(StoreError::sqlite)?
                .map(|json| decode_children(&json))
                .transpose()?
                .unwrap_or_default();
            let mut marked = BTreeSet::new();
            let mut pending = VecDeque::from(root_children);
            while let Some(object_id) = pending.pop_front() {
                if !marked.insert(object_id.clone()) {
                    continue;
                }
                if let Some(object) = object_rows.get(&object_id) {
                    pending.extend(object.children.iter().cloned());
                }
            }
            report.marked_objects += marked.len() as u64;
            for (object_id, object) in object_rows {
                if marked.contains(&object_id) || object.created_at_epoch_ms > cutoff {
                    continue;
                }
                transaction
                    .execute(
                        "DELETE FROM objects WHERE account_locator = ?1 AND object_id = ?2",
                        params![account, object_id],
                    )
                    .map_err(StoreError::sqlite)?;
                release_account_usage(&transaction, &account, object.storage_bytes, 1)?;
                report.deleted_objects += 1;
                report.deleted_ciphertext_bytes += object.ciphertext_bytes;
                if let Some(key) = object.blob_storage_key {
                    blob_keys.push(key);
                }
            }
            transaction.commit().map_err(StoreError::sqlite)?;
            let database_work_ms = elapsed_ms(database_pause_started);
            report.database_pause_ms = report.database_pause_ms.max(database_work_ms);
            report.database_work_ms = report.database_work_ms.saturating_add(database_work_ms);
            drop(connection);
            account_complete(&account);
            thread::yield_now();
        }
        let referenced_blob_keys = {
            let connection = self.read_connection()?;
            query_strings(
                &connection,
                "SELECT blob_storage_key FROM objects WHERE blob_storage_key IS NOT NULL",
                [],
            )?
            .into_iter()
            .collect::<BTreeSet<_>>()
        };
        for key in blob_keys {
            if remove_file_if_present(&self.blob_path(&key))? {
                report.deleted_blob_files += 1;
            }
        }
        for path in blob_files(&self.inner.blob_root)? {
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let referenced = file_name
                .strip_suffix(".blob")
                .is_some_and(|key| referenced_blob_keys.contains(key));
            if referenced || file_modified_epoch_ms(&path).is_some_and(|modified| modified > cutoff)
            {
                continue;
            }
            let bytes = file_size(&path);
            if remove_file_if_present(&path)? {
                report.deleted_blob_files += 1;
                report.deleted_ciphertext_bytes += bytes;
            }
        }
        if report.deleted_objects > 0 {
            self.note_durable_write(now_epoch_ms);
        }
        Ok(report)
    }

    pub(crate) fn status(&self, now_epoch_ms: i64) -> StoreResult<AcsStatusResponse> {
        let connection = self.read_connection()?;
        let mode = AccountMode::parse(
            &connection
                .query_row(
                    "SELECT mode FROM service_state WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(StoreError::sqlite)?,
        )?;
        let gauges = connection
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(stored_bytes), 0), COALESCE(SUM(object_count), 0) FROM accounts",
                [],
                |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?)),
            )
            .map_err(StoreError::sqlite)?;
        let placements = connection
            .query_row(
                "SELECT COALESCE(SUM(CASE WHEN inline_ciphertext IS NOT NULL THEN ciphertext_bytes ELSE 0 END), 0), COALESCE(SUM(CASE WHEN blob_storage_key IS NOT NULL THEN ciphertext_bytes ELSE 0 END), 0), COALESCE(SUM(CASE WHEN state = 'pending' THEN 1 ELSE 0 END), 0), COALESCE(SUM(CASE WHEN state = 'pending' THEN ciphertext_bytes ELSE 0 END), 0) FROM objects",
                [],
                |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?, row.get::<_, u64>(3)?)),
            )
            .map_err(StoreError::sqlite)?;
        let root_bytes = connection
            .query_row(
                "SELECT COALESCE(SUM(ciphertext_bytes), 0) FROM roots",
                [],
                |row| row.get::<_, u64>(0),
            )
            .map_err(StoreError::sqlite)?;
        let retained_events = connection
            .query_row("SELECT COUNT(*) FROM root_events", [], |row| {
                row.get::<_, u64>(0)
            })
            .map_err(StoreError::sqlite)?;
        let backup = connection
            .query_row(
                "SELECT runs, failures, first_required_at_epoch_ms, last_completed_at_epoch_ms, last_elapsed_ms, peak_elapsed_ms, last_sqlite_snapshot_ms, peak_sqlite_snapshot_ms, last_wal_growth_bytes, peak_wal_growth_bytes, last_linked_blob_count, peak_linked_blob_count, last_linked_blob_bytes, peak_linked_blob_bytes FROM backup_state WHERE singleton = 1",
                [],
                |row| {
                    Ok(BackupStatus {
                        runs: row.get(0)?,
                        failures: row.get(1)?,
                        first_required_at_epoch_ms: row.get(2)?,
                        last_completed_at_epoch_ms: row.get(3)?,
                        last_elapsed_ms: row.get(4)?,
                        peak_elapsed_ms: row.get(5)?,
                        last_sqlite_snapshot_ms: row.get(6)?,
                        peak_sqlite_snapshot_ms: row.get(7)?,
                        last_wal_growth_bytes: row.get(8)?,
                        peak_wal_growth_bytes: row.get(9)?,
                        last_linked_blob_count: row.get(10)?,
                        peak_linked_blob_count: row.get(11)?,
                        last_linked_blob_bytes: row.get(12)?,
                        peak_linked_blob_bytes: row.get(13)?,
                    })
                },
            )
            .map_err(StoreError::sqlite)?;
        let creation_bucket_rows = {
            let mut statement = connection
                .prepare(
                    "SELECT bucket_key, available_quanta, updated_at_epoch_ms FROM durable_token_buckets",
                )
                .map_err(StoreError::sqlite)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })
                .map_err(StoreError::sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(StoreError::sqlite)?;
            rows
        };
        let mut global_creation_consumed = 0;
        let mut network_creation_peak_consumed = 0;
        for (key, available_quanta, updated_at_epoch_ms) in creation_bucket_rows {
            let bucket = TokenBucket {
                available_quanta: u128::from(available_quanta),
                updated_at_epoch_ms,
                initialized: true,
            };
            if key == "account_creation_global" {
                global_creation_consumed =
                    bucket.consumed(self.inner.config.creation_global_bucket, now_epoch_ms);
            } else if key.starts_with("account_creation_network:") {
                network_creation_peak_consumed = network_creation_peak_consumed
                    .max(bucket.consumed(self.inner.config.creation_network_bucket, now_epoch_ms));
            }
        }
        let database_bytes = file_size(&self.inner.database_path);
        let wal_bytes = file_size(&self.inner.database_path.with_extension("sqlite3-wal"));
        let filesystem_free_bytes = fs2::available_space(self.inner.layout.root()).unwrap_or(0);
        let counters = &self.inner.counters;
        let limits = self
            .inner
            .limits
            .lock()
            .map_err(|_| StoreError::internal("cloud limit mutex is poisoned"))?;
        let current_global_operation = limits
            .global_operations
            .consumed(self.inner.config.global_operation_bucket, now_epoch_ms);
        let current_network_operation_peak = limits
            .network_operations
            .values()
            .map(|bucket| bucket.consumed(self.inner.config.network_operation_bucket, now_epoch_ms))
            .max()
            .unwrap_or(0);
        let current_account_operation_peak = limits
            .account_operations
            .values()
            .map(|bucket| bucket.consumed(self.inner.config.account_operation_bucket, now_epoch_ms))
            .max()
            .unwrap_or(0);
        let current_global_ingress = limits
            .global_ingress_bytes
            .consumed(self.inner.config.global_ingress_bucket, now_epoch_ms);
        let current_network_ingress_peak = limits
            .network_ingress_bytes
            .values()
            .map(|bucket| bucket.consumed(self.inner.config.network_ingress_bucket, now_epoch_ms))
            .max()
            .unwrap_or(0);
        let current_account_ingress_peak = limits
            .account_ingress_bytes
            .values()
            .map(|bucket| bucket.consumed(self.inner.config.account_ingress_bucket, now_epoch_ms))
            .max()
            .unwrap_or(0);
        let current_global_egress = limits
            .global_egress_bytes
            .consumed(self.inner.config.global_egress_bucket, now_epoch_ms);
        let current_network_egress_peak = limits
            .network_egress_bytes
            .values()
            .map(|bucket| bucket.consumed(self.inner.config.network_egress_bucket, now_epoch_ms))
            .max()
            .unwrap_or(0);
        let current_account_egress_peak = limits
            .account_egress_bytes
            .values()
            .map(|bucket| bucket.consumed(self.inner.config.account_egress_bucket, now_epoch_ms))
            .max()
            .unwrap_or(0);
        let backup_age_seconds = now_epoch_ms
            .saturating_sub(
                backup
                    .last_completed_at_epoch_ms
                    .unwrap_or(backup.first_required_at_epoch_ms),
            )
            .max(0) as u64
            / 1_000;
        let mut metrics = vec![
            gauge("account_count", gauges.0, None, None, None),
            gauge(
                "stored_bytes",
                gauges.1,
                Some(self.inner.config.stored_bytes_warning),
                Some(self.inner.config.stored_bytes_critical),
                Some(self.inner.config.global_storage_limit_bytes),
            ),
            gauge("object_count", gauges.2, None, None, None),
            gauge("inline_bytes", placements.0, None, None, None),
            gauge("filesystem_blob_bytes", placements.1, None, None, None),
            gauge("root_bytes", root_bytes, None, None, None),
            gauge("pending_uploads", placements.2, None, None, None),
            gauge("pending_upload_bytes", placements.3, None, None, None),
            gauge("retained_sse_events", retained_events, None, None, None),
            gauge("sqlite_bytes", database_bytes, None, None, None),
            gauge("wal_bytes", wal_bytes, None, None, None),
            low_gauge(
                "filesystem_free_bytes",
                filesystem_free_bytes,
                Some(self.inner.config.filesystem_free_bytes_warning),
                Some(self.inner.config.filesystem_free_bytes_critical),
                Some(0),
            ),
            gauge(
                "current_sse_connections",
                counters.current_sse_connections.load(Ordering::Relaxed),
                Some(self.inner.config.sse_connections_warning),
                Some(self.inner.config.sse_connections_critical),
                Some(self.inner.config.global_sse_limit),
            ),
            gauge(
                "peak_sse_connections",
                counters.peak_sse_connections.load(Ordering::Relaxed),
                None,
                None,
                Some(self.inner.config.global_sse_limit),
            ),
            gauge(
                "gc_runs",
                counters.gc_runs.load(Ordering::Relaxed),
                None,
                None,
                None,
            ),
            observation_metric(
                "gc_database_pause_ms",
                counters.gc_last_database_pause_ms.load(Ordering::Relaxed),
                counters.gc_peak_database_pause_ms.load(Ordering::Relaxed),
                self.inner.config.gc_database_pause_ms_warning,
                self.inner.config.gc_database_pause_ms_critical,
            ),
            gauge(
                "gc_database_work_ms_total",
                counters.gc_total_database_work_ms.load(Ordering::Relaxed),
                None,
                None,
                None,
            ),
            observation_metric(
                "gc_elapsed_ms",
                counters.gc_last_elapsed_ms.load(Ordering::Relaxed),
                counters.gc_peak_elapsed_ms.load(Ordering::Relaxed),
                self.inner.config.gc_elapsed_ms_warning,
                self.inner.config.gc_elapsed_ms_critical,
            ),
            gauge("backup_runs", backup.runs, None, None, None),
            gauge("backup_failures", backup.failures, None, None, None),
            observation_metric(
                "backup_age_seconds",
                backup_age_seconds,
                backup_age_seconds,
                self.inner.config.backup_age_seconds_warning,
                self.inner.config.backup_age_seconds_critical,
            ),
            observation_metric(
                "backup_elapsed_ms",
                backup.last_elapsed_ms,
                backup.peak_elapsed_ms,
                self.inner.config.backup_elapsed_ms_warning,
                self.inner.config.backup_elapsed_ms_critical,
            ),
            observation_metric(
                "backup_sqlite_snapshot_ms",
                backup.last_sqlite_snapshot_ms,
                backup.peak_sqlite_snapshot_ms,
                self.inner.config.backup_sqlite_snapshot_ms_warning,
                self.inner.config.backup_sqlite_snapshot_ms_critical,
            ),
            observation_metric(
                "backup_wal_growth_bytes",
                backup.last_wal_growth_bytes,
                backup.peak_wal_growth_bytes,
                self.inner.config.backup_wal_growth_bytes_warning,
                self.inner.config.backup_wal_growth_bytes_critical,
            ),
            observation_metric(
                "backup_linked_blob_count",
                backup.last_linked_blob_count,
                backup.peak_linked_blob_count,
                self.inner.config.backup_linked_blob_count_warning,
                self.inner.config.backup_linked_blob_count_critical,
            ),
            observation_metric(
                "backup_linked_blob_bytes",
                backup.last_linked_blob_bytes,
                backup.peak_linked_blob_bytes,
                self.inner.config.backup_linked_blob_bytes_warning,
                self.inner.config.backup_linked_blob_bytes_critical,
            ),
            bucket_gauge(
                "global_operations_per_minute",
                current_global_operation,
                self.inner.config.global_operation_bucket,
            ),
            bucket_gauge(
                "max_network_operations_per_minute",
                current_network_operation_peak,
                self.inner.config.network_operation_bucket,
            ),
            bucket_gauge(
                "max_account_operations_per_minute",
                current_account_operation_peak,
                self.inner.config.account_operation_bucket,
            ),
            bucket_gauge(
                "global_ingress_bytes_per_minute",
                current_global_ingress,
                self.inner.config.global_ingress_bucket,
            ),
            bucket_gauge(
                "max_network_ingress_bytes_per_minute",
                current_network_ingress_peak,
                self.inner.config.network_ingress_bucket,
            ),
            bucket_gauge(
                "max_account_ingress_bytes_per_minute",
                current_account_ingress_peak,
                self.inner.config.account_ingress_bucket,
            ),
            bucket_gauge(
                "global_egress_bytes_per_minute",
                current_global_egress,
                self.inner.config.global_egress_bucket,
            ),
            bucket_gauge(
                "max_network_egress_bytes_per_minute",
                current_network_egress_peak,
                self.inner.config.network_egress_bucket,
            ),
            bucket_gauge(
                "max_account_egress_bytes_per_minute",
                current_account_egress_peak,
                self.inner.config.account_egress_bucket,
            ),
            bucket_gauge(
                "max_network_account_creation_tokens_consumed",
                network_creation_peak_consumed,
                self.inner.config.creation_network_bucket,
            ),
            bucket_gauge(
                "global_account_creation_tokens_consumed",
                global_creation_consumed,
                self.inner.config.creation_global_bucket,
            ),
        ];
        for (id, counter) in [
            ("reads", &counters.reads),
            ("creates", &counters.creates),
            ("root_cas", &counters.root_cas),
            ("lists", &counters.lists),
            ("ingress_bytes", &counters.ingress_bytes),
            ("egress_bytes", &counters.egress_bytes),
            (
                "authentication_rejections",
                &counters.authentication_rejections,
            ),
            ("replay_rejections", &counters.replay_rejections),
            ("quota_rejections", &counters.quota_rejections),
            ("rate_limit_rejections", &counters.rate_limit_rejections),
            ("malformed_rejections", &counters.malformed_rejections),
            (
                "account_creation_attempts",
                &counters.account_creation_attempts,
            ),
            (
                "account_creation_successes",
                &counters.account_creation_successes,
            ),
            (
                "account_creation_rejections",
                &counters.account_creation_rejections,
            ),
            (
                "account_creation_network_rate_rejections",
                &counters.account_creation_network_rate_rejections,
            ),
            (
                "account_creation_global_rate_rejections",
                &counters.account_creation_global_rate_rejections,
            ),
            (
                "account_creation_other_rejections",
                &counters.account_creation_other_rejections,
            ),
            (
                "account_creation_idempotent_successes",
                &counters.account_creation_idempotent_successes,
            ),
        ] {
            metrics.push(gauge(id, counter.load(Ordering::Relaxed), None, None, None));
        }
        let creation_metrics = self
            .inner
            .creation_metrics
            .lock()
            .map_err(|_| StoreError::internal("cloud creation metric mutex is poisoned"))?;
        for (minutes, suffix) in [(1, "1m"), (5, "5m"), (60, "60m")] {
            for (id, metric, rejected) in [
                ("account_creation_attempts", CreationMetric::Attempt, false),
                ("account_creation_successes", CreationMetric::Success, false),
                (
                    "account_creation_network_rate_rejections",
                    CreationMetric::NetworkRateRejection,
                    true,
                ),
                (
                    "account_creation_global_rate_rejections",
                    CreationMetric::GlobalRateRejection,
                    true,
                ),
                (
                    "account_creation_other_rejections",
                    CreationMetric::OtherRejection,
                    true,
                ),
            ] {
                let current = creation_metrics.count(metric, now_epoch_ms, minutes);
                metrics.push(window_counter(
                    &format!("{id}_{suffix}"),
                    current,
                    (minutes * 60) as u64,
                    rejected,
                ));
            }
        }
        let mut statement = connection
            .prepare("SELECT account_locator, stored_bytes FROM accounts ORDER BY stored_bytes DESC, account_locator LIMIT 10")
            .map_err(StoreError::sqlite)?;
        let top_contributors = statement
            .query_map([], |row| {
                Ok(AcsStatusTopContributor {
                    metric_id: "stored_bytes".to_string(),
                    opaque_subject: row.get(0)?,
                    current: row.get(1)?,
                    window_seconds: None,
                })
            })
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        Ok(AcsStatusResponse {
            contract_id: ACS_CONTRACT_ID.to_string(),
            started_at_epoch_ms: self.inner.started_at_epoch_ms,
            server_time_epoch_ms: now_epoch_ms,
            mode: mode.service_mode(),
            schema_version: SCHEMA_VERSION,
            database_healthy: true,
            last_durable_read_epoch_ms: atomic_epoch(&self.inner.last_durable_read_epoch_ms),
            last_durable_write_epoch_ms: atomic_epoch(&self.inner.last_durable_write_epoch_ms),
            metrics,
            top_contributors,
        })
    }

    pub(crate) fn health(&self, now_epoch_ms: i64) -> StoreResult<AcsHealthResponse> {
        let connection = self.read_connection()?;
        let mode = AccountMode::parse(
            &connection
                .query_row(
                    "SELECT mode FROM service_state WHERE singleton = 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .map_err(StoreError::sqlite)?,
        )?;
        Ok(AcsHealthResponse {
            contract_id: ACS_CONTRACT_ID.to_string(),
            server_time_epoch_ms: now_epoch_ms,
            mode: mode.service_mode(),
            database_healthy: true,
        })
    }

    fn reconcile_pending_blobs(&self) -> StoreResult<()> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT account_locator, object_id, ciphertext_sha256, ciphertext_bytes, children_json, blob_storage_key FROM objects WHERE state = 'pending'")
            .map_err(StoreError::sqlite)?;
        let pending = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(StoreError::sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::sqlite)?;
        drop(statement);
        drop(connection);
        for (account, object_id, expected_hash, expected_bytes, children_json, key) in pending {
            let path = self.blob_path(&key);
            let valid = fs::read(&path).ok().is_some_and(|bytes| {
                bytes.len() as u64 == expected_bytes && sha256_hex(&bytes) == expected_hash
            });
            let connection = self.connection()?;
            if valid {
                connection
                    .execute(
                        "UPDATE objects SET state = 'ready' WHERE account_locator = ?1 AND object_id = ?2 AND state = 'pending'",
                        params![account, object_id],
                    )
                    .map_err(StoreError::sqlite)?;
            } else {
                connection
                    .execute(
                        "DELETE FROM objects WHERE account_locator = ?1 AND object_id = ?2 AND state = 'pending'",
                        params![account, object_id],
                    )
                    .map_err(StoreError::sqlite)?;
                connection
                    .execute(
                        "UPDATE accounts SET stored_bytes = stored_bytes - ?2, object_count = object_count - 1 WHERE account_locator = ?1",
                        params![
                            account,
                            object_storage_bytes(&object_id, expected_bytes, &children_json)
                        ],
                    )
                    .map_err(StoreError::sqlite)?;
            }
        }
        Ok(())
    }

    fn write_blob(&self, key: &str, bytes: &[u8]) -> StoreResult<()> {
        let path = self.blob_path(key);
        let parent = path
            .parent()
            .ok_or_else(|| StoreError::internal("blob path has no parent"))?;
        fs::create_dir_all(parent).map_err(|error| StoreError::io("create blob shard", error))?;
        if path.exists() {
            let existing =
                fs::read(&path).map_err(|error| StoreError::io("read existing blob", error))?;
            if existing == bytes {
                return Ok(());
            }
            return Err(StoreError::internal("blob storage key collision"));
        }
        let temporary = parent.join(format!(".{key}.{}.tmp", random_token(8)?));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| StoreError::io("create temporary blob", error))?;
        file.write_all(bytes)
            .map_err(|error| StoreError::io("write temporary blob", error))?;
        file.sync_all()
            .map_err(|error| StoreError::io("sync temporary blob", error))?;
        fs::rename(&temporary, &path).map_err(|error| StoreError::io("install blob", error))?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| StoreError::io("sync blob directory", error))?;
        Ok(())
    }

    fn remove_pending_object(
        &self,
        account_locator: &str,
        object_id: &str,
        storage_bytes: u64,
    ) -> StoreResult<()> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::sqlite)?;
        transaction
            .execute(
                "DELETE FROM objects WHERE account_locator = ?1 AND object_id = ?2 AND state = 'pending'",
                params![account_locator, object_id],
            )
            .map_err(StoreError::sqlite)?;
        release_account_usage(&transaction, account_locator, storage_bytes, 1)?;
        transaction.commit().map_err(StoreError::sqlite)
    }

    fn read_placed_ciphertext(
        &self,
        inline: Option<Vec<u8>>,
        blob_storage_key: Option<&str>,
    ) -> StoreResult<Vec<u8>> {
        match (inline, blob_storage_key) {
            (Some(bytes), None) => Ok(bytes),
            (None, Some(key)) => fs::read(self.blob_path(key))
                .map_err(|error| StoreError::io("read cloud blob", error)),
            _ => Err(StoreError::internal("object has invalid storage placement")),
        }
    }

    fn blob_path(&self, key: &str) -> PathBuf {
        self.inner
            .blob_root
            .join(&key[..2])
            .join(format!("{key}.blob"))
    }

    fn note_durable_read(&self, now_epoch_ms: i64) {
        self.inner
            .last_durable_read_epoch_ms
            .store(now_epoch_ms, Ordering::Relaxed);
    }

    fn note_durable_write(&self, now_epoch_ms: i64) {
        self.inner
            .last_durable_write_epoch_ms
            .store(now_epoch_ms, Ordering::Relaxed);
    }
}

fn configure_database(connection: &Connection) -> StoreResult<()> {
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = FULL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(StoreError::sqlite)
}

fn configure_read_database(connection: &Connection) -> StoreResult<()> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(StoreError::sqlite)?;
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA query_only = ON;",
        )
        .map_err(StoreError::sqlite)
}

fn initialize_schema(connection: &Connection) -> StoreResult<()> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS metadata(schema_version INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS service_state(singleton INTEGER PRIMARY KEY CHECK(singleton = 1), mode TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS accounts(
                 account_locator TEXT PRIMARY KEY,
                 signing_key_id TEXT NOT NULL,
                 signing_public_key BLOB NOT NULL,
                 mode TEXT NOT NULL,
                 quota_class TEXT NOT NULL,
                 quota_bytes INTEGER NOT NULL,
                 object_limit INTEGER NOT NULL,
                 stored_bytes INTEGER NOT NULL,
                 object_count INTEGER NOT NULL,
                 event_sequence INTEGER NOT NULL,
                 creation_network_pseudonym TEXT NOT NULL,
                 created_at_epoch_ms INTEGER NOT NULL,
                 updated_at_epoch_ms INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS accounts_creation_network_time ON accounts(creation_network_pseudonym, created_at_epoch_ms);
             CREATE TABLE IF NOT EXISTS roots(
                 account_locator TEXT PRIMARY KEY REFERENCES accounts(account_locator) ON DELETE CASCADE,
                 revision INTEGER NOT NULL,
                 root_hash TEXT NOT NULL,
                 ciphertext_sha256 TEXT NOT NULL,
                 ciphertext_bytes INTEGER NOT NULL,
                 ciphertext BLOB NOT NULL,
                 children_json TEXT NOT NULL,
                 updated_at_epoch_ms INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS objects(
                 account_locator TEXT NOT NULL REFERENCES accounts(account_locator) ON DELETE CASCADE,
                 object_id TEXT NOT NULL,
                 authenticated_hash TEXT NOT NULL,
                 ciphertext_sha256 TEXT NOT NULL,
                 ciphertext_bytes INTEGER NOT NULL,
                 children_json TEXT NOT NULL,
                 state TEXT NOT NULL CHECK(state IN ('pending', 'ready')),
                 inline_ciphertext BLOB,
                 blob_storage_key TEXT,
                 created_at_epoch_ms INTEGER NOT NULL,
                 PRIMARY KEY(account_locator, object_id),
                 CHECK((inline_ciphertext IS NULL) != (blob_storage_key IS NULL))
             );
             CREATE INDEX IF NOT EXISTS objects_account_created ON objects(account_locator, created_at_epoch_ms);
             CREATE TABLE IF NOT EXISTS request_nonces(
                 account_locator TEXT NOT NULL,
                 signing_key_id TEXT NOT NULL,
                 nonce TEXT NOT NULL,
                 expires_at_epoch_ms INTEGER NOT NULL,
                 PRIMARY KEY(account_locator, signing_key_id, nonce)
             );
             CREATE INDEX IF NOT EXISTS request_nonces_expiry ON request_nonces(expires_at_epoch_ms);
             CREATE TABLE IF NOT EXISTS challenges(
                 challenge TEXT PRIMARY KEY,
                 network_pseudonym TEXT NOT NULL,
                 issued_at_epoch_ms INTEGER NOT NULL,
                 expires_at_epoch_ms INTEGER NOT NULL,
                 used INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS challenges_network ON challenges(network_pseudonym, expires_at_epoch_ms);
             CREATE INDEX IF NOT EXISTS challenges_expiry ON challenges(expires_at_epoch_ms);
             CREATE TABLE IF NOT EXISTS root_events(
                 account_locator TEXT NOT NULL REFERENCES accounts(account_locator) ON DELETE CASCADE,
                 sequence INTEGER NOT NULL,
                 root_revision INTEGER NOT NULL,
                 root_hash TEXT NOT NULL,
                 created_at_epoch_ms INTEGER NOT NULL,
                 PRIMARY KEY(account_locator, sequence)
             );
             CREATE TABLE IF NOT EXISTS sse_tickets(
                 ticket_hash TEXT PRIMARY KEY,
                 account_locator TEXT NOT NULL REFERENCES accounts(account_locator) ON DELETE CASCADE,
                 network_pseudonym TEXT NOT NULL,
                 last_event_sequence INTEGER,
                 expires_at_epoch_ms INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS durable_token_buckets(
                 bucket_key TEXT PRIMARY KEY,
                 available_quanta INTEGER NOT NULL,
                 updated_at_epoch_ms INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS backup_state(
                 singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                 runs INTEGER NOT NULL,
                 failures INTEGER NOT NULL,
                 first_required_at_epoch_ms INTEGER NOT NULL,
                 last_completed_at_epoch_ms INTEGER,
                 last_elapsed_ms INTEGER NOT NULL,
                 peak_elapsed_ms INTEGER NOT NULL,
                 last_sqlite_snapshot_ms INTEGER NOT NULL,
                 peak_sqlite_snapshot_ms INTEGER NOT NULL,
                 last_wal_growth_bytes INTEGER NOT NULL,
                 peak_wal_growth_bytes INTEGER NOT NULL,
                 last_linked_blob_count INTEGER NOT NULL,
                 peak_linked_blob_count INTEGER NOT NULL,
                 last_linked_blob_bytes INTEGER NOT NULL,
                 peak_linked_blob_bytes INTEGER NOT NULL,
                 last_snapshot_name TEXT
             );
             CREATE TABLE IF NOT EXISTS operator_audit(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 action TEXT NOT NULL,
                 reason TEXT NOT NULL,
                 created_at_epoch_ms INTEGER NOT NULL
             );",
        )
        .map_err(StoreError::sqlite)?;
    let version = connection
        .query_row("SELECT schema_version FROM metadata LIMIT 1", [], |row| {
            row.get::<_, u32>(0)
        })
        .optional()
        .map_err(StoreError::sqlite)?;
    match version {
        None => {
            connection
                .execute(
                    "INSERT INTO metadata(schema_version) VALUES (?1)",
                    [SCHEMA_VERSION],
                )
                .map_err(StoreError::sqlite)?;
        }
        Some(1 | 2) => {
            connection
                .execute("UPDATE metadata SET schema_version = ?1", [SCHEMA_VERSION])
                .map_err(StoreError::sqlite)?;
        }
        Some(version) if version == SCHEMA_VERSION => {}
        Some(version) => {
            return Err(StoreError::internal(format!(
                "unsupported ACS database schema {version}; expected {SCHEMA_VERSION}"
            )))
        }
    }
    connection
        .execute(
            "INSERT OR IGNORE INTO service_state(singleton, mode) VALUES (1, 'normal')",
            [],
        )
        .map_err(StoreError::sqlite)?;
    connection
        .execute(
            "INSERT OR IGNORE INTO backup_state(singleton, runs, failures, first_required_at_epoch_ms, last_elapsed_ms, peak_elapsed_ms, last_sqlite_snapshot_ms, peak_sqlite_snapshot_ms, last_wal_growth_bytes, peak_wal_growth_bytes, last_linked_blob_count, peak_linked_blob_count, last_linked_blob_bytes, peak_linked_blob_bytes) VALUES (1, 0, 0, unixepoch('subsec') * 1000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)",
            [],
        )
        .map_err(StoreError::sqlite)?;
    Ok(())
}

fn repair_storage_accounting(
    connection: &mut Connection,
    global_storage_limit_bytes: u64,
) -> StoreResult<()> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(StoreError::sqlite)?;
    let mut usage = query_strings(
        &transaction,
        "SELECT account_locator FROM accounts ORDER BY account_locator",
        [],
    )?
    .into_iter()
    .map(|account| (account, 0_u64))
    .collect::<BTreeMap<_, _>>();
    {
        let mut statement = transaction
            .prepare(
                "SELECT account_locator, object_id, ciphertext_bytes, children_json FROM objects",
            )
            .map_err(StoreError::sqlite)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(StoreError::sqlite)?;
        for row in rows {
            let (account, object_id, ciphertext_bytes, children_json) =
                row.map_err(StoreError::sqlite)?;
            let account_usage = usage
                .get_mut(&account)
                .ok_or_else(|| StoreError::internal("object belongs to a missing account"))?;
            *account_usage = account_usage.saturating_add(object_storage_bytes(
                &object_id,
                ciphertext_bytes,
                &children_json,
            ));
        }
    }
    {
        let mut statement = transaction
            .prepare("SELECT account_locator, ciphertext_bytes, children_json FROM roots")
            .map_err(StoreError::sqlite)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(StoreError::sqlite)?;
        for row in rows {
            let (account, ciphertext_bytes, children_json) = row.map_err(StoreError::sqlite)?;
            let account_usage = usage
                .get_mut(&account)
                .ok_or_else(|| StoreError::internal("root belongs to a missing account"))?;
            *account_usage =
                account_usage.saturating_add(root_storage_bytes(ciphertext_bytes, &children_json));
        }
    }
    for (account, storage_bytes) in &usage {
        transaction
            .execute(
                "UPDATE accounts SET stored_bytes = ?2 WHERE account_locator = ?1 AND stored_bytes != ?2",
                params![account, storage_bytes],
            )
            .map_err(StoreError::sqlite)?;
    }
    let total = usage.values().copied().fold(0_u64, u64::saturating_add);
    if total > global_storage_limit_bytes {
        transaction
            .execute(
                "UPDATE service_state SET mode = 'read_only' WHERE singleton = 1",
                [],
            )
            .map_err(StoreError::sqlite)?;
    }
    transaction.commit().map_err(StoreError::sqlite)
}

fn ensure_account_readable(connection: &Connection, account_locator: &str) -> StoreResult<()> {
    let service_mode = connection
        .query_row(
            "SELECT mode FROM service_state WHERE singleton = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(StoreError::sqlite)?;
    enforce_mode(AccountMode::parse(&service_mode)?, false, "service")?;
    let account_mode = connection
        .query_row(
            "SELECT mode FROM accounts WHERE account_locator = ?1",
            [account_locator],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StoreError::sqlite)?
        .ok_or_else(|| StoreError::new(AcsErrorCode::NotFound, "cloud account does not exist"))?;
    enforce_mode(AccountMode::parse(&account_mode)?, false, "account")
}

fn ensure_account_writeable(
    transaction: &Transaction<'_>,
    account_locator: &str,
) -> StoreResult<()> {
    ensure_service_writeable(transaction)?;
    let account_mode = transaction
        .query_row(
            "SELECT mode FROM accounts WHERE account_locator = ?1",
            [account_locator],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StoreError::sqlite)?
        .ok_or_else(|| StoreError::new(AcsErrorCode::NotFound, "cloud account does not exist"))?;
    enforce_mode(AccountMode::parse(&account_mode)?, true, "account")
}

fn ensure_service_writeable(transaction: &Transaction<'_>) -> StoreResult<()> {
    let service_mode = transaction
        .query_row(
            "SELECT mode FROM service_state WHERE singleton = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(StoreError::sqlite)?;
    enforce_mode(AccountMode::parse(&service_mode)?, true, "service")
}

fn enforce_mode(mode: AccountMode, write: bool, subject: &str) -> StoreResult<()> {
    match mode {
        AccountMode::Normal => Ok(()),
        AccountMode::ReadOnly if !write => Ok(()),
        AccountMode::ReadOnly => Err(StoreError::new(
            AcsErrorCode::ReadOnly,
            format!("{subject} is read-only"),
        )),
        AccountMode::Suspended => Err(StoreError::new(
            AcsErrorCode::AccountSuspended,
            format!("{subject} is suspended"),
        )),
    }
}

fn object_storage_bytes(object_id: &str, ciphertext_bytes: u64, children_json: &str) -> u64 {
    ciphertext_bytes
        .saturating_add(object_id.len() as u64)
        .saturating_add(children_json.len() as u64)
        .saturating_add(OBJECT_STORAGE_ACCOUNTING_OVERHEAD_BYTES)
}

fn root_storage_bytes(ciphertext_bytes: u64, children_json: &str) -> u64 {
    ciphertext_bytes
        .saturating_add(children_json.len() as u64)
        .saturating_add(ROOT_STORAGE_ACCOUNTING_OVERHEAD_BYTES)
}

fn serialized_response_bytes(value: &impl Serialize) -> StoreResult<u64> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .map_err(|error| StoreError::internal(format!("encode cloud response: {error}")))
}

fn reserve_account_usage(
    transaction: &Transaction<'_>,
    account_locator: &str,
    ciphertext_bytes: u64,
    global_limit: u64,
) -> StoreResult<()> {
    let account = transaction
        .query_row(
            "SELECT stored_bytes, object_count, quota_bytes, object_limit FROM accounts WHERE account_locator = ?1",
            [account_locator],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?, row.get::<_, u64>(2)?, row.get::<_, u64>(3)?)),
        )
        .map_err(StoreError::sqlite)?;
    if account.0.saturating_add(ciphertext_bytes) > account.2
        || account.1.saturating_add(1) > account.3
    {
        return Err(StoreError::new(
            AcsErrorCode::QuotaExceeded,
            "cloud account storage quota is exceeded",
        ));
    }
    let global_bytes = transaction
        .query_row(
            "SELECT COALESCE(SUM(stored_bytes), 0) FROM accounts",
            [],
            |row| row.get::<_, u64>(0),
        )
        .map_err(StoreError::sqlite)?;
    if global_bytes.saturating_add(ciphertext_bytes) > global_limit {
        return Err(StoreError::new(
            AcsErrorCode::ReadOnly,
            "cloud service storage ceiling is reached",
        ));
    }
    transaction
        .execute(
            "UPDATE accounts SET stored_bytes = stored_bytes + ?2, object_count = object_count + 1 WHERE account_locator = ?1",
            params![account_locator, ciphertext_bytes],
        )
        .map_err(StoreError::sqlite)?;
    Ok(())
}

fn replace_account_root_usage(
    transaction: &Transaction<'_>,
    account_locator: &str,
    previous_bytes: u64,
    replacement_bytes: u64,
    global_limit: u64,
) -> StoreResult<()> {
    let account = transaction
        .query_row(
            "SELECT stored_bytes, quota_bytes FROM accounts WHERE account_locator = ?1",
            [account_locator],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)),
        )
        .map_err(StoreError::sqlite)?;
    let replacement_total = account
        .0
        .saturating_sub(previous_bytes)
        .saturating_add(replacement_bytes);
    if replacement_total > account.1 {
        return Err(StoreError::new(
            AcsErrorCode::QuotaExceeded,
            "cloud account storage quota is exceeded",
        ));
    }
    let global_bytes = transaction
        .query_row(
            "SELECT COALESCE(SUM(stored_bytes), 0) FROM accounts",
            [],
            |row| row.get::<_, u64>(0),
        )
        .map_err(StoreError::sqlite)?;
    let replacement_global = global_bytes
        .saturating_sub(previous_bytes)
        .saturating_add(replacement_bytes);
    if replacement_global > global_limit {
        return Err(StoreError::new(
            AcsErrorCode::ReadOnly,
            "cloud service storage ceiling is reached",
        ));
    }
    transaction
        .execute(
            "UPDATE accounts SET stored_bytes = ?2 WHERE account_locator = ?1",
            params![account_locator, replacement_total],
        )
        .map_err(StoreError::sqlite)?;
    Ok(())
}

fn release_account_usage(
    transaction: &Transaction<'_>,
    account_locator: &str,
    ciphertext_bytes: u64,
    object_count: u64,
) -> StoreResult<()> {
    transaction
        .execute(
            "UPDATE accounts SET stored_bytes = MAX(stored_bytes - ?2, 0), object_count = MAX(object_count - ?3, 0) WHERE account_locator = ?1",
            params![account_locator, ciphertext_bytes, object_count],
        )
        .map_err(StoreError::sqlite)?;
    Ok(())
}

fn ensure_children_exist(
    transaction: &Transaction<'_>,
    account_locator: &str,
    children: &[String],
) -> StoreResult<()> {
    for child in children {
        validate_opaque_id(child, "child object ID")?;
    }
    if children.is_empty() {
        return Ok(());
    }
    let existing = query_strings(
        transaction,
        "SELECT object_id FROM objects WHERE account_locator = ?1 AND state = 'ready' ORDER BY object_id",
        [account_locator],
    )?
    .into_iter()
    .collect::<BTreeSet<_>>();
    for child in children {
        if !existing.contains(child) {
            return Err(StoreError::new(
                AcsErrorCode::MissingChildObject,
                format!("referenced child object {child:?} does not exist"),
            ));
        }
    }
    Ok(())
}

fn read_root_row(
    connection: &Connection,
    account_locator: &str,
) -> StoreResult<Option<AcsRootSnapshot>> {
    connection
        .query_row(
            "SELECT revision, root_hash, ciphertext_sha256, ciphertext, children_json, updated_at_epoch_ms FROM roots WHERE account_locator = ?1",
            [account_locator],
            |row| {
                let ciphertext: Vec<u8> = row.get(3)?;
                let children_json: String = row.get(4)?;
                let child_object_ids = serde_json::from_str(&children_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        children_json.len(),
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(AcsRootSnapshot {
                    revision: row.get(0)?,
                    root_hash: row.get(1)?,
                    value: AcsEncryptedValue {
                        ciphertext_base64url: URL_SAFE_NO_PAD.encode(ciphertext),
                        ciphertext_sha256: row.get(2)?,
                        child_object_ids,
                    },
                    updated_at_epoch_ms: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(StoreError::sqlite)
}

fn current_event_state(
    connection: &Connection,
    account_locator: &str,
) -> StoreResult<(u64, u64, Option<String>)> {
    let sequence = connection
        .query_row(
            "SELECT event_sequence FROM accounts WHERE account_locator = ?1",
            [account_locator],
            |row| row.get::<_, u64>(0),
        )
        .optional()
        .map_err(StoreError::sqlite)?
        .ok_or_else(|| StoreError::new(AcsErrorCode::NotFound, "cloud account does not exist"))?;
    let root = read_root_row(connection, account_locator)?;
    Ok((
        sequence,
        root.as_ref().map_or(0, |root| root.revision),
        root.map(|root| root.root_hash),
    ))
}

fn event_state_at(
    connection: &Connection,
    account_locator: &str,
    cursor: u64,
) -> StoreResult<(u64, Option<String>)> {
    if cursor == 0 {
        return Ok((0, None));
    }
    connection
        .query_row(
            "SELECT root_revision, root_hash FROM root_events WHERE account_locator = ?1 AND sequence <= ?2 ORDER BY sequence DESC LIMIT 1",
            params![account_locator, cursor],
            |row| Ok((row.get(0)?, Some(row.get(1)?))),
        )
        .optional()
        .map_err(StoreError::sqlite)?
        .ok_or_else(|| StoreError::new(AcsErrorCode::Conflict, "SSE cursor state is no longer retained"))
}

#[derive(Debug)]
struct GcObject {
    children: Vec<String>,
    storage_bytes: u64,
    ciphertext_bytes: u64,
    blob_storage_key: Option<String>,
    created_at_epoch_ms: i64,
}

fn load_object_graph(
    transaction: &Transaction<'_>,
    account_locator: &str,
) -> StoreResult<BTreeMap<String, GcObject>> {
    let mut statement = transaction
        .prepare("SELECT object_id, children_json, ciphertext_bytes, blob_storage_key, created_at_epoch_ms FROM objects WHERE account_locator = ?1")
        .map_err(StoreError::sqlite)?;
    let rows = statement
        .query_map([account_locator], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(StoreError::sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::sqlite)?;
    rows.into_iter()
        .map(
            |(id, children, ciphertext_bytes, blob_storage_key, created_at_epoch_ms)| {
                let storage_bytes = object_storage_bytes(&id, ciphertext_bytes, &children);
                Ok((
                    id,
                    GcObject {
                        children: decode_children(&children)?,
                        storage_bytes,
                        ciphertext_bytes,
                        blob_storage_key,
                        created_at_epoch_ms,
                    },
                ))
            },
        )
        .collect()
}

fn decode_children(json: &str) -> StoreResult<Vec<String>> {
    serde_json::from_str(json)
        .map_err(|error| StoreError::internal(format!("decode visible object edges: {error}")))
}

fn query_strings<P: rusqlite::Params>(
    connection: &Connection,
    sql: &str,
    params: P,
) -> StoreResult<Vec<String>> {
    let mut statement = connection.prepare(sql).map_err(StoreError::sqlite)?;
    let rows = statement
        .query_map(params, |row| row.get::<_, String>(0))
        .map_err(StoreError::sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(StoreError::sqlite)?;
    Ok(rows)
}

fn reserve_durable_token_bucket(
    transaction: &Transaction<'_>,
    bucket_key: &str,
    config: TokenBucketConfig,
    now_epoch_ms: i64,
) -> StoreResult<Result<(), u64>> {
    let persisted = transaction
        .query_row(
            "SELECT available_quanta, updated_at_epoch_ms FROM durable_token_buckets WHERE bucket_key = ?1",
            [bucket_key],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .map_err(StoreError::sqlite)?;
    let mut bucket = match persisted {
        Some((available_quanta, updated_at_epoch_ms)) => TokenBucket {
            available_quanta: u128::from(available_quanta),
            updated_at_epoch_ms,
            initialized: true,
        },
        None => TokenBucket::default(),
    };
    if let Err(retry_after_ms) = bucket.reserve(1, config, now_epoch_ms) {
        return Ok(Err(retry_after_ms));
    }
    let available_quanta = u64::try_from(bucket.available_quanta)
        .map_err(|_| StoreError::internal("durable token bucket exceeds SQLite range"))?;
    transaction
        .execute(
            "INSERT INTO durable_token_buckets(bucket_key, available_quanta, updated_at_epoch_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(bucket_key) DO UPDATE SET available_quanta = excluded.available_quanta, updated_at_epoch_ms = excluded.updated_at_epoch_ms",
            params![bucket_key, available_quanta, bucket.updated_at_epoch_ms],
        )
        .map_err(StoreError::sqlite)?;
    Ok(Ok(()))
}

fn gauge(
    id: &str,
    current: u64,
    warning_at: Option<u64>,
    critical_at: Option<u64>,
    hard_limit: Option<u64>,
) -> AcsStatusMetric {
    AcsStatusMetric {
        id: id.to_string(),
        current,
        peak: current,
        warning_at,
        critical_at,
        hard_limit,
        window_seconds: None,
        rejected_in_window: 0,
        lower_is_worse: false,
    }
}

fn low_gauge(
    id: &str,
    current: u64,
    warning_at: Option<u64>,
    critical_at: Option<u64>,
    hard_limit: Option<u64>,
) -> AcsStatusMetric {
    AcsStatusMetric {
        id: id.to_string(),
        current,
        peak: current,
        warning_at,
        critical_at,
        hard_limit,
        window_seconds: None,
        rejected_in_window: 0,
        lower_is_worse: true,
    }
}

fn bucket_gauge(id: &str, current: u64, config: TokenBucketConfig) -> AcsStatusMetric {
    let hard_limit = config.capacity;
    AcsStatusMetric {
        id: id.to_string(),
        current,
        peak: current,
        warning_at: Some(hard_limit.saturating_mul(8).div_ceil(10).max(1)),
        critical_at: Some(hard_limit.saturating_mul(9).div_ceil(10).max(1)),
        hard_limit: Some(hard_limit),
        window_seconds: Some(config.refill_period_ms / 1_000),
        rejected_in_window: 0,
        lower_is_worse: false,
    }
}

fn observation_metric(
    id: &str,
    current: u64,
    peak: u64,
    warning_at: u64,
    critical_at: u64,
) -> AcsStatusMetric {
    AcsStatusMetric {
        id: id.to_string(),
        current,
        peak,
        warning_at: Some(warning_at),
        critical_at: Some(critical_at),
        hard_limit: None,
        window_seconds: None,
        rejected_in_window: 0,
        lower_is_worse: false,
    }
}

fn window_counter(id: &str, current: u64, window_seconds: u64, rejected: bool) -> AcsStatusMetric {
    AcsStatusMetric {
        id: id.to_string(),
        current,
        peak: current,
        warning_at: None,
        critical_at: None,
        hard_limit: None,
        window_seconds: Some(window_seconds),
        rejected_in_window: if rejected { current } else { 0 },
        lower_is_worse: false,
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

fn decrement_scope(scopes: &mut HashMap<String, u64>, scope: &str) {
    if let Some(count) = scopes.get_mut(scope) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            scopes.remove(scope);
        }
    }
}

fn validate_opaque_id(value: &str, label: &str) -> StoreResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(StoreError::new(
            AcsErrorCode::InvalidRequest,
            format!("{label} is invalid"),
        ));
    }
    Ok(())
}

fn blob_storage_key(account_locator: &str, object_id: &str, ciphertext_sha256: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"aerobag-cloud-blob-v2\0");
    hash.update(account_locator.as_bytes());
    hash.update([0]);
    hash.update(object_id.as_bytes());
    hash.update([0]);
    hash.update(ciphertext_sha256.as_bytes());
    hex_bytes(&hash.finalize())
}

fn random_token(bytes: usize) -> StoreResult<String> {
    let mut value = vec![0_u8; bytes];
    getrandom::fill(&mut value).map_err(|error| {
        StoreError::internal(format!("secure random generation failed: {error}"))
    })?;
    Ok(URL_SAFE_NO_PAD.encode(value))
}

fn remove_file_if_present(path: &Path) -> StoreResult<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(StoreError::io("remove cloud blob", error)),
    }
}

fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |metadata| metadata.len())
}

fn blob_files(blob_root: &Path) -> StoreResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for shard in
        fs::read_dir(blob_root).map_err(|error| StoreError::io("scan cloud blob root", error))?
    {
        let shard = shard.map_err(|error| StoreError::io("read cloud blob shard", error))?;
        if !shard
            .file_type()
            .map_err(|error| StoreError::io("read cloud blob shard type", error))?
            .is_dir()
        {
            continue;
        }
        for entry in fs::read_dir(shard.path())
            .map_err(|error| StoreError::io("scan cloud blob shard", error))?
        {
            let entry = entry.map_err(|error| StoreError::io("read cloud blob entry", error))?;
            if entry
                .file_type()
                .map_err(|error| StoreError::io("read cloud blob entry type", error))?
                .is_file()
            {
                files.push(entry.path());
            }
        }
    }
    Ok(files)
}

fn file_modified_epoch_ms(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

fn atomic_epoch(value: &AtomicI64) -> Option<i64> {
    let value = value.load(Ordering::Relaxed);
    (value != 0).then_some(value)
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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
    use tempfile::TempDir;

    fn store_with_inline_threshold(threshold: u64) -> (TempDir, CloudStore) {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.inline_threshold_bytes = threshold;
        (root, CloudStore::open(config).unwrap())
    }

    fn create_account(store: &CloudStore, account: &str, now: i64) {
        create_account_for_network(store, account, "network", now).unwrap();
    }

    fn create_account_for_network(
        store: &CloudStore,
        account: &str,
        network: &str,
        now: i64,
    ) -> StoreResult<AcsCreateAccountResponse> {
        let challenge = store.issue_creation_challenge(network, now).unwrap();
        store.create_account(
            &AcsCreateAccountRequest {
                contract_id: ACS_CONTRACT_ID.to_string(),
                account_locator: account.to_string(),
                signing_key_id: "key".to_string(),
                signing_public_key_base64url: URL_SAFE_NO_PAD.encode([7_u8; 32]),
                creation_challenge: challenge.challenge,
            },
            &[7_u8; 32],
            network,
            now,
        )
    }

    fn value(bytes: &[u8], children: Vec<String>) -> AcsEncryptedValue {
        AcsEncryptedValue::from_ciphertext(bytes, children)
    }

    #[test]
    fn read_queries_do_not_wait_for_the_writer_connection() {
        let (_root, store) = store_with_inline_threshold(u64::MAX);
        let writer = store.connection().unwrap();
        let reader = store.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            sender.send(reader.health(123)).unwrap();
        });

        let result = receiver.recv_timeout(std::time::Duration::from_secs(2));
        drop(writer);
        thread.join().unwrap();

        assert!(
            result.is_ok(),
            "read-only queries must use a WAL reader instead of waiting for the writer"
        );
        assert_eq!(result.unwrap().unwrap().server_time_epoch_ms, 123);
    }

    #[test]
    fn gc_releases_the_writer_between_accounts() {
        let (_root, store) = store_with_inline_threshold(u64::MAX);
        create_account(&store, "account-a", 10);
        create_account(&store, "account-b", 11);
        let mut completed = Vec::new();

        let report = store
            .run_gc_inner_with_progress(1_000, 100, |account| {
                assert!(
                    store.inner.connection.try_lock().is_ok(),
                    "GC must release the writer before advancing to another account"
                );
                completed.push(account.to_string());
            })
            .unwrap();

        assert_eq!(completed, ["account-a", "account-b"]);
        assert!(report.database_pause_ms <= report.database_work_ms);
    }

    #[test]
    fn token_bucket_refills_continuously_without_fixed_window_cliffs() {
        let config = TokenBucketConfig::per_minute(2, 2);
        let mut bucket = TokenBucket::default();
        assert_eq!(bucket.reserve(1, config, 1_000), Ok(()));
        assert_eq!(bucket.reserve(1, config, 1_000), Ok(()));
        assert_eq!(bucket.reserve(1, config, 1_000), Err(30_000));
        assert_eq!(bucket.reserve(1, config, 30_999), Err(1));
        assert_eq!(bucket.reserve(1, config, 31_000), Ok(()));
    }

    #[test]
    fn rejected_scoped_limits_do_not_consume_broader_tokens() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.global_operation_bucket = TokenBucketConfig::per_minute(2, 2);
        config.network_operation_bucket = TokenBucketConfig::per_minute(1, 1);
        let store = CloudStore::open(config).unwrap();

        store.check_network_operation("network-a", 1_000).unwrap();
        let error = store
            .check_network_operation("network-a", 1_001)
            .unwrap_err();
        assert_eq!(
            error.rate_limit_gate,
            Some(AcsRateLimitGate::NetworkOperations)
        );
        store.check_network_operation("network-b", 1_002).unwrap();

        let error = store
            .check_network_operation("network-c", 1_003)
            .unwrap_err();
        assert_eq!(
            error.rate_limit_gate,
            Some(AcsRateLimitGate::GlobalOperations)
        );
    }

    #[test]
    fn ingress_limits_are_scoped_atomic_and_reported() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.global_ingress_bucket = TokenBucketConfig::per_minute(10, 10);
        config.network_ingress_bucket = TokenBucketConfig::per_minute(5, 5);
        config.account_ingress_bucket = TokenBucketConfig::per_minute(3, 3);
        let store = CloudStore::open(config).unwrap();

        store.admit_network_ingress("network-a", 5, 1_000).unwrap();
        assert_eq!(
            store
                .admit_network_ingress("network-a", 1, 1_000)
                .unwrap_err()
                .rate_limit_gate,
            Some(AcsRateLimitGate::NetworkIngress)
        );
        store.admit_network_ingress("network-b", 5, 1_000).unwrap();
        assert_eq!(
            store
                .admit_network_ingress("network-c", 1, 1_000)
                .unwrap_err()
                .rate_limit_gate,
            Some(AcsRateLimitGate::GlobalIngress)
        );
        store.admit_account_ingress("account", 3, 1_000).unwrap();
        assert_eq!(
            store
                .admit_account_ingress("account", 1, 1_000)
                .unwrap_err()
                .rate_limit_gate,
            Some(AcsRateLimitGate::AccountIngress)
        );

        let status = store.status(1_000).unwrap();
        for (id, expected) in [
            ("global_ingress_bytes_per_minute", 10),
            ("max_network_ingress_bytes_per_minute", 5),
            ("max_account_ingress_bytes_per_minute", 3),
        ] {
            assert_eq!(
                status
                    .metrics
                    .iter()
                    .find(|metric| metric.id == id)
                    .unwrap()
                    .current,
                expected,
                "unexpected {id} usage"
            );
        }
    }

    #[test]
    fn visible_graph_metadata_is_charged_and_legacy_accounting_is_repaired() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.anonymous_quota_bytes = 1_000;
        let store = CloudStore::open(config.clone()).unwrap();
        create_account(&store, "account", 10);
        let first_id = "a".repeat(128);
        let second_id = "b".repeat(128);
        store
            .create_object("account", &first_id, &value(b"", vec![]), 20)
            .unwrap();
        assert_eq!(
            store
                .create_object("account", &second_id, &value(b"", vec![]), 21)
                .unwrap_err()
                .code,
            AcsErrorCode::QuotaExceeded
        );
        let expected = object_storage_bytes(&first_id, 0, "[]");
        assert_eq!(
            store
                .connection()
                .unwrap()
                .query_row(
                    "SELECT stored_bytes FROM accounts WHERE account_locator = 'account'",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap(),
            expected
        );

        store
            .connection()
            .unwrap()
            .execute(
                "UPDATE accounts SET stored_bytes = 0 WHERE account_locator = 'account'",
                [],
            )
            .unwrap();
        drop(store);
        let reopened = CloudStore::open(config).unwrap();
        assert_eq!(
            reopened
                .connection()
                .unwrap()
                .query_row(
                    "SELECT stored_bytes FROM accounts WHERE account_locator = 'account'",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap(),
            expected
        );
    }

    #[test]
    fn containment_modes_block_challenge_and_account_creation() {
        let root = TempDir::new().unwrap();
        let config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        let store = CloudStore::open(config.clone()).unwrap();
        let challenge = store.issue_creation_challenge("network", 10).unwrap();
        store.set_service_mode(AccountMode::ReadOnly, 11).unwrap();
        assert_eq!(
            store
                .issue_creation_challenge("network", 12)
                .unwrap_err()
                .code,
            AcsErrorCode::ReadOnly
        );
        assert_eq!(
            store
                .create_account(
                    &AcsCreateAccountRequest {
                        contract_id: ACS_CONTRACT_ID.to_string(),
                        account_locator: "account".to_string(),
                        signing_key_id: "key".to_string(),
                        signing_public_key_base64url: URL_SAFE_NO_PAD.encode([7_u8; 32]),
                        creation_challenge: challenge.challenge,
                    },
                    &[7_u8; 32],
                    "network",
                    13,
                )
                .unwrap_err()
                .code,
            AcsErrorCode::ReadOnly
        );
        drop(store);
        let reopened = CloudStore::open(config).unwrap();
        assert_eq!(reopened.health(14).unwrap().mode, AcsServiceMode::ReadOnly);
    }

    #[test]
    fn filesystem_pressure_enters_durable_read_only_mode_before_writes() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.filesystem_free_bytes_critical = u64::MAX;
        let store = CloudStore::open(config.clone()).unwrap();
        let error = store.issue_creation_challenge("network", 10).unwrap_err();
        assert_eq!(error.code, AcsErrorCode::ReadOnly);
        assert!(error.message.contains("filesystem exhaustion"));
        drop(store);
        let reopened = CloudStore::open(config).unwrap();
        assert_eq!(reopened.health(11).unwrap().mode, AcsServiceMode::ReadOnly);
    }

    #[test]
    fn schema_one_database_is_upgraded_with_durable_creation_buckets() {
        let root = TempDir::new().unwrap();
        let database_path = StorageLayout::new(root.path().to_path_buf()).database_path();
        fs::create_dir_all(database_path.parent().unwrap()).unwrap();
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute_batch("CREATE TABLE metadata(schema_version INTEGER NOT NULL); INSERT INTO metadata(schema_version) VALUES (1);")
            .unwrap();
        drop(connection);

        let store =
            CloudStore::open(StoreConfig::for_test_data_root(root.path().to_path_buf())).unwrap();
        let connection = store.connection().unwrap();
        assert_eq!(
            connection
                .query_row("SELECT schema_version FROM metadata", [], |row| row
                    .get::<_, u32>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'durable_token_buckets'",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn account_creation_network_bucket_refills_and_survives_restart() {
        let root = TempDir::new().unwrap();
        let config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        let store = CloudStore::open(config.clone()).unwrap();
        for index in 0..3 {
            create_account_for_network(&store, &format!("account-{index}"), "network", 1_000)
                .unwrap();
        }
        drop(store);

        let reopened = CloudStore::open(config).unwrap();
        let error =
            create_account_for_network(&reopened, "account-3", "network", 1_001).unwrap_err();
        assert_eq!(
            error.rate_limit_gate,
            Some(AcsRateLimitGate::AccountCreationNetwork)
        );
        assert_eq!(error.retry_after_ms, Some(8 * 60 * 60 * 1_000 - 1));
        create_account_for_network(
            &reopened,
            "account-3",
            "network",
            8 * 60 * 60 * 1_000 + 1_000,
        )
        .unwrap();
    }

    #[test]
    fn idempotent_account_creation_does_not_consume_a_refilled_token() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.creation_network_bucket = TokenBucketConfig::per_day(1, 1);
        let store = CloudStore::open(config).unwrap();
        create_account_for_network(&store, "account", "network", 1_000).unwrap();
        let next_day = 1_000 + DAY_MS as i64;
        create_account_for_network(&store, "account", "network", next_day).unwrap();
        create_account_for_network(&store, "other", "network", next_day).unwrap();
    }

    #[test]
    fn global_creation_rejection_rolls_back_network_reservation() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.creation_network_bucket = TokenBucketConfig::per_day(1, 1);
        config.creation_global_bucket = TokenBucketConfig::per_day(1, 1);
        let store = CloudStore::open(config).unwrap();
        create_account_for_network(&store, "account-a", "network-a", 1_000).unwrap();
        let error =
            create_account_for_network(&store, "account-b", "network-b", 1_000).unwrap_err();
        assert_eq!(
            error.rate_limit_gate,
            Some(AcsRateLimitGate::AccountCreationGlobal)
        );
        let connection = store.connection().unwrap();
        let network_b_bucket: u64 = connection
            .query_row(
                "SELECT COUNT(*) FROM durable_token_buckets WHERE bucket_key = 'account_creation_network:network-b'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(network_b_bucket, 0);
    }

    #[test]
    fn create_once_never_overwrites_and_restart_reads_inline_and_blob_objects() {
        let (root, store) = store_with_inline_threshold(4);
        create_account(&store, "account", 10);
        assert_eq!(
            store
                .create_object("account", "small", &value(b"tiny", vec![]), 20)
                .unwrap(),
            AcsCreateObjectOutcome::Created
        );
        assert_eq!(
            store
                .create_object("account", "large", &value(b"larger", vec![]), 21)
                .unwrap(),
            AcsCreateObjectOutcome::Created
        );
        assert_eq!(
            store
                .create_object("account", "large", &value(b"larger", vec![]), 22)
                .unwrap(),
            AcsCreateObjectOutcome::AlreadyExists
        );
        assert_eq!(
            store
                .create_object("account", "large", &value(b"different", vec![]), 23)
                .unwrap_err()
                .code,
            AcsErrorCode::ObjectIdCollision
        );
        drop(store);

        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.inline_threshold_bytes = 4;
        let reopened = CloudStore::open(config).unwrap();
        assert_eq!(
            reopened
                .read_object("account", "small", "network")
                .unwrap()
                .value
                .ciphertext()
                .unwrap(),
            b"tiny"
        );
        assert_eq!(
            reopened
                .read_object("account", "large", "network")
                .unwrap()
                .value
                .ciphertext()
                .unwrap(),
            b"larger"
        );
    }

    #[test]
    fn root_cas_is_atomic_and_emits_one_replayable_event() {
        let (_root, store) = store_with_inline_threshold(1024);
        create_account(&store, "account", 10);
        store
            .create_object("account", "page", &value(b"page", vec![]), 20)
            .unwrap();
        let request = AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision: 0,
            expected_root_hash: None,
            replacement: value(b"root", vec!["page".to_string()]),
        };
        let committed = store
            .compare_and_swap_root("account", &request, "network", 30)
            .unwrap();
        assert!(matches!(
            committed,
            AcsCompareAndSwapRootResponse::Committed { .. }
        ));
        assert!(matches!(
            store
                .compare_and_swap_root("account", &request, "network", 31)
                .unwrap(),
            AcsCompareAndSwapRootResponse::Conflict {
                current_revision: 1,
                ..
            }
        ));
        assert_eq!(
            store.initial_sse_events("account", Some(0)).unwrap(),
            vec![
                AcsSseEvent::Ready {
                    sequence: 0,
                    root_revision: 0,
                    root_hash: None,
                },
                AcsSseEvent::RootChanged {
                    sequence: 1,
                    root_revision: 1,
                    root_hash: store.read_root("account", "network").unwrap().root_hash,
                },
            ]
        );
    }

    #[test]
    fn stale_root_cas_rejects_before_validating_its_large_replacement_graph() {
        let (_root, store) = store_with_inline_threshold(1024);
        create_account(&store, "account", 10);
        let initial = AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision: 0,
            expected_root_hash: None,
            replacement: value(b"root", vec![]),
        };
        store
            .compare_and_swap_root("account", &initial, "network", 20)
            .unwrap();

        let stale = AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision: 0,
            expected_root_hash: None,
            replacement: value(b"replacement", vec!["missing-child".to_string()]),
        };
        assert!(matches!(
            store
                .compare_and_swap_root("account", &stale, "network", 21)
                .unwrap(),
            AcsCompareAndSwapRootResponse::Conflict {
                current_revision: 1,
                ..
            }
        ));
    }

    #[test]
    fn quota_is_transactional_and_gc_preserves_reachable_objects() {
        let (_root, store) = store_with_inline_threshold(1024);
        create_account(&store, "account", 10);
        let object_bytes = object_storage_bytes("keep", 4, "[]");
        let root_bytes = root_storage_bytes(4, "[\"keep\"]");
        store
            .set_account_quota("account", object_bytes * 2 + root_bytes, 11)
            .unwrap();
        store
            .create_object("account", "keep", &value(b"1234", vec![]), 20)
            .unwrap();
        store
            .create_object("account", "drop", &value(b"5678", vec![]), 21)
            .unwrap();
        assert_eq!(
            store
                .create_object("account", "overflow", &value(b"xxxxx", vec![]), 22)
                .unwrap_err()
                .code,
            AcsErrorCode::QuotaExceeded
        );
        let request = AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision: 0,
            expected_root_hash: None,
            replacement: value(b"root", vec!["keep".to_string()]),
        };
        store
            .compare_and_swap_root("account", &request, "network", 30)
            .unwrap();
        let report = store.run_gc(1_000, 100).unwrap();
        assert_eq!(report.deleted_objects, 1);
        let status = store.status(1_001).unwrap();
        assert!(status
            .metrics
            .iter()
            .any(|metric| metric.id == "gc_runs" && metric.current == 1));
        let pause = status
            .metrics
            .iter()
            .find(|metric| metric.id == "gc_database_pause_ms")
            .unwrap();
        assert_eq!(pause.current, report.database_pause_ms);
        assert!(pause.peak >= pause.current);
        let elapsed = status
            .metrics
            .iter()
            .find(|metric| metric.id == "gc_elapsed_ms")
            .unwrap();
        assert_eq!(elapsed.current, report.total_elapsed_ms);
        assert!(elapsed.peak >= elapsed.current);
        assert!(store.read_object("account", "keep", "network").is_ok());
        assert_eq!(
            store
                .read_object("account", "drop", "network")
                .unwrap_err()
                .code,
            AcsErrorCode::NotFound
        );
    }

    #[test]
    fn monitoring_reports_limits_and_opaque_top_contributors() {
        let (_root, store) = store_with_inline_threshold(1024);
        create_account(&store, "opaque-account", 10);
        store
            .create_object("opaque-account", "page", &value(b"page", vec![]), 20)
            .unwrap();
        let status = store.status(30).unwrap();
        assert_eq!(status.contract_id, ACS_CONTRACT_ID);
        assert!(status.metrics.iter().any(|metric| {
            metric.id == "stored_bytes"
                && metric.current == object_storage_bytes("page", 4, "[]")
                && metric.hard_limit == Some(store.inner.config.global_storage_limit_bytes)
        }));
        assert_eq!(status.top_contributors[0].opaque_subject, "opaque-account");
    }

    #[test]
    fn write_resume_is_checked_and_forced_resume_is_explicitly_audited() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.global_storage_limit_bytes = 1_000;
        config.write_resume_headroom_bytes = 600;
        config.write_resume_min_filesystem_free_bytes = 1;
        let store = CloudStore::open(config).unwrap();
        create_account(&store, "account", 10);
        store
            .create_object(
                "account",
                "page",
                &value(b"twenty bytes payload", vec![]),
                20,
            )
            .unwrap();
        store.set_service_mode(AccountMode::ReadOnly, 30).unwrap();

        let error = store.resume_writes(40).unwrap_err();
        assert!(error.message.contains("quota headroom"));
        let report = store
            .force_resume_writes("operator reviewed disk pressure", 50)
            .unwrap();
        assert!(report.resumed);
        assert!(report.forced);
        let audit: (String, String) = store
            .connection()
            .unwrap()
            .query_row(
                "SELECT action, reason FROM operator_audit ORDER BY id DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(audit.0, "force_resume_writes");
        assert_eq!(audit.1, "operator reviewed disk pressure");
    }

    #[test]
    fn concurrent_root_compare_and_swap_has_one_winner_and_one_event() {
        let (_root, store) = store_with_inline_threshold(1024);
        create_account(&store, "account", 10);
        for object in ["a", "b"] {
            store
                .create_object("account", object, &value(object.as_bytes(), vec![]), 20)
                .unwrap();
        }
        let barrier = Arc::new(std::sync::Barrier::new(3));
        let handles = ["a", "b"].map(|object| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let request = AcsCompareAndSwapRootRequest {
                    contract_id: ACS_CONTRACT_ID.to_string(),
                    expected_revision: 0,
                    expected_root_hash: None,
                    replacement: value(b"root", vec![object.to_string()]),
                };
                barrier.wait();
                store
                    .compare_and_swap_root("account", &request, "network", 30)
                    .unwrap()
            })
        });
        barrier.wait();
        let outcomes = handles.map(|handle| handle.join().unwrap());
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(
                    outcome,
                    AcsCompareAndSwapRootResponse::Committed { .. }
                ))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, AcsCompareAndSwapRootResponse::Conflict { .. }))
                .count(),
            1
        );
        assert_eq!(
            store.initial_sse_events("account", Some(0)).unwrap().len(),
            2
        );
    }

    #[test]
    fn restart_removes_interrupted_blob_reservation_and_releases_quota() {
        let (root, store) = store_with_inline_threshold(1);
        create_account(&store, "account", 10);
        store
            .create_object("account", "blob", &value(b"large", vec![]), 20)
            .unwrap();
        let key = store
            .connection()
            .unwrap()
            .query_row(
                "SELECT blob_storage_key FROM objects WHERE account_locator = 'account' AND object_id = 'blob'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        let path = store.blob_path(&key);
        store
            .connection()
            .unwrap()
            .execute(
                "UPDATE objects SET state = 'pending' WHERE account_locator = 'account' AND object_id = 'blob'",
                [],
            )
            .unwrap();
        fs::remove_file(path).unwrap();
        drop(store);

        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.inline_threshold_bytes = 1;
        let reopened = CloudStore::open(config).unwrap();
        assert_eq!(
            reopened
                .read_object("account", "blob", "network")
                .unwrap_err()
                .code,
            AcsErrorCode::NotFound
        );
        let status = reopened.status(30).unwrap();
        assert!(status
            .metrics
            .iter()
            .any(|metric| metric.id == "stored_bytes" && metric.current == 0));
    }

    #[test]
    fn restart_finishes_blob_installed_before_ready_row_transition() {
        let (root, store) = store_with_inline_threshold(1);
        create_account(&store, "account", 10);
        store
            .create_object("account", "blob", &value(b"large", vec![]), 20)
            .unwrap();
        store
            .connection()
            .unwrap()
            .execute(
                "UPDATE objects SET state = 'pending' WHERE account_locator = 'account' AND object_id = 'blob'",
                [],
            )
            .unwrap();
        drop(store);

        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.inline_threshold_bytes = 1;
        let reopened = CloudStore::open(config).unwrap();
        assert_eq!(
            reopened
                .read_object("account", "blob", "network")
                .unwrap()
                .value
                .ciphertext()
                .unwrap(),
            b"large"
        );
    }

    #[test]
    fn committed_root_and_event_replay_survive_restart() {
        let (root, store) = store_with_inline_threshold(1024);
        create_account(&store, "account", 10);
        let request = AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision: 0,
            expected_root_hash: None,
            replacement: value(b"root", vec![]),
        };
        store
            .compare_and_swap_root("account", &request, "network", 20)
            .unwrap();
        drop(store);

        let reopened =
            CloudStore::open(StoreConfig::for_test_data_root(root.path().to_path_buf())).unwrap();
        assert_eq!(
            reopened.read_root("account", "network").unwrap().revision,
            1
        );
        assert!(matches!(
            reopened
                .initial_sse_events("account", Some(0))
                .unwrap()
                .as_slice(),
            [
                AcsSseEvent::Ready { sequence: 0, .. },
                AcsSseEvent::RootChanged { sequence: 1, .. }
            ]
        ));
    }

    #[test]
    fn invalid_sse_ticket_is_consumed_instead_of_becoming_a_probe_oracle() {
        let (_root, store) = store_with_inline_threshold(1024);
        create_account(&store, "account", 10);
        let ticket = store
            .create_sse_ticket("account", None, "network-a", 20)
            .unwrap();
        assert_eq!(
            store
                .consume_sse_ticket(&ticket.ticket, "network-b", 21)
                .unwrap_err()
                .code,
            AcsErrorCode::Unauthorized
        );
        assert_eq!(
            store
                .consume_sse_ticket(&ticket.ticket, "network-a", 22)
                .unwrap_err()
                .code,
            AcsErrorCode::Unauthorized
        );
    }

    #[test]
    fn root_bytes_are_included_in_account_quota_and_replacement_accounting() {
        let (_root, store) = store_with_inline_threshold(1024);
        create_account(&store, "account", 10);
        let first_root_bytes = root_storage_bytes(6, "[]");
        store
            .set_account_quota("account", first_root_bytes, 11)
            .unwrap();
        let first = AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision: 0,
            expected_root_hash: None,
            replacement: value(b"123456", vec![]),
        };
        let committed = store
            .compare_and_swap_root("account", &first, "network", 20)
            .unwrap();
        let AcsCompareAndSwapRootResponse::Committed { root } = committed else {
            panic!("first root did not commit");
        };
        let too_large = AcsCompareAndSwapRootRequest {
            contract_id: ACS_CONTRACT_ID.to_string(),
            expected_revision: root.revision,
            expected_root_hash: Some(root.root_hash),
            replacement: value(b"1234567", vec![]),
        };
        assert_eq!(
            store
                .compare_and_swap_root("account", &too_large, "network", 21)
                .unwrap_err()
                .code,
            AcsErrorCode::QuotaExceeded
        );
        assert!(store
            .status(22)
            .unwrap()
            .metrics
            .iter()
            .any(|metric| metric.id == "stored_bytes" && metric.current == first_root_bytes));
    }

    #[test]
    fn operation_egress_and_sse_limits_are_enforced_and_reported() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.network_operation_bucket = TokenBucketConfig::per_minute(1, 1);
        config.account_operation_bucket = TokenBucketConfig::per_minute(1, 1);
        let snapshot = AcsObjectSnapshot {
            object_id: "page".to_string(),
            value: value(b"page", vec![]),
            created_at_epoch_ms: 20,
        };
        let response_bytes = serialized_response_bytes(&snapshot).unwrap();
        config.global_egress_bucket =
            TokenBucketConfig::per_minute(response_bytes * 10, response_bytes * 10);
        config.network_egress_bucket =
            TokenBucketConfig::per_minute(response_bytes * 10, response_bytes * 10);
        config.account_egress_bucket =
            TokenBucketConfig::per_minute(response_bytes, response_bytes);
        config.global_sse_limit = 1;
        config.account_sse_limit = 1;
        config.network_sse_limit = 1;
        let store = CloudStore::open(config).unwrap();
        create_account(&store, "account", 10);
        store
            .create_object("account", "page", &value(b"page", vec![]), 20)
            .unwrap();

        store.check_network_operation("network", 30).unwrap();
        assert_eq!(
            store
                .check_network_operation("network", 31)
                .unwrap_err()
                .code,
            AcsErrorCode::RateLimited
        );
        store.check_account_operation("account", 30).unwrap();
        assert_eq!(
            store
                .check_account_operation("account", 31)
                .unwrap_err()
                .code,
            AcsErrorCode::RateLimited
        );
        store.read_object("account", "page", "network").unwrap();
        assert_eq!(
            {
                let error = store.read_object("account", "page", "network").unwrap_err();
                assert_eq!(error.rate_limit_gate, Some(AcsRateLimitGate::AccountEgress));
                error.code
            },
            AcsErrorCode::RateLimited
        );
        store.begin_sse_connection("account", "network").unwrap();
        assert_eq!(
            store
                .begin_sse_connection("account", "network")
                .unwrap_err()
                .code,
            AcsErrorCode::RateLimited
        );
        store.end_sse_connection("account", "network");
        store.begin_sse_connection("account", "network").unwrap();
        store.end_sse_connection("account", "network");

        let status = store.status(40).unwrap();
        assert!(status.metrics.iter().any(|metric| {
            metric.id == "max_network_operations_per_minute"
                && metric.hard_limit == Some(1)
                && metric.window_seconds == Some(60)
        }));
        assert!(status.metrics.iter().any(|metric| {
            metric.id == "global_egress_bytes_per_minute" && metric.current == response_bytes
        }));
        assert!(status.metrics.iter().any(|metric| {
            metric.id == "max_network_egress_bytes_per_minute" && metric.current == response_bytes
        }));
        assert!(status
            .metrics
            .iter()
            .any(|metric| { metric.id == "rate_limit_rejections" && metric.current >= 4 }));
    }

    #[test]
    fn egress_limits_cover_serialized_responses_at_global_and_network_scopes() {
        let snapshot = AcsObjectSnapshot {
            object_id: "page".to_string(),
            value: value(b"page", vec![]),
            created_at_epoch_ms: 20,
        };
        let response_bytes = serialized_response_bytes(&snapshot).unwrap();

        let global_root = TempDir::new().unwrap();
        let mut global_config = StoreConfig::for_test_data_root(global_root.path().to_path_buf());
        global_config.global_egress_bucket =
            TokenBucketConfig::per_minute(response_bytes, response_bytes);
        global_config.network_egress_bucket =
            TokenBucketConfig::per_minute(response_bytes * 10, response_bytes * 10);
        global_config.account_egress_bucket =
            TokenBucketConfig::per_minute(response_bytes * 10, response_bytes * 10);
        let global_store = CloudStore::open(global_config).unwrap();
        create_account(&global_store, "account", 10);
        global_store
            .create_object("account", "page", &value(b"page", vec![]), 20)
            .unwrap();
        global_store
            .read_object("account", "page", "network-a")
            .unwrap();
        assert_eq!(
            global_store
                .read_object("account", "page", "network-b")
                .unwrap_err()
                .rate_limit_gate,
            Some(AcsRateLimitGate::GlobalEgress)
        );

        let network_root = TempDir::new().unwrap();
        let mut network_config = StoreConfig::for_test_data_root(network_root.path().to_path_buf());
        network_config.global_egress_bucket =
            TokenBucketConfig::per_minute(response_bytes * 10, response_bytes * 10);
        network_config.network_egress_bucket =
            TokenBucketConfig::per_minute(response_bytes, response_bytes);
        network_config.account_egress_bucket =
            TokenBucketConfig::per_minute(response_bytes * 10, response_bytes * 10);
        let network_store = CloudStore::open(network_config).unwrap();
        create_account(&network_store, "account", 10);
        network_store
            .create_object("account", "page", &value(b"page", vec![]), 20)
            .unwrap();
        network_store
            .read_object("account", "page", "network-a")
            .unwrap();
        assert_eq!(
            network_store
                .read_object("account", "page", "network-a")
                .unwrap_err()
                .rate_limit_gate,
            Some(AcsRateLimitGate::NetworkEgress)
        );
        network_store
            .read_object("account", "page", "network-b")
            .unwrap();
    }

    #[test]
    fn root_cas_rolls_back_when_its_response_exceeds_egress_limit() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.global_egress_bucket = TokenBucketConfig::per_minute(10_000, 10_000);
        config.network_egress_bucket = TokenBucketConfig::per_minute(10_000, 10_000);
        config.account_egress_bucket = TokenBucketConfig::per_minute(1, 1);
        let store = CloudStore::open(config).unwrap();
        create_account(&store, "account", 10);
        let error = store
            .compare_and_swap_root(
                "account",
                &AcsCompareAndSwapRootRequest {
                    contract_id: ACS_CONTRACT_ID.to_string(),
                    expected_revision: 0,
                    expected_root_hash: None,
                    replacement: value(b"root", vec![]),
                },
                "network",
                20,
            )
            .unwrap_err();
        assert_eq!(error.rate_limit_gate, Some(AcsRateLimitGate::AccountEgress));
        let connection = store.connection().unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM roots", [], |row| row.get::<_, u64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT stored_bytes FROM accounts WHERE account_locator = 'account'",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn crossing_global_storage_ceiling_persists_read_only_mode() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.global_storage_limit_bytes = 4;
        let store = CloudStore::open(config).unwrap();
        create_account(&store, "account", 10);
        assert_eq!(
            store
                .create_object("account", "too-big", &value(b"12345", vec![]), 20)
                .unwrap_err()
                .code,
            AcsErrorCode::ReadOnly
        );
        assert_eq!(store.status(21).unwrap().mode, AcsServiceMode::ReadOnly);
        assert_eq!(
            store
                .create_object("account", "small", &value(b"1", vec![]), 22)
                .unwrap_err()
                .code,
            AcsErrorCode::ReadOnly
        );
        drop(store);
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.global_storage_limit_bytes = 4;
        assert_eq!(
            CloudStore::open(config).unwrap().status(23).unwrap().mode,
            AcsServiceMode::ReadOnly
        );
    }

    #[test]
    fn gc_removes_old_unreferenced_blob_files_without_touching_referenced_blobs() {
        let (_root, store) = store_with_inline_threshold(1);
        create_account(&store, "account", 10);
        store
            .create_object("account", "kept", &value(b"kept", vec![]), 20)
            .unwrap();
        store
            .compare_and_swap_root(
                "account",
                &AcsCompareAndSwapRootRequest {
                    contract_id: ACS_CONTRACT_ID.to_string(),
                    expected_revision: 0,
                    expected_root_hash: None,
                    replacement: value(b"root", vec!["kept".to_string()]),
                },
                "network",
                21,
            )
            .unwrap();
        let orphan_dir = store.inner.blob_root.join("ff");
        fs::create_dir_all(&orphan_dir).unwrap();
        let orphan = orphan_dir.join("orphan.blob");
        fs::write(&orphan, b"orphan").unwrap();
        let report = store.run_gc(now_epoch_ms() + 1_000, 0).unwrap();
        assert!(!orphan.exists());
        assert!(report.deleted_blob_files >= 1);
        assert_eq!(
            store
                .read_object("account", "kept", "network")
                .unwrap()
                .value
                .ciphertext()
                .unwrap(),
            b"kept"
        );
    }

    #[test]
    fn sse_cursor_outside_retained_history_resets_to_current_state() {
        let root = TempDir::new().unwrap();
        let mut config = StoreConfig::for_test_data_root(root.path().to_path_buf());
        config.event_retention = 2;
        let store = CloudStore::open(config).unwrap();
        create_account(&store, "account", 10);
        let mut revision = 0;
        let mut hash = None;
        for sequence in 1..=3 {
            let request = AcsCompareAndSwapRootRequest {
                contract_id: ACS_CONTRACT_ID.to_string(),
                expected_revision: revision,
                expected_root_hash: hash,
                replacement: value(format!("root-{sequence}").as_bytes(), vec![]),
            };
            let AcsCompareAndSwapRootResponse::Committed { root } = store
                .compare_and_swap_root("account", &request, "network", 20 + sequence)
                .unwrap()
            else {
                panic!("root CAS unexpectedly conflicted");
            };
            revision = root.revision;
            hash = Some(root.root_hash);
        }
        for cursor in [Some(1), Some(99)] {
            assert!(matches!(
                store
                    .initial_sse_events("account", cursor)
                    .unwrap()
                    .as_slice(),
                [AcsSseEvent::Reset {
                    sequence: 3,
                    root_revision: 3,
                    ..
                }]
            ));
        }
    }
}
