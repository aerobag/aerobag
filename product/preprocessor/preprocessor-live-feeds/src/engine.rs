use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc, Mutex,
    },
    time::{Duration as StdDuration, SystemTime},
};

use anyhow::{bail, Context};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub trait Clock {
    fn now_utc(&self) -> DateTime<Utc>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone)]
pub struct FixedClock {
    now: DateTime<Utc>,
}

impl FixedClock {
    pub fn new(now: DateTime<Utc>) -> Self {
        Self { now }
    }
}

impl Clock for FixedClock {
    fn now_utc(&self) -> DateTime<Utc> {
        self.now
    }
}

pub trait UpstreamSource {
    fn product_id(&self) -> &str;
    fn poll_due(&mut self, now: DateTime<Utc>) -> anyhow::Result<Vec<UpstreamEvent>>;
}

#[derive(Debug, Clone)]
pub struct IntervalLiveFeedSource {
    product_id: String,
    interval: StdDuration,
    last_polled_at_utc: Option<DateTime<Utc>>,
}

impl IntervalLiveFeedSource {
    pub fn new(product_id: impl Into<String>, interval: StdDuration) -> Self {
        Self {
            product_id: product_id.into(),
            interval,
            last_polled_at_utc: None,
        }
    }
}

impl UpstreamSource for IntervalLiveFeedSource {
    fn product_id(&self) -> &str {
        &self.product_id
    }

    fn poll_due(&mut self, now: DateTime<Utc>) -> anyhow::Result<Vec<UpstreamEvent>> {
        let due = self
            .last_polled_at_utc
            .map(|last| {
                now.signed_duration_since(last)
                    .to_std()
                    .map(|elapsed| elapsed >= self.interval)
                    .unwrap_or(false)
            })
            .unwrap_or(true);
        if !due {
            return Ok(Vec::new());
        }
        self.last_polled_at_utc = Some(now);
        Ok(vec![UpstreamEvent {
            product: self.product_id.clone(),
            source_id: format!(
                "{}:{}",
                self.product_id,
                now.to_rfc3339_opts(SecondsFormat::Secs, true)
            ),
            observed_at_utc: now,
            payload_path: None,
        }])
    }
}

#[derive(Debug, Clone)]
pub struct QueuedLiveFeedSource {
    product_id: String,
    sender: Sender<UpstreamEvent>,
    receiver: Arc<Mutex<Receiver<UpstreamEvent>>>,
}

impl QueuedLiveFeedSource {
    pub fn new(product_id: impl Into<String>) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            product_id: product_id.into(),
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }

    pub fn sender(&self) -> Sender<UpstreamEvent> {
        self.sender.clone()
    }

    pub fn push(&self, event: UpstreamEvent) -> anyhow::Result<()> {
        if event.product != self.product_id {
            bail!(
                "cannot queue {} event in {} source",
                event.product,
                self.product_id
            );
        }
        self.sender
            .send(event)
            .context("failed to queue live-feed upstream event")
    }
}

impl UpstreamSource for QueuedLiveFeedSource {
    fn product_id(&self) -> &str {
        &self.product_id
    }

