use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, fs,
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use anyhow::{bail, Context};
use chrono::Utc;
use preprocessor_fetch::{FetchCacheConfig, FetchCacheMode};
use preprocessor_live_feeds::{
    engine::{
        default_poll_interval, prune_live_feed_scratch_root, run_upstream_live_feed_publish_tick,
        write_live_feeds_current_manifest, CompiledFixtureCache, FileLiveFeedPublisher, FixedClock,
        FixtureCacheKeyPart, LiveFeedCurrentHistoryEntry, LiveFeedInvalidation,
        LiveFeedPollingTask, LiveFeedSourceAndBuilder, LiveFeedTaskPhase, LiveFeedTickResult,
        LiveFeedVersionManifest, LiveFeedsCurrentManifest, ProductBuilder, PublishedLiveFeedUpdate,
        QueuedLiveFeedSource, SseBroker, SystemClock, UpstreamEvent, LIVE_FEEDS_SCHEMA_VERSION,
        LIVE_FEED_FAILED_SCRATCH_RETAIN_COUNT,
    },
    notam_store::{
        AppliedRawNotamSummary, NotamPersistentStore, NotamRejectionStatus,
        SwimNotamSubscriptionIdentity,
    },
    products::{
        fetch_tfr_detail_backfill_once, LiveFeedFetchConfig, MetarLiveFeedBuilder,
        NexradSourceGridLiveFeedBuilder, NotamLiveFeedBuilder, ObstaclesLiveFeedBuilder,
        TafLiveFeedBuilder, TfrLiveFeedBuilder, WindsAloftLiveFeedBuilder,
    },
    simulation::{
        fixture_loop_duration, next_fixture_loop_virtual_zero, timeline_from_live_feed_root,
        CompiledFixtureStateBuilder, SimulatedLiveFeedSource,
    },
    tfr_detail_backfill::TfrDetailBackfillStore,
};
use serde::{Deserialize, Serialize};

const STATUS_HISTORY_LIMIT: usize = 256;
const SIMULATION_RETAIN_VERSIONS_PER_PRODUCT: usize = 8;
const SIMULATION_PUBLICATION_DIRS: &[&str] = &["states", "versions", "deltas", "packages"];

fn live_feeds_contract_dir_name() -> String {
    format!("v{LIVE_FEEDS_SCHEMA_VERSION}")
}

fn live_feeds_contract_request_prefix() -> String {
    format!("/live-feeds/{}/", live_feeds_contract_dir_name())
}

fn live_feeds_contract_request_path(relative: &str) -> String {
    format!("{}{relative}", live_feeds_contract_request_prefix())
}

fn live_feeds_contract_root(live_root: &Path) -> PathBuf {
    live_root.join(live_feeds_contract_dir_name())
}

fn usage() -> &'static str {
    "usage:
  aerobag-live-feedsd --live-root <path> --listen <addr> [--scratch-root <path>] [--event-interval-ms <n>] [--swim-notams-config <path> --swim-notams-environment <dev|prod> --swim-notams-collector <path> [--swim-notams-state-root <path>]]
  aerobag-live-feedsd --simulation --live-root <path> --listen <addr> [--fixture-root <path>] [--fixture-cache <path>] [--speedup <n>] [--event-interval-ms <n>]
  aerobag-live-feedsd --check-config --live-root <path> --listen <addr> [--simulation --fixture-root <path>]

