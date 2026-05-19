use std::{
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
        default_poll_interval, run_upstream_live_feed_publish_tick, CompiledFixtureCache,
        FileLiveFeedPublisher, FixedClock, FixtureCacheKeyPart, IntervalLiveFeedSource,
        LiveFeedInvalidation, LiveFeedPollingTask, LiveFeedSourceAndBuilder, SseBroker,
        SystemClock,
    },
    products::{
        LiveFeedFetchConfig, MetarLiveFeedBuilder, NexradSourceGridLiveFeedBuilder,
        ObstaclesLiveFeedBuilder, TfrLiveFeedBuilder, WindsAloftLiveFeedBuilder,
    },
    simulation::{
        fixture_loop_duration, next_fixture_loop_virtual_zero, timeline_from_live_feed_root,
        CompiledFixtureStateBuilder, SimulatedLiveFeedSource,
    },
};
use serde::Serialize;

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
}

impl DaemonConfig {
    fn parse(args: impl IntoIterator<Item = String>) -> anyhow::Result<Self> {
        let mut args = args.into_iter();
        let _program = args.next();
        let mut live_root = None;
        let mut listen = None;
        let mut scratch_root = None;
        let mut fetch_cache_root = None;
        let mut fetch_cache_mode = "cache-first".to_string();
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
    start_live_feed_driver(&config, broker.clone())?;
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
                thread::spawn(move || {
                    if let Err(error) = handle_connection(stream, &config, &broker) {
                        eprintln!("live-feed request failed: {error:#}");
                    }
                });
            }
            Err(error) => eprintln!("live-feed accept failed: {error}"),
        }
    }
    Ok(())
}

fn start_live_feed_driver(config: &DaemonConfig, broker: BroadcastSseBroker) -> anyhow::Result<()> {
    if let Some(simulation) = &config.simulation {
        return start_simulation_driver(config, simulation, broker);
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
) -> anyhow::Result<()> {
    let fixture_root = simulation
        .fixture_root
        .clone()
        .unwrap_or_else(|| config.live_root.clone());
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
                log_tick_result("simulation", &result);
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

fn production_tasks(fetch: LiveFeedFetchConfig) -> Vec<Box<dyn LiveFeedPollingTask + Send>> {
    vec![
        production_task(
            "metars",
            fetch.clone(),
            MetarLiveFeedBuilder::new(fetch.clone()),
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
    if request_path == "/live-feeds/events" {
        if method == "HEAD" {
            return write_sse_headers(&mut stream);
        }
        return write_sse_stream(
            &mut stream,
            &config.live_root,
            Duration::from_millis(config.event_interval_ms),
            broker,
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
    event_limit: Option<usize>,
) -> anyhow::Result<()> {
    write_sse_headers(writer)?;
    writeln!(writer, ": aerobag live-feed root {}\n", live_root.display())
        .context("failed to write SSE banner")?;
    let receiver = broker.subscribe();
    let frames = list_live_feed_event_frames(live_root)?;
    let mut sent_events = 0_usize;
    if frames.is_empty() {
        writeln!(
            writer,
            "event: live-feed-heartbeat\ndata: {{\"schema_version\":1,\"products\":[]}}\n"
        )
        .context("failed to write empty SSE heartbeat")?;
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
            Ok(invalidation) => {
                let event = live_feed_sse_event_from_invalidation(invalidation);
                write_sse_event(writer, &event)?;
                sent_events += 1;
                writer.flush().context("failed to flush SSE event")?;
                if event_limit.is_some_and(|limit| sent_events >= limit) {
                    return Ok(());
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                writeln!(
                    writer,
                    "event: live-feed-heartbeat\ndata: {{\"schema_version\":1,\"products\":[]}}\n"
                )
                .context("failed to write SSE heartbeat")?;
                writer.flush().context("failed to flush SSE heartbeat")?;
            }
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
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
            "schema_version": 1,
            "product": event.product,
            "version": event.version,
            "version_manifest_url": event.version_manifest_url,
            "state_url": event.state_url,
            "state_sha256": event.state_sha256,
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
    }
}

#[derive(Clone, Default)]
struct BroadcastSseBroker {
    subscribers: Arc<Mutex<Vec<Sender<LiveFeedInvalidation>>>>,
}

impl BroadcastSseBroker {
    fn subscribe(&self) -> Receiver<LiveFeedInvalidation> {
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
        let mut subscribers = self
            .subscribers
            .lock()
            .expect("live-feed SSE subscriber lock");
        subscribers.retain(|subscriber| subscriber.send(event.clone()).is_ok());
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
            });
        }
    }
    events.sort_by(|left, right| left.id.cmp(&right.id));
    Ok((!events.is_empty()).then_some(events).into_iter().collect())
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
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n\r\n",
        content_type(&file_path),
        bytes.len()
    )
    .context("failed to write response headers")?;
    if method != "HEAD" {
        stream
            .write_all(&bytes)
            .context("failed to write response body")?;
    }
    Ok(())
}

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
        }];
        let mut bytes = Vec::new();
        write_sse_frame(&mut bytes, &frame)?;
        let text = String::from_utf8(bytes)?;
        assert!(text.contains("id: metars:m1\n"));
        assert!(text.contains("event: live-feed-current\n"));
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
        assert_eq!(event.product, "metars");
        assert_eq!(event.version, "m1");
        Ok(())
    }

    #[test]
    fn live_feed_paths_cannot_escape_root() {
        assert!(safe_relative_path("states/metars/v1.json").is_ok());
        assert!(safe_relative_path("../current.json").is_err());
        assert!(safe_relative_path("states/../current.json").is_err());
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
        assert!(response.contains("\"product\":\"metars\""), "{response}");
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
            delta_policy: DeltaPolicy::KeyedRecords {
                records_key: "records".to_string(),
                count_key: Some("record_count".to_string()),
            },
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
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept request");
            handle_connection(stream, &config, &broker).expect("handle request");
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
