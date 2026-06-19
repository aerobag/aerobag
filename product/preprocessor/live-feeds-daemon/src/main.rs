use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, fs,
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
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
        default_poll_interval, run_upstream_live_feed_publish_tick,
        write_live_feeds_current_manifest, CompiledFixtureCache, FileLiveFeedPublisher, FixedClock,
        FixtureCacheKeyPart, IntervalLiveFeedSource, LiveFeedInvalidation, LiveFeedPollingTask,
        LiveFeedSourceAndBuilder, LiveFeedTickResult, LiveFeedVersionManifest,
        LiveFeedsCurrentManifest, PublishedLiveFeedUpdate, SseBroker, SystemClock,
        LIVE_FEEDS_SCHEMA_VERSION,
    },
    products::{
        LiveFeedFetchConfig, MetarLiveFeedBuilder, NexradSourceGridLiveFeedBuilder,
        ObstaclesLiveFeedBuilder, TafLiveFeedBuilder, TfrLiveFeedBuilder,
        WindsAloftLiveFeedBuilder,
    },
    simulation::{
        fixture_loop_duration, next_fixture_loop_virtual_zero, timeline_from_live_feed_root,
        CompiledFixtureStateBuilder, SimulatedLiveFeedSource,
    },
};
use serde::Serialize;

const STATUS_HISTORY_LIMIT: usize = 256;
const SIMULATION_RETAIN_VERSIONS_PER_PRODUCT: usize = 8;
const SIMULATION_PUBLICATION_DIRS: &[&str] = &["states", "versions", "deltas", "packages"];

fn usage() -> &'static str {
    "usage:
  aerobag-live-feedsd --live-root <path> --listen <addr> [--scratch-root <path>] [--event-interval-ms <n>]
  aerobag-live-feedsd --simulation --live-root <path> --listen <addr> [--fixture-root <path>] [--fixture-cache <path>] [--speedup <n>] [--event-interval-ms <n>]
  aerobag-live-feedsd --check-config --live-root <path> --listen <addr> [--simulation --fixture-root <path>]