The daemon owns live-feed polling, publication, static live-feed payload
serving, and SSE invalidation. Vite may proxy /live-feeds to this process in
dev, but Vite must not synthesize live-feed events. --live-root is the durable
base directory; this daemon publishes the active contract below a vN child."
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DaemonConfig {
    live_root: PathBuf,
    listen: SocketAddr,
    scratch_root: PathBuf,
    fetch_cache_root: PathBuf,
    fetch_cache_mode: String,
    fetch_jobs: usize,
    poll_loop_interval_ms: u64,
    event_interval_ms: u64,
    simulation: Option<SimulationConfig>,
    swim_notams: Option<SwimNotamsConfig>,
    tfr_detail_backfill_state_root: PathBuf,
    check_config: bool,
    sse_event_limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimulationConfig {
    fixture_root: Option<PathBuf>,
    fixture_cache: PathBuf,
    speedup: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SwimNotamsConfig {
    config_path: PathBuf,
    environment: String,
    collector_path: PathBuf,
    state_root: PathBuf,
    retry_interval_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct SwimNotamsCredentialFile {
    #[serde(rename = "aerobagEnvironment", alias = "aerobag_environment")]
    aerobag_environment: String,
    #[serde(rename = "providerUrl")]
    provider_url: String,
    queue: String,
    #[serde(rename = "connectionFactory")]
    connection_factory: String,
    username: String,
    vpn: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct LiveFeedSseEvent {
    id: String,
    product: String,
    version: String,
    version_manifest_url: String,
    state_url: String,
    state_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    collected_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    history: Vec<LiveFeedCurrentHistoryEntry>,
}

#[derive(Clone)]
struct DaemonStatus {
    inner: Arc<Mutex<DaemonStatusState>>,
}

struct DaemonStatusState {
    started_at_utc: chrono::DateTime<Utc>,
    next_client_id: u64,
    active_clients: BTreeMap<u64, chrono::DateTime<Utc>>,
    client_update_latency_ms: VecDeque<u64>,
    products: BTreeMap<String, ProductStatusHistory>,
}

#[derive(Default)]
struct ProductStatusHistory {
    last_update_at_utc: Option<chrono::DateTime<Utc>>,
    nominal_interval_seconds: Option<u64>,
    last_attempt_at_utc: Option<chrono::DateTime<Utc>>,
    last_success_at_utc: Option<chrono::DateTime<Utc>>,
    last_published_at_utc: Option<String>,
    last_source_timestamp_utc: Option<String>,
    last_failure_at_utc: Option<chrono::DateTime<Utc>>,
    last_failure_phase: Option<String>,
    last_error: Option<String>,
    current_version: Option<String>,
    current_error_count: u64,
    current_warning_count: u64,
    consecutive_failure_count: u32,
    quality: Option<serde_json::Value>,
    attempts: VecDeque<ProductAttemptSample>,
    samples: VecDeque<ProductUpdateSample>,
    source_samples: VecDeque<SourceIngestSample>,
}

#[derive(Debug, Clone, Serialize)]
struct DaemonStatusSnapshot {
    schema_version: u32,
    generated_at_utc: chrono::DateTime<Utc>,
    started_at_utc: chrono::DateTime<Utc>,
    active_sse_clients: usize,
    client_connection_age_cdf: CdfSummary,
    client_update_latency_cdf: CdfSummary,
    products: BTreeMap<String, ProductStatusSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
struct CdfSummary {
    sample_count: usize,
    min_ms: Option<u64>,
    p50_ms: Option<u64>,
    p90_ms: Option<u64>,
    p95_ms: Option<u64>,
    p99_ms: Option<u64>,
    max_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct ProductStatusSnapshot {
    nominal_interval_seconds: Option<u64>,
    last_attempt_at_utc: Option<chrono::DateTime<Utc>>,
    last_success_at_utc: Option<chrono::DateTime<Utc>>,
    last_published_at_utc: Option<String>,
    last_source_timestamp_utc: Option<String>,
    last_failure_at_utc: Option<chrono::DateTime<Utc>>,
    last_failure_phase: Option<String>,
    last_error: Option<String>,
    current_version: Option<String>,
    current_error_count: u64,
    current_warning_count: u64,
    consecutive_failure_count: u32,
    quality: Option<serde_json::Value>,
    attempts: Vec<ProductAttemptSample>,
    samples: Vec<ProductUpdateSample>,
    source_samples: Vec<SourceIngestSample>,
}

#[derive(Debug, Clone, Serialize)]
struct ProductAttemptSample {
    attempted_at_utc: chrono::DateTime<Utc>,
    result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unchanged: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    published_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_timestamp_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ProductUpdateSample {
    observed_at_utc: chrono::DateTime<Utc>,
    version: String,
    update_interval_ms: Option<u64>,
    delta_bytes: Option<u64>,
    state_bytes: Option<u64>,
    changed_count: usize,
    removed_count: usize,
}

#[derive(Debug, Clone, Serialize)]
struct SourceIngestSample {
    observed_at_utc: chrono::DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_timestamp_utc: Option<String>,
    interval_ms: Option<u64>,
    message_count: usize,
    changed_count: usize,
    removed_count: usize,
    rejected_count: usize,
    new_rejected_count: usize,
    unresolved_rejected_count: usize,
    cursor: i64,
}

impl Default for DaemonStatus {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(DaemonStatusState {
                started_at_utc: Utc::now(),
                next_client_id: 1,
                active_clients: BTreeMap::new(),
                client_update_latency_ms: VecDeque::new(),
                products: BTreeMap::new(),
            })),
        }
    }
}

impl DaemonStatus {
    fn connect_client(&self) -> ClientConnectionGuard {
        let mut state = self.inner.lock().expect("live-feed status lock");
        let client_id = state.next_client_id;
        state.next_client_id += 1;
        state.active_clients.insert(client_id, Utc::now());
        ClientConnectionGuard {
            status: self.clone(),
            client_id,
        }
    }

    fn disconnect_client(&self, client_id: u64) {
        self.inner
            .lock()
            .expect("live-feed status lock")
            .active_clients
            .remove(&client_id);
    }

    fn record_client_update_latency(&self, latency_ms: u64) {
        let mut state = self.inner.lock().expect("live-feed status lock");
        push_limited(&mut state.client_update_latency_ms, latency_ms);
    }

    fn register_product(&self, product: &str, nominal_interval: Duration) {
        let mut state = self.inner.lock().expect("live-feed status lock");
        let history = state.products.entry(product.to_string()).or_default();
        history.nominal_interval_seconds = Some(nominal_interval.as_secs());
    }

    fn record_tick_result(&self, result: &LiveFeedTickResult) {
        for update in &result.published {
            self.record_product_success(update);
        }
        for failure in &result.failures {
            self.record_product_failure(failure);
        }
    }

    fn record_source_success(
        &self,
        product: &str,
        source_timestamp_utc: Option<String>,
        detail: impl Into<String>,
    ) {
        let observed_at_utc = Utc::now();
        let detail = detail.into();
        let mut state = self.inner.lock().expect("live-feed status lock");
        let history = state.products.entry(product.to_string()).or_default();
        history.last_attempt_at_utc = Some(observed_at_utc);
        history.last_success_at_utc = Some(observed_at_utc);
        history.last_source_timestamp_utc = source_timestamp_utc.clone();
        history.consecutive_failure_count = 0;
        push_limited(
            &mut history.attempts,
            ProductAttemptSample {
                attempted_at_utc: observed_at_utc,
                result: detail,
                version: None,
                unchanged: None,
                published_at_utc: None,
                source_timestamp_utc,
                phase: Some("source".to_string()),
                error: None,
            },
        );
    }

    fn record_source_ingest(
        &self,
        product: &str,
        observed_at_utc: chrono::DateTime<Utc>,
        source_timestamp_utc: Option<String>,
        message_count: usize,
        changed_count: usize,
        removed_count: usize,
        rejected_count: usize,
        new_rejected_count: usize,
        unresolved_rejected_count: usize,
        cursor: i64,
    ) {
        let mut state = self.inner.lock().expect("live-feed status lock");
        let history = state.products.entry(product.to_string()).or_default();
        let interval_ms = history.source_samples.back().and_then(|last| {
            checked_duration_ms(observed_at_utc.signed_duration_since(last.observed_at_utc))
        });
        push_limited(
            &mut history.source_samples,
            SourceIngestSample {
                observed_at_utc,
                source_timestamp_utc,
                interval_ms,
                message_count,
                changed_count,
                removed_count,
                rejected_count,
                new_rejected_count,
                unresolved_rejected_count,
                cursor,
            },
        );
    }

    fn record_notam_rejection_status(&self, rejection: &NotamRejectionStatus) {
        let mut state = self.inner.lock().expect("live-feed status lock");
        let history = state.products.entry("notams".to_string()).or_default();
        history.quality = Some(serde_json::json!({
            "rejected_row_count": rejection.unresolved_count,
            "oldest_rejected_ingest_seq": rejection.oldest_unresolved_ingest_seq,
            "latest_rejected_ingest_seq": rejection.latest_unresolved_ingest_seq,
            "last_rejection_error": rejection.last_error,
            "recent_rejections": rejection.recent_rejections,
        }));
    }

    fn record_source_failure(&self, product: &str, error: impl Into<String>) {
        self.record_product_failure(&preprocessor_live_feeds::engine::FailedLiveFeedTask {
            product: product.to_string(),
            phase: LiveFeedTaskPhase::Poll,
            error: error.into(),
        });
    }

    fn record_product_success(&self, update: &PublishedLiveFeedUpdate) {
        let observed_at_utc = Utc::now();
        let delta_bytes = update
            .delta_path
            .as_ref()
            .and_then(|path| fs::metadata(path).ok())
            .map(|metadata| metadata.len());
        let state_bytes = state_bytes_for_status(&update.state_path).ok();
        let quality = quality_facts_for_status(update).ok().flatten();
        let mut state = self.inner.lock().expect("live-feed status lock");
        let history = state.products.entry(update.product.clone()).or_default();
        history.last_attempt_at_utc = Some(observed_at_utc);
        history.last_success_at_utc = Some(observed_at_utc);
        history.last_published_at_utc = update.published_at_utc.clone();
        history.last_source_timestamp_utc = update.collected_at_utc.clone();
        history.current_version = Some(update.version.clone());
        history.current_error_count = 0;
        history.current_warning_count = 0;
        history.consecutive_failure_count = 0;
        if quality.is_some() {
            history.quality = quality;
        }
        push_limited(
            &mut history.attempts,
            ProductAttemptSample {
                attempted_at_utc: observed_at_utc,
                result: "success".to_string(),
                version: Some(update.version.clone()),
                unchanged: Some(update.unchanged),
                published_at_utc: update.published_at_utc.clone(),
                source_timestamp_utc: update.collected_at_utc.clone(),
                phase: None,
                error: None,
            },
        );
        if update.unchanged {
            return;
        }
        let update_interval_ms = history
            .last_update_at_utc
            .and_then(|last| checked_duration_ms(observed_at_utc.signed_duration_since(last)));
        history.last_update_at_utc = Some(observed_at_utc);
        push_limited(
            &mut history.samples,
            ProductUpdateSample {
                observed_at_utc,
                version: update.version.clone(),
                update_interval_ms,
                delta_bytes,
                state_bytes,
                changed_count: update.changed_count,
                removed_count: update.removed_count,
            },
        );
    }

    fn record_product_failure(
        &self,
        failure: &preprocessor_live_feeds::engine::FailedLiveFeedTask,
    ) {
        let observed_at_utc = Utc::now();
        let phase = live_feed_phase_name(failure.phase).to_string();
        let mut state = self.inner.lock().expect("live-feed status lock");
        let history = state.products.entry(failure.product.clone()).or_default();
        history.last_attempt_at_utc = Some(observed_at_utc);
        history.last_failure_at_utc = Some(observed_at_utc);
        history.last_failure_phase = Some(phase.clone());
        history.last_error = Some(failure.error.clone());
        history.consecutive_failure_count = history.consecutive_failure_count.saturating_add(1);
        push_limited(
            &mut history.attempts,
            ProductAttemptSample {
                attempted_at_utc: observed_at_utc,
                result: "failure".to_string(),
                version: None,
                unchanged: None,
                published_at_utc: None,
                source_timestamp_utc: None,
                phase: Some(phase),
                error: Some(failure.error.clone()),
            },
        );
    }

    fn snapshot(&self) -> DaemonStatusSnapshot {
        let now = Utc::now();
        let state = self.inner.lock().expect("live-feed status lock");
        let connection_ages = state
            .active_clients
            .values()
            .filter_map(|started| checked_duration_ms(now.signed_duration_since(*started)))
            .collect::<Vec<_>>();
        let products = state
            .products
            .iter()
            .map(|(product, history)| {
                (
                    product.clone(),
                    ProductStatusSnapshot {
                        nominal_interval_seconds: history.nominal_interval_seconds,
                        last_attempt_at_utc: history.last_attempt_at_utc,
                        last_success_at_utc: history.last_success_at_utc,
                        last_published_at_utc: history.last_published_at_utc.clone(),
                        last_source_timestamp_utc: history.last_source_timestamp_utc.clone(),
                        last_failure_at_utc: history.last_failure_at_utc,
                        last_failure_phase: history.last_failure_phase.clone(),
                        last_error: history.last_error.clone(),
                        current_version: history.current_version.clone(),
                        current_error_count: history.current_error_count,
                        current_warning_count: history.current_warning_count,
                        consecutive_failure_count: history.consecutive_failure_count,
                        quality: history.quality.clone(),
                        attempts: history.attempts.iter().cloned().collect(),
                        samples: history.samples.iter().cloned().collect(),
                        source_samples: history.source_samples.iter().cloned().collect(),
                    },
                )
            })
            .collect();
        DaemonStatusSnapshot {
            schema_version: 1,
            generated_at_utc: now,
            started_at_utc: state.started_at_utc,
            active_sse_clients: state.active_clients.len(),
            client_connection_age_cdf: cdf_summary(connection_ages),
            client_update_latency_cdf: cdf_summary(
                state.client_update_latency_ms.iter().copied().collect(),
            ),
            products,
        }
    }
}

struct ClientConnectionGuard {
    status: DaemonStatus,
    client_id: u64,
}

impl Drop for ClientConnectionGuard {
    fn drop(&mut self) {
        self.status.disconnect_client(self.client_id);
    }
}

fn push_limited<T>(values: &mut VecDeque<T>, value: T) {
    values.push_back(value);
    while values.len() > STATUS_HISTORY_LIMIT {
        values.pop_front();
    }
}

fn checked_duration_ms(duration: chrono::Duration) -> Option<u64> {
    (duration.num_milliseconds() >= 0).then_some(duration.num_milliseconds() as u64)
}

fn path_bytes(path: &Path) -> anyhow::Result<u64> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to stat live-feed state {}", path.display()))?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(path)
        .with_context(|| format!("failed to read live-feed state dir {}", path.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read {}", path.display()))?;
        total = total.saturating_add(path_bytes(&entry.path())?);
    }
    Ok(total)
}

fn state_bytes_for_status(state_path: &Path) -> anyhow::Result<u64> {
    if state_path.file_name().and_then(|name| name.to_str()) == Some("manifest.json") {
        if let Some(parent) = state_path.parent().filter(|parent| parent.is_dir()) {
            return path_bytes(parent);
        }
    }
    let bytes = path_bytes(state_path)?;
    let value = match serde_json::from_slice::<serde_json::Value>(
        &fs::read(state_path)
            .with_context(|| format!("failed to read live-feed state {}", state_path.display()))?,
    ) {
        Ok(value) => value,
        Err(_) => return Ok(bytes),
    };
    let referenced_bytes = value
        .get("files")
        .and_then(serde_json::Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(|file| file.get("size_bytes").and_then(serde_json::Value::as_u64))
                .sum::<u64>()
        })
        .unwrap_or(0);
    Ok(bytes.saturating_add(referenced_bytes))
}

fn quality_facts_for_status(
    update: &PublishedLiveFeedUpdate,
) -> anyhow::Result<Option<serde_json::Value>> {
    if update.product != "nexrad" {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(&update.state_path)
            .with_context(|| format!("failed to read {}", update.state_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", update.state_path.display()))?;
    Ok(value.get("quality").cloned())
}

fn live_feed_phase_name(phase: LiveFeedTaskPhase) -> &'static str {
    match phase {
        LiveFeedTaskPhase::Poll => "poll",
        LiveFeedTaskPhase::Build => "build",
        LiveFeedTaskPhase::Publish => "publish",
        LiveFeedTaskPhase::Announce => "announce",
        LiveFeedTaskPhase::Cleanup => "cleanup",
    }
}

fn cdf_summary(mut samples: Vec<u64>) -> CdfSummary {
    samples.sort_unstable();
    CdfSummary {
        sample_count: samples.len(),
        min_ms: samples.first().copied(),
        p50_ms: percentile(&samples, 0.50),
        p90_ms: percentile(&samples, 0.90),
        p95_ms: percentile(&samples, 0.95),
        p99_ms: percentile(&samples, 0.99),
        max_ms: samples.last().copied(),
    }
}

fn percentile(samples: &[u64], fraction: f64) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let index = ((samples.len() - 1) as f64 * fraction).round() as usize;
    samples.get(index).copied()
}

impl DaemonConfig {
    fn parse(args: impl IntoIterator<Item = String>) -> anyhow::Result<Self> {
        let mut args = args.into_iter();
        let _program = args.next();
        let mut live_root = None;
        let mut listen = None;
        let mut scratch_root = None;
        let mut fetch_cache_root = None;
        let mut fetch_cache_mode = "fill".to_string();
        let mut fetch_jobs = 4_usize;
        let mut poll_loop_interval_ms = 5_000_u64;
        let mut simulation = false;
        let mut fixture_root = None;
        let mut fixture_cache = None;
        let mut speedup = 1_u32;
        let mut swim_notams_config = None;
        let mut swim_notams_environment = None;
        let mut swim_notams_collector = None;
        let mut swim_notams_state_root = None;
        let mut swim_notams_retry_interval_ms = 60_000_u64;
        let mut tfr_detail_backfill_state_root = None;
        let mut event_interval_ms = 5_000_u64;
        let mut check_config = false;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!("{}", usage());
                    std::process::exit(0);
                }
                "--live-root" => live_root = Some(next_path(&mut args, "--live-root")?),
                "--fetch-cache-root" => {
                    fetch_cache_root = Some(next_path(&mut args, "--fetch-cache-root")?)
                }
                "--fetch-cache-mode" => {
                    fetch_cache_mode = next_value(&mut args, "--fetch-cache-mode")?;
                    FetchCacheMode::parse(&fetch_cache_mode)?;
                }
                "--fetch-jobs" => {
                    let value = next_value(&mut args, "--fetch-jobs")?;
                    fetch_jobs = value
                        .parse::<usize>()
                        .with_context(|| format!("invalid --fetch-jobs {value}"))?;
                    if fetch_jobs == 0 {
                        bail!("--fetch-jobs must be greater than zero");
                    }
                }
                "--poll-loop-interval-ms" => {
                    let value = next_value(&mut args, "--poll-loop-interval-ms")?;
                    poll_loop_interval_ms = value
                        .parse::<u64>()
                        .with_context(|| format!("invalid --poll-loop-interval-ms {value}"))?;
                    if poll_loop_interval_ms == 0 {
                        bail!("--poll-loop-interval-ms must be greater than zero");
                    }
                }
                "--listen" => {
                    let value = next_value(&mut args, "--listen")?;
                    listen = Some(
                        value
                            .parse::<SocketAddr>()
                            .with_context(|| format!("invalid --listen address {value}"))?,
                    );
                }
                "--scratch-root" => scratch_root = Some(next_path(&mut args, "--scratch-root")?),
                "--simulation" => simulation = true,
                "--fixture-root" => fixture_root = Some(next_path(&mut args, "--fixture-root")?),
                "--fixture-cache" => fixture_cache = Some(next_path(&mut args, "--fixture-cache")?),
                "--speedup" => {
                    let value = next_value(&mut args, "--speedup")?;
                    speedup = value
                        .parse::<u32>()
                        .with_context(|| format!("invalid --speedup {value}"))?;
                    if speedup == 0 {
                        bail!("--speedup must be greater than zero");
                    }
                }
                "--swim-notams-config" => {
                    swim_notams_config = Some(next_path(&mut args, "--swim-notams-config")?)
                }
                "--swim-notams-environment" => {
                    let value = next_value(&mut args, "--swim-notams-environment")?;
                    match value.as_str() {
                        "dev" | "prod" => swim_notams_environment = Some(value),
                        _ => bail!("--swim-notams-environment must be dev or prod"),
                    }
                }
                "--swim-notams-collector" => {
                    swim_notams_collector = Some(next_path(&mut args, "--swim-notams-collector")?)
                }
                "--swim-notams-state-root" => {
                    swim_notams_state_root = Some(next_path(&mut args, "--swim-notams-state-root")?)
                }
                "--swim-notams-retry-interval-ms" => {
                    let value = next_value(&mut args, "--swim-notams-retry-interval-ms")?;
                    swim_notams_retry_interval_ms = value.parse::<u64>().with_context(|| {
                        format!("invalid --swim-notams-retry-interval-ms {value}")
                    })?;
                    if swim_notams_retry_interval_ms == 0 {
                        bail!("--swim-notams-retry-interval-ms must be greater than zero");
                    }
                }
                "--tfr-detail-backfill-state-root" => {
                    tfr_detail_backfill_state_root =
                        Some(next_path(&mut args, "--tfr-detail-backfill-state-root")?)
                }
                "--event-interval-ms" => {
                    let value = next_value(&mut args, "--event-interval-ms")?;
                    event_interval_ms = value
                        .parse::<u64>()
                        .with_context(|| format!("invalid --event-interval-ms {value}"))?;
                    if event_interval_ms == 0 {
                        bail!("--event-interval-ms must be greater than zero");
                    }
                }
                "--check-config" => check_config = true,
                _ => bail!("unknown argument {arg}\n\n{}", usage()),
            }
        }

        let live_root = live_root.context("missing --live-root")?;
        let listen = listen.context("missing --listen")?;
        let scratch_root = scratch_root.unwrap_or_else(|| live_root.join("../scratch/live-feeds"));
        let fetch_cache_root = fetch_cache_root.unwrap_or_else(|| live_root.join("../cache/fetch"));
        let simulation = if simulation {
            let fixture_cache =
                fixture_cache.unwrap_or_else(|| scratch_root.join("live-feeds-fixtures"));
            Some(SimulationConfig {
                fixture_root,
                fixture_cache,
                speedup,
            })
        } else {
            if fixture_root.is_some() || fixture_cache.is_some() {
                bail!("fixture arguments require --simulation");
            }
            None
        };
        let swim_notams_state_root_supplied = swim_notams_state_root.is_some();
        let swim_notams_environment_supplied = swim_notams_environment.is_some();
        let swim_notams = match (
            swim_notams_config,
            swim_notams_environment,
            swim_notams_collector,
        ) {
            (Some(config_path), Some(environment), Some(collector_path)) => Some(SwimNotamsConfig {
                config_path,
                environment,
                collector_path,
                state_root: swim_notams_state_root
                    .unwrap_or_else(|| live_root.join("../state/swim-notams")),
                retry_interval_ms: swim_notams_retry_interval_ms,
            }),
            (None, None, None) => None,
            _ => bail!(
                "--swim-notams-config, --swim-notams-environment, and --swim-notams-collector must be supplied together"
            ),
        };
        if swim_notams.is_none() && swim_notams_state_root_supplied {
            bail!("--swim-notams-state-root requires --swim-notams-config and --swim-notams-collector");
        }
        if swim_notams.is_none() && swim_notams_environment_supplied {
            bail!("--swim-notams-environment requires --swim-notams-config and --swim-notams-collector");
        }
        let tfr_detail_backfill_state_root = tfr_detail_backfill_state_root
            .unwrap_or_else(|| live_root.join("../state/tfr-detail-backfill"));

        Ok(Self {
            live_root,
            listen,
            scratch_root,
            fetch_cache_root,
            fetch_cache_mode,
            fetch_jobs,
            poll_loop_interval_ms,
            event_interval_ms,
            simulation,
            swim_notams,
            tfr_detail_backfill_state_root,
            check_config,
            sse_event_limit: None,
        })
    }
}

fn main() -> anyhow::Result<()> {
    let config = DaemonConfig::parse(env::args())?;
    validate_config(&config)?;
    if config.check_config {
        println!("OK live-feed daemon config: {config:#?}");
        return Ok(());
    }
    run_server(config)
}

fn validate_config(config: &DaemonConfig) -> anyhow::Result<()> {
    ensure_parent(&config.live_root, "--live-root")?;
    ensure_parent(
        &live_feeds_contract_root(&config.live_root),
        "--live-root contract root",
    )?;
    ensure_parent(&config.scratch_root, "--scratch-root")?;
    ensure_parent(&config.fetch_cache_root, "--fetch-cache-root")?;
    FetchCacheMode::parse(&config.fetch_cache_mode)?;
    if let Some(simulation) = &config.simulation {
        if let Some(fixture_root) = &simulation.fixture_root {
            if !fixture_root.exists() {
                bail!("--fixture-root does not exist: {}", fixture_root.display());
            }
        }
        ensure_parent(&simulation.fixture_cache, "--fixture-cache")?;
        let fixture_root_key = simulation
            .fixture_root
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| "<live-root>".to_string());
        let key = preprocessor_live_feeds::engine::fixture_cache_key(&[
            FixtureCacheKeyPart {
                name: "daemon-schema".to_string(),
                sha256: "0".repeat(64),
            },
            FixtureCacheKeyPart {
                name: "fixture-root".to_string(),
                sha256: preprocessor_live_feeds::engine::sha256_hex(fixture_root_key.as_bytes()),
            },
        ]);
        let cache = CompiledFixtureCache::new(
            simulation.fixture_cache.clone(),
            FixedClock::new(Utc::now()),
        );
        let _ = cache.compiled_root(&key);
    }
    if let Some(swim_notams) = &config.swim_notams {
        if !swim_notams.config_path.is_file() {
            bail!(
                "--swim-notams-config does not exist: {}",
                swim_notams.config_path.display()
            );
        }
        let credential = read_swim_notams_credential_file(&swim_notams.config_path)?;
        if credential.aerobag_environment != swim_notams.environment {
            bail!(
                "--swim-notams-config {} declares environment {}, but daemon expected {}",
                swim_notams.config_path.display(),
                credential.aerobag_environment,
                swim_notams.environment
            );
        }
        if !swim_notams.collector_path.is_file() {
            bail!(
                "--swim-notams-collector does not exist: {}",
                swim_notams.collector_path.display()
            );
        }
        ensure_parent(&swim_notams.state_root, "--swim-notams-state-root")?;
    }
    ensure_parent(
        &config.tfr_detail_backfill_state_root,
        "--tfr-detail-backfill-state-root",
    )?;
    let _clock = SystemClock;
    Ok(())
}