    fn poll_due(&mut self, _now: DateTime<Utc>) -> anyhow::Result<Vec<UpstreamEvent>> {
        let receiver = self
            .receiver
            .lock()
            .map_err(|_| anyhow::anyhow!("live-feed upstream queue poisoned"))?;
        let mut events = Vec::new();
        loop {
            match receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty) => return Ok(events),
                Err(TryRecvError::Disconnected) => return Ok(events),
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpstreamEvent {
    pub product: String,
    pub source_id: String,
    pub observed_at_utc: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_path: Option<PathBuf>,
}

pub trait ProductBuilder {
    fn product_id(&self) -> &str;
    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState>;
}

#[derive(Debug, Clone)]
pub enum LiveFeedStatePayload {
    JsonFile {
        path: PathBuf,
        value: Value,
    },
    Directory {
        root: PathBuf,
        manifest_path: PathBuf,
        manifest_value: Value,
    },
}

#[derive(Debug, Clone)]
pub struct BuiltLiveFeedState {
    pub product: String,
    pub version: String,
    pub payload: LiveFeedStatePayload,
    pub delta_policy: DeltaPolicy,
    pub changed_count_if_no_delta: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaPolicy {
    None,
    KeyedRecords {
        records_key: String,
        count_key: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveFeedsCurrentManifest {
    pub schema_version: u32,
    pub generated_at_utc: String,
    pub products: BTreeMap<String, LiveFeedCurrentEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveFeedCurrentEntry {
    pub current: String,
    pub version_manifest_url: String,
    pub state_url: String,
    pub state_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveFeedVersionManifest {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    pub state: LivePayloadRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_from_previous: Option<LiveDeltaRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivePayloadRef {
    pub url: String,
    pub bytes: u64,
    pub blob_sha256: String,
    pub state_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDeltaRef {
    pub from_version: String,
    pub from_state_sha256: String,
    pub to_version: String,
    pub to_state_sha256: String,
    pub url: String,
    pub bytes: u64,
    pub blob_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveFeedRecordDelta {
    pub schema_version: u32,
    pub product: String,
    pub from_version: String,
    pub to_version: String,
    pub top_level_changed: BTreeMap<String, Value>,
    pub top_level_removed: Vec<String>,
    pub changed: BTreeMap<String, Value>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedLiveFeedUpdate {
    pub product: String,
    pub version: String,
    pub unchanged: bool,
    pub state_path: PathBuf,
    pub version_manifest_path: PathBuf,
    pub version_manifest_url: String,
    pub state_url: String,
    pub state_sha256: String,
    pub delta_path: Option<PathBuf>,
    pub changed_count: usize,
    pub removed_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveFeedInvalidation {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    pub version_manifest_url: String,
    pub state_url: String,
    pub state_sha256: String,
}

pub trait LiveFeedPublisher {
    fn publish(&self, built: BuiltLiveFeedState) -> anyhow::Result<PublishedLiveFeedUpdate>;
}

pub trait SseBroker {
    fn announce(&self, event: LiveFeedInvalidation) -> anyhow::Result<()>;
}

pub trait LiveFeedProductTask {
    fn product_id(&self) -> &str;
    fn build_state(&mut self) -> anyhow::Result<BuiltLiveFeedState>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveFeedTickResult {
    pub published: Vec<PublishedLiveFeedUpdate>,
    pub failures: Vec<FailedLiveFeedTask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedLiveFeedTask {
    pub product: String,
    pub phase: LiveFeedTaskPhase,
    pub error: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveFeedTaskPhase {
    Poll,
    Build,
    Publish,
    Announce,
}

pub trait LiveFeedPollingTask {
    fn product_id(&self) -> &str;
    fn poll_due(&mut self, now: DateTime<Utc>) -> anyhow::Result<Vec<UpstreamEvent>>;
    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState>;
}

impl<T> LiveFeedPollingTask for Box<T>
where
    T: LiveFeedPollingTask + ?Sized,
{
    fn product_id(&self) -> &str {
        (**self).product_id()
    }

    fn poll_due(&mut self, now: DateTime<Utc>) -> anyhow::Result<Vec<UpstreamEvent>> {
        (**self).poll_due(now)
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        (**self).build_state(event, scratch_dir)
    }
}

#[derive(Debug, Clone)]
pub struct LiveFeedSourceAndBuilder<S, B> {
    source: S,
    builder: B,
}

impl<S, B> LiveFeedSourceAndBuilder<S, B> {
    pub fn new(source: S, builder: B) -> Self {
        Self { source, builder }
    }
}

impl<S, B> LiveFeedPollingTask for LiveFeedSourceAndBuilder<S, B>
where
    S: UpstreamSource,
    B: ProductBuilder,
{
    fn product_id(&self) -> &str {
        self.source.product_id()
    }

    fn poll_due(&mut self, now: DateTime<Utc>) -> anyhow::Result<Vec<UpstreamEvent>> {
        self.source.poll_due(now)
    }

    fn build_state(
        &self,
        event: &UpstreamEvent,
        scratch_dir: &Path,
    ) -> anyhow::Result<BuiltLiveFeedState> {
        if self.builder.product_id() != self.source.product_id() {
            bail!(
                "source product {} is wired to builder product {}",
                self.source.product_id(),
                self.builder.product_id()
            );
        }
        self.builder.build_state(event, scratch_dir)
    }
}

pub fn run_live_feed_publish_tick<T, P, B>(
    products: &mut [T],
    publisher: &P,
    broker: &B,
) -> LiveFeedTickResult
where
    T: LiveFeedProductTask,
    P: LiveFeedPublisher,
    B: SseBroker,
{
    let mut published = Vec::new();
    let mut failures = Vec::new();
    for product in products {
        let product_id = product.product_id().to_string();
        let built = match product.build_state() {
            Ok(built) => built,
            Err(error) => {
                failures.push(FailedLiveFeedTask {
                    product: product_id,
                    phase: LiveFeedTaskPhase::Build,
                    error: format!("{error:#}"),
                });
                continue;
            }
        };
        if let Some(update) =
            publish_and_announce(product_id, built, publisher, broker, &mut failures)
        {
            published.push(update);
        }
    }
    LiveFeedTickResult {
        published,
        failures,
    }
}

pub fn run_upstream_live_feed_publish_tick<T, P, B>(
    now: DateTime<Utc>,
    tasks: &mut [T],
    scratch_root: &Path,
    publisher: &P,
    broker: &B,
) -> LiveFeedTickResult
where
    T: LiveFeedPollingTask,
    P: LiveFeedPublisher,
    B: SseBroker,
{
    let mut published = Vec::new();
    let mut failures = Vec::new();
    for task in tasks {
        let product_id = task.product_id().to_string();
        let events = match task.poll_due(now) {
            Ok(events) => events,
            Err(error) => {
                failures.push(FailedLiveFeedTask {
                    product: product_id,
                    phase: LiveFeedTaskPhase::Poll,
                    error: format!("{error:#}"),
                });
                continue;
            }
        };
        for event in events {
            if event.product != product_id {
                failures.push(FailedLiveFeedTask {
                    product: product_id.clone(),
                    phase: LiveFeedTaskPhase::Build,
                    error: format!("source emitted event for product {}", event.product),
                });
                continue;
            }
            let scratch_dir = live_feed_event_scratch_dir(scratch_root, &product_id, &event);
            let built = match task.build_state(&event, &scratch_dir) {
                Ok(built) => built,
                Err(error) => {
                    failures.push(FailedLiveFeedTask {
                        product: product_id.clone(),
                        phase: LiveFeedTaskPhase::Build,
                        error: format!("{error:#}"),
                    });
                    continue;
                }
            };
            if let Some(update) =
                publish_and_announce(product_id.clone(), built, publisher, broker, &mut failures)
            {
                published.push(update);
            }
        }
    }
    LiveFeedTickResult {
        published,
        failures,
    }
}

fn publish_and_announce<P, B>(
    product_id: String,
    built: BuiltLiveFeedState,
    publisher: &P,
    broker: &B,
    failures: &mut Vec<FailedLiveFeedTask>,
) -> Option<PublishedLiveFeedUpdate>
where
    P: LiveFeedPublisher,
    B: SseBroker,
{
    let update = match publisher.publish(built) {
        Ok(update) => update,
        Err(error) => {
            failures.push(FailedLiveFeedTask {
                product: product_id,
                phase: LiveFeedTaskPhase::Publish,
                error: format!("{error:#}"),
            });
            return None;
        }
    };
    if update.unchanged {
        return Some(update);
    }
    let invalidation = live_feed_invalidation_from_update(&update);
    if let Err(error) = broker.announce(invalidation) {
        failures.push(FailedLiveFeedTask {
            product: update.product.clone(),
            phase: LiveFeedTaskPhase::Announce,
            error: format!("{error:#}"),
        });
    }
    Some(update)
}

pub fn live_feed_event_scratch_dir(
    scratch_root: &Path,
    product: &str,
    event: &UpstreamEvent,
) -> PathBuf {
    scratch_root
        .join(product)
        .join(sha256_hex(event.source_id.as_bytes()))
}

pub fn default_poll_interval(product_id: &str) -> Option<StdDuration> {
    match product_id {
        "nexrad" => Some(StdDuration::from_secs(60)),
        "metars" | "tfrs" => Some(StdDuration::from_secs(5 * 60)),
        "winds-aloft" => Some(StdDuration::from_secs(60 * 60)),
        "obstacles" => Some(StdDuration::from_secs(6 * 60 * 60)),
        _ => None,
    }
}

pub fn live_feed_invalidation_from_update(
    update: &PublishedLiveFeedUpdate,
) -> LiveFeedInvalidation {
    LiveFeedInvalidation {
        schema_version: 1,
        product: update.product.clone(),
        version: update.version.clone(),
        version_manifest_url: update.version_manifest_url.clone(),
        state_url: update.state_url.clone(),
        state_sha256: update.state_sha256.clone(),
    }
}

pub struct FileLiveFeedPublisher<C> {
    root: PathBuf,
    clock: C,
}

impl<C: Clock> FileLiveFeedPublisher<C> {
    pub fn new(root: PathBuf, clock: C) -> Self {
        Self { root, clock }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn publish_and_invalidation(
        &self,
        built: BuiltLiveFeedState,
    ) -> anyhow::Result<(PublishedLiveFeedUpdate, LiveFeedInvalidation)> {
        let update = self.publish(built)?;
        let invalidation = live_feed_invalidation_from_update(&update);
        Ok((update, invalidation))
    }
}

impl<C: Clock> LiveFeedPublisher for FileLiveFeedPublisher<C> {
    fn publish(&self, built: BuiltLiveFeedState) -> anyhow::Result<PublishedLiveFeedUpdate> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))?;
        let BuiltLiveFeedState {
            product,
            version,
            payload,
            delta_policy,
            changed_count_if_no_delta,
        } = built;
        match payload {
            LiveFeedStatePayload::JsonFile { path, value } => self.publish_json_state(
                product,
                version,
                path,
                value,
                delta_policy,
                changed_count_if_no_delta,
            ),
            LiveFeedStatePayload::Directory {
                root,
                manifest_path,
                manifest_value,
            } => self.publish_directory_state(
                product,
                version,
                root,
                manifest_path,
                manifest_value,
                delta_policy,
                changed_count_if_no_delta,
            ),
        }
    }
}

impl<C: Clock> FileLiveFeedPublisher<C> {
    fn publish_json_state(
        &self,
        product: String,
        version: String,
        source_path: PathBuf,
        state_value: Value,
        delta_policy: DeltaPolicy,
        changed_count_if_no_delta: usize,
    ) -> anyhow::Result<PublishedLiveFeedUpdate> {
        let state_dir = self.root.join("states").join(&product);
        let state_path = state_dir.join(format!("{version}.json"));
        fs::create_dir_all(&state_dir)
            .with_context(|| format!("failed to create {}", state_dir.display()))?;
        copy_file_if_missing(&source_path, &state_path)?;
        self.publish_state_common(
            product,
            version,
            state_path,
            state_value,
            delta_policy,
            changed_count_if_no_delta,
            None,
        )
    }

    fn publish_directory_state(
        &self,
        product: String,
        version: String,
        source_root: PathBuf,
        manifest_path: PathBuf,
        manifest_value: Value,
        delta_policy: DeltaPolicy,
        changed_count_if_no_delta: usize,
    ) -> anyhow::Result<PublishedLiveFeedUpdate> {
        let state_dir = self.root.join("states").join(&product).join(&version);
        let state_path = state_dir.join("manifest.json");
        if !state_path.is_file() {
            if state_dir.exists() {
                fs::remove_dir_all(&state_dir)
                    .with_context(|| format!("failed to remove {}", state_dir.display()))?;
            }
            hardlink_or_copy_dir_recursive(&source_root, &state_dir)?;
        }
        if !state_path.is_file() {
            copy_file_if_missing(&manifest_path, &state_path)?;
        }
        self.publish_state_common(
            product,
            version,
            state_path,
            manifest_value,
            delta_policy,
            changed_count_if_no_delta,
            Some(state_dir),
        )
    }

    fn publish_state_common(
        &self,
        product: String,
        version: String,
        state_path: PathBuf,
        state_value: Value,
        delta_policy: DeltaPolicy,
        changed_count_if_no_delta: usize,
        state_root: Option<PathBuf>,
    ) -> anyhow::Result<PublishedLiveFeedUpdate> {
        let state_bytes = fs::read(&state_path)
            .with_context(|| format!("failed to read {}", state_path.display()))?;
        let state_blob_sha256 = sha256_hex(&state_bytes);
        let state_sha256 = canonical_json_sha256(&state_value)?;
        let previous_entry = read_live_feeds_current(&self.root)?
            .and_then(|current| current.products.get(&product).cloned());

        if let Some(previous) = previous_entry.as_ref() {
            if previous.current == version {
                if previous.state_sha256 != state_sha256 {
                    bail!(
                        "current {product} state hash mismatch for {}: expected {}, got {}",
                        previous.current,
                        previous.state_sha256,
                        state_sha256
                    );
                }
                return Ok(PublishedLiveFeedUpdate {
                    product,
                    version,
                    unchanged: true,
                    state_path,
                    version_manifest_path: self.root.join(&previous.version_manifest_url),
                    version_manifest_url: previous.version_manifest_url.clone(),
                    state_url: previous.state_url.clone(),
                    state_sha256: previous.state_sha256.clone(),
                    delta_path: None,
                    changed_count: 0,
                    removed_count: 0,
                });
            }
        }

        let mut previous_version = None;
        let mut delta_ref = None;
        let mut delta_path = None;
        let mut changed_count = changed_count_if_no_delta;
        let mut removed_count = 0;

        if let (Some(previous), DeltaPolicy::KeyedRecords { records_key, .. }) =
            (previous_entry.as_ref(), &delta_policy)
        {
            let previous_state_path = self.root.join(&previous.state_url);
            let previous_state = read_json_value(&previous_state_path)?;
            let previous_sha256 = canonical_json_sha256(&previous_state)?;
            if previous_sha256 != previous.state_sha256 {
                bail!(
                    "previous {product} state hash mismatch for {}: expected {}, got {}",
                    previous.current,
                    previous.state_sha256,
                    previous_sha256
                );
            }
            let delta = build_record_delta(&product, records_key, &previous_state, &state_value)?;
            changed_count = delta.changed.len();
            removed_count = delta.removed.len();
            let product_delta_dir = self.root.join("deltas").join(&product);
            fs::create_dir_all(&product_delta_dir)
                .with_context(|| format!("failed to create {}", product_delta_dir.display()))?;
            let path = product_delta_dir.join(format!("{}__{}.json", previous.current, version));
            write_json_pretty_file(&path, &delta)?;
            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            delta_ref = Some(LiveDeltaRef {
                from_version: previous.current.clone(),
                from_state_sha256: previous.state_sha256.clone(),
                to_version: version.clone(),
                to_state_sha256: state_sha256.clone(),
                url: live_feeds_relative_url(&self.root, &path)?,
                bytes: bytes.len() as u64,
                blob_sha256: sha256_hex(&bytes),
            });
            previous_version = Some(previous.current.clone());
            delta_path = Some(path);
        }

        let state_ref = LivePayloadRef {
            url: live_feeds_relative_url(&self.root, &state_path)?,
            bytes: state_bytes.len() as u64,
            blob_sha256: state_blob_sha256,
            state_sha256: state_sha256.clone(),
        };
        let version_dir = self.root.join("versions").join(&product);
        fs::create_dir_all(&version_dir)
            .with_context(|| format!("failed to create {}", version_dir.display()))?;
        let version_manifest_path = version_dir.join(format!("{version}.json"));
        let version_manifest_url = live_feeds_relative_url(&self.root, &version_manifest_path)?;
        let state_url = state_root
            .as_ref()
            .map(|_| live_feeds_relative_url(&self.root, &state_path))
            .unwrap_or_else(|| live_feeds_relative_url(&self.root, &state_path))?;
        write_json_pretty_file(
            &version_manifest_path,
            &LiveFeedVersionManifest {
                schema_version: 1,
                product: product.clone(),
                version: version.clone(),
                previous: previous_version,
                state: state_ref,
                delta_from_previous: delta_ref,
            },
        )?;
        let mut current =
            read_live_feeds_current(&self.root)?.unwrap_or(LiveFeedsCurrentManifest {
                schema_version: 1,
                generated_at_utc: self
                    .clock
                    .now_utc()
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
                products: BTreeMap::new(),
            });
        current.generated_at_utc = self
            .clock
            .now_utc()
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        current.products.insert(
            product.clone(),
            LiveFeedCurrentEntry {
                current: version.clone(),
                version_manifest_url: version_manifest_url.clone(),
                state_url: state_url.clone(),
                state_sha256: state_sha256.clone(),
            },
        );
        write_live_feeds_current_manifest(&self.root, &current)?;

        Ok(PublishedLiveFeedUpdate {
            product,
            version,
            unchanged: false,
            state_path,
            version_manifest_path,
            version_manifest_url,
            state_url,
            state_sha256,
            delta_path,
            changed_count,
            removed_count,
        })
    }
}

pub fn build_record_delta(
    product: &str,
    records_key: &str,
    from_state: &Value,
    to_state: &Value,
) -> anyhow::Result<LiveFeedRecordDelta> {
    let from_version = state_version_label(from_state)?;
    let to_version = state_version_label(to_state)?;
    let from_object = state_object(from_state)?;
    let to_object = state_object(to_state)?;
    let from_records = state_record_map(from_state, records_key)?;
    let to_records = state_record_map(to_state, records_key)?;

    let mut top_level_changed = BTreeMap::new();
    for (key, to_value) in to_object {
        if key == "version_label" || key == records_key {
            continue;
        }
        if from_object.get(key) != Some(to_value) {
            top_level_changed.insert(key.clone(), to_value.clone());
        }
    }
    let mut top_level_removed = from_object
        .keys()
        .filter(|key| key.as_str() != "version_label" && key.as_str() != records_key)
        .filter(|key| !to_object.contains_key(*key))
        .cloned()
        .collect::<Vec<_>>();
    top_level_removed.sort();

    let mut changed = BTreeMap::new();
    for (record_id, to_record) in to_records {
        if from_records.get(record_id) != Some(to_record) {
            changed.insert(record_id.clone(), to_record.clone());
        }
    }
    let mut removed = from_records
        .keys()
        .filter(|record_id| !to_records.contains_key(*record_id))
        .cloned()
        .collect::<Vec<_>>();
    removed.sort();

    Ok(LiveFeedRecordDelta {
        schema_version: 1,
        product: product.to_string(),
        from_version: from_version.to_string(),
        to_version: to_version.to_string(),
        top_level_changed,
        top_level_removed,
        changed,
        removed,
    })
}

pub fn apply_record_delta(
    records_key: &str,
    count_key: Option<&str>,
    from_state: &Value,
    delta: &LiveFeedRecordDelta,
) -> anyhow::Result<Value> {
    let from_version = state_version_label(from_state)?;
    if from_version != delta.from_version {
        bail!(
            "delta starts at {}, but local state is {}",
            delta.from_version,
            from_version
        );
    }
    let mut result = from_state.clone();
    {
        let result_object = result
            .as_object_mut()
            .context("live feed state must be a JSON object")?;
        for key in &delta.top_level_removed {
            result_object.remove(key);
        }
        for (key, value) in &delta.top_level_changed {
            result_object.insert(key.clone(), value.clone());
        }
    }
    let record_count = {
        let records = result
            .get_mut(records_key)
            .and_then(Value::as_object_mut)
            .with_context(|| format!("state missing {records_key} object"))?;
        for record_id in &delta.removed {
            records.remove(record_id);
        }
        for (record_id, record) in &delta.changed {
            records.insert(record_id.clone(), record.clone());
        }
        records.len()
    };
    *result
        .get_mut("version_label")
        .context("state missing version_label")? = Value::String(delta.to_version.clone());
    if let Some(count_key) = count_key {
        if let Some(count) = result.get_mut(count_key) {
            *count = serde_json::json!(record_count);
        }
    }
    Ok(result)
}

fn state_object(state: &Value) -> anyhow::Result<&serde_json::Map<String, Value>> {
    state
        .as_object()
        .context("live feed state must be a JSON object")
}

pub fn read_live_feeds_current(root: &Path) -> anyhow::Result<Option<LiveFeedsCurrentManifest>> {
    let path = root.join("current.json");
    if !path.is_file() {
        return Ok(None);
    }
    let current = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(current))
}

pub fn write_live_feeds_current_manifest(
    root: &Path,
    current: &LiveFeedsCurrentManifest,
) -> anyhow::Result<PathBuf> {
    let path = root.join("current.json");
    write_json_pretty_file(&path, current)?;
    Ok(path)
}

pub fn canonical_json_sha256(value: &Value) -> anyhow::Result<String> {
    let canonical = canonical_json_value(value);
    let bytes = serde_json::to_vec(&canonical).context("failed to encode canonical JSON")?;
    Ok(sha256_hex(&bytes))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

pub fn read_json_value(path: &Path) -> anyhow::Result<Value> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

pub fn write_json_pretty_file(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(value).context("failed to encode JSON")?,
    )
    .with_context(|| format!("failed to write {}", path.display()))
}

pub fn fixture_cache_key(parts: &[FixtureCacheKeyPart]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.name.as_bytes());
        hasher.update([0]);
        hasher.update(part.sha256.as_bytes());
        hasher.update([0xff]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureCacheKeyPart {
    pub name: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledFixtureManifest {
    pub schema_version: u32,
    pub fixture_id: String,
    pub cache_key: String,
    pub generated_at_utc: String,
    pub timeline_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledFixtureTimeline {
    pub schema_version: u32,
    pub fixture_id: String,
    pub events: Vec<CompiledFixtureEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompiledFixtureEvent {
    pub product: String,
    pub version: String,
    pub observed_at_utc: DateTime<Utc>,
    pub version_manifest_url: String,
    pub state_url: String,
    pub state_sha256: String,
}

pub struct CompiledFixtureCache<C> {
    root: PathBuf,
    clock: C,
}

impl<C: Clock> CompiledFixtureCache<C> {
    pub fn new(root: PathBuf, clock: C) -> Self {
        Self { root, clock }
    }

    pub fn compiled_root(&self, cache_key: &str) -> PathBuf {
        self.root.join(cache_key)
    }

    pub fn load_manifest(
        &self,
        cache_key: &str,
    ) -> anyhow::Result<Option<CompiledFixtureManifest>> {
        let path = self.compiled_root(cache_key).join("manifest.json");
        if !path.is_file() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_slice(
            &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
        )?))
    }

    pub fn write_manifest_and_timeline(
        &self,
        fixture_id: &str,
        cache_key: &str,
        timeline: &CompiledFixtureTimeline,
    ) -> anyhow::Result<CompiledFixtureManifest> {
        let root = self.compiled_root(cache_key);
        fs::create_dir_all(&root)
            .with_context(|| format!("failed to create {}", root.display()))?;
        let timeline_path = root.join("timeline.json");
        write_json_pretty_file(&timeline_path, timeline)?;
        let manifest = CompiledFixtureManifest {
            schema_version: 1,
            fixture_id: fixture_id.to_string(),
            cache_key: cache_key.to_string(),
            generated_at_utc: self
                .clock
                .now_utc()
                .to_rfc3339_opts(SecondsFormat::Secs, true),
            timeline_path: "timeline.json".to_string(),
        };
        write_json_pretty_file(&root.join("manifest.json"), &manifest)?;
        Ok(manifest)
    }
}

fn state_version_label(state: &Value) -> anyhow::Result<&str> {
    state
        .get("version_label")
        .and_then(Value::as_str)
        .context("state missing version_label")
}

fn state_record_map<'a>(
    state: &'a Value,
    records_key: &str,
) -> anyhow::Result<&'a serde_json::Map<String, Value>> {
    state
        .get(records_key)
        .and_then(Value::as_object)
        .with_context(|| format!("state missing {records_key} object"))
}

fn canonical_json_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), canonical_json_value(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(canonical_json_value).collect()),
        other => other.clone(),
    }
}

fn live_feeds_relative_url(root: &Path, path: &Path) -> anyhow::Result<String> {
    path.strip_prefix(root)
        .with_context(|| format!("{} is not under {}", path.display(), root.display()))
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn copy_file_if_missing(source: &Path, target: &Path) -> anyhow::Result<()> {
    if target.is_file() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    match fs::hard_link(source, target) {
        Ok(()) => Ok(()),
        Err(link_error) => {
            fs::copy(source, target).with_context(|| {
                format!(
                    "failed to copy {} to {} after hard-link error: {link_error}",
                    source.display(),
                    target.display()
                )
            })?;
            Ok(())
        }
    }
}

fn hardlink_or_copy_dir_recursive(source_dir: &Path, output_dir: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create {}", output_dir.display()))?;
    let mut entries = fs::read_dir(source_dir)
        .with_context(|| format!("failed to read {}", source_dir.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let source = entry.path();
        let output = output_dir.join(entry.file_name());
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", source.display()))?;
        if file_type.is_dir() {
            hardlink_or_copy_dir_recursive(&source, &output)?;
        } else if file_type.is_file() {
            copy_file_if_missing(&source, &output)?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn modified_time(path: &Path) -> SystemTime {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::{Cursor, Read, Write};
    use std::sync::Mutex;
    use tempfile::tempdir;
    use zip::{
        write::SimpleFileOptions, CompressionMethod, DateTime as ZipDateTime, ZipArchive, ZipWriter,
    };

    #[test]
    fn record_delta_round_trips_canonical_state() -> anyhow::Result<()> {
        let from = serde_json::json!({
            "version_label": "v1",
            "record_count": 2,
            "records": {
                "A": {"value": 1},
                "B": {"value": 2}
            }
        });
        let to = serde_json::json!({
            "version_label": "v2",
            "record_count": 2,
            "records": {
                "B": {"value": 3},
                "C": {"value": 4}
            }
        });

        let delta = build_record_delta("test", "records", &from, &to)?;
        assert_eq!(
            delta.changed.keys().cloned().collect::<Vec<_>>(),
            vec!["B", "C"]
        );
        assert_eq!(delta.removed, vec!["A"]);
        assert_eq!(
            apply_record_delta("records", Some("record_count"), &from, &delta)?,
            to
        );
        Ok(())
    }

    #[test]
    fn record_delta_round_trips_changed_top_level_fields() -> anyhow::Result<()> {
        let from = serde_json::json!({
            "version_label": "v1",
            "record_count": 1,
            "generated_at_utc": "2026-05-18T20:00:00Z",
            "records": {
                "KAAA": {"value": 1}
            }
        });
        let to = serde_json::json!({
            "version_label": "v2",
            "record_count": 1,
            "generated_at_utc": "2026-05-18T20:05:00Z",
            "records": {
                "KAAA": {"value": 1}
            }
        });

        let delta = build_record_delta("metars", "records", &from, &to)?;

        assert_eq!(
            apply_record_delta("records", Some("record_count"), &from, &delta)?,
            to
        );
        Ok(())
    }

    #[test]
    fn generic_metar_delta_fixture_reconstructs_three_hour_capture() -> anyhow::Result<()> {
        let states = metar_delta_fixture_states()?;

        println!(
            "{:<17} {:>10} {:>10} {:>10} {:>10} {:>8} {:>8} {:>8} {:>8}",
            "to_version",
            "state_raw",
            "delta_raw",
            "state_zip",
            "delta_zip",
            "zip_rat",
            "changed",
            "removed",
            "top"
        );
        let mut compressed_ratios = Vec::new();
        for pair in states.windows(2) {
            let from = &pair[0];
            let to = &pair[1];
            let delta = build_record_delta("metars", "metars_by_station", from, to)?;
            let applied =
                apply_record_delta("metars_by_station", Some("metar_count"), from, &delta)?;
            assert_eq!(
                applied, *to,
                "delta {} -> {} did not reconstruct target state",
                delta.from_version, delta.to_version
            );

            let state_bytes = serde_json::to_vec(to)?;
            let delta_bytes = serde_json::to_vec(&delta)?;
            let state_zip_bytes = deflated_zip_member_size(&state_bytes)?;
            let delta_zip_bytes = deflated_zip_member_size(&delta_bytes)?;
            let compressed_ratio = delta_zip_bytes as f64 / state_zip_bytes as f64;
            compressed_ratios.push(compressed_ratio);
            println!(
                "{:<17} {:>10} {:>10} {:>10} {:>10} {:>8.3} {:>8} {:>8} {:>8}",
                delta.to_version,
                state_bytes.len(),
                delta_bytes.len(),
                state_zip_bytes,
                delta_zip_bytes,
                compressed_ratio,
                delta.changed.len(),
                delta.removed.len(),
                delta.top_level_changed.len() + delta.top_level_removed.len()
            );
        }
        assert!(
            compressed_ratios.iter().all(|ratio| *ratio < 0.5),
            "expected all compressed METAR deltas to be less than 0.5 of compressed full state: {compressed_ratios:?}"
        );
        compressed_ratios.sort_by(|left, right| left.total_cmp(right));
        let median_ratio = compressed_ratios[compressed_ratios.len() / 2];
        assert!(
            median_ratio < 0.15,
            "expected median compressed METAR delta ratio below 0.15, got {median_ratio:.3}"
        );
        Ok(())
    }

    #[test]
    fn file_publisher_writes_state_delta_current_and_invalidation() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path().join("live-feeds");
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 1, 2, 3).unwrap());
        let publisher = FileLiveFeedPublisher::new(root.clone(), clock);

        let first_path = temp.path().join("first.json");
        let first = serde_json::json!({
            "version_label": "v1",
            "records": {"A": {"value": 1}},
            "record_count": 1
        });
        write_json_pretty_file(&first_path, &first)?;
        let (first_update, first_event) =
            publisher.publish_and_invalidation(BuiltLiveFeedState {
                product: "metars".to_string(),
                version: "v1".to_string(),
                payload: LiveFeedStatePayload::JsonFile {
                    path: first_path,
                    value: first,
                },
                delta_policy: DeltaPolicy::KeyedRecords {
                    records_key: "records".to_string(),
                    count_key: Some("record_count".to_string()),
                },
                changed_count_if_no_delta: 1,
            })?;
        assert_eq!(first_update.changed_count, 1);
        assert_eq!(first_event.product, "metars");
        assert!(root.join("current.json").is_file());

        let second_path = temp.path().join("second.json");
        let second = serde_json::json!({
            "version_label": "v2",
            "records": {"A": {"value": 2}, "B": {"value": 3}},
            "record_count": 2
        });
        write_json_pretty_file(&second_path, &second)?;
        let (second_update, second_event) =
            publisher.publish_and_invalidation(BuiltLiveFeedState {
                product: "metars".to_string(),
                version: "v2".to_string(),
                payload: LiveFeedStatePayload::JsonFile {
                    path: second_path,
                    value: second,
                },
                delta_policy: DeltaPolicy::KeyedRecords {
                    records_key: "records".to_string(),
                    count_key: Some("record_count".to_string()),
                },
                changed_count_if_no_delta: 2,
            })?;
        assert_eq!(second_update.changed_count, 2);
        assert_eq!(second_update.removed_count, 0);
        assert!(second_update.delta_path.expect("delta").is_file());
        assert_eq!(second_event.version, "v2");
        assert!(root.join("versions/metars/v2.json").is_file());
        Ok(())
    }

    #[test]
    fn publish_tick_continues_after_one_product_fails() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new(
            root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 4, 5, 6).unwrap()),
        );
        let broker = RecordingBroker::default();

        let mut products = vec![
            StaticProductTask::state(
                "metars",
                json_state(
                    temp.path(),
                    "metars",
                    "metars-v1",
                    "m1",
                    "records",
                    &[("KSEA", 1)],
                )?,
            ),
            StaticProductTask::failure("tfrs", "upstream unavailable"),
            StaticProductTask::state(
                "winds-aloft",
                json_state(
                    temp.path(),
                    "winds-aloft",
                    "winds-v1",
                    "w1",
                    "records",
                    &[("SEA030", 3000)],
                )?,
            ),
        ];

        let result = run_live_feed_publish_tick(&mut products, &publisher, &broker);

        assert_eq!(
            result
                .published
                .iter()
                .map(|update| update.product.as_str())
                .collect::<Vec<_>>(),
            vec!["metars", "winds-aloft"]
        );
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].product, "tfrs");
        assert_eq!(result.failures[0].phase, LiveFeedTaskPhase::Build);
        let current = read_live_feeds_current(&root)?.expect("current");
        assert!(current.products.contains_key("metars"));
        assert!(current.products.contains_key("winds-aloft"));
        assert!(!current.products.contains_key("tfrs"));
        assert_eq!(
            broker
                .events()
                .iter()
                .map(|event| event.product.as_str())
                .collect::<Vec<_>>(),
            vec!["metars", "winds-aloft"]
        );
        Ok(())
    }

    #[test]
    fn upstream_publish_tick_polls_builds_publishes_and_announces_due_events() -> anyhow::Result<()>
    {
        let temp = tempdir()?;
        let live_root = temp.path().join("live-feeds");
        let scratch_root = temp.path().join("scratch");
        let publisher = FileLiveFeedPublisher::new(
            live_root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 4, 5, 6).unwrap()),
        );
        let broker = RecordingBroker::default();
        let mut tasks = vec![LiveFeedSourceAndBuilder::new(
            StaticSource {
                product: "metars".to_string(),
                events: vec![UpstreamEvent {
                    product: "metars".to_string(),
                    source_id: "m1".to_string(),
                    observed_at_utc: Utc.with_ymd_and_hms(2026, 5, 18, 4, 0, 0).unwrap(),
                    payload_path: None,
                }],
            },
            EchoBuilder {
                product: "metars".to_string(),
            },
        )];

        let result = run_upstream_live_feed_publish_tick(
            Utc.with_ymd_and_hms(2026, 5, 18, 4, 0, 0).unwrap(),
            &mut tasks,
            &scratch_root,
            &publisher,
            &broker,
        );

        assert!(result.failures.is_empty(), "{:#?}", result.failures);
        assert_eq!(result.published.len(), 1);
        assert_eq!(result.published[0].product, "metars");
        assert!(result.published[0].state_path.is_file());
        assert!(result.published[0].version_manifest_path.is_file());
        assert_eq!(broker.events()[0].version, "m1");
        assert!(read_live_feeds_current(&live_root)?
            .expect("current")
            .products
            .contains_key("metars"));
        Ok(())
    }

    #[test]
    fn repeated_version_publish_does_not_reannounce() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let live_root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new(
            live_root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 4, 5, 6).unwrap()),
        );
        let broker = RecordingBroker::default();
        let mut products = vec![StaticProductTask::state(
            "metars",
            json_state(
                temp.path(),
                "metars",
                "metars-v1",
                "v1",
                "records",
                &[("A", 1)],
            )?,
        )];

        let first = run_live_feed_publish_tick(&mut products, &publisher, &broker);
        assert!(first.failures.is_empty());
        assert_eq!(first.published.len(), 1);
        assert!(!first.published[0].unchanged);
        assert_eq!(broker.events().len(), 1);

        let mut products = vec![StaticProductTask::state(
            "metars",
            json_state(
                temp.path(),
                "metars",
                "metars-v1-again",
                "v1",
                "records",
                &[("A", 1)],
            )?,
        )];
        let second = run_live_feed_publish_tick(&mut products, &publisher, &broker);

        assert!(second.failures.is_empty());
        assert_eq!(second.published.len(), 1);
        assert!(second.published[0].unchanged);
        assert_eq!(
            broker.events().len(),
            1,
            "unchanged states should not emit duplicate SSE invalidations"
        );
        assert_eq!(
            read_live_feeds_current(&live_root)?
                .expect("current")
                .products["metars"]
                .current,
            "v1"
        );
        Ok(())
    }

    #[test]
    fn interval_source_and_push_source_emit_due_events() -> anyhow::Result<()> {
        let t0 = Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap();
        let mut interval = IntervalLiveFeedSource::new("metars", StdDuration::from_secs(60));
        assert_eq!(interval.poll_due(t0)?.len(), 1);
        assert!(interval
            .poll_due(t0 + chrono::Duration::seconds(59))?
            .is_empty());
        assert_eq!(
            interval
                .poll_due(t0 + chrono::Duration::seconds(60))?
                .first()
                .map(|event| event.product.as_str()),
            Some("metars")
        );

        let mut queued = QueuedLiveFeedSource::new("tfrs");
        queued.push(UpstreamEvent {
            product: "tfrs".to_string(),
            source_id: "event-1".to_string(),
            observed_at_utc: t0,
            payload_path: None,
        })?;
        let events = queued.poll_due(t0)?;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source_id, "event-1");
        assert!(queued.poll_due(t0)?.is_empty());
        Ok(())
    }