The daemon owns live-feed polling, publication, static live-feed payload
serving, and SSE invalidation. Vite may proxy /live-feeds to this process in
dev, but Vite must not synthesize live-feed events."
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
    check_config: bool,
    sse_event_limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimulationConfig {
    fixture_root: Option<PathBuf>,
    fixture_cache: PathBuf,
    speedup: u32,
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
    samples: VecDeque<ProductUpdateSample>,
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
    samples: Vec<ProductUpdateSample>,
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

    fn record_tick_result(&self, result: &LiveFeedTickResult) {
        for update in &result.published {
            self.record_product_seen(update);
            if !update.unchanged {
                self.record_product_update(update);
            }
        }
    }

    fn record_product_seen(&self, update: &PublishedLiveFeedUpdate) {
        self.inner
            .lock()
            .expect("live-feed status lock")
            .products
            .entry(update.product.clone())
            .or_default();
    }

    fn record_product_update(&self, update: &PublishedLiveFeedUpdate) {
        let observed_at_utc = Utc::now();
        let delta_bytes = update
            .delta_path
            .as_ref()
            .and_then(|path| fs::metadata(path).ok())
            .map(|metadata| metadata.len());
        let state_bytes = state_bytes_for_status(&update.state_path).ok();
        let mut state = self.inner.lock().expect("live-feed status lock");
        let history = state.products.entry(update.product.clone()).or_default();
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
                        samples: history.samples.iter().cloned().collect(),
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
        let scratch_root = scratch_root.unwrap_or_else(|| live_root.join("../private-work"));
        let fetch_cache_root = fetch_cache_root.unwrap_or_else(|| scratch_root.join("fetch-cache"));
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
        "aerobag-live-feedsd serving {} on http://{}",
        config.live_root.display(),
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
    let live_root = config.live_root.clone();
    let scratch_root = config.scratch_root.join("live-feed-build");
    let poll_interval = Duration::from_millis(config.poll_loop_interval_ms);
    let fetch = live_feed_fetch_config(config)?;
    thread::spawn(move || {
        let publisher = FileLiveFeedPublisher::new(live_root, SystemClock);
        let mut tasks = production_tasks(fetch);
        loop {
            let result = run_upstream_live_feed_publish_tick(
                Utc::now(),
                &mut tasks,
                &scratch_root,
                &publisher,
                &broker,
            );
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
        .unwrap_or_else(|| config.live_root.clone());
    if fixture_root == config.live_root {
        bail!(
            "simulation --fixture-root must be separate from --live-root so generated output can be reset safely"
        );
    }
    let timeline = timeline_from_live_feed_root(&fixture_root, "daemon-simulation")?;
    let products = timeline
        .events
        .iter()
        .map(|event| event.product.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let loop_duration = fixture_loop_duration(&timeline, simulation.speedup)?;
    let live_root = config.live_root.clone();
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

fn production_tasks(fetch: LiveFeedFetchConfig) -> Vec<Box<dyn LiveFeedPollingTask + Send>> {
    vec![
        production_task(
            "metars",
            fetch.clone(),
            MetarLiveFeedBuilder::new(fetch.clone()),
        ),
        production_task(
            "tafs",
            fetch.clone(),
            TafLiveFeedBuilder::new(fetch.clone()),
        ),
        production_task(
            "nexrad",
            fetch.clone(),
            NexradSourceGridLiveFeedBuilder::new(fetch.clone(), false),
        ),
        production_task(
            "tfrs",
            fetch.clone(),
            TfrLiveFeedBuilder::new(fetch.clone()),
        ),
        production_task(
            "winds-aloft",
            fetch.clone(),
            WindsAloftLiveFeedBuilder::new(fetch.clone()),
        ),
        production_task(
            "obstacles",
            fetch.clone(),
            ObstaclesLiveFeedBuilder::new(fetch),
        ),
    ]
}

fn production_task<B>(
    product: &str,
    _fetch: LiveFeedFetchConfig,
    builder: B,
) -> Box<dyn LiveFeedPollingTask + Send>
where
    B: preprocessor_live_feeds::engine::ProductBuilder + Send + 'static,
{
    let interval = default_poll_interval(product).unwrap_or_else(|| Duration::from_secs(5 * 60));
    Box::new(LiveFeedSourceAndBuilder::new(
        IntervalLiveFeedSource::new(product, interval),
        builder,
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
    if request_path == "/live-feeds/events" {
        if method == "HEAD" {
            return write_sse_headers(&mut stream);
        }
        return write_sse_stream(
            &mut stream,
            &config.live_root,
            Duration::from_millis(config.event_interval_ms),
            broker,
            status,
            config.sse_event_limit,
        );
    }
    if let Some(relative) = request_path.strip_prefix("/live-feeds/") {
        return serve_live_feed_file(&mut stream, method, &config.live_root, relative);
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
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache, no-transform\r\nConnection: keep-alive\r\n\r\n"
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
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: {}\r\n\r\n",
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
async function render() {
  const response = await fetch("/live-feeds/status.json", { cache: "no-store" });
  const status = await response.json();
  const products = Object.entries(status.products).sort(([left], [right]) => left.localeCompare(right));
  statusEl.className = "";
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
        <div class="plots">
          <div id="${plotId(product, "update_interval_ms")}" class="plot"></div>
          <div id="${plotId(product, "delta_bytes")}" class="plot"></div>
        </div>
      </div>
    `).join("")}
  `;
  for (const [product, data] of products) {
    renderUpdateIntervalPlot(product, data);
    renderSizePlot(product, data);
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
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
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
        }];
        let mut bytes = Vec::new();
        write_sse_frame(&mut bytes, &frame)?;
        let text = String::from_utf8(bytes)?;
        assert!(text.contains("id: metars:m1\n"));
        assert!(text.contains("event: live-feed-current\n"));
        assert!(text.contains("\"schema_version\":2"), "{text}");
        assert!(text.contains("\"product\":\"metars\""));
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
        assert!(!temp.path().join("versions/metars/v007.json").exists());
        assert!(!temp.path().join("states/metars/v007.json.xz").exists());
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
        let state_path = temp.path().join("states/metars/m1.json");
        fs::create_dir_all(state_path.parent().expect("state parent"))?;
        fs::write(&state_path, b"{\"version_label\":\"m1\"}")?;

        let response = request_once(
            temp.path(),
            "GET /live-feeds/states/metars/m1.json HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )?;
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.contains("Content-Type: application/json"),
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
        publish_json_version(temp.path(), "metars", "m1")?;

        let response = request_once(
            temp.path(),
            "GET /live-feeds/events HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )?;
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(
            response.contains("Content-Type: text/event-stream"),
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
                delta_path: None,
                changed_count: 0,
                removed_count: 0,
            }],
            failures: Vec::new(),
        });

        let snapshot = status.snapshot();
        let metars = snapshot.products.get("metars").expect("METAR status");
        assert!(metars.samples.is_empty());
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

    fn request_once(root: &Path, request: &str) -> anyhow::Result<String> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let config = DaemonConfig {
            live_root: root.to_path_buf(),
            listen: addr,
            scratch_root: root.join("../private-work"),
            fetch_cache_root: root.join("../private-work/fetch-cache"),
            fetch_cache_mode: "offline".to_string(),
            fetch_jobs: 1,
            poll_loop_interval_ms: 1,
            event_interval_ms: 1,
            simulation: None,
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