fn run_server(config: DaemonConfig) -> anyhow::Result<()> {
    let listener = TcpListener::bind(config.listen)
        .with_context(|| format!("failed to bind {}", config.listen))?;
    let broker = BroadcastSseBroker::default();
    let status = DaemonStatus::default();
    start_live_feed_driver(&config, broker.clone(), status.clone())?;
    eprintln!(
        "aerobag-live-feedsd serving {} under /live-feeds/{}/ on http://{}",
        config.live_root.display(),
        live_feeds_contract_dir_name(),
        config.listen
    );
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let config = config.clone();
                let broker = broker.clone();
                let status = status.clone();
                thread::spawn(move || {
                    if let Err(error) = handle_connection(stream, &config, &broker, &status) {
                        eprintln!("live-feed request failed: {error:#}");
                    }
                });
            }
            Err(error) => eprintln!("live-feed accept failed: {error}"),
        }
    }
    Ok(())
}

fn start_live_feed_driver(
    config: &DaemonConfig,
    broker: BroadcastSseBroker,
    status: DaemonStatus,
) -> anyhow::Result<()> {
    if let Some(simulation) = &config.simulation {
        return start_simulation_driver(config, simulation, broker, status);
    }
    let live_root = live_feeds_contract_root(&config.live_root);
    let scratch_root = config.scratch_root.join("live-feed-build");
    let poll_interval = Duration::from_millis(config.poll_loop_interval_ms);
    let fetch = live_feed_fetch_config(config)?;
    let swim_notams = config.swim_notams.clone();
    let tfr_detail_backfill_state_root = config.tfr_detail_backfill_state_root.clone();
    if let Err(error) =
        prune_live_feed_scratch_root(&scratch_root, LIVE_FEED_FAILED_SCRATCH_RETAIN_COUNT)
    {
        eprintln!("live-feed startup scratch prune failed: {error:#}");
    }
    thread::spawn(move || {
        let publisher = FileLiveFeedPublisher::new(live_root, SystemClock);
        let notam_state_root_for_enrichment = swim_notams
            .as_ref()
            .map(|swim_notams| swim_notams.state_root.clone());
        let mut tasks = production_tasks(
            fetch.clone(),
            notam_state_root_for_enrichment,
            tfr_detail_backfill_state_root.clone(),
        );
        start_tfr_detail_backfill_supervisor(
            fetch.clone(),
            tfr_detail_backfill_state_root,
            scratch_root.join("tfr-detail-backfill"),
            status.clone(),
        );
        if let Some(swim_notams) = swim_notams {
            let source = QueuedLiveFeedSource::new("notams");
            let notam_state_root = swim_notams.state_root.clone();
            start_swim_notams_supervisor(
                swim_notams,
                notam_state_root.clone(),
                source.sender(),
                status.clone(),
            );
            tasks.push(Box::new(QueuedDaemonLiveFeedTask::new(
                LiveFeedSourceAndBuilder::new(source, NotamLiveFeedBuilder::new(notam_state_root)),
                Duration::from_secs(60),
            )));
        }
        for task in &tasks {
            status.register_product(task.product_id(), task.nominal_interval());
        }
        loop {
            let now = Utc::now();
            let result = run_upstream_live_feed_publish_tick(
                now,
                &mut tasks,
                &scratch_root,
                &publisher,
                &broker,
            );
            for task in &mut tasks {
                task.observe_tick_result(now, &result);
            }
            status.record_tick_result(&result);
            log_tick_result("production", &result);
            thread::sleep(poll_interval);
        }
    });
    Ok(())
}

fn start_simulation_driver(
    config: &DaemonConfig,
    simulation: &SimulationConfig,
    broker: BroadcastSseBroker,
    status: DaemonStatus,
) -> anyhow::Result<()> {
    let fixture_root = simulation
        .fixture_root
        .clone()
        .unwrap_or_else(|| live_feeds_contract_root(&config.live_root));
    let live_root = live_feeds_contract_root(&config.live_root);
    if fixture_root == live_root {
        bail!(
            "simulation --fixture-root must be separate from the versioned live-feed output root so generated output can be reset safely"
        );
    }
    let timeline = timeline_from_live_feed_root(&fixture_root, "daemon-simulation")?;
    let products = timeline
        .events
        .iter()
        .map(|event| event.product.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let loop_duration = fixture_loop_duration(&timeline, simulation.speedup)?;
    let scratch_root = config.scratch_root.join("live-feed-simulation");
    let poll_interval = Duration::from_millis(config.poll_loop_interval_ms.min(1_000));
    let speedup = simulation.speedup;
    reset_simulation_publication(&live_root)?;
    reset_simulation_scratch(&scratch_root)?;
    thread::spawn(move || {
        let publisher = FileLiveFeedPublisher::new(live_root, SystemClock);
        let mut virtual_zero = None;
        loop {
            let delivery_zero = Utc::now();
            let current_virtual_zero = virtual_zero.unwrap_or(delivery_zero);
            let tasks = products
                .iter()
                .map(
                    |product| -> anyhow::Result<Box<dyn LiveFeedPollingTask + Send>> {
                        let source = SimulatedLiveFeedSource::from_timeline_with_virtual_start(
                            product.clone(),
                            timeline.clone(),
                            delivery_zero,
                            current_virtual_zero,
                            speedup,
                        )?;
                        let builder =
                            CompiledFixtureStateBuilder::new(fixture_root.clone(), product);
                        Ok(Box::new(LiveFeedSourceAndBuilder::new(source, builder)))
                    },
                )
                .collect::<anyhow::Result<Vec<_>>>();
            let mut tasks = match tasks {
                Ok(tasks) => tasks,
                Err(error) => {
                    eprintln!("live-feed simulation setup failed: {error:#}");
                    return;
                }
            };
            loop {
                let now = Utc::now();
                let result = run_upstream_live_feed_publish_tick(
                    now,
                    &mut tasks,
                    &scratch_root,
                    &publisher,
                    &broker,
                );
                status.record_tick_result(&result);
                log_tick_result("simulation", &result);
                if let Err(error) = prune_simulation_publication(
                    &publisher.root(),
                    SIMULATION_RETAIN_VERSIONS_PER_PRODUCT,
                ) {
                    eprintln!("live-feed simulation prune failed: {error:#}");
                }
                if let Err(error) = reset_simulation_scratch(&scratch_root) {
                    eprintln!("live-feed simulation scratch cleanup failed: {error:#}");
                }
                if loop_duration
                    .is_some_and(|duration| now.signed_duration_since(delivery_zero) >= duration)
                {
                    eprintln!(
                        "live-feed simulation restarting fixture timeline after {} ms",
                        now.signed_duration_since(delivery_zero).num_milliseconds()
                    );
                    virtual_zero =
                        match next_fixture_loop_virtual_zero(&timeline, current_virtual_zero) {
                            Ok(next) => next,
                            Err(error) => {
                                eprintln!(
                                    "live-feed simulation virtual clock advance failed: {error:#}"
                                );
                                return;
                            }
                        };
                    break;
                }
                thread::sleep(poll_interval);
            }
        }
    });
    Ok(())
}

fn reset_simulation_publication(live_root: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(live_root)
        .with_context(|| format!("failed to create {}", live_root.display()))?;
    for child in SIMULATION_PUBLICATION_DIRS {
        remove_path_if_exists(&live_root.join(child))?;
    }
    remove_path_if_exists(&live_root.join("current.json"))?;
    reset_simulation_current_manifest(live_root)
}

fn reset_simulation_scratch(scratch_root: &Path) -> anyhow::Result<()> {
    remove_path_if_exists(scratch_root)?;
    fs::create_dir_all(scratch_root)
        .with_context(|| format!("failed to create {}", scratch_root.display()))
}

fn remove_path_if_exists(path: &Path) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))
        }
        Ok(_) => {
            fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to stat {}", path.display())),
    }
}

fn reset_simulation_current_manifest(live_root: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(live_root)
        .with_context(|| format!("failed to create {}", live_root.display()))?;
    write_live_feeds_current_manifest(
        live_root,
        &LiveFeedsCurrentManifest {
            schema_version: LIVE_FEEDS_SCHEMA_VERSION,
            generated_at_utc: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            products: BTreeMap::new(),
        },
    )
    .map(|_| ())
}

fn prune_simulation_publication(live_root: &Path, retain_per_product: usize) -> anyhow::Result<()> {
    if retain_per_product == 0 {
        bail!("simulation retention must keep at least one version per product");
    }
    let mut retained = BTreeSet::new();
    retained.insert(live_root.join("current.json"));
    retain_current_manifest_refs(live_root, &mut retained)?;

    let versions_root = live_root.join("versions");
    if versions_root.is_dir() {
        for product_entry in fs::read_dir(&versions_root)
            .with_context(|| format!("failed to read {}", versions_root.display()))?
        {
            let product_entry = product_entry
                .with_context(|| format!("failed to read {}", versions_root.display()))?;
            if !product_entry
                .file_type()
                .with_context(|| format!("failed to stat {}", product_entry.path().display()))?
                .is_dir()
            {
                continue;
            }
            let product_dir = product_entry.path();
            let mut version_manifests = Vec::new();
            for version_entry in fs::read_dir(&product_dir)
                .with_context(|| format!("failed to read {}", product_dir.display()))?
            {
                let version_entry = version_entry
                    .with_context(|| format!("failed to read {}", product_dir.display()))?;
                let path = version_entry.path();
                if version_entry
                    .file_type()
                    .with_context(|| format!("failed to stat {}", path.display()))?
                    .is_file()
                    && path.extension().and_then(|extension| extension.to_str()) == Some("json")
                {
                    version_manifests.push(path);
                }
            }
            version_manifests.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
            for path in version_manifests
                .iter()
                .skip(version_manifests.len().saturating_sub(retain_per_product))
            {
                retain_version_manifest(live_root, path, &mut retained)?;
            }
        }
    }

    for child in SIMULATION_PUBLICATION_DIRS {
        prune_product_children(&live_root.join(child), &retained)?;
    }
    Ok(())
}