    #[test]
    fn default_poll_intervals_match_measured_product_cadence() {
        assert_eq!(
            default_poll_interval("nexrad"),
            Some(StdDuration::from_secs(60))
        );
        assert_eq!(
            default_poll_interval("metars"),
            Some(StdDuration::from_secs(5 * 60))
        );
        assert_eq!(
            default_poll_interval("tfrs"),
            Some(StdDuration::from_secs(5 * 60))
        );
        assert_eq!(
            default_poll_interval("winds-aloft"),
            Some(StdDuration::from_secs(60 * 60))
        );
        assert_eq!(
            default_poll_interval("obstacles"),
            Some(StdDuration::from_secs(6 * 60 * 60))
        );
        assert_eq!(default_poll_interval("unknown"), None);
    }

    #[test]
    fn fixture_cache_key_is_stable_and_manifest_is_reusable() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let cache = CompiledFixtureCache::new(
            temp.path().join("compiled"),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 0, 0, 0).unwrap()),
        );
        let key = fixture_cache_key(&[
            FixtureCacheKeyPart {
                name: "raw".to_string(),
                sha256: "a".repeat(64),
            },
            FixtureCacheKeyPart {
                name: "builder".to_string(),
                sha256: "b".repeat(64),
            },
        ]);
        assert_eq!(key.len(), 64);
        assert!(cache.load_manifest(&key)?.is_none());

        let timeline = CompiledFixtureTimeline {
            schema_version: 1,
            fixture_id: "metars-three-hour".to_string(),
            events: vec![CompiledFixtureEvent {
                product: "metars".to_string(),
                version: "v1".to_string(),
                observed_at_utc: Utc.with_ymd_and_hms(2026, 5, 18, 0, 0, 0).unwrap(),
                version_manifest_url: "versions/metars/v1.json".to_string(),
                state_url: "states/metars/v1.json".to_string(),
                state_sha256: "c".repeat(64),
            }],
        };
        cache.write_manifest_and_timeline("metars-three-hour", &key, &timeline)?;
        assert_eq!(cache.load_manifest(&key)?.expect("manifest").cache_key, key);
        Ok(())
    }

    struct StaticProductTask {
        product: String,
        result: Option<anyhow::Result<BuiltLiveFeedState>>,
    }

    impl StaticProductTask {
        fn state(product: &str, state: BuiltLiveFeedState) -> Self {
            Self {
                product: product.to_string(),
                result: Some(Ok(state)),
            }
        }

        fn failure(product: &str, message: &'static str) -> Self {
            Self {
                product: product.to_string(),
                result: Some(Err(anyhow::anyhow!(message))),
            }
        }
    }

    impl LiveFeedProductTask for StaticProductTask {
        fn product_id(&self) -> &str {
            &self.product
        }

        fn build_state(&mut self) -> anyhow::Result<BuiltLiveFeedState> {
            self.result.take().expect("task called once")
        }
    }

    #[derive(Default)]
    struct RecordingBroker {
        events: Mutex<Vec<LiveFeedInvalidation>>,
    }

    impl RecordingBroker {
        fn events(&self) -> Vec<LiveFeedInvalidation> {
            self.events.lock().expect("events lock").clone()
        }
    }

    impl SseBroker for RecordingBroker {
        fn announce(&self, event: LiveFeedInvalidation) -> anyhow::Result<()> {
            self.events.lock().expect("events lock").push(event);
            Ok(())
        }
    }

    struct StaticSource {
        product: String,
        events: Vec<UpstreamEvent>,
    }

    impl UpstreamSource for StaticSource {
        fn product_id(&self) -> &str {
            &self.product
        }

        fn poll_due(&mut self, _now: DateTime<Utc>) -> anyhow::Result<Vec<UpstreamEvent>> {
            Ok(std::mem::take(&mut self.events))
        }
    }

    struct EchoBuilder {
        product: String,
    }

    impl ProductBuilder for EchoBuilder {
        fn product_id(&self) -> &str {
            &self.product
        }

        fn build_state(
            &self,
            event: &UpstreamEvent,
            scratch_dir: &Path,
        ) -> anyhow::Result<BuiltLiveFeedState> {
            fs::create_dir_all(scratch_dir)?;
            json_state(
                scratch_dir,
                &self.product,
                "state",
                &event.source_id,
                "records",
                &[("KSEA", 1)],
            )
        }
    }

    fn json_state(
        root: &Path,
        product: &str,
        file_stem: &str,
        version: &str,
        records_key: &str,
        records: &[(&str, i64)],
    ) -> anyhow::Result<BuiltLiveFeedState> {
        let value = serde_json::json!({
            "version_label": version,
            "record_count": records.len(),
            records_key: records.iter().map(|(id, value)| {
                (id.to_string(), serde_json::json!({"value": value}))
            }).collect::<serde_json::Map<_, _>>()
        });
        let path = root.join(format!("{file_stem}.json"));
        write_json_pretty_file(&path, &value)?;
        Ok(BuiltLiveFeedState {
            product: product.to_string(),
            version: version.to_string(),
            payload: LiveFeedStatePayload::JsonFile { path, value },
            delta_policy: DeltaPolicy::KeyedRecords {
                records_key: records_key.to_string(),
                count_key: Some("record_count".to_string()),
            },
            changed_count_if_no_delta: records.len(),
        })
    }

    fn metar_delta_fixture_states() -> anyhow::Result<Vec<Value>> {
        let test_artifacts_root = std::env::var_os("AEROBAG_TEST_ARTIFACTS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("..")
                    .join("..")
                    .join("..")
                    .join("aerobag-test-artifacts")
            });
        let fixture_root = test_artifacts_root.join("metars").join("delta-three-hour");
        let mut zip_paths = fs::read_dir(&fixture_root)
            .with_context(|| format!("failed to read {}", fixture_root.display()))?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("zip"))
            .collect::<Vec<_>>();
        zip_paths.sort();
        assert!(
            zip_paths.len() >= 20,
            "expected about two dozen METAR fixture states"
        );
        zip_paths
            .iter()
            .map(|path| read_metars_json_from_zip(path))
            .collect::<anyhow::Result<Vec<_>>>()
    }

    fn read_metars_json_from_zip(path: &Path) -> anyhow::Result<Value> {
        let file = fs::File::open(path)
            .with_context(|| format!("failed to open METAR fixture {}", path.display()))?;
        let mut archive = ZipArchive::new(file)
            .with_context(|| format!("failed to read METAR fixture zip {}", path.display()))?;
        let mut member = archive
            .by_name("metars.json")
            .with_context(|| format!("{} missing metars.json", path.display()))?;
        let mut bytes = Vec::new();
        member
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read metars.json from {}", path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse metars.json from {}", path.display()))
    }

    fn deflated_zip_member_size(bytes: &[u8]) -> anyhow::Result<usize> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .last_modified_time(ZipDateTime::default());
        writer.start_file("payload.json", options)?;
        writer.write_all(bytes)?;
        let cursor = writer.finish()?;
        Ok(cursor.into_inner().len())
    }
}