fn retain_current_manifest_refs(
    live_root: &Path,
    retained: &mut BTreeSet<PathBuf>,
) -> anyhow::Result<()> {
    let current_path = live_root.join("current.json");
    if !current_path.is_file() {
        return Ok(());
    }
    let current: LiveFeedsCurrentManifest = serde_json::from_slice(
        &fs::read(&current_path)
            .with_context(|| format!("failed to read {}", current_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", current_path.display()))?;
    for entry in current.products.values() {
        let version_manifest_path =
            retain_live_relative_path(live_root, retained, &entry.version_manifest_url)?;
        retain_live_relative_path(live_root, retained, &entry.state_url)?;
        if version_manifest_path.is_file() {
            retain_version_manifest(live_root, &version_manifest_path, retained)?;
        }
        for history in &entry.history {
            let history_manifest_path =
                retain_live_relative_path(live_root, retained, &history.version_manifest_url)?;
            if let Some(state_url) = history.state_url.as_deref() {
                retain_live_relative_path(live_root, retained, state_url)?;
            }
            if history_manifest_path.is_file() {
                retain_version_manifest(live_root, &history_manifest_path, retained)?;
            }
        }
    }
    Ok(())
}

fn retain_version_manifest(
    live_root: &Path,
    version_manifest_path: &Path,
    retained: &mut BTreeSet<PathBuf>,
) -> anyhow::Result<()> {
    retained.insert(version_manifest_path.to_path_buf());
    let manifest: LiveFeedVersionManifest = serde_json::from_slice(
        &fs::read(version_manifest_path)
            .with_context(|| format!("failed to read {}", version_manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", version_manifest_path.display()))?;
    retain_live_relative_path(live_root, retained, &manifest.state.url)?;
    if let Some(install_state) = manifest.install_state.as_ref() {
        retain_live_relative_path(live_root, retained, &install_state.url)?;
    }
    if let Some(delta) = manifest.delta_from_previous.as_ref() {
        retain_live_relative_path(live_root, retained, &delta.url)?;
    }
    Ok(())
}

fn retain_live_relative_path(
    live_root: &Path,
    retained: &mut BTreeSet<PathBuf>,
    relative: &str,
) -> anyhow::Result<PathBuf> {
    let path = live_root.join(safe_relative_path(relative)?);
    retained.insert(path.clone());
    Ok(path)
}

fn prune_product_children(root: &Path, retained: &BTreeSet<PathBuf>) -> anyhow::Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    for product_entry in
        fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))?
    {
        let product_entry =
            product_entry.with_context(|| format!("failed to read {}", root.display()))?;
        let product_path = product_entry.path();
        if product_entry
            .file_type()
            .with_context(|| format!("failed to stat {}", product_path.display()))?
            .is_dir()
        {
            prune_version_children(&product_path, retained)?;
            if fs::read_dir(&product_path)
                .with_context(|| format!("failed to read {}", product_path.display()))?
                .next()
                .is_none()
            {
                fs::remove_dir(&product_path)
                    .with_context(|| format!("failed to remove {}", product_path.display()))?;
            }
        } else if !path_is_retained(&product_path, retained) {
            remove_path_if_exists(&product_path)?;
        }
    }
    Ok(())
}

fn prune_version_children(root: &Path, retained: &BTreeSet<PathBuf>) -> anyhow::Result<()> {
    for child_entry in
        fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))?
    {
        let child_entry =
            child_entry.with_context(|| format!("failed to read {}", root.display()))?;
        let child_path = child_entry.path();
        if !path_is_retained(&child_path, retained) {
            remove_path_if_exists(&child_path)?;
        }
    }
    Ok(())
}

fn path_is_retained(path: &Path, retained: &BTreeSet<PathBuf>) -> bool {
    retained.contains(path) || retained.iter().any(|retained| retained.starts_with(path))
}

struct ProductionLiveFeedTask {
    product_id: String,
    nominal_interval: Duration,
    next_due_at_utc: Option<chrono::DateTime<Utc>>,
    consecutive_failures: u32,
    builder: Box<dyn ProductBuilder + Send>,
}

trait DaemonLiveFeedTask: LiveFeedPollingTask + Send {
    fn nominal_interval(&self) -> Duration;

    fn observe_tick_result(&mut self, _now: chrono::DateTime<Utc>, _result: &LiveFeedTickResult) {}
}

impl ProductionLiveFeedTask {
    fn new(
        product_id: impl Into<String>,
        nominal_interval: Duration,
        builder: Box<dyn ProductBuilder + Send>,
    ) -> Self {
        Self {
            product_id: product_id.into(),
            nominal_interval,
            next_due_at_utc: None,
            consecutive_failures: 0,
            builder,
        }
    }

    fn observe_tick_result(&mut self, now: chrono::DateTime<Utc>, result: &LiveFeedTickResult) {
        let succeeded = result
            .published
            .iter()
            .any(|update| update.product == self.product_id);
        let failed = result
            .failures
            .iter()
            .any(|failure| failure.product == self.product_id);
        if succeeded {
            self.consecutive_failures = 0;
            self.next_due_at_utc = Some(now + chrono_duration_from_std(self.nominal_interval));
        } else if failed {
            self.consecutive_failures = self.consecutive_failures.saturating_add(1);
            self.next_due_at_utc = Some(
                now + chrono_duration_from_std(failure_retry_delay(
                    self.nominal_interval,
                    self.consecutive_failures,
                )),
            );
        }
    }
}

impl DaemonLiveFeedTask for ProductionLiveFeedTask {
    fn nominal_interval(&self) -> Duration {
        self.nominal_interval
    }

    fn observe_tick_result(&mut self, now: chrono::DateTime<Utc>, result: &LiveFeedTickResult) {
        ProductionLiveFeedTask::observe_tick_result(self, now, result);
    }
}

impl LiveFeedPollingTask for ProductionLiveFeedTask {
    fn product_id(&self) -> &str {
        &self.product_id
    }

    fn poll_due(&mut self, now: chrono::DateTime<Utc>) -> anyhow::Result<Vec<UpstreamEvent>> {
        if self.next_due_at_utc.is_some_and(|next_due| now < next_due) {
            return Ok(Vec::new());
        }
        Ok(vec![UpstreamEvent {
            product: self.product_id.clone(),
            source_id: format!(
                "{}:{}",
                self.product_id,
                now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            ),
            previous_source_id: None,
            observed_at_utc: now,
            payload_path: None,
        }])
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<preprocessor_live_feeds::engine::BuiltLiveFeedState> {
        if self.builder.product_id() != self.product_id {
            bail!(
                "production task {} is wired to builder {}",
                self.product_id,
                self.builder.product_id()
            );
        }
        self.builder.build_state(event, scratch_dir)
    }
}

struct QueuedDaemonLiveFeedTask<T> {
    task: T,
    nominal_interval: Duration,
}

impl<T> QueuedDaemonLiveFeedTask<T> {
    fn new(task: T, nominal_interval: Duration) -> Self {
        Self {
            task,
            nominal_interval,
        }
    }
}

impl<T> LiveFeedPollingTask for QueuedDaemonLiveFeedTask<T>
where
    T: LiveFeedPollingTask + Send,
{
    fn product_id(&self) -> &str {
        self.task.product_id()
    }

    fn poll_due(&mut self, now: chrono::DateTime<Utc>) -> anyhow::Result<Vec<UpstreamEvent>> {
        self.task.poll_due(now)
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<preprocessor_live_feeds::engine::BuiltLiveFeedState> {
        self.task.build_state(event, scratch_dir)
    }
}

impl<T> DaemonLiveFeedTask for QueuedDaemonLiveFeedTask<T>
where
    T: LiveFeedPollingTask + Send,
{
    fn nominal_interval(&self) -> Duration {
        self.nominal_interval
    }
}

fn start_swim_notams_supervisor(
    config: SwimNotamsConfig,
    state_root: PathBuf,
    sender: Sender<UpstreamEvent>,
    status: DaemonStatus,
) {
    thread::spawn(move || loop {
        if let Err(error) =
            run_swim_notams_supervisor_session(&config, &state_root, &sender, &status)
        {
            status.record_source_failure(
                "notams",
                format!("NOTAM supervisor session failed: {error:#}"),
            );
        }
        thread::sleep(Duration::from_millis(config.retry_interval_ms));
    });
}

fn run_swim_notams_supervisor_session(
    config: &SwimNotamsConfig,
    state_root: &Path,
    sender: &Sender<UpstreamEvent>,
    status: &DaemonStatus,
) -> anyhow::Result<()> {
    let identity = read_swim_notams_subscription_identity(&config.config_path)?;
    let store = NotamPersistentStore::new(state_root);
    store.initialize(&identity)?;
    let _lock = store.acquire_lock()?;
    let retry_resolved_rows = match store.retry_rejected_raw_messages() {
        Ok(Some(summary)) => {
            eprintln!(
                "NOTAM quarantine retry resolved={} retried={} applied={} superseded={} changed={} removed={}",
                summary.resolved_count,
                summary.retried_count,
                summary.applied_count,
                summary.superseded_count,
                summary.changed_count,
                summary.removed_count
            );
            summary.resolved_count > 0
        }
        Ok(None) => false,
        Err(error) => {
            status.record_source_failure(
                "notams",
                format!("failed to retry rejected NOTAM messages: {error:#}"),
            );
            false
        }
    };
    match store.rejection_status() {
        Ok(rejection) => status.record_notam_rejection_status(&rejection),
        Err(error) => status.record_source_failure(
            "notams",
            format!("failed to read rejected NOTAM status: {error:#}"),
        ),
    }
    if let Err(error) = apply_and_queue_swim_notams(&store, &sender, &status) {
        status.record_source_failure("notams", format!("{error:#}"));
    }
    if let Err(error) =
        queue_persisted_notam_state_event(&store, &sender, &status, retry_resolved_rows)
    {
        status.record_source_failure("notams", format!("{error:#}"));
    }
    loop {
        match start_swim_notams_collector_process(config, &store) {
            Ok(mut child) => {
                status.record_source_success("notams", None, "source_connected");
                let mut failure_after_start = false;
                loop {
                    if let Err(error) = apply_and_queue_swim_notams(&store, sender, status) {
                        status.record_source_failure("notams", format!("{error:#}"));
                    }
                    match child.try_wait() {
                        Ok(Some(exit_status)) => {
                            if exit_status.success() {
                                status.record_source_success("notams", None, "collector_exited");
                            } else {
                                status.record_source_failure(
                                    "notams",
                                    format!("SWIM NOTAM collector exited with {exit_status}"),
                                );
                                failure_after_start = true;
                            }
                            break;
                        }
                        Ok(None) => thread::sleep(Duration::from_secs(1)),
                        Err(error) => {
                            status.record_source_failure(
                                "notams",
                                format!("failed to poll SWIM NOTAM collector: {error:#}"),
                            );
                            let _ = child.kill();
                            failure_after_start = true;
                            break;
                        }
                    }
                }
                if failure_after_start {
                    thread::sleep(Duration::from_millis(config.retry_interval_ms));
                } else {
                    thread::sleep(Duration::from_secs(1));
                }
            }
            Err(error) => {
                status.record_source_failure("notams", format!("{error:#}"));
                thread::sleep(Duration::from_millis(config.retry_interval_ms));
            }
        }
    }
}

fn start_tfr_detail_backfill_supervisor(
    fetch: LiveFeedFetchConfig,
    state_root: PathBuf,
    scratch_root: PathBuf,
    status: DaemonStatus,
) {
    const PRODUCT: &str = "tfr-detail-backfill";
    const INTERVAL: Duration = Duration::from_secs(60);
    const MAX_FETCHES_PER_TICK: usize = 12;
    status.register_product(PRODUCT, INTERVAL);
    thread::spawn(move || {
        let store = TfrDetailBackfillStore::new(&state_root);
        if let Err(error) = store.initialize() {
            status.record_source_failure(PRODUCT, format!("{error:#}"));
            return;
        }
        let _lock = match store.acquire_lock() {
            Ok(lock) => lock,
            Err(error) => {
                status.record_source_failure(PRODUCT, format!("{error:#}"));
                return;
            }
        };
        loop {
            let tick_scratch = scratch_root.join(Utc::now().format("%Y%m%dT%H%M%SZ").to_string());
            match fetch_tfr_detail_backfill_once(
                &fetch,
                &state_root,
                &tick_scratch,
                MAX_FETCHES_PER_TICK,
            ) {
                Ok(summary) if summary.failed == 0 => status.record_source_success(
                    PRODUCT,
                    None,
                    format!(
                        "attempted={} succeeded={} stored={}/{} remaining_unfetched={} remaining_due={} failures={}",
                        summary.attempted,
                        summary.succeeded,
                        summary.stored,
                        summary.desired,
                        summary.remaining_unfetched,
                        summary.remaining_due,
                        summary.total_failures
                    ),
                ),
                Ok(summary) => status.record_source_failure(
                    PRODUCT,
                    format!(
                        "attempted={} succeeded={} failed={} stored={}/{} remaining_unfetched={} remaining_due={} failures={}",
                        summary.attempted,
                        summary.succeeded,
                        summary.failed,
                        summary.stored,
                        summary.desired,
                        summary.remaining_unfetched,
                        summary.remaining_due,
                        summary.total_failures
                    ),
                ),
                Err(error) => status.record_source_failure(PRODUCT, format!("{error:#}")),
            }
            thread::sleep(INTERVAL);
        }
    });
}

fn queue_persisted_notam_state_event(
    store: &NotamPersistentStore,
    sender: &Sender<UpstreamEvent>,
    status: &DaemonStatus,
    include_empty: bool,
) -> anyhow::Result<()> {
    if !include_empty && store.current_records()?.is_empty() {
        return Ok(());
    }
    let observed_at_utc = Utc::now();
    let source_timestamp = observed_at_utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let source_id = format!("notams:persisted:{}", store.current_fingerprint()?);
    sender
        .send(UpstreamEvent {
            product: "notams".to_string(),
            source_id,
            previous_source_id: None,
            observed_at_utc,
            payload_path: None,
        })
        .context("failed to queue persisted NOTAM state event")?;
    status.record_source_success("notams", Some(source_timestamp), "source_persisted_state");
    Ok(())
}

fn apply_and_queue_swim_notams(
    store: &NotamPersistentStore,
    sender: &Sender<UpstreamEvent>,
    status: &DaemonStatus,
) -> anyhow::Result<()> {
    while let Some(summary) = store.apply_pending_raw_messages(500)? {
        let rejection = store.rejection_status()?;
        status.record_notam_rejection_status(&rejection);
        queue_notam_raw_ingest_event(sender, status, &summary, &rejection)?;
        if summary.applied_count < 500 {
            break;
        }
    }
    Ok(())
}

fn start_swim_notams_collector_process(
    config: &SwimNotamsConfig,
    store: &NotamPersistentStore,
) -> anyhow::Result<Child> {
    Command::new(&config.collector_path)
        .arg("--config")
        .arg(&config.config_path)
        .arg("--sqlite")
        .arg(store.sqlite_path())
        .arg("--max-messages")
        .arg("0")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| {
            format!(
                "failed to start SWIM NOTAM collector {}",
                config.collector_path.display()
            )
        })
}

fn read_swim_notams_subscription_identity(
    config_path: &Path,
) -> anyhow::Result<SwimNotamSubscriptionIdentity> {
    let config = read_swim_notams_credential_file(config_path)?;
    Ok(SwimNotamSubscriptionIdentity {
        provider_url: config.provider_url,
        queue: config.queue,
        connection_factory: config.connection_factory,
        username: config.username,
        vpn: config.vpn,
    })
}

fn read_swim_notams_credential_file(
    config_path: &Path,
) -> anyhow::Result<SwimNotamsCredentialFile> {
    serde_json::from_slice(
        &fs::read(config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", config_path.display()))
}

fn queue_notam_raw_ingest_event(
    sender: &Sender<UpstreamEvent>,
    status: &DaemonStatus,
    summary: &AppliedRawNotamSummary,
    rejection: &NotamRejectionStatus,
) -> anyhow::Result<()> {
    let observed_at_utc = summary
        .last_received_at_utc
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let source_timestamp = observed_at_utc.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let event = UpstreamEvent {
        product: "notams".to_string(),
        source_id: format!("notams:raw:{}", summary.max_ingest_seq),
        previous_source_id: None,
        observed_at_utc,
        payload_path: None,
    };
    sender
        .send(event)
        .context("failed to queue NOTAM raw ingest event")?;
    status.record_source_ingest(
        "notams",
        observed_at_utc,
        Some(source_timestamp.clone()),
        summary.applied_count,
        summary.changed_count,
        summary.removed_count,
        summary.rejected_count,
        summary.new_rejected_count,
        rejection.unresolved_count,
        summary.max_ingest_seq,
    );
    status.record_source_success(
        "notams",
        Some(source_timestamp),
        format!(
            "source_messages applied={} changed={} removed={} rejected={} new_rejected={} unresolved_rejected={} cursor={}",
            summary.applied_count,
            summary.changed_count,
            summary.removed_count,
            summary.rejected_count,
            summary.new_rejected_count,
            rejection.unresolved_count,
            summary.max_ingest_seq
        ),
    );
    Ok(())
}

fn failure_retry_delay(nominal_interval: Duration, consecutive_failures: u32) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(3);
    let seconds = 30_u64.saturating_mul(1_u64 << exponent);
    Duration::from_secs(seconds).min(nominal_interval)
}

fn chrono_duration_from_std(duration: Duration) -> chrono::Duration {
    chrono::Duration::from_std(duration).unwrap_or_else(|_| chrono::Duration::seconds(i64::MAX))
}

fn production_tasks(
    fetch: LiveFeedFetchConfig,
    notam_state_root_for_tfr_enrichment: Option<PathBuf>,
    tfr_detail_backfill_state_root: PathBuf,
) -> Vec<Box<dyn DaemonLiveFeedTask + Send>> {
    let tfr_builder = match notam_state_root_for_tfr_enrichment {
        Some(state_root) => {
            TfrLiveFeedBuilder::new(fetch.clone()).with_notam_state_root(state_root)
        }
        None => TfrLiveFeedBuilder::new(fetch.clone()),
    }
    .with_tfr_detail_backfill_state_root(tfr_detail_backfill_state_root);
    vec![
        production_task("metars", MetarLiveFeedBuilder::new(fetch.clone())),
        production_task("tafs", TafLiveFeedBuilder::new(fetch.clone())),
        production_task(
            "nexrad",
            NexradSourceGridLiveFeedBuilder::new(fetch.clone(), false),
        ),
        production_task("tfrs", tfr_builder),
        production_task("winds-aloft", WindsAloftLiveFeedBuilder::new(fetch.clone())),
        production_task("obstacles", ObstaclesLiveFeedBuilder::new(fetch)),
    ]
}

fn production_task<B>(product: &str, builder: B) -> Box<dyn DaemonLiveFeedTask + Send>
where
    B: preprocessor_live_feeds::engine::ProductBuilder + Send + 'static,
{
    let interval = default_poll_interval(product).unwrap_or_else(|| Duration::from_secs(5 * 60));
    Box::new(ProductionLiveFeedTask::new(
        product,
        interval,
        Box::new(builder),
    ))
}

fn live_feed_fetch_config(config: &DaemonConfig) -> anyhow::Result<LiveFeedFetchConfig> {
    Ok(LiveFeedFetchConfig::new(
        config.fetch_jobs,
        Some(FetchCacheConfig {
            root: config.fetch_cache_root.clone(),
            mode: FetchCacheMode::parse(&config.fetch_cache_mode)?,
        }),
    ))
}

fn log_tick_result(label: &str, result: &preprocessor_live_feeds::engine::LiveFeedTickResult) {
    for update in &result.published {
        eprintln!(
            "live-feed {label} published {} {} changed={} removed={}",
            update.product, update.version, update.changed_count, update.removed_count
        );
    }
    for failure in &result.failures {
        eprintln!(
            "live-feed {label} {} {:?} failed: {}",
            failure.product, failure.phase, failure.error
        );
    }
}

fn handle_connection(
    mut stream: TcpStream,
    config: &DaemonConfig,
    broker: &BroadcastSseBroker,
    status: &DaemonStatus,
) -> anyhow::Result<()> {
    let mut reader = BufReader::new(stream.try_clone().context("failed to clone TCP stream")?);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .context("failed to read request line")?;
    if request_line.trim().is_empty() {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    drain_headers(&mut reader)?;
    if method == "OPTIONS" {
        return write_options_response(&mut stream);
    }
    if method != "GET" && method != "HEAD" {
        return write_status(&mut stream, 405, "method not allowed");
    }
    let request_path = target.split('?').next().unwrap_or("/");
    if request_path == "/live-feeds/status.json" {
        return serve_status_json(&mut stream, method, status);
    }
    if request_path == "/live-feeds/status" || request_path == "/live-feeds/status.html" {
        return serve_status_html(&mut stream, method);
    }
    let contract_events_path = live_feeds_contract_request_path("events");
    if request_path == contract_events_path {
        if method == "HEAD" {
            return write_sse_headers(&mut stream);
        }
        return write_sse_stream(
            &mut stream,
            &live_feeds_contract_root(&config.live_root),
            Duration::from_millis(config.event_interval_ms),
            broker,
            status,
            config.sse_event_limit,
        );
    }
    let contract_prefix = live_feeds_contract_request_prefix();
    if let Some(relative) = request_path.strip_prefix(&contract_prefix) {
        return serve_live_feed_file(
            &mut stream,
            method,
            &live_feeds_contract_root(&config.live_root),
            relative,
        );
    }
    write_status(&mut stream, 404, "not found")
}

fn drain_headers(reader: &mut BufReader<TcpStream>) -> anyhow::Result<()> {
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .context("failed to read request header")?;
        if line == "\r\n" || line == "\n" || line.is_empty() {
            return Ok(());
        }
    }
}

fn write_sse_stream(
    writer: &mut impl Write,
    live_root: &Path,
    interval: Duration,
    broker: &BroadcastSseBroker,
    status: &DaemonStatus,
    event_limit: Option<usize>,
) -> anyhow::Result<()> {
    write_sse_headers(writer)?;
    let _client = status.connect_client();
    writeln!(writer, ": aerobag live-feed root {}\n", live_root.display())
        .context("failed to write SSE banner")?;
    let receiver = broker.subscribe();
    let frames = list_live_feed_event_frames(live_root)?;
    let mut sent_events = 0_usize;
    if frames.is_empty() {
        write_sse_heartbeat(writer).context("failed to write empty SSE heartbeat")?;
        writer.flush().context("failed to flush SSE heartbeat")?;
        if event_limit == Some(0) {
            return Ok(());
        }
    }
    for frame in frames {
        for event in frame {
            write_sse_event(writer, &event)?;
            sent_events += 1;
            if event_limit.is_some_and(|limit| sent_events >= limit) {
                writer.flush().context("failed to flush SSE frame")?;
                return Ok(());
            }
        }
        writer.flush().context("failed to flush SSE frame")?;
        thread::sleep(interval);
    }
    loop {
        match receiver.recv_timeout(Duration::from_secs(30)) {
            Ok(queued) => {
                let latency_ms =
                    checked_duration_ms(Utc::now().signed_duration_since(queued.announced_at_utc))
                        .unwrap_or(0);
                let event = live_feed_sse_event_from_invalidation(queued.invalidation);
                write_sse_event(writer, &event)?;
                status.record_client_update_latency(latency_ms);
                sent_events += 1;
                writer.flush().context("failed to flush SSE event")?;
                if event_limit.is_some_and(|limit| sent_events >= limit) {
                    return Ok(());
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                write_sse_heartbeat(writer).context("failed to write SSE heartbeat")?;
                writer.flush().context("failed to flush SSE heartbeat")?;
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

fn write_sse_heartbeat(writer: &mut impl Write) -> anyhow::Result<()> {
    writeln!(
        writer,
        "event: live-feed-heartbeat\ndata: {}\n",
        serde_json::to_string(&serde_json::json!({
            "schema_version": LIVE_FEEDS_SCHEMA_VERSION,
            "products": [],
        }))
        .context("failed to encode SSE heartbeat")?
    )
    .context("failed to write SSE heartbeat")
}

fn write_sse_headers(writer: &mut impl Write) -> anyhow::Result<()> {
    write!(
        writer,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache, no-transform\r\nConnection: keep-alive\r\nAccess-Control-Allow-Origin: *\r\n\r\n"
    )
    .context("failed to write SSE headers")
}

#[cfg(test)]
fn write_sse_frame(writer: &mut impl Write, frame: &[LiveFeedSseEvent]) -> anyhow::Result<()> {
    for event in frame {
        write_sse_event(writer, event)?;
    }
    Ok(())
}

fn write_sse_event(writer: &mut impl Write, event: &LiveFeedSseEvent) -> anyhow::Result<()> {
    writeln!(writer, "id: {}", event.id).context("failed to write SSE id")?;
    writeln!(writer, "event: live-feed-current").context("failed to write SSE event")?;
    writeln!(
        writer,
        "data: {}\n",
        serde_json::to_string(&serde_json::json!({
            "schema_version": LIVE_FEEDS_SCHEMA_VERSION,
            "product": event.product,
            "version": event.version,
            "version_manifest_url": event.version_manifest_url,
            "state_url": event.state_url,
            "state_sha256": event.state_sha256,
            "published_at_utc": event.published_at_utc,
            "collected_at_utc": event.collected_at_utc,
            "history": event.history,
        }))
        .context("failed to encode SSE payload")?
    )
    .context("failed to write SSE data")
}

fn live_feed_sse_event_from_invalidation(invalidation: LiveFeedInvalidation) -> LiveFeedSseEvent {
    LiveFeedSseEvent {
        id: format!("{}:{}", invalidation.product, invalidation.version),
        product: invalidation.product,
        version: invalidation.version,
        version_manifest_url: invalidation.version_manifest_url,
        state_url: invalidation.state_url,
        state_sha256: invalidation.state_sha256,
        published_at_utc: invalidation.published_at_utc,
        collected_at_utc: invalidation.collected_at_utc,
        history: invalidation.history,
    }
}

#[derive(Clone, Default)]
struct BroadcastSseBroker {
    subscribers: Arc<Mutex<Vec<Sender<BrokerSseEvent>>>>,
}

#[derive(Debug, Clone)]
struct BrokerSseEvent {
    invalidation: LiveFeedInvalidation,
    announced_at_utc: chrono::DateTime<Utc>,
}

impl BroadcastSseBroker {
    fn subscribe(&self) -> Receiver<BrokerSseEvent> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers
            .lock()
            .expect("live-feed SSE subscriber lock")
            .push(sender);
        receiver
    }
}

impl SseBroker for BroadcastSseBroker {
    fn announce(&self, event: LiveFeedInvalidation) -> anyhow::Result<()> {
        let queued = BrokerSseEvent {
            invalidation: event,
            announced_at_utc: Utc::now(),
        };
        let mut subscribers = self
            .subscribers
            .lock()
            .expect("live-feed SSE subscriber lock");
        subscribers.retain(|subscriber| subscriber.send(queued.clone()).is_ok());
        Ok(())
    }
}

fn list_live_feed_event_frames(root: &Path) -> anyhow::Result<Vec<Vec<LiveFeedSseEvent>>> {
    let current = root.join("current.json");
    if !current.is_file() {
        return Ok(Vec::new());
    }
    let current: serde_json::Value = serde_json::from_slice(
        &fs::read(&current).with_context(|| format!("failed to read {}", current.display()))?,
    )
    .with_context(|| format!("failed to parse {}", current.display()))?;
    let mut events = Vec::new();
    if let Some(products) = current
        .get("products")
        .and_then(serde_json::Value::as_object)
    {
        for (product, entry) in products {
            let Some(version) = entry.get("current").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Some(version_manifest_url) = entry
                .get("version_manifest_url")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let Some(state_url) = entry.get("state_url").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let Some(state_sha256) = entry
                .get("state_sha256")
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let history = entry
                .get("history")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .with_context(|| format!("failed to parse {product} current history"))?
                .unwrap_or_default();
            events.push(LiveFeedSseEvent {
                id: format!("{product}:{version}"),
                product: product.clone(),
                version: version.to_string(),
                version_manifest_url: version_manifest_url.to_string(),
                state_url: state_url.to_string(),
                state_sha256: state_sha256.to_string(),
                published_at_utc: entry
                    .get("published_at_utc")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                collected_at_utc: entry
                    .get("collected_at_utc")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                history,
            });
        }
    }
    events.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((!events.is_empty()).then_some(events).into_iter().collect())
}

fn serve_status_json(
    stream: &mut TcpStream,
    method: &str,
    status: &DaemonStatus,
) -> anyhow::Result<()> {
    let body =
        serde_json::to_string_pretty(&status.snapshot()).context("failed to encode status")?;
    write_response(
        stream,
        method,
        "application/json",
        "no-cache, no-store",
        body.as_bytes(),
    )
}

fn serve_status_html(stream: &mut TcpStream, method: &str) -> anyhow::Result<()> {
    write_response(
        stream,
        method,
        "text/html; charset=utf-8",
        "no-cache, no-store",
        LIVE_FEEDS_STATUS_HTML.as_bytes(),
    )
}

fn serve_live_feed_file(
    stream: &mut TcpStream,
    method: &str,
    root: &Path,
    relative: &str,
) -> anyhow::Result<()> {
    let relative_path = safe_relative_path(relative)?;
    let file_path = root.join(relative_path);
    if !file_path.is_file() {
        return write_status(stream, 404, "not found");
    }
    let bytes =
        fs::read(&file_path).with_context(|| format!("failed to read {}", file_path.display()))?;
    write_response(stream, method, content_type(&file_path), "no-cache", &bytes)
}

fn write_response(
    stream: &mut TcpStream,
    method: &str,
    content_type: &str,
    cache_control: &str,
    bytes: &[u8],
) -> anyhow::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        content_type,
        bytes.len(),
        cache_control,
    )
    .context("failed to write response headers")?;
    if method != "HEAD" {
        stream
            .write_all(bytes)
            .context("failed to write response body")?;
    }
    Ok(())
}

fn write_options_response(stream: &mut TcpStream) -> anyhow::Result<()> {
    write!(
        stream,
        "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, HEAD, OPTIONS\r\nAccess-Control-Allow-Headers: Last-Event-ID, Cache-Control, Content-Type\r\nAccess-Control-Max-Age: 600\r\nContent-Length: 0\r\n\r\n"
    )
    .context("failed to write CORS options response")
}

const LIVE_FEEDS_STATUS_HTML: &str = r##"<!doctype html>
<meta charset="utf-8">
<title>Aerobag live-feed status</title>
<style>
body { font: 14px system-ui, sans-serif; margin: 24px; color: #111; background: #f7f7f4; }
h1 { margin: 0 0 8px; }
h2 { margin: 24px 0 8px; }
.muted { color: #666; }
.summary, .product { background: white; border: 1px solid #ddd; border-radius: 6px; padding: 12px; margin: 12px 0; }
table { border-collapse: collapse; margin: 8px 0; }
th, td { border-bottom: 1px solid #e3e3df; padding: 4px 8px; text-align: right; }
th:first-child, td:first-child { text-align: left; }
.plots { display: grid; grid-template-columns: repeat(auto-fit, minmax(420px, 1fr)); gap: 12px; }
.plot { height: 260px; background: #fbfbf8; border: 1px solid #ddd; }
details.product-details { margin: 0 0 12px; }
details.product-details summary { cursor: pointer; color: #245; font-weight: 600; }
details.product-details table { margin-top: 8px; }
</style>
<h1>Aerobag Live Feeds</h1>
<div id="status" class="muted">Loading...</div>
<script src="https://cdn.plot.ly/plotly-2.35.2.min.js"></script>
<script>
const statusEl = document.getElementById("status");
const ms = (value) => value == null ? "-" : value < 1000 ? `${value} ms` : `${(value / 1000).toFixed(1)} s`;
const seconds = (value) => value == null ? "-" : `${value.toFixed(value < 10 ? 2 : 1)} s`;
const bytes = (value) => {
  if (value == null) return "-";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${(value / 1024 / 1024).toFixed(2)} MiB`;
};
function cdfTable(title, cdf) {
  return `<h2>${title}</h2><table><tr><th>samples</th><th>min</th><th>p50</th><th>p90</th><th>p95</th><th>p99</th><th>max</th></tr>` +
    `<tr><td>${cdf.sample_count}</td><td>${ms(cdf.min_ms)}</td><td>${ms(cdf.p50_ms)}</td><td>${ms(cdf.p90_ms)}</td><td>${ms(cdf.p95_ms)}</td><td>${ms(cdf.p99_ms)}</td><td>${ms(cdf.max_ms)}</td></tr></table>`;
}
const detailsOpenState = new Map();
function detailsKey(product) {
  return encodeURIComponent(product);
}
function captureDetailsOpenState() {
  document.querySelectorAll("details.product-details[data-details-key]").forEach((details) => {
    detailsOpenState.set(details.dataset.detailsKey, details.open);
  });
}
function productDetails(product, data) {
  const latestAttempt = data.attempts.length === 0 ? null : data.attempts[data.attempts.length - 1];
  const key = detailsKey(product);
  return `<details class="product-details" data-details-key="${key}"${detailsOpenState.get(key) ? " open" : ""}>
    <summary>Status details</summary>
    <table>
    <tr><th>current version</th><td>${data.current_version ?? "-"}</td></tr>
    <tr><th>last attempt</th><td>${data.last_attempt_at_utc ?? "-"}</td></tr>
    <tr><th>last success</th><td>${data.last_success_at_utc ?? "-"}</td></tr>
    <tr><th>last source timestamp</th><td>${data.last_source_timestamp_utc ?? "-"}</td></tr>
    <tr><th>last failure</th><td>${data.last_failure_at_utc ?? "-"}</td></tr>
    <tr><th>failure phase</th><td>${data.last_failure_phase ?? "-"}</td></tr>
    <tr><th>consecutive failures</th><td>${data.consecutive_failure_count}</td></tr>
    <tr><th>last error</th><td>${data.last_error ?? "-"}</td></tr>
    <tr><th>latest attempt result</th><td>${latestAttempt?.result ?? "-"}</td></tr>
  </table>
  </details>`;
}
function plotId(product, field) {
  return `plot-${product.replace(/[^a-zA-Z0-9_-]/g, "_")}-${field}`;
}
function basePlotLayout(title, yTitle) {
  return {
    title: { text: title, font: { size: 14 } },
    margin: { l: 56, r: 18, t: 36, b: 42 },
    paper_bgcolor: "#fbfbf8",
    plot_bgcolor: "#fbfbf8",
    xaxis: { title: "observed UTC", type: "date", tickfont: { size: 10 }, gridcolor: "#e1e1dc" },
    yaxis: { title: yTitle, tickfont: { size: 10 }, gridcolor: "#e1e1dc", rangemode: "tozero" },
  };
}
function purgeExistingPlots() {
  if (!window.Plotly) return;
  document.querySelectorAll(".plot").forEach((node) => {
    if (node.data || node.layout || node._fullLayout) {
      Plotly.purge(node);
    }
  });
}
function renderUpdateIntervalPlot(product, data) {
  const samples = data.samples.filter((sample) => sample.update_interval_ms != null);
  const node = document.getElementById(plotId(product, "update_interval_ms"));
  if (!node) return;
  if (!window.Plotly) {
    node.textContent = "Plotly failed to load.";
    node.className = "plot muted";
    return;
  }
  if (samples.length === 0) {
    node.textContent = "No samples yet.";
    node.className = "plot muted";
    return;
  }
  const values = samples.map((sample) => sample.update_interval_ms / 1000);
  Plotly.react(node, [{
    type: "scatter",
    mode: "lines+markers",
    x: samples.map((sample) => sample.observed_at_utc),
    y: values,
    text: samples.map((sample) => `${sample.version}<br>${seconds(sample.update_interval_ms / 1000)}`),
    hovertemplate: "%{x}<br>%{text}<extra></extra>",
    line: { color: "#0067a8", width: 2 },
    marker: { color: "#0067a8", size: 5 },
  }], basePlotLayout(`${product} update interval`, "seconds"), { responsive: true, displaylogo: false });
}
function renderSizePlot(product, data) {
  const deltaSamples = data.samples.filter((sample) => sample.delta_bytes != null);
  const stateSamples = data.samples.filter((sample) => sample.state_bytes != null);
  const node = document.getElementById(plotId(product, "delta_bytes"));
  if (!node) return;
  if (!window.Plotly) {
    node.textContent = "Plotly failed to load.";
    node.className = "plot muted";
    return;
  }
  if (deltaSamples.length === 0 && stateSamples.length === 0) {
    node.textContent = "No samples yet.";
    node.className = "plot muted";
    return;
  }
  const traces = [];
  if (deltaSamples.length > 0) {
    traces.push({
      type: "scatter",
      mode: "lines+markers",
      name: "delta",
      x: deltaSamples.map((sample) => sample.observed_at_utc),
      y: deltaSamples.map((sample) => sample.delta_bytes),
      text: deltaSamples.map((sample) => `${sample.version}<br>delta ${bytes(sample.delta_bytes)}`),
      hovertemplate: "%{x}<br>%{text}<extra></extra>",
      line: { color: "#0067a8", width: 2 },
      marker: { color: "#0067a8", size: 5 },
      yaxis: "y",
    });
  }
  if (stateSamples.length > 0) {
    traces.push({
      type: "scatter",
      mode: "lines+markers",
      name: "full product",
      x: stateSamples.map((sample) => sample.observed_at_utc),
      y: stateSamples.map((sample) => sample.state_bytes),
      text: stateSamples.map((sample) => `${sample.version}<br>full ${bytes(sample.state_bytes)}`),
      hovertemplate: "%{x}<br>%{text}<extra></extra>",
      line: { color: "#a84b00", width: 2 },
      marker: { color: "#a84b00", size: 5 },
      yaxis: "y2",
    });
  }
  const layout = basePlotLayout(`${product} payload size`, "delta bytes");
  layout.margin.r = 64;
  layout.yaxis2 = {
    title: "full product bytes",
    overlaying: "y",
    side: "right",
    tickfont: { size: 10 },
    rangemode: "tozero",
    showgrid: false,
  };
  layout.legend = { orientation: "h", x: 0, y: 1.12 };
  Plotly.react(node, traces, layout, { responsive: true, displaylogo: false });
}
function minuteBucketMs(timestamp) {
  return Math.floor(new Date(timestamp).getTime() / 60000) * 60000;
}
function renderSourceRatePlot(product, data, generatedAtUtc) {
  const samples = data.source_samples || [];
  const node = document.getElementById(plotId(product, "source_rate"));
  if (!node) return;
  if (!window.Plotly) {
    node.textContent = "Plotly failed to load.";
    node.className = "plot muted";
    return;
  }
  if (samples.length === 0) {
    node.textContent = "No source-rate samples yet.";
    node.className = "plot muted";
    return;
  }
  const firstBucket = minuteBucketMs(samples[0].observed_at_utc);
  const lastBucket = minuteBucketMs(generatedAtUtc);
  const buckets = new Map();
  for (let bucket = firstBucket; bucket <= lastBucket; bucket += 60000) {
    buckets.set(bucket, { messages: 0, changed: 0, removed: 0, cursor: null });
  }
  for (const sample of samples) {
    const bucket = minuteBucketMs(sample.observed_at_utc);
    const value = buckets.get(bucket) || { messages: 0, changed: 0, removed: 0, cursor: null };
    value.messages += sample.message_count;
    value.changed += sample.changed_count;
    value.removed += sample.removed_count;
    value.cursor = value.cursor == null ? sample.cursor : Math.max(value.cursor, sample.cursor);
    buckets.set(bucket, value);
  }
  const entries = Array.from(buckets.entries()).sort(([left], [right]) => left - right);
  Plotly.react(node, [{
    type: "scatter",
    mode: "lines+markers",
    x: entries.map(([bucket]) => new Date(bucket).toISOString()),
    y: entries.map(([, value]) => value.messages),
    text: entries.map(([, value]) =>
      `${value.messages} messages/min<br>` +
      `${value.changed} changed, ${value.removed} removed<br>` +
      `cursor ${value.cursor ?? "-"}`
    ),
    hovertemplate: "%{x}<br>%{text}<extra></extra>",
    line: { color: "#4b7d19", width: 2 },
    marker: { color: "#4b7d19", size: 5 },
  }], basePlotLayout(`${product} source message rate`, "messages/min"), { responsive: true, displaylogo: false });
}
async function render() {
  const response = await fetch("/live-feeds/status.json", { cache: "no-store" });
  const status = await response.json();
  const products = Object.entries(status.products).sort(([left], [right]) => left.localeCompare(right));
  statusEl.className = "";
  captureDetailsOpenState();
  purgeExistingPlots();
  statusEl.innerHTML = `
    <div class="summary">
      <div><b>Generated</b> ${status.generated_at_utc}</div>
      <div><b>Started</b> ${status.started_at_utc}</div>
      <div><b>SSE clients</b> ${status.active_sse_clients}</div>
      ${cdfTable("Client Connection Age CDF", status.client_connection_age_cdf)}
      ${cdfTable("Client Update Latency CDF", status.client_update_latency_cdf)}
    </div>
    ${products.map(([product, data]) => `
      <div class="product">
        <h2>${product}</h2>
        ${productDetails(product, data)}
        <div class="plots">
          <div id="${plotId(product, "update_interval_ms")}" class="plot"></div>
          <div id="${plotId(product, "delta_bytes")}" class="plot"></div>
          ${(data.source_samples || []).length > 0 ? `<div id="${plotId(product, "source_rate")}" class="plot"></div>` : ""}
        </div>
      </div>
    `).join("")}
  `;
  for (const [product, data] of products) {
    renderUpdateIntervalPlot(product, data);
    renderSizePlot(product, data);
    renderSourceRatePlot(product, data, status.generated_at_utc);
  }
}
render().catch((error) => { statusEl.textContent = String(error); });
setInterval(() => render().catch((error) => { statusEl.textContent = String(error); }), 5000);
</script>
"##;

fn safe_relative_path(relative: &str) -> anyhow::Result<PathBuf> {
    let path = PathBuf::from(relative.trim_start_matches('/'));
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid live-feed path: {relative}");
    }
    Ok(path)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

fn write_status(stream: &mut TcpStream, status: u16, body: &str) -> anyhow::Result<()> {
    let reason = match status {
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
        body.len(),
        body
    )
    .context("failed to write status response")
}

fn ensure_parent(path: &Path, label: &str) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{label} has no parent: {}", path.display()))?;
    if !parent.exists() {
        bail!("{label} parent does not exist: {}", parent.display());
    }
    Ok(())
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> anyhow::Result<String> {
    args.next()
        .with_context(|| format!("{flag} requires a value"))
}

fn next_path(args: &mut impl Iterator<Item = String>, flag: &str) -> anyhow::Result<PathBuf> {
    Ok(PathBuf::from(next_value(args, flag)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    use preprocessor_live_feeds::engine::{
        run_live_feed_publish_tick, write_json_pretty_file, BuiltLiveFeedState, DeltaPolicy,
        FileLiveFeedPublisher, LiveFeedProductTask, LiveFeedPublisher, LiveFeedStatePayload,
    };
    use tempfile::tempdir;

    #[test]
    fn parses_production_config() -> anyhow::Result<()> {
        let config = DaemonConfig::parse(
            [
                "aerobag-live-feedsd",
                "--check-config",
                "--live-root",
                "/tmp/live-feeds",
                "--listen",
                "127.0.0.1:8095",
                "--event-interval-ms",
                "17",
            ]
            .into_iter()
            .map(str::to_string),
        )?;
        assert_eq!(config.live_root, PathBuf::from("/tmp/live-feeds"));
        assert_eq!(config.listen, "127.0.0.1:8095".parse::<SocketAddr>()?);
        assert_eq!(config.fetch_cache_mode, "fill");
        assert_eq!(config.event_interval_ms, 17);
        assert!(config.check_config);
        assert!(config.simulation.is_none());
        Ok(())
    }

    #[test]
    fn parses_simulation_config() -> anyhow::Result<()> {
        let config = DaemonConfig::parse(
            [
                "aerobag-live-feedsd",
                "--simulation",
                "--live-root",
                "/tmp/live-feeds",
                "--listen",
                "127.0.0.1:8095",
                "--fixture-root",
                "/tmp/fixtures",
                "--speedup",
                "24",
            ]
            .into_iter()
            .map(str::to_string),
        )?;
        let simulation = config.simulation.expect("simulation");
        assert_eq!(
            simulation.fixture_root.expect("fixture root"),
            PathBuf::from("/tmp/fixtures")
        );
        assert_eq!(simulation.speedup, 24);
        Ok(())
    }

    #[test]
    fn rejects_fixture_args_without_simulation() {
        assert!(DaemonConfig::parse(
            [
                "aerobag-live-feedsd",
                "--live-root",
                "/tmp/live-feeds",
                "--listen",
                "127.0.0.1:8095",
                "--fixture-root",
                "/tmp/fixtures",
            ]
            .into_iter()
            .map(str::to_string)
        )
        .is_err());
    }

    #[test]
    fn event_frames_snapshot_current_products_only() -> anyhow::Result<()> {
        let temp = tempdir()?;
        publish_json_version(temp.path(), "metars", "m1")?;
        publish_json_version(temp.path(), "metars", "m2")?;
        publish_json_version(temp.path(), "nexrad", "n1")?;

        let frames = list_live_feed_event_frames(temp.path())?;
        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0]
                .iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            vec!["metars:m2", "nexrad:n1"]
        );
        let metars = frames[0]
            .iter()
            .find(|event| event.id == "metars:m2")
            .expect("metars event");
        assert_eq!(metars.history.len(), 1);
        assert_eq!(metars.history[0].version, "m1");
        Ok(())
    }

    #[test]
    fn sse_frame_contains_core_ingestible_live_feed_event() -> anyhow::Result<()> {
        let frame = vec![LiveFeedSseEvent {
            id: "metars:m1".to_string(),
            product: "metars".to_string(),
            version: "m1".to_string(),
            version_manifest_url: "versions/metars/m1.json".to_string(),
            state_url: "states/metars/m1.json".to_string(),
            state_sha256: "a".repeat(64),
            published_at_utc: None,
            collected_at_utc: None,
            history: vec![LiveFeedCurrentHistoryEntry {
                version: "m0".to_string(),
                version_manifest_url: "versions/metars/m0.json".to_string(),
                state_url: Some("states/metars/m0.json".to_string()),
                state_sha256: Some("b".repeat(64)),
            }],
        }];
        let mut bytes = Vec::new();
        write_sse_frame(&mut bytes, &frame)?;
        let text = String::from_utf8(bytes)?;
        assert!(text.contains("id: metars:m1\n"));
        assert!(text.contains("event: live-feed-current\n"));
        assert!(text.contains("\"schema_version\":2"), "{text}");
        assert!(text.contains("\"product\":\"metars\""));
        assert!(text.contains("\"history\":[{"), "{text}");
        assert!(text.contains("\"version\":\"m0\""), "{text}");
        Ok(())
    }

    #[test]
    fn shared_publish_tick_announces_through_daemon_broker() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let publisher =
            FileLiveFeedPublisher::new(temp.path().join("live-feeds"), FixedClock::new(Utc::now()));
        let broker = BroadcastSseBroker::default();
        let receiver = broker.subscribe();
        let mut tasks = vec![StaticTask {
            product: "metars".to_string(),
            state: Some(json_built_state(temp.path(), "metars", "m1")?),
        }];

        let result = run_live_feed_publish_tick(&mut tasks, &publisher, &broker);

        assert!(result.failures.is_empty(), "{:#?}", result.failures);
        assert_eq!(result.published.len(), 1);
        let event = receiver.recv_timeout(Duration::from_secs(1))?;
        assert_eq!(event.invalidation.product, "metars");
        assert_eq!(event.invalidation.version, "m1");
        Ok(())
    }

    #[test]
    fn live_feed_paths_cannot_escape_root() {
        assert!(safe_relative_path("states/metars/v1.json").is_ok());
        assert!(safe_relative_path("../current.json").is_err());
        assert!(safe_relative_path("states/../current.json").is_err());
    }

    #[test]
    fn simulation_reset_removes_generated_publication_and_rewrites_current() -> anyhow::Result<()> {
        let temp = tempdir()?;
        for child in SIMULATION_PUBLICATION_DIRS {
            let path = temp.path().join(child).join("metars");
            fs::create_dir_all(&path)?;
            fs::write(path.join("stale.json"), b"{}")?;
        }
        fs::write(temp.path().join("current.json"), b"{\"stale\":true}")?;

        reset_simulation_publication(temp.path())?;

        for child in SIMULATION_PUBLICATION_DIRS {
            assert!(
                !temp.path().join(child).exists(),
                "{child} should be removed"
            );
        }
        let current: LiveFeedsCurrentManifest =
            serde_json::from_slice(&fs::read(temp.path().join("current.json"))?)?;
        assert_eq!(current.schema_version, LIVE_FEEDS_SCHEMA_VERSION);
        assert!(current.products.is_empty());
        Ok(())
    }

    #[test]
    fn simulation_prune_keeps_recent_versions_and_referenced_payloads() -> anyhow::Result<()> {
        let temp = tempdir()?;
        for index in 1..=10 {
            publish_json_version(temp.path(), "metars", &format!("v{index:03}"))?;
        }
        publish_json_version(temp.path(), "nexrad", "v001")?;
        let current_path = temp.path().join("current.json");
        let mut current: LiveFeedsCurrentManifest =
            serde_json::from_slice(&fs::read(&current_path)?)?;
        current
            .products
            .get_mut("metars")
            .expect("metars current")
            .history
            .retain(|entry| entry.version == "v007");
        write_live_feeds_current_manifest(temp.path(), &current)?;

        prune_simulation_publication(temp.path(), 3)?;

        for version in ["v008", "v009", "v010"] {
            assert!(
                temp.path()
                    .join("versions")
                    .join("metars")
                    .join(format!("{version}.json"))
                    .is_file(),
                "{version} manifest should be retained"
            );
            assert!(
                temp.path()
                    .join("states")
                    .join("metars")
                    .join(format!("{version}.json.xz"))
                    .is_file(),
                "{version} state should be retained"
            );
        }
        assert!(temp.path().join("versions/metars/v007.json").is_file());
        assert!(temp.path().join("states/metars/v007.json.xz").is_file());
        assert!(!temp.path().join("versions/metars/v006.json").exists());
        assert!(!temp.path().join("states/metars/v006.json.xz").exists());
        assert!(!temp
            .path()
            .join("deltas/metars/v001__v002.json.xz")
            .exists());
        assert!(temp
            .path()
            .join("deltas/metars/v009__v010.json.xz")
            .is_file());
        assert!(temp.path().join("versions/nexrad/v001.json").is_file());
        let current: LiveFeedsCurrentManifest =
            serde_json::from_slice(&fs::read(temp.path().join("current.json"))?)?;
        assert_eq!(current.products["metars"].current, "v010");
        Ok(())
    }

    #[test]
    fn server_serves_static_live_feed_payloads() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let contract_root = live_feeds_contract_root(temp.path());
        let state_path = contract_root.join("states/metars/m1.json");
        fs::create_dir_all(state_path.parent().expect("state parent"))?;
        fs::write(&state_path, b"{\"version_label\":\"m1\"}")?;

        let response = request_once(
            temp.path(),
            "GET /live-feeds/v2/states/metars/m1.json HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )?;
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.contains("Content-Type: application/json"),
            "{response}"
        );
        assert!(
            response.contains("Access-Control-Allow-Origin: *"),
            "{response}"
        );
        assert!(
            response.ends_with("{\"version_label\":\"m1\"}"),
            "{response}"
        );
        Ok(())
    }

    #[test]
    fn server_serves_sse_events_from_shared_live_feed_manifests() -> anyhow::Result<()> {
        let temp = tempdir()?;
        publish_json_version(&live_feeds_contract_root(temp.path()), "metars", "m1")?;

        let response = request_once(
            temp.path(),
            "GET /live-feeds/v2/events HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )?;
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.contains("Content-Type: text/event-stream"),
            "{response}"
        );
        assert!(
            response.contains("Access-Control-Allow-Origin: *"),
            "{response}"
        );
        assert!(response.contains("event: live-feed-current"), "{response}");
        assert!(response.contains("\"schema_version\":2"), "{response}");
        assert!(response.contains("\"product\":\"metars\""), "{response}");
        Ok(())
    }

    #[test]
    fn server_serves_status_json_and_html() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let response = request_once(
            temp.path(),
            "GET /live-feeds/status.json HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )?;
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.contains("Content-Type: application/json"),
            "{response}"
        );
        assert!(
            response.contains("Access-Control-Allow-Origin: *"),
            "{response}"
        );
        assert!(response.contains("\"active_sse_clients\""), "{response}");

        let response = request_once(
            temp.path(),
            "GET /live-feeds/status.html HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )?;
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.contains("Content-Type: text/html"), "{response}");
        assert!(response.contains("Aerobag Live Feeds"), "{response}");
        Ok(())
    }

    #[test]
    fn server_allows_browser_cors_preflight_for_live_feeds() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let response = request_once(
            temp.path(),
            "OPTIONS /live-feeds/v2/current.json HTTP/1.1\r\nHost: localhost\r\nOrigin: http://example.test\r\nAccess-Control-Request-Method: GET\r\n\r\n",
        )?;
        assert!(
            response.starts_with("HTTP/1.1 204 No Content"),
            "{response}"
        );
        assert!(
            response.contains("Access-Control-Allow-Origin: *"),
            "{response}"
        );
        assert!(
            response.contains("Access-Control-Allow-Methods: GET, HEAD, OPTIONS"),
            "{response}"
        );
        Ok(())
    }

    #[test]
    fn status_includes_unchanged_published_products() {
        let status = DaemonStatus::default();
        status.record_tick_result(&LiveFeedTickResult {
            published: vec![PublishedLiveFeedUpdate {
                product: "metars".to_string(),
                version: "v1".to_string(),
                unchanged: true,
                state_path: PathBuf::from("states/metars/v1.json"),
                version_manifest_path: PathBuf::from("versions/metars/v1.json"),
                version_manifest_url: "versions/metars/v1.json".to_string(),
                state_url: "states/metars/v1.json".to_string(),
                state_sha256: "sha".to_string(),
                published_at_utc: None,
                collected_at_utc: None,
                history: Vec::new(),
                delta_path: None,
                changed_count: 0,
                removed_count: 0,
            }],
            failures: Vec::new(),
        });

        let snapshot = status.snapshot();
        let metars = snapshot.products.get("metars").expect("METAR status");
        assert_eq!(metars.current_version.as_deref(), Some("v1"));
        assert!(metars.last_attempt_at_utc.is_some());
        assert!(metars.last_success_at_utc.is_some());
        assert_eq!(metars.consecutive_failure_count, 0);
        assert_eq!(metars.attempts.len(), 1);
        assert_eq!(metars.attempts[0].result, "success");
        assert_eq!(metars.attempts[0].unchanged, Some(true));
        assert!(metars.samples.is_empty());
    }

    #[test]
    fn status_records_structured_source_ingest_samples() {
        let status = DaemonStatus::default();
        let first = chrono::DateTime::parse_from_rfc3339("2026-07-12T02:47:00Z")
            .expect("test timestamp")
            .with_timezone(&Utc);
        let second = chrono::DateTime::parse_from_rfc3339("2026-07-12T02:48:00Z")
            .expect("test timestamp")
            .with_timezone(&Utc);

        status.record_source_ingest(
            "notams",
            first,
            Some("2026-07-12T02:47:00Z".to_string()),
            3,
            2,
            1,
            0,
            0,
            0,
            42,
        );
        status.record_source_ingest(
            "notams",
            second,
            Some("2026-07-12T02:48:00Z".to_string()),
            6,
            4,
            0,
            1,
            1,
            1,
            48,
        );
        status.record_notam_rejection_status(&NotamRejectionStatus {
            unresolved_count: 1,
            oldest_unresolved_ingest_seq: Some(47),
            latest_unresolved_ingest_seq: Some(47),
            last_error: Some("unsupported NOTAM type X".to_string()),
            recent_rejections: vec![preprocessor_live_feeds::notam_store::NotamRejectionDetail {
                ingest_seq: 47,
                first_rejected_at_utc: "2026-07-12T02:48:00Z".to_string(),
                last_rejected_at_utc: "2026-07-12T02:48:00Z".to_string(),
                rejection_count: 1,
                error: "unsupported NOTAM type X".to_string(),
            }],
        });

        let snapshot = status.snapshot();
        let notams = snapshot.products.get("notams").expect("NOTAM status");
        assert_eq!(notams.source_samples.len(), 2);
        assert_eq!(notams.source_samples[0].interval_ms, None);
        assert_eq!(notams.source_samples[0].message_count, 3);
        assert_eq!(notams.source_samples[0].changed_count, 2);
        assert_eq!(notams.source_samples[0].removed_count, 1);
        assert_eq!(notams.source_samples[0].rejected_count, 0);
        assert_eq!(notams.source_samples[0].unresolved_rejected_count, 0);
        assert_eq!(notams.source_samples[0].cursor, 42);
        assert_eq!(notams.source_samples[1].interval_ms, Some(60_000));
        assert_eq!(notams.source_samples[1].message_count, 6);
        assert_eq!(notams.source_samples[1].changed_count, 4);
        assert_eq!(notams.source_samples[1].removed_count, 0);
        assert_eq!(notams.source_samples[1].rejected_count, 1);
        assert_eq!(notams.source_samples[1].new_rejected_count, 1);
        assert_eq!(notams.source_samples[1].unresolved_rejected_count, 1);
        assert_eq!(notams.source_samples[1].cursor, 48);
        assert_eq!(notams.quality.as_ref().unwrap()["rejected_row_count"], 1);
        assert_eq!(
            notams.quality.as_ref().unwrap()["last_rejection_error"],
            "unsupported NOTAM type X"
        );
        assert_eq!(
            notams.quality.as_ref().unwrap()["recent_rejections"][0]["ingest_seq"],
            47
        );
    }

    #[test]
    fn repaired_empty_notam_projection_is_queued_for_publication() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let store = NotamPersistentStore::new(temp.path());
        store.initialize(&SwimNotamSubscriptionIdentity {
            provider_url: "smfs://example.test:55443".to_string(),
            queue: "example.queue".to_string(),
            connection_factory: "example.CF".to_string(),
            username: "example.user".to_string(),
            vpn: "example-vpn".to_string(),
        })?;
        let (sender, receiver) = mpsc::channel();

        queue_persisted_notam_state_event(&store, &sender, &DaemonStatus::default(), true)?;

        let event = receiver.recv_timeout(Duration::from_secs(1))?;
        assert_eq!(event.product, "notams");
        assert!(event.source_id.starts_with("notams:persisted:"));
        Ok(())
    }

    #[test]
    fn status_records_live_feed_failures() {
        let status = DaemonStatus::default();
        status.record_tick_result(&LiveFeedTickResult {
            published: Vec::new(),
            failures: vec![preprocessor_live_feeds::engine::FailedLiveFeedTask {
                product: "metars".to_string(),
                phase: LiveFeedTaskPhase::Build,
                error: "builder exploded".to_string(),
            }],
        });

        let snapshot = status.snapshot();
        let metars = snapshot.products.get("metars").expect("METAR status");
        assert!(metars.last_attempt_at_utc.is_some());
        assert!(metars.last_failure_at_utc.is_some());
        assert_eq!(metars.last_failure_phase.as_deref(), Some("build"));
        assert_eq!(metars.last_error.as_deref(), Some("builder exploded"));
        assert_eq!(metars.consecutive_failure_count, 1);
        assert_eq!(metars.attempts.len(), 1);
        assert_eq!(metars.attempts[0].result, "failure");
        assert_eq!(metars.attempts[0].phase.as_deref(), Some("build"));
        assert_eq!(
            metars.attempts[0].error.as_deref(),
            Some("builder exploded")
        );
    }

    #[test]
    fn swim_notams_config_requires_matching_environment() -> anyhow::Result<()> {
        let temp = tempdir()?;
        fs::create_dir_all(temp.path().join("live"))?;
        fs::create_dir_all(temp.path().join("scratch"))?;
        fs::create_dir_all(temp.path().join("cache"))?;
        fs::create_dir_all(temp.path().join("state"))?;
        let config_path = temp.path().join("swim-notams.json");
        let collector_path = temp.path().join("collector");
        fs::write(
            &config_path,
            r#"{
              "aerobagEnvironment": "dev",
              "providerUrl": "tcps://example.test:55443",
              "queue": "dev.queue",
              "connectionFactory": "dev.CF",
              "username": "dev-user",
              "password": "dev-pass",
              "vpn": "dev-vpn"
            }"#,
        )?;
        fs::write(&collector_path, "#!/bin/sh\n")?;

        let config = DaemonConfig::parse(
            [
                "aerobag-live-feedsd",
                "--live-root",
                temp.path().join("live").to_str().expect("utf8 temp path"),
                "--listen",
                "127.0.0.1:0",
                "--swim-notams-config",
                config_path.to_str().expect("utf8 temp path"),
                "--swim-notams-environment",
                "prod",
                "--swim-notams-collector",
                collector_path.to_str().expect("utf8 temp path"),
            ]
            .into_iter()
            .map(str::to_string),
        )?;

        let error = validate_config(&config).expect_err("environment mismatch should fail");
        assert!(
            format!("{error:#}").contains("declares environment dev, but daemon expected prod"),
            "{error:#}"
        );
        Ok(())
    }

    #[test]
    fn swim_notams_args_must_include_environment() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let config_path = temp.path().join("swim-notams.json");
        let collector_path = temp.path().join("collector");
        fs::write(&config_path, "{}")?;
        fs::write(&collector_path, "#!/bin/sh\n")?;

        let error = DaemonConfig::parse(
            [
                "aerobag-live-feedsd",
                "--live-root",
                temp.path().join("live").to_str().expect("utf8 temp path"),
                "--listen",
                "127.0.0.1:0",
                "--swim-notams-config",
                config_path.to_str().expect("utf8 temp path"),
                "--swim-notams-collector",
                collector_path.to_str().expect("utf8 temp path"),
            ]
            .into_iter()
            .map(str::to_string),
        )
        .expect_err("missing environment should fail");
        assert!(
            format!("{error:#}").contains(
                "--swim-notams-config, --swim-notams-environment, and --swim-notams-collector must be supplied together"
            ),
            "{error:#}"
        );
        Ok(())
    }

    #[test]
    fn status_exposes_nexrad_quality_facts() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let manifest_path = temp.path().join("manifest.json");
        fs::write(
            &manifest_path,
            r#"{
              "schema_version": 1,
              "quality": {
                "palette_error_max": 4.2,
                "palette_error_p95": 1.7,
                "poor_color_match_count": 3
              }
            }"#,
        )?;
        let status = DaemonStatus::default();
        status.record_tick_result(&LiveFeedTickResult {
            published: vec![PublishedLiveFeedUpdate {
                product: "nexrad".to_string(),
                version: "n1".to_string(),
                unchanged: false,
                state_path: manifest_path,
                version_manifest_path: PathBuf::from("versions/nexrad/n1.json"),
                version_manifest_url: "versions/nexrad/n1.json".to_string(),
                state_url: "states/nexrad/n1/manifest.json".to_string(),
                state_sha256: "sha".to_string(),
                published_at_utc: Some("2026-06-19T18:57:16Z".to_string()),
                collected_at_utc: Some("2026-06-19T18:56:41Z".to_string()),
                history: Vec::new(),
                delta_path: None,
                changed_count: 0,
                removed_count: 0,
            }],
            failures: Vec::new(),
        });

        let snapshot = status.snapshot();
        let nexrad = snapshot.products.get("nexrad").expect("NEXRAD status");
        assert_eq!(nexrad.current_version.as_deref(), Some("n1"));
        let quality = nexrad.quality.as_ref().expect("NEXRAD quality");
        assert_eq!(quality["palette_error_max"], 4.2);
        assert_eq!(quality["palette_error_p95"], 1.7);
        assert_eq!(quality["poor_color_match_count"], 3);
        Ok(())
    }

    #[test]
    fn production_live_feed_task_retries_failures_before_nominal_interval() -> anyhow::Result<()> {
        let start =
            chrono::DateTime::parse_from_rfc3339("2026-06-19T18:00:00Z")?.with_timezone(&Utc);
        let mut task = ProductionLiveFeedTask::new(
            "metars",
            Duration::from_secs(300),
            Box::new(NoopProductBuilder {
                product: "metars".to_string(),
            }),
        );

        assert_eq!(task.poll_due(start)?.len(), 1);
        task.observe_tick_result(
            start,
            &LiveFeedTickResult {
                published: Vec::new(),
                failures: vec![preprocessor_live_feeds::engine::FailedLiveFeedTask {
                    product: "metars".to_string(),
                    phase: LiveFeedTaskPhase::Poll,
                    error: "network down".to_string(),
                }],
            },
        );
        assert!(task
            .poll_due(start + chrono::Duration::seconds(29))?
            .is_empty());
        assert_eq!(
            task.poll_due(start + chrono::Duration::seconds(30))?.len(),
            1
        );

        let retry_time = start + chrono::Duration::seconds(30);
        task.observe_tick_result(
            retry_time,
            &LiveFeedTickResult {
                published: Vec::new(),
                failures: vec![preprocessor_live_feeds::engine::FailedLiveFeedTask {
                    product: "metars".to_string(),
                    phase: LiveFeedTaskPhase::Build,
                    error: "still down".to_string(),
                }],
            },
        );
        assert!(task
            .poll_due(retry_time + chrono::Duration::seconds(59))?
            .is_empty());
        assert_eq!(
            task.poll_due(retry_time + chrono::Duration::seconds(60))?
                .len(),
            1
        );

        let success_time = retry_time + chrono::Duration::seconds(60);
        task.observe_tick_result(
            success_time,
            &LiveFeedTickResult {
                published: vec![PublishedLiveFeedUpdate {
                    product: "metars".to_string(),
                    version: "m1".to_string(),
                    unchanged: false,
                    state_path: PathBuf::from("states/metars/m1.json"),
                    version_manifest_path: PathBuf::from("versions/metars/m1.json"),
                    version_manifest_url: "versions/metars/m1.json".to_string(),
                    state_url: "states/metars/m1.json".to_string(),
                    state_sha256: "sha".to_string(),
                    published_at_utc: None,
                    collected_at_utc: None,
                    history: Vec::new(),
                    delta_path: None,
                    changed_count: 1,
                    removed_count: 0,
                }],
                failures: Vec::new(),
            },
        );
        assert!(task
            .poll_due(success_time + chrono::Duration::seconds(299))?
            .is_empty());
        assert_eq!(
            task.poll_due(success_time + chrono::Duration::seconds(300))?
                .len(),
            1
        );
        Ok(())
    }

    #[test]
    fn status_state_bytes_count_directory_states_and_manifest_references() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let source_grid_state = temp.path().join("nexrad-state");
        fs::create_dir_all(source_grid_state.join("tiles/res0"))?;
        fs::write(source_grid_state.join("manifest.json"), b"{}")?;
        fs::write(source_grid_state.join("tiles/res0/0_0.png"), [0_u8; 11])?;
        fs::write(source_grid_state.join("tiles/res0/0_1.png"), [0_u8; 17])?;
        assert_eq!(
            state_bytes_for_status(&source_grid_state.join("manifest.json"))?,
            30
        );

        let winds_state = temp.path().join("winds.json");
        fs::write(
            &winds_state,
            r#"{"files":[{"size_bytes":1000},{"size_bytes":2000}]}"#,
        )?;
        assert_eq!(state_bytes_for_status(&winds_state)?, 3051);
        Ok(())
    }

    fn publish_json_version(root: &Path, product: &str, version: &str) -> anyhow::Result<()> {
        let state = json_built_state(root, product, version)?;
        let publisher = FileLiveFeedPublisher::new(root.to_path_buf(), FixedClock::new(Utc::now()));
        publisher.publish(state)?;
        Ok(())
    }

    fn json_built_state(
        root: &Path,
        product: &str,
        version: &str,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        let source_dir = root.join("source").join(product);
        let source_path = source_dir.join(format!("{version}.json"));
        let value = serde_json::json!({
            "version_label": version,
            "record_count": 1,
            "records": {
                format!("{product}:{version}"): {"value": version}
            }
        });
        write_json_pretty_file(&source_path, &value)?;
        Ok(BuiltLiveFeedState {
            product: product.to_string(),
            version: version.to_string(),
            payload: LiveFeedStatePayload::JsonFile {
                path: source_path,
                value,
            },
            state_sha256: None,
            state_payload_kind: None,
            status_timestamps: Default::default(),
            delta_policy: DeltaPolicy::KeyedRecords {
                records_key: "records".to_string(),
                count_key: Some("record_count".to_string()),
            },
            precomputed_delta: None,
            changed_count_if_no_delta: 1,
        })
    }

    struct StaticTask {
        product: String,
        state: Option<BuiltLiveFeedState>,
    }

    impl LiveFeedProductTask for StaticTask {
        fn product_id(&self) -> &str {
            &self.product
        }

        fn build_state(&mut self) -> anyhow::Result<BuiltLiveFeedState> {
            self.state.take().context("static task was called twice")
        }
    }

    struct NoopProductBuilder {
        product: String,
    }

    impl ProductBuilder for NoopProductBuilder {
        fn product_id(&self) -> &str {
            &self.product
        }

        fn build_state(
            &self,
            _event: &UpstreamEvent,
            _scratch_dir: &Path,
        ) -> anyhow::Result<BuiltLiveFeedState> {
            bail!("NoopProductBuilder should not build state in this test")
        }
    }

    fn request_once(root: &Path, request: &str) -> anyhow::Result<String> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let config = DaemonConfig {
            live_root: root.to_path_buf(),
            listen: addr,
            scratch_root: root.join("../scratch/live-feeds"),
            fetch_cache_root: root.join("../cache/fetch"),
            fetch_cache_mode: "offline".to_string(),
            fetch_jobs: 1,
            poll_loop_interval_ms: 1,
            event_interval_ms: 1,
            simulation: None,
            swim_notams: None,
            tfr_detail_backfill_state_root: root.join("../state/tfr-detail-backfill"),
            check_config: false,
            sse_event_limit: Some(1),
        };
        let broker = BroadcastSseBroker::default();
        let status = DaemonStatus::default();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept request");
            handle_connection(stream, &config, &broker, &status).expect("handle request");
        });
        let mut stream = TcpStream::connect(addr)?;
        stream.write_all(request.as_bytes())?;
        stream.shutdown(std::net::Shutdown::Write)?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        handle.join().expect("server thread");
        Ok(response)
    }
}
