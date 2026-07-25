// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc, Mutex,
    },
    time::{Duration as StdDuration, SystemTime},
};

use anyhow::{bail, Context};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use had_nav_kv::{
    apply_nav_kv_delta, build_nav_kv_delta, nav_kv_canonical_sha256_from_pairs, NavKvDelta,
    NavKvPair, NavKvRoot,
};
use notam_state::{
    NotamApplyWork, NotamCheckpoint, NotamDelta, NotamMutation, NotamState, NOTAM_PRODUCT_ID,
};
use preprocessor_core::xz_compress_bytes_with_system_xz;
use preprocessor_zip::{write_deterministic_zip, ZipSource};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::notam_store::{
    NotamPersistentStore, NotamPublicationCursor, NotamPublicationSnapshot,
    NotamPublicationTransition,
};

pub use product_contracts::LIVE_FEEDS_SCHEMA_VERSION;
pub const LIVE_FEED_CURRENT_HISTORY_MAX_ENTRIES: usize = 12;
pub const LIVE_FEED_FAILED_SCRATCH_RETAIN_COUNT: usize = 5;
const NEXRAD_POLL_INTERVAL_SECS: u64 = 5 * 60;
const NEXRAD_CURRENT_HISTORY_TAIL_SECS: u64 = 34 * 60;
const LIVE_FEED_PUBLICATION_DIRS: &[&str] = &["states", "versions", "deltas", "packages"];
const NOTAM_MAX_RETAINED_MUTATIONS: u64 = 100;

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
            previous_source_id: None,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_source_id: Option<String>,
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
    NotamIncremental {
        state_root: PathBuf,
    },
}

#[derive(Debug, Clone)]
pub struct BuiltLiveFeedState {
    pub product: String,
    pub version: String,
    pub payload: LiveFeedStatePayload,
    pub state_sha256: Option<String>,
    pub state_payload_kind: Option<String>,
    pub status_timestamps: LiveFeedStatusTimestamps,
    pub delta_policy: DeltaPolicy,
    pub precomputed_delta: Option<LiveFeedRecordDelta>,
    pub changed_count_if_no_delta: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveFeedStatusTimestamps {
    pub published_at_utc: Option<DateTime<Utc>>,
    pub collected_at_utc: Option<DateTime<Utc>>,
}

impl LiveFeedStatusTimestamps {
    fn published_at_text(&self) -> Option<String> {
        self.published_at_utc
            .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
    }

    fn collected_at_text(&self) -> Option<String> {
        self.collected_at_utc
            .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeltaPolicy {
    None,
    KeyedRecords {
        records_key: String,
        count_key: Option<String>,
    },
    NavKv {
        pairs: Vec<NavKvPair>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collected_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<LiveFeedCurrentHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveFeedCurrentHistoryEntry {
    pub version: String,
    pub version_manifest_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_sha256: Option<String>,
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
    pub install_state: Option<LivePayloadRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_from_previous: Option<LiveDeltaRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_deltas: Vec<LiveDeltaRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LivePayloadRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub url: String,
    pub bytes: u64,
    pub blob_sha256: String,
    pub state_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveDeltaRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub from_version: String,
    pub from_state_sha256: String,
    pub to_version: String,
    pub to_state_sha256: String,
    pub url: String,
    pub bytes: u64,
    pub blob_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_count: Option<u64>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveFeedNavKvDelta {
    pub schema_version: u32,
    pub product: String,
    pub from_version: String,
    pub to_version: String,
    pub from_state_sha256: String,
    pub to_state_sha256: String,
    pub entries: Vec<LiveFeedNavKvDeltaEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveFeedNavKvDeltaEntry {
    pub key: String,
    pub value: Option<Vec<u8>>,
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
    pub published_at_utc: Option<String>,
    pub collected_at_utc: Option<String>,
    pub history: Vec<LiveFeedCurrentHistoryEntry>,
    pub delta_path: Option<PathBuf>,
    pub changed_count: usize,
    pub removed_count: usize,
    #[doc(hidden)]
    pub publication_ack: Option<NotamPublicationAck>,
    #[doc(hidden)]
    pub notam_compaction: Option<NotamCompactionRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotamPublicationAck {
    pub state_root: PathBuf,
    pub journal_seq: i64,
    pub expected_from_state_id: Option<String>,
    pub to_state_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotamCompactionRequest {
    pub state_root: PathBuf,
    pub state_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveFeedInvalidation {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    pub version_manifest_url: String,
    pub state_url: String,
    pub state_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collected_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<LiveFeedCurrentHistoryEntry>,
}

pub trait LiveFeedPublisher {
    fn publish(&self, built: BuiltLiveFeedState) -> anyhow::Result<PublishedLiveFeedUpdate>;

    fn acknowledge(&self, _update: &PublishedLiveFeedUpdate) -> anyhow::Result<()> {
        Ok(())
    }

    fn maintain_after_acknowledgement(
        &self,
        _update: &PublishedLiveFeedUpdate,
    ) -> anyhow::Result<()> {
        Ok(())
    }
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
    Cleanup,
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
                    retain_failed_live_feed_scratch(
                        scratch_root,
                        &product_id,
                        Some(&scratch_dir),
                        &mut failures,
                    );
                    continue;
                }
            };
            if let Some(update) =
                publish_and_announce(product_id.clone(), built, publisher, broker, &mut failures)
            {
                cleanup_successful_live_feed_scratch(&product_id, &scratch_dir, &mut failures);
                published.push(update);
            } else {
                retain_failed_live_feed_scratch(
                    scratch_root,
                    &product_id,
                    Some(&scratch_dir),
                    &mut failures,
                );
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
        if let Err(error) = publisher.acknowledge(&update) {
            failures.push(FailedLiveFeedTask {
                product: update.product.clone(),
                phase: LiveFeedTaskPhase::Publish,
                error: format!("failed to acknowledge published live feed: {error:#}"),
            });
            return None;
        }
        if let Err(error) = publisher.maintain_after_acknowledgement(&update) {
            failures.push(FailedLiveFeedTask {
                product: update.product.clone(),
                phase: LiveFeedTaskPhase::Publish,
                error: format!("failed post-publication maintenance: {error:#}"),
            });
        }
        return Some(update);
    }
    let invalidation = live_feed_invalidation_from_update(&update);
    if let Err(error) = broker.announce(invalidation) {
        failures.push(FailedLiveFeedTask {
            product: update.product.clone(),
            phase: LiveFeedTaskPhase::Announce,
            error: format!("{error:#}"),
        });
        return None;
    }
    if let Err(error) = publisher.acknowledge(&update) {
        failures.push(FailedLiveFeedTask {
            product: update.product.clone(),
            phase: LiveFeedTaskPhase::Publish,
            error: format!("failed to acknowledge published live feed: {error:#}"),
        });
        return None;
    }
    if let Err(error) = publisher.maintain_after_acknowledgement(&update) {
        failures.push(FailedLiveFeedTask {
            product: update.product.clone(),
            phase: LiveFeedTaskPhase::Publish,
            error: format!("failed post-publication maintenance: {error:#}"),
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
        "nexrad" => Some(StdDuration::from_secs(NEXRAD_POLL_INTERVAL_SECS)),
        "metars" | "tafs" | "tfrs" => Some(StdDuration::from_secs(5 * 60)),
        "winds-aloft" => Some(StdDuration::from_secs(60 * 60)),
        "obstacles" => Some(StdDuration::from_secs(6 * 60 * 60)),
        _ => None,
    }
}

pub fn prune_live_feed_scratch_root(
    scratch_root: &Path,
    retain_count_per_product: usize,
) -> anyhow::Result<()> {
    if !scratch_root.is_dir() {
        return Ok(());
    }
    for product_entry in fs::read_dir(scratch_root)
        .with_context(|| format!("failed to read {}", scratch_root.display()))?
    {
        let product_entry =
            product_entry.with_context(|| format!("failed to read {}", scratch_root.display()))?;
        let product_path = product_entry.path();
        if !product_entry
            .file_type()
            .with_context(|| format!("failed to stat {}", product_path.display()))?
            .is_dir()
        {
            continue;
        }
        prune_failed_live_feed_scratch(&product_path, retain_count_per_product, None)?;
    }
    remove_empty_dir_if_exists(scratch_root);
    Ok(())
}

fn cleanup_successful_live_feed_scratch(
    product_id: &str,
    scratch_dir: &Path,
    failures: &mut Vec<FailedLiveFeedTask>,
) {
    if let Err(error) = remove_path_if_exists(scratch_dir) {
        failures.push(FailedLiveFeedTask {
            product: product_id.to_string(),
            phase: LiveFeedTaskPhase::Cleanup,
            error: format!(
                "failed to remove successful scratch {}: {error:#}",
                scratch_dir.display()
            ),
        });
        return;
    }
    if let Some(product_dir) = scratch_dir.parent() {
        remove_empty_dir_if_exists(product_dir);
    }
}

fn retain_failed_live_feed_scratch(
    scratch_root: &Path,
    product_id: &str,
    active_scratch_dir: Option<&Path>,
    failures: &mut Vec<FailedLiveFeedTask>,
) {
    let product_scratch_root = scratch_root.join(product_id);
    if let Err(error) = prune_failed_live_feed_scratch(
        &product_scratch_root,
        LIVE_FEED_FAILED_SCRATCH_RETAIN_COUNT,
        active_scratch_dir,
    ) {
        failures.push(FailedLiveFeedTask {
            product: product_id.to_string(),
            phase: LiveFeedTaskPhase::Cleanup,
            error: format!(
                "failed to prune failed scratch under {}: {error:#}",
                product_scratch_root.display()
            ),
        });
    }
}

pub fn live_feed_invalidation_from_update(
    update: &PublishedLiveFeedUpdate,
) -> LiveFeedInvalidation {
    LiveFeedInvalidation {
        schema_version: LIVE_FEEDS_SCHEMA_VERSION,
        product: update.product.clone(),
        version: update.version.clone(),
        version_manifest_url: update.version_manifest_url.clone(),
        state_url: update.state_url.clone(),
        state_sha256: update.state_sha256.clone(),
        published_at_utc: update.published_at_utc.clone(),
        collected_at_utc: update.collected_at_utc.clone(),
        history: update.history.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveFeedRetentionPolicy {
    default_recent_tail: StdDuration,
    recent_tail_by_product: BTreeMap<String, StdDuration>,
}

impl LiveFeedRetentionPolicy {
    pub fn new(default_recent_tail: StdDuration) -> Self {
        Self {
            default_recent_tail,
            recent_tail_by_product: BTreeMap::new(),
        }
    }

    pub fn with_product_recent_tail(
        mut self,
        product: impl Into<String>,
        recent_tail: StdDuration,
    ) -> Self {
        self.recent_tail_by_product
            .insert(product.into(), recent_tail);
        self
    }

    fn recent_tail_for(&self, product: &str) -> StdDuration {
        self.recent_tail_by_product
            .get(product)
            .copied()
            .unwrap_or(self.default_recent_tail)
    }
}

impl Default for LiveFeedRetentionPolicy {
    fn default() -> Self {
        Self::new(StdDuration::from_secs(3 * 60 * 60))
            .with_product_recent_tail(
                "nexrad",
                StdDuration::from_secs(NEXRAD_CURRENT_HISTORY_TAIL_SECS),
            )
            .with_product_recent_tail("metars", StdDuration::from_secs(3 * 60 * 60))
            .with_product_recent_tail("tafs", StdDuration::from_secs(3 * 60 * 60))
            .with_product_recent_tail("tfrs", StdDuration::from_secs(3 * 60 * 60))
            .with_product_recent_tail("winds-aloft", StdDuration::from_secs(7 * 24 * 60 * 60))
            .with_product_recent_tail("obstacles", StdDuration::from_secs(7 * 24 * 60 * 60))
    }
}

pub struct FileLiveFeedPublisher<C> {
    root: PathBuf,
    clock: C,
    retention: LiveFeedRetentionPolicy,
}

impl<C: Clock> FileLiveFeedPublisher<C> {
    pub fn new(root: PathBuf, clock: C) -> Self {
        Self {
            root,
            clock,
            retention: LiveFeedRetentionPolicy::default(),
        }
    }

    pub fn new_with_retention_policy(
        root: PathBuf,
        clock: C,
        retention: LiveFeedRetentionPolicy,
    ) -> Self {
        Self {
            root,
            clock,
            retention,
        }
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
            state_sha256,
            state_payload_kind,
            status_timestamps,
            delta_policy,
            precomputed_delta,
            changed_count_if_no_delta,
        } = built;
        match payload {
            LiveFeedStatePayload::JsonFile { path, value } => self.publish_json_state(
                product,
                version,
                path,
                value,
                state_sha256,
                state_payload_kind,
                status_timestamps,
                delta_policy,
                precomputed_delta,
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
                state_sha256,
                state_payload_kind,
                status_timestamps,
                delta_policy,
                precomputed_delta,
                changed_count_if_no_delta,
            ),
            LiveFeedStatePayload::NotamIncremental { state_root } => {
                self.publish_notam_incremental(product, version, state_root, status_timestamps)
            }
        }
    }

    fn acknowledge(&self, update: &PublishedLiveFeedUpdate) -> anyhow::Result<()> {
        let Some(ack) = update.publication_ack.as_ref() else {
            return Ok(());
        };
        if update.product != NOTAM_PRODUCT_ID || update.version != ack.to_state_id {
            bail!(
                "NOTAM publication acknowledgement does not match update {}/{}",
                update.product,
                update.version
            );
        }
        let store = NotamPersistentStore::new(&ack.state_root);
        store.advance_publication_cursor(
            ack.journal_seq,
            ack.expected_from_state_id.as_deref(),
            &ack.to_state_id,
        )?;
        self.prune_notam_journal_best_effort(&store);
        Ok(())
    }

    fn maintain_after_acknowledgement(
        &self,
        update: &PublishedLiveFeedUpdate,
    ) -> anyhow::Result<()> {
        let Some(request) = update.notam_compaction.as_ref() else {
            return Ok(());
        };
        self.compact_notam_head(request)
    }
}

impl<C: Clock> FileLiveFeedPublisher<C> {
    fn publish_json_state(
        &self,
        product: String,
        version: String,
        source_path: PathBuf,
        state_value: Value,
        state_sha256: Option<String>,
        state_payload_kind: Option<String>,
        status_timestamps: LiveFeedStatusTimestamps,
        delta_policy: DeltaPolicy,
        precomputed_delta: Option<LiveFeedRecordDelta>,
        changed_count_if_no_delta: usize,
    ) -> anyhow::Result<PublishedLiveFeedUpdate> {
        let state_dir = self.root.join("states").join(&product);
        let state_path = state_dir.join(format!("{version}.json.xz"));
        fs::create_dir_all(&state_dir)
            .with_context(|| format!("failed to create {}", state_dir.display()))?;
        let _ = source_path;
        if !state_path.is_file() {
            write_xz_json_pretty_file(&state_path, &state_value)?;
        }
        self.publish_state_common(
            product,
            version,
            state_path,
            state_value,
            state_sha256,
            state_payload_kind.or_else(|| Some("json_xz".to_string())),
            status_timestamps,
            delta_policy,
            precomputed_delta,
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
        state_sha256: Option<String>,
        state_payload_kind: Option<String>,
        status_timestamps: LiveFeedStatusTimestamps,
        delta_policy: DeltaPolicy,
        precomputed_delta: Option<LiveFeedRecordDelta>,
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
        if state_payload_kind.as_deref() == Some("nav_kv") {
            xz_nav_kv_state_dir_pages(&state_dir, &manifest_value)?;
        }
        self.publish_state_common(
            product,
            version,
            state_path,
            manifest_value,
            state_sha256,
            state_payload_kind.or_else(|| Some("json".to_string())),
            status_timestamps,
            delta_policy,
            precomputed_delta,
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
        state_sha256: Option<String>,
        state_payload_kind: Option<String>,
        status_timestamps: LiveFeedStatusTimestamps,
        delta_policy: DeltaPolicy,
        precomputed_delta: Option<LiveFeedRecordDelta>,
        changed_count_if_no_delta: usize,
        state_root: Option<PathBuf>,
    ) -> anyhow::Result<PublishedLiveFeedUpdate> {
        let state_bytes = fs::read(&state_path)
            .with_context(|| format!("failed to read {}", state_path.display()))?;
        let state_blob_sha256 = sha256_hex(&state_bytes);
        let state_sha256 = state_sha256
            .map(Ok)
            .unwrap_or_else(|| canonical_json_sha256(&state_value))?;
        let published_at_utc = status_timestamps.published_at_text();
        let collected_at_utc = status_timestamps.collected_at_text();
        let mut current =
            read_live_feeds_current(&self.root)?.unwrap_or(LiveFeedsCurrentManifest {
                schema_version: LIVE_FEEDS_SCHEMA_VERSION,
                generated_at_utc: self
                    .clock
                    .now_utc()
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
                products: BTreeMap::new(),
            });
        let previous_entry = current.products.get(&product).cloned();

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
                let now = self.clock.now_utc();
                let history = live_feed_current_history_entries(
                    &self.root,
                    &product,
                    &version,
                    &self.retention,
                    now,
                )?;
                let next_entry = LiveFeedCurrentEntry {
                    current: version.clone(),
                    version_manifest_url: previous.version_manifest_url.clone(),
                    state_url: previous.state_url.clone(),
                    state_sha256: previous.state_sha256.clone(),
                    published_at_utc: published_at_utc.clone(),
                    collected_at_utc: collected_at_utc.clone(),
                    history: history.clone(),
                };
                let metadata_changed = previous != &next_entry;
                if metadata_changed {
                    current.generated_at_utc = now.to_rfc3339_opts(SecondsFormat::Secs, true);
                    current.products.insert(product.clone(), next_entry);
                    write_live_feeds_current_manifest(&self.root, &current)?;
                    self.prune_publication_best_effort();
                }
                return Ok(PublishedLiveFeedUpdate {
                    product,
                    version,
                    unchanged: !metadata_changed,
                    state_path,
                    version_manifest_path: self.root.join(&previous.version_manifest_url),
                    version_manifest_url: previous.version_manifest_url.clone(),
                    state_url: previous.state_url.clone(),
                    state_sha256: previous.state_sha256.clone(),
                    published_at_utc,
                    collected_at_utc,
                    history,
                    delta_path: None,
                    changed_count: 0,
                    removed_count: 0,
                    publication_ack: None,
                    notam_compaction: None,
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
            let delta = if let Some(delta) = precomputed_delta {
                if delta.product != product {
                    bail!(
                        "precomputed delta is for {}, expected {product}",
                        delta.product
                    );
                }
                if delta.from_version != previous.current || delta.to_version != version {
                    bail!(
                        "precomputed {product} delta is {}->{}, expected {}->{}",
                        delta.from_version,
                        delta.to_version,
                        previous.current,
                        version
                    );
                }
                delta
            } else {
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
                build_record_delta(&product, records_key, &previous_state, &state_value)?
            };
            changed_count = delta.changed.len();
            removed_count = delta.removed.len();
            let product_delta_dir = self.root.join("deltas").join(&product);
            fs::create_dir_all(&product_delta_dir)
                .with_context(|| format!("failed to create {}", product_delta_dir.display()))?;
            let path = product_delta_dir.join(format!("{}__{}.json.xz", previous.current, version));
            write_xz_json_pretty_file(&path, &delta)?;
            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            delta_ref = Some(LiveDeltaRef {
                kind: Some("record_json_delta_xz".to_string()),
                from_version: previous.current.clone(),
                from_state_sha256: previous.state_sha256.clone(),
                to_version: version.clone(),
                to_state_sha256: state_sha256.clone(),
                url: live_feeds_relative_url(&self.root, &path)?,
                bytes: bytes.len() as u64,
                blob_sha256: sha256_hex(&bytes),
                mutation_count: None,
            });
            previous_version = Some(previous.current.clone());
            delta_path = Some(path);
        }
        if let (Some(previous), DeltaPolicy::NavKv { pairs }) =
            (previous_entry.as_ref(), &delta_policy)
        {
            let previous_state_path = self.root.join(&previous.state_url);
            let previous_state_dir = previous_state_path
                .parent()
                .with_context(|| format!("{} has no parent", previous_state_path.display()))?;
            let previous_pairs = read_nav_kv_pairs_from_dir(previous_state_dir)?;
            let previous_sha256 = nav_kv_canonical_sha256_from_pairs(&previous_pairs);
            if previous_sha256 != previous.state_sha256 {
                bail!(
                    "previous {product} HAD state hash mismatch for {}: expected {}, got {}",
                    previous.current,
                    previous.state_sha256,
                    previous_sha256
                );
            }
            let delta = build_nav_kv_delta(&previous_pairs, pairs)
                .map_err(|err| anyhow::anyhow!("failed to build {product} HAD delta: {err}"))?;
            let applied = apply_nav_kv_delta(&previous_pairs, &delta)
                .map_err(|err| anyhow::anyhow!("failed to verify {product} HAD delta: {err}"))?;
            let applied_sha256 = nav_kv_canonical_sha256_from_pairs(&applied);
            if applied_sha256 != state_sha256 {
                bail!(
                    "{product} HAD delta target hash mismatch for {version}: expected {}, got {}",
                    state_sha256,
                    applied_sha256
                );
            }
            changed_count = delta
                .entries
                .iter()
                .filter(|entry| entry.value.is_some())
                .count();
            removed_count = delta
                .entries
                .iter()
                .filter(|entry| entry.value.is_none())
                .count();
            let product_delta_dir = self.root.join("deltas").join(&product);
            fs::create_dir_all(&product_delta_dir)
                .with_context(|| format!("failed to create {}", product_delta_dir.display()))?;
            let path = product_delta_dir.join(format!(
                "{}__{}.nav-kv-delta.json.xz",
                previous.current, version
            ));
            write_xz_json_pretty_file(
                &path,
                &live_feed_nav_kv_delta_from_delta(
                    &product,
                    &previous.current,
                    &version,
                    &previous.state_sha256,
                    &state_sha256,
                    &delta,
                ),
            )?;
            let bytes =
                fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
            delta_ref = Some(LiveDeltaRef {
                kind: Some("nav_kv_delta_xz".to_string()),
                from_version: previous.current.clone(),
                from_state_sha256: previous.state_sha256.clone(),
                to_version: version.clone(),
                to_state_sha256: state_sha256.clone(),
                url: live_feeds_relative_url(&self.root, &path)?,
                bytes: bytes.len() as u64,
                blob_sha256: sha256_hex(&bytes),
                mutation_count: None,
            });
            previous_version = Some(previous.current.clone());
            delta_path = Some(path);
        }

        let state_ref = LivePayloadRef {
            kind: state_payload_kind,
            url: live_feeds_relative_url(&self.root, &state_path)?,
            bytes: state_bytes.len() as u64,
            blob_sha256: state_blob_sha256,
            state_sha256: state_sha256.clone(),
        };
        let install_state_ref = if let Some(state_root) = state_root.as_ref() {
            let install_kind = match state_ref.kind.as_deref() {
                Some("nav_kv") => "nav_kv_package",
                _ => "directory_package",
            };
            Some(self.write_install_state_package(
                &product,
                &version,
                state_root,
                install_kind,
                &state_sha256,
            )?)
        } else {
            None
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
                schema_version: LIVE_FEEDS_SCHEMA_VERSION,
                product: product.clone(),
                version: version.clone(),
                previous: previous_version,
                state: state_ref,
                install_state: install_state_ref,
                delta_from_previous: delta_ref,
                recent_deltas: Vec::new(),
            },
        )?;
        let now = self.clock.now_utc();
        let history = live_feed_current_history_entries(
            &self.root,
            &product,
            &version,
            &self.retention,
            now,
        )?;
        current.generated_at_utc = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        current.products.insert(
            product.clone(),
            LiveFeedCurrentEntry {
                current: version.clone(),
                version_manifest_url: version_manifest_url.clone(),
                state_url: state_url.clone(),
                state_sha256: state_sha256.clone(),
                published_at_utc: published_at_utc.clone(),
                collected_at_utc: collected_at_utc.clone(),
                history: history.clone(),
            },
        );
        write_live_feeds_current_manifest(&self.root, &current)?;
        self.prune_publication_best_effort();

        Ok(PublishedLiveFeedUpdate {
            product,
            version,
            unchanged: false,
            state_path,
            version_manifest_path,
            version_manifest_url,
            state_url,
            state_sha256,
            published_at_utc,
            collected_at_utc,
            history,
            delta_path,
            changed_count,
            removed_count,
            publication_ack: None,
            notam_compaction: None,
        })
    }

    fn publish_notam_incremental(
        &self,
        product: String,
        requested_version: String,
        state_root: PathBuf,
        status_timestamps: LiveFeedStatusTimestamps,
    ) -> anyhow::Result<PublishedLiveFeedUpdate> {
        if product != NOTAM_PRODUCT_ID {
            bail!("incremental NOTAM payload declares product {product}");
        }
        let store = NotamPersistentStore::new(state_root);
        let mut snapshot = store.publication_snapshot()?;
        if requested_version != snapshot.current_state_id {
            bail!(
                "NOTAM build requested state {requested_version}, but projection is {}",
                snapshot.current_state_id
            );
        }
        let published_at_utc = status_timestamps.published_at_text();
        let collected_at_utc = status_timestamps.collected_at_text();
        let mut current =
            read_live_feeds_current(&self.root)?.unwrap_or(LiveFeedsCurrentManifest {
                schema_version: LIVE_FEEDS_SCHEMA_VERSION,
                generated_at_utc: self
                    .clock
                    .now_utc()
                    .to_rfc3339_opts(SecondsFormat::Secs, true),
                products: BTreeMap::new(),
            });
        let mut previous_entry = current.products.get(NOTAM_PRODUCT_ID).cloned();
        let (reconciled_snapshot, reset_source_epoch) =
            self.reconcile_published_notam_prefix(&store, snapshot, previous_entry.as_ref())?;
        snapshot = reconciled_snapshot;
        if reset_source_epoch {
            previous_entry = None;
        }
        if requested_version != snapshot.current_state_id {
            bail!(
                "NOTAM projection advanced from requested state {requested_version} to {} while reconciling its published prefix",
                snapshot.current_state_id
            );
        }

        if previous_entry
            .as_ref()
            .is_some_and(|entry| entry.current == snapshot.current_state_id)
        {
            let previous = previous_entry.context("checked NOTAM current entry disappeared")?;
            let manifest = validate_published_notam_head(
                &self.root,
                &previous,
                &snapshot.current_state_id,
                &snapshot.cursor,
                &snapshot.transitions,
            )?;
            let notam_compaction = (notam_mutations_after_state(
                &manifest.state.state_sha256,
                &manifest.recent_deltas,
                &snapshot.current_state_id,
            )? >= NOTAM_MAX_RETAINED_MUTATIONS)
                .then(|| NotamCompactionRequest {
                    state_root: store.root().to_path_buf(),
                    state_id: snapshot.current_state_id.clone(),
                });
            let final_journal_seq = snapshot
                .transitions
                .last()
                .map(|transition| transition.journal_seq)
                .unwrap_or(snapshot.cursor.published_through_journal_seq);
            let publication_ack = (snapshot.cursor.published_head_state_id.as_deref()
                != Some(snapshot.current_state_id.as_str()))
            .then(|| NotamPublicationAck {
                state_root: store.root().to_path_buf(),
                journal_seq: final_journal_seq,
                expected_from_state_id: snapshot.cursor.published_head_state_id.clone(),
                to_state_id: snapshot.current_state_id.clone(),
            });
            let now = self.clock.now_utc();
            let history = live_feed_current_history_entries(
                &self.root,
                NOTAM_PRODUCT_ID,
                &snapshot.current_state_id,
                &self.retention,
                now,
            )?;
            let next_entry = LiveFeedCurrentEntry {
                current: previous.current.clone(),
                version_manifest_url: previous.version_manifest_url.clone(),
                state_url: previous.state_url.clone(),
                state_sha256: previous.state_sha256.clone(),
                published_at_utc: published_at_utc.clone(),
                collected_at_utc: collected_at_utc.clone(),
                history: history.clone(),
            };
            let metadata_changed = previous != next_entry;
            if metadata_changed {
                current.generated_at_utc = now.to_rfc3339_opts(SecondsFormat::Secs, true);
                current
                    .products
                    .insert(NOTAM_PRODUCT_ID.to_string(), next_entry);
                write_live_feeds_current_manifest(&self.root, &current)?;
                self.prune_publication_best_effort();
            }
            let state_path = self.root.join(safe_relative_path(&manifest.state.url)?);
            let delta_path = manifest
                .delta_from_previous
                .as_ref()
                .map(|delta| safe_relative_path(&delta.url).map(|path| self.root.join(path)))
                .transpose()?;
            return Ok(PublishedLiveFeedUpdate {
                product,
                version: snapshot.current_state_id,
                unchanged: !metadata_changed && publication_ack.is_none(),
                state_path,
                version_manifest_path: self.root.join(&previous.version_manifest_url),
                version_manifest_url: previous.version_manifest_url,
                state_url: previous.state_url,
                state_sha256: previous.state_sha256,
                published_at_utc,
                collected_at_utc,
                history,
                delta_path,
                changed_count: 0,
                removed_count: 0,
                publication_ack,
                notam_compaction,
            });
        }

        let (previous_version, delta, changed_count, removed_count) = if let Some(head) =
            snapshot.cursor.published_head_state_id.as_deref()
        {
            let previous = previous_entry.as_ref().with_context(|| {
                format!("NOTAM SQLite cursor is {head}, but current.json has no NOTAM product")
            })?;
            if previous.current != head {
                bail!(
                    "NOTAM SQLite cursor is {head}, but current.json points at {}",
                    previous.current
                );
            }
            let delta = collapse_notam_transitions(&snapshot.cursor, &snapshot.transitions)?;
            let changed_count = delta
                .mutations
                .iter()
                .filter(|mutation| matches!(mutation, NotamMutation::Upsert { .. }))
                .count();
            let removed_count = delta.mutations.len() - changed_count;
            (
                Some(head.to_string()),
                Some(delta),
                changed_count,
                removed_count,
            )
        } else {
            if previous_entry.is_some() {
                bail!("NOTAM SQLite has no published cursor, but current.json already has NOTAMs");
            }
            (None, None, snapshot.counters.notam_count as usize, 0)
        };

        let mut delta_ref = None;
        let mut delta_path = None;
        let previous_manifest = previous_entry
            .as_ref()
            .map(|entry| read_notam_version_manifest(&self.root, entry))
            .transpose()?;
        if let Some(delta) = delta.as_ref() {
            let product_delta_dir = self.root.join("deltas").join(NOTAM_PRODUCT_ID);
            let path = product_delta_dir.join(format!(
                "{}__{}.json.xz",
                delta.from_state_id, delta.to_state_id
            ));
            let bytes = write_immutable_xz_json_pretty_file(&path, delta)?;
            delta_ref = Some(LiveDeltaRef {
                kind: Some("notam_ordered_delta_xz".to_string()),
                from_version: delta.from_state_id.clone(),
                from_state_sha256: delta.from_state_id.clone(),
                to_version: delta.to_state_id.clone(),
                to_state_sha256: delta.to_state_id.clone(),
                url: live_feeds_relative_url(&self.root, &path)?,
                bytes: bytes.len() as u64,
                blob_sha256: sha256_hex(&bytes),
                mutation_count: Some(delta.mutations.len() as u64),
            });
            delta_path = Some(path);
        }

        let mut recent_deltas = previous_manifest
            .as_ref()
            .map(|manifest| manifest.recent_deltas.clone())
            .unwrap_or_default();
        if let Some(delta) = delta_ref.clone() {
            recent_deltas.push(delta);
        }
        let notam_compaction = match previous_manifest.as_ref() {
            Some(manifest) => {
                validate_notam_delta_chain(
                    &manifest.state.state_sha256,
                    &recent_deltas,
                    &snapshot.current_state_id,
                )?;
                (notam_mutations_after_state(
                    &manifest.state.state_sha256,
                    &recent_deltas,
                    &snapshot.current_state_id,
                )? >= NOTAM_MAX_RETAINED_MUTATIONS)
                    .then(|| NotamCompactionRequest {
                        state_root: store.root().to_path_buf(),
                        state_id: snapshot.current_state_id.clone(),
                    })
            }
            None => None,
        };

        let state_ref = match previous_manifest.as_ref() {
            Some(manifest) => manifest.state.clone(),
            None => {
                self.write_notam_checkpoint(&store, &snapshot.current_state_id, &snapshot.counters)?
            }
        };
        trim_notam_delta_suffix(&state_ref.state_sha256, &mut recent_deltas)?;
        validate_notam_delta_chain(
            &state_ref.state_sha256,
            &recent_deltas,
            &snapshot.current_state_id,
        )?;

        let version_dir = self.root.join("versions").join(NOTAM_PRODUCT_ID);
        fs::create_dir_all(&version_dir)
            .with_context(|| format!("failed to create {}", version_dir.display()))?;
        let version_manifest_path = version_dir.join(format!("{}.json", snapshot.current_state_id));
        let version_manifest_url = live_feeds_relative_url(&self.root, &version_manifest_path)?;
        let version_manifest = LiveFeedVersionManifest {
            schema_version: LIVE_FEEDS_SCHEMA_VERSION,
            product: NOTAM_PRODUCT_ID.to_string(),
            version: snapshot.current_state_id.clone(),
            previous: previous_version,
            state: state_ref.clone(),
            install_state: None,
            delta_from_previous: delta_ref,
            recent_deltas,
        };
        write_json_pretty_file(&version_manifest_path, &version_manifest)?;

        let now = self.clock.now_utc();
        let history = if reset_source_epoch {
            Vec::new()
        } else {
            live_feed_current_history_entries(
                &self.root,
                NOTAM_PRODUCT_ID,
                &snapshot.current_state_id,
                &self.retention,
                now,
            )?
        };
        current.generated_at_utc = now.to_rfc3339_opts(SecondsFormat::Secs, true);
        current.products.insert(
            NOTAM_PRODUCT_ID.to_string(),
            LiveFeedCurrentEntry {
                current: snapshot.current_state_id.clone(),
                version_manifest_url: version_manifest_url.clone(),
                state_url: state_ref.url.clone(),
                state_sha256: snapshot.current_state_id.clone(),
                published_at_utc: published_at_utc.clone(),
                collected_at_utc: collected_at_utc.clone(),
                history: history.clone(),
            },
        );
        write_live_feeds_current_manifest(&self.root, &current)?;
        let final_journal_seq = snapshot
            .transitions
            .last()
            .map(|transition| transition.journal_seq)
            .unwrap_or(snapshot.cursor.published_through_journal_seq);
        self.prune_publication_best_effort();

        Ok(PublishedLiveFeedUpdate {
            product,
            version: snapshot.current_state_id.clone(),
            unchanged: false,
            state_path: self.root.join(safe_relative_path(&state_ref.url)?),
            version_manifest_path,
            version_manifest_url,
            state_url: state_ref.url,
            state_sha256: snapshot.current_state_id.clone(),
            published_at_utc,
            collected_at_utc,
            history,
            delta_path,
            changed_count,
            removed_count,
            publication_ack: Some(NotamPublicationAck {
                state_root: store.root().to_path_buf(),
                journal_seq: final_journal_seq,
                expected_from_state_id: snapshot.cursor.published_head_state_id,
                to_state_id: snapshot.current_state_id,
            }),
            notam_compaction,
        })
    }

    fn reconcile_published_notam_prefix(
        &self,
        store: &NotamPersistentStore,
        snapshot: NotamPublicationSnapshot,
        published_entry: Option<&LiveFeedCurrentEntry>,
    ) -> anyhow::Result<(NotamPublicationSnapshot, bool)> {
        let Some(published_entry) = published_entry else {
            return Ok((snapshot, false));
        };
        if published_entry.current == snapshot.current_state_id
            || snapshot.cursor.published_head_state_id.as_deref()
                == Some(published_entry.current.as_str())
        {
            return Ok((snapshot, false));
        }

        let (prefix_len, journal_seq) = if snapshot.cursor.published_head_state_id.is_none()
            && snapshot
                .transitions
                .first()
                .is_some_and(|transition| transition.from_state_id == published_entry.current)
        {
            (0, snapshot.cursor.published_through_journal_seq)
        } else {
            let Some(index) = snapshot
                .transitions
                .iter()
                .position(|transition| transition.to_state_id == published_entry.current)
            else {
                if snapshot.cursor.published_head_state_id.is_none() {
                    return Ok((snapshot, true));
                }
                bail!(
                    "NOTAM SQLite cursor is {:?}, but current.json points at {} outside the pending journal",
                    snapshot.cursor.published_head_state_id,
                    published_entry.current
                );
            };
            (index + 1, snapshot.transitions[index].journal_seq)
        };
        let published_prefix = &snapshot.transitions[..prefix_len];
        validate_published_notam_head(
            &self.root,
            published_entry,
            &published_entry.current,
            &snapshot.cursor,
            published_prefix,
        )
        .context("failed to verify unacknowledged published NOTAM journal prefix")?;
        store.advance_publication_cursor(
            journal_seq,
            snapshot.cursor.published_head_state_id.as_deref(),
            &published_entry.current,
        )?;
        self.prune_notam_journal_best_effort(store);

        let reconciled = store.publication_snapshot()?;
        if reconciled.cursor.published_head_state_id.as_deref()
            != Some(published_entry.current.as_str())
        {
            bail!(
                "NOTAM publication cursor did not reconcile to verified current.json head {}",
                published_entry.current
            );
        }
        Ok((reconciled, false))
    }

    fn write_notam_checkpoint(
        &self,
        store: &NotamPersistentStore,
        expected_state_id: &str,
        expected_counters: &notam_state::NotamCounters,
    ) -> anyhow::Result<LivePayloadRef> {
        let checkpoint = store.current_checkpoint()?;
        self.write_notam_checkpoint_value(&checkpoint, expected_state_id, expected_counters)
    }

    fn write_notam_checkpoint_value(
        &self,
        checkpoint: &NotamCheckpoint,
        expected_state_id: &str,
        expected_counters: &notam_state::NotamCounters,
    ) -> anyhow::Result<LivePayloadRef> {
        if checkpoint.state_id != expected_state_id || checkpoint.counters != *expected_counters {
            bail!(
                "NOTAM projection changed while publishing checkpoint: expected {} {:?}, got {} {:?}",
                expected_state_id,
                expected_counters,
                checkpoint.state_id,
                checkpoint.counters
            );
        }
        let (recomputed_state_id, recomputed_counters) =
            notam_state::recompute_checkpoint_identity(&checkpoint.records)
                .map_err(anyhow::Error::msg)
                .context("failed to fully recompute NOTAM checkpoint identity")?;
        if recomputed_state_id != checkpoint.state_id || recomputed_counters != checkpoint.counters
        {
            bail!(
                "NOTAM checkpoint failed full recomputation: declared {} {:?}, recomputed {} {:?}",
                checkpoint.state_id,
                checkpoint.counters,
                recomputed_state_id,
                recomputed_counters
            );
        }
        let state_path = self
            .root
            .join("states")
            .join(NOTAM_PRODUCT_ID)
            .join(format!("{}.json.xz", checkpoint.state_id));
        let bytes = write_immutable_xz_json_pretty_file(&state_path, &checkpoint)?;
        Ok(LivePayloadRef {
            kind: Some("notam_checkpoint_xz".to_string()),
            url: live_feeds_relative_url(&self.root, &state_path)?,
            bytes: bytes.len() as u64,
            blob_sha256: sha256_hex(&bytes),
            state_sha256: checkpoint.state_id.clone(),
        })
    }

    fn compact_notam_head(&self, request: &NotamCompactionRequest) -> anyhow::Result<()> {
        let current = read_live_feeds_current(&self.root)?
            .context("NOTAM checkpoint compaction requires current.json")?;
        let Some(entry) = current.products.get(NOTAM_PRODUCT_ID).cloned() else {
            bail!("NOTAM checkpoint compaction requires a current NOTAM entry");
        };
        if entry.current != request.state_id {
            return Ok(());
        }
        let previous_manifest = read_notam_version_manifest(&self.root, &entry)?;
        let replay_mutations = notam_mutations_after_state(
            &previous_manifest.state.state_sha256,
            &previous_manifest.recent_deltas,
            &request.state_id,
        )?;
        if replay_mutations < NOTAM_MAX_RETAINED_MUTATIONS {
            return Ok(());
        }

        let store = NotamPersistentStore::new(&request.state_root);
        let checkpoint = store.current_checkpoint()?;
        if checkpoint.state_id != request.state_id {
            // Source ingestion advanced while the delta was being acknowledged.
            // The next publication will compact the newer acknowledged head.
            return Ok(());
        }
        let state_ref = self.write_notam_checkpoint_value(
            &checkpoint,
            &checkpoint.state_id,
            &checkpoint.counters,
        )?;
        let mut recent_deltas = previous_manifest.recent_deltas.clone();
        trim_notam_delta_suffix(&state_ref.state_sha256, &mut recent_deltas)?;
        validate_notam_delta_chain(&state_ref.state_sha256, &recent_deltas, &request.state_id)?;

        let compacted_manifest = LiveFeedVersionManifest {
            schema_version: LIVE_FEEDS_SCHEMA_VERSION,
            product: NOTAM_PRODUCT_ID.to_string(),
            version: request.state_id.clone(),
            previous: previous_manifest.previous,
            state: state_ref.clone(),
            install_state: None,
            delta_from_previous: previous_manifest.delta_from_previous,
            recent_deltas,
        };
        let manifest_path = self
            .root
            .join("versions")
            .join(NOTAM_PRODUCT_ID)
            .join(format!("{}.checkpoint.json", request.state_id));
        write_immutable_json_pretty_file(&manifest_path, &compacted_manifest)?;
        let manifest_url = live_feeds_relative_url(&self.root, &manifest_path)?;

        let mut latest = read_live_feeds_current(&self.root)?
            .context("NOTAM checkpoint compaction lost current.json")?;
        let Some(latest_entry) = latest.products.get_mut(NOTAM_PRODUCT_ID) else {
            bail!("NOTAM checkpoint compaction lost the current NOTAM entry");
        };
        if latest_entry.current != request.state_id
            || latest_entry.version_manifest_url != entry.version_manifest_url
        {
            return Ok(());
        }
        latest_entry.version_manifest_url = manifest_url;
        latest_entry.state_url = state_ref.url;
        latest.generated_at_utc = self
            .clock
            .now_utc()
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        write_live_feeds_current_manifest(&self.root, &latest)?;
        self.prune_publication_best_effort();
        Ok(())
    }

    fn prune_publication_best_effort(&self) {
        if let Err(error) =
            prune_live_feed_publication(&self.root, &self.retention, self.clock.now_utc())
                .with_context(|| {
                    format!(
                        "failed to prune live-feed output under {}",
                        self.root.display()
                    )
                })
        {
            eprintln!("live-feed publication GC failed: {error:#}");
        }
    }

    fn prune_notam_journal_best_effort(&self, store: &NotamPersistentStore) {
        let recent_tail = self.retention.recent_tail_for(NOTAM_PRODUCT_ID);
        let Ok(recent_tail) = Duration::from_std(recent_tail) else {
            eprintln!("NOTAM journal GC failed: publication grace does not fit chrono duration");
            return;
        };
        let cutoff = (self.clock.now_utc() - recent_tail).to_rfc3339();
        if let Err(error) = store
            .prune_published_journal_before(&cutoff)
            .context("failed to prune published NOTAM journal")
        {
            eprintln!("NOTAM journal GC failed: {error:#}");
        }
    }

    fn write_install_state_package(
        &self,
        product: &str,
        version: &str,
        state_root: &Path,
        kind: &str,
        state_sha256: &str,
    ) -> anyhow::Result<LivePayloadRef> {
        let package_dir = self.root.join("packages").join(product);
        let package_path = package_dir.join(format!("{version}.zip"));
        if !package_path.is_file() {
            if kind == "nav_kv_package" {
                let (manifest, root, pages) = read_nav_kv_members_from_dir(product, state_root)?;
                let bytes = nav_kv_package::write_stored_xz_package_bytes_with_encoder(
                    &manifest,
                    &root,
                    &pages,
                    producer_xz_compress_bytes,
                )
                .map_err(|err| {
                    anyhow::anyhow!("failed to encode {product} nav_kv package: {err}")
                })?;
                if let Some(parent) = package_path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create {}", parent.display()))?;
                }
                fs::write(&package_path, bytes)
                    .with_context(|| format!("failed to write {}", package_path.display()))?;
            } else {
                let members = zip_members_for_dir(state_root)?;
                write_deterministic_zip(&package_path, &members)
                    .with_context(|| format!("failed to write {}", package_path.display()))?;
            }
        }
        let bytes = fs::read(&package_path)
            .with_context(|| format!("failed to read {}", package_path.display()))?;
        Ok(LivePayloadRef {
            kind: Some(kind.to_string()),
            url: live_feeds_relative_url(&self.root, &package_path)?,
            bytes: bytes.len() as u64,
            blob_sha256: sha256_hex(&bytes),
            state_sha256: state_sha256.to_string(),
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
        schema_version: LIVE_FEEDS_SCHEMA_VERSION,
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

fn live_feed_nav_kv_delta_from_delta(
    product: &str,
    from_version: &str,
    to_version: &str,
    from_state_sha256: &str,
    to_state_sha256: &str,
    delta: &NavKvDelta,
) -> LiveFeedNavKvDelta {
    LiveFeedNavKvDelta {
        schema_version: LIVE_FEEDS_SCHEMA_VERSION,
        product: product.to_string(),
        from_version: from_version.to_string(),
        to_version: to_version.to_string(),
        from_state_sha256: from_state_sha256.to_string(),
        to_state_sha256: to_state_sha256.to_string(),
        entries: delta
            .entries
            .iter()
            .map(|entry| LiveFeedNavKvDeltaEntry {
                key: entry.key.clone(),
                value: entry.value.clone(),
            })
            .collect(),
    }
}

pub(crate) fn read_nav_kv_pairs_from_dir(state_dir: &Path) -> anyhow::Result<Vec<NavKvPair>> {
    let root_path = state_dir.join("root");
    let root_bytes =
        fs::read(&root_path).with_context(|| format!("failed to read {}", root_path.display()))?;
    let root = NavKvRoot::parse(&root_bytes)
        .map_err(|err| anyhow::anyhow!("failed to parse {}: {err}", root_path.display()))?;
    root.pairs(|page| read_nav_kv_page_from_dir(state_dir, page).ok())
        .ok_or_else(|| anyhow::anyhow!("failed to read HAD pages under {}", state_dir.display()))
}

fn read_nav_kv_members_from_dir(
    product: &str,
    state_dir: &Path,
) -> anyhow::Result<(Vec<u8>, Vec<u8>, Vec<Vec<u8>>)> {
    let manifest_path = state_dir.join("manifest.json");
    let manifest = fs::read(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest_value: Value = serde_json::from_slice(&manifest)
        .with_context(|| format!("failed to decode {}", manifest_path.display()))?;
    let page_count = nav_kv_manifest_page_count(product, &manifest_value)?;
    let root_path = state_dir.join("root");
    let root =
        fs::read(&root_path).with_context(|| format!("failed to read {}", root_path.display()))?;
    let mut pages = Vec::new();
    for page in 0..page_count {
        pages.push(read_nav_kv_page_from_dir(state_dir, page)?);
    }
    Ok((manifest, root, pages))
}

fn xz_nav_kv_state_dir_pages(state_dir: &Path, manifest_value: &Value) -> anyhow::Result<()> {
    let page_count = nav_kv_manifest_page_count("nav_kv", manifest_value)?;
    for page in 0..page_count {
        let path = state_dir.join(format!("page_{page:04}"));
        let bytes =
            fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        if nav_kv_package::is_xz(&bytes) {
            continue;
        }
        let encoded = producer_xz_compress_bytes(&bytes)
            .map_err(|err| anyhow::anyhow!("failed to encode {}: {err}", path.display()))?;
        fs::write(&path, encoded).with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn read_nav_kv_page_from_dir(state_dir: &Path, page: u32) -> anyhow::Result<Vec<u8>> {
    let path = state_dir.join(format!("page_{page:04}"));
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    nav_kv_package::decode_xz_if_needed(&bytes)
        .map(|page| page.into_owned())
        .map_err(|err| anyhow::anyhow!("failed to decode {}: {err}", path.display()))
}

fn nav_kv_manifest_page_count(product: &str, manifest_value: &Value) -> anyhow::Result<u32> {
    manifest_value
        .get("page_count")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| anyhow::anyhow!("{product} nav_kv manifest missing page_count"))
}

fn state_object(state: &Value) -> anyhow::Result<&serde_json::Map<String, Value>> {
    state
        .as_object()
        .context("live feed state must be a JSON object")
}

fn collapse_notam_transitions(
    cursor: &NotamPublicationCursor,
    transitions: &[NotamPublicationTransition],
) -> anyhow::Result<NotamDelta> {
    let first = transitions
        .first()
        .context("NOTAM publication has no pending transition")?;
    let last = transitions
        .last()
        .context("NOTAM publication has no final transition")?;
    if cursor.published_head_state_id.as_deref() != Some(&first.from_state_id) {
        bail!(
            "NOTAM pending chain starts at {}, but publication cursor is {:?}",
            first.from_state_id,
            cursor.published_head_state_id
        );
    }
    let mut by_id = BTreeMap::new();
    for transition in transitions {
        for mutation in &transition.mutations {
            by_id.insert(mutation.notam_id().to_string(), mutation.clone());
        }
    }
    let delta = NotamDelta::new(
        first.from_state_id.clone(),
        last.to_state_id.clone(),
        last.counters,
        by_id.into_values().collect(),
    );
    notam_state::validate_mutation_order(&delta.mutations)
        .map_err(anyhow::Error::msg)
        .context("collapsed NOTAM publication mutations are not ordered")?;
    Ok(delta)
}

fn read_notam_version_manifest(
    live_root: &Path,
    entry: &LiveFeedCurrentEntry,
) -> anyhow::Result<LiveFeedVersionManifest> {
    let path = live_root.join(safe_relative_path(&entry.version_manifest_url)?);
    let manifest: LiveFeedVersionManifest = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    validate_live_feeds_schema("NOTAM version manifest", manifest.schema_version)?;
    if manifest.product != NOTAM_PRODUCT_ID || manifest.version != entry.current {
        bail!(
            "NOTAM version manifest declares {}/{}, expected {}/{}",
            manifest.product,
            manifest.version,
            NOTAM_PRODUCT_ID,
            entry.current
        );
    }
    validate_notam_delta_chain(
        &manifest.state.state_sha256,
        &manifest.recent_deltas,
        &manifest.version,
    )?;
    if manifest.delta_from_previous.as_ref() != manifest.recent_deltas.last() {
        bail!("NOTAM version manifest latest delta is not its retained chain tail");
    }
    Ok(manifest)
}

fn notam_delta_mutation_count(delta: &LiveDeltaRef) -> anyhow::Result<u64> {
    delta
        .mutation_count
        .with_context(|| format!("NOTAM delta {} has no mutation_count", delta.url))
}

fn notam_mutations_after_state(
    checkpoint_state_id: &str,
    deltas: &[LiveDeltaRef],
    target_state_id: &str,
) -> anyhow::Result<u64> {
    if checkpoint_state_id == target_state_id {
        return Ok(0);
    }
    let mut state_id = deltas
        .first()
        .map(|delta| delta.from_state_sha256.as_str())
        .context("NOTAM checkpoint is behind the head, but no deltas are retained")?;
    let mut after_checkpoint = state_id == checkpoint_state_id;
    let mut mutations = 0_u64;
    for delta in deltas {
        if after_checkpoint {
            mutations = mutations.saturating_add(notam_delta_mutation_count(delta)?);
        }
        state_id = &delta.to_state_sha256;
        if state_id == checkpoint_state_id {
            after_checkpoint = true;
        }
    }
    if !after_checkpoint {
        bail!("NOTAM checkpoint {checkpoint_state_id} is outside the retained delta chain");
    }
    if state_id != target_state_id {
        bail!("NOTAM retained chain ends at {state_id}, expected {target_state_id}");
    }
    Ok(mutations)
}

fn trim_notam_delta_suffix(
    checkpoint_state_id: &str,
    deltas: &mut Vec<LiveDeltaRef>,
) -> anyhow::Result<()> {
    let mut retained_mutations = deltas.iter().try_fold(0_u64, |total, delta| {
        Ok::<_, anyhow::Error>(total.saturating_add(notam_delta_mutation_count(delta)?))
    })?;
    while deltas.len() > 1 && retained_mutations > NOTAM_MAX_RETAINED_MUTATIONS {
        let remaining = &deltas[1..];
        let checkpoint_remains_reachable = remaining
            .first()
            .is_some_and(|delta| delta.from_state_sha256 == checkpoint_state_id)
            || remaining
                .iter()
                .any(|delta| delta.to_state_sha256 == checkpoint_state_id);
        if !checkpoint_remains_reachable {
            break;
        }
        retained_mutations =
            retained_mutations.saturating_sub(notam_delta_mutation_count(&deltas[0])?);
        deltas.remove(0);
    }
    Ok(())
}

fn validate_notam_delta_chain(
    checkpoint_state_id: &str,
    deltas: &[LiveDeltaRef],
    target_state_id: &str,
) -> anyhow::Result<()> {
    if deltas.is_empty() {
        if checkpoint_state_id != target_state_id {
            bail!(
                "NOTAM checkpoint is {checkpoint_state_id}, but head is {target_state_id} with no deltas"
            );
        }
        return Ok(());
    }
    let mut head = deltas[0].from_state_sha256.as_str();
    let mut checkpoint_reachable = head == checkpoint_state_id;
    for delta in deltas {
        if delta.kind.as_deref() != Some("notam_ordered_delta_xz") {
            bail!("NOTAM retained delta {} has wrong kind", delta.url);
        }
        if delta.from_version != head || delta.from_state_sha256 != head {
            bail!(
                "NOTAM retained delta {} starts at {}/{}, expected {head}",
                delta.url,
                delta.from_version,
                delta.from_state_sha256
            );
        }
        if delta.to_version != delta.to_state_sha256 {
            bail!(
                "NOTAM retained delta {} has divergent target identities {}/{}",
                delta.url,
                delta.to_version,
                delta.to_state_sha256
            );
        }
        delta
            .mutation_count
            .context("NOTAM retained delta is missing mutation_count")?;
        head = &delta.to_state_sha256;
        checkpoint_reachable |= head == checkpoint_state_id;
    }
    if head != target_state_id {
        bail!("NOTAM retained chain ends at {head}, expected {target_state_id}");
    }
    if !checkpoint_reachable {
        bail!("NOTAM checkpoint {checkpoint_state_id} does not occur in the retained delta chain");
    }
    Ok(())
}

fn validate_published_notam_head(
    live_root: &Path,
    entry: &LiveFeedCurrentEntry,
    current_state_id: &str,
    cursor: &NotamPublicationCursor,
    transitions: &[NotamPublicationTransition],
) -> anyhow::Result<LiveFeedVersionManifest> {
    if entry.state_sha256 != current_state_id {
        bail!(
            "NOTAM current entry identifies {}, expected {}",
            entry.state_sha256,
            current_state_id
        );
    }
    let manifest = read_notam_version_manifest(live_root, entry)?;
    if entry.state_url != manifest.state.url {
        bail!(
            "NOTAM current state URL {} differs from version manifest {}",
            entry.state_url,
            manifest.state.url
        );
    }
    validate_notam_delta_chain(
        &manifest.state.state_sha256,
        &manifest.recent_deltas,
        current_state_id,
    )?;
    validate_notam_materialized_chain(live_root, &manifest)?;

    if let Some(cursor_head) = cursor.published_head_state_id.as_deref() {
        if cursor_head != current_state_id {
            let expected = collapse_notam_transitions(cursor, transitions)?;
            let delta_ref = manifest.delta_from_previous.as_ref().context(
                "published NOTAM head is ahead of SQLite cursor but has no latest delta",
            )?;
            if delta_ref.from_state_sha256 != expected.from_state_id
                || delta_ref.to_state_sha256 != expected.to_state_id
                || delta_ref.mutation_count != Some(expected.mutations.len() as u64)
            {
                bail!("published NOTAM delta does not match pending SQLite journal");
            }
            let delta_path = live_root.join(safe_relative_path(&delta_ref.url)?);
            let published_delta: NotamDelta = serde_json::from_value(read_json_value(&delta_path)?)
                .with_context(|| format!("failed to decode {}", delta_path.display()))?;
            if published_delta != expected {
                bail!("published NOTAM delta differs from pending SQLite journal");
            }
        } else if !transitions.is_empty() {
            bail!("NOTAM cursor is current but pending journal is not empty");
        }
    } else {
        if manifest.state.state_sha256 != current_state_id || manifest.delta_from_previous.is_some()
        {
            bail!("initial published NOTAM head is not a full current checkpoint");
        }
    }
    Ok(manifest)
}

fn validate_notam_materialized_chain(
    live_root: &Path,
    manifest: &LiveFeedVersionManifest,
) -> anyhow::Result<()> {
    if manifest.state.kind.as_deref() != Some("notam_checkpoint_xz") {
        bail!("published NOTAM state is not a checkpoint");
    }
    let checkpoint: NotamCheckpoint = read_verified_notam_blob(
        live_root,
        &manifest.state.url,
        manifest.state.bytes,
        &manifest.state.blob_sha256,
    )?;
    if checkpoint.state_id != manifest.state.state_sha256 {
        bail!(
            "NOTAM checkpoint payload identifies {}, manifest identifies {}",
            checkpoint.state_id,
            manifest.state.state_sha256
        );
    }
    let mut work = NotamApplyWork::default();
    let mut state = NotamState::from_checkpoint(checkpoint, &mut work)
        .map_err(anyhow::Error::msg)
        .context("published NOTAM checkpoint failed full identity verification")?;

    for delta_ref in &manifest.recent_deltas {
        let delta: NotamDelta = read_verified_notam_blob(
            live_root,
            &delta_ref.url,
            delta_ref.bytes,
            &delta_ref.blob_sha256,
        )?;
        delta
            .validate_contract()
            .map_err(anyhow::Error::msg)
            .context("published NOTAM delta has the wrong contract")?;
        if delta.from_state_id != delta_ref.from_state_sha256
            || delta.to_state_id != delta_ref.to_state_sha256
            || delta.mutations.len() as u64 != notam_delta_mutation_count(delta_ref)?
        {
            bail!(
                "published NOTAM delta payload does not match {}",
                delta_ref.url
            );
        }
        notam_state::validate_mutation_order(&delta.mutations)
            .map_err(anyhow::Error::msg)
            .with_context(|| {
                format!("published NOTAM delta is not canonical: {}", delta_ref.url)
            })?;

        if delta.from_state_id == state.state_id() {
            state
                .apply_delta(delta, &mut work)
                .map_err(anyhow::Error::msg)
                .with_context(|| {
                    format!("failed to replay published NOTAM delta {}", delta_ref.url)
                })?;
        } else if delta.to_state_id != state.state_id() {
            // The retained suffix may begin before the checkpoint. Chain-shape
            // validation guarantees these skipped transitions lead to it.
            continue;
        }
    }
    if state.state_id() != manifest.version {
        bail!(
            "published NOTAM payload chain ends at {}, manifest identifies {}",
            state.state_id(),
            manifest.version
        );
    }
    Ok(())
}

fn read_verified_notam_blob<T: DeserializeOwned>(
    live_root: &Path,
    relative_url: &str,
    expected_bytes: u64,
    expected_sha256: &str,
) -> anyhow::Result<T> {
    let path = live_root.join(safe_relative_path(relative_url)?);
    let encoded = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    if encoded.len() as u64 != expected_bytes || sha256_hex(&encoded) != expected_sha256 {
        bail!(
            "NOTAM immutable blob identity mismatch at {}",
            path.display()
        );
    }
    let decoded = nav_kv_package::decode_xz_if_needed(&encoded)
        .map_err(|error| anyhow::anyhow!("failed to decode {}: {error}", path.display()))?;
    serde_json::from_slice(decoded.as_ref())
        .with_context(|| format!("failed to parse {}", path.display()))
}

pub fn read_live_feeds_current(root: &Path) -> anyhow::Result<Option<LiveFeedsCurrentManifest>> {
    let path = root.join("current.json");
    if !path.is_file() {
        return Ok(None);
    }
    let current: LiveFeedsCurrentManifest = serde_json::from_slice(
        &fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))?;
    validate_live_feeds_schema("current manifest", current.schema_version)?;
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

fn live_feed_current_history_entries(
    live_root: &Path,
    product: &str,
    current_version: &str,
    policy: &LiveFeedRetentionPolicy,
    now: DateTime<Utc>,
) -> anyhow::Result<Vec<LiveFeedCurrentHistoryEntry>> {
    let product_versions_root = live_root.join("versions").join(product);
    if !product_versions_root.is_dir() {
        return Ok(Vec::new());
    }
    let recent_tail = policy.recent_tail_for(product);
    let mut entries = Vec::new();
    for version_entry in fs::read_dir(&product_versions_root)
        .with_context(|| format!("failed to read {}", product_versions_root.display()))?
    {
        let version_entry = version_entry
            .with_context(|| format!("failed to read {}", product_versions_root.display()))?;
        let version_path = version_entry.path();
        if !version_entry
            .file_type()
            .with_context(|| format!("failed to stat {}", version_path.display()))?
            .is_file()
            || version_path
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("json")
            || !path_modified_within(&version_path, now, recent_tail)
        {
            continue;
        }
        let manifest: LiveFeedVersionManifest = serde_json::from_slice(
            &fs::read(&version_path)
                .with_context(|| format!("failed to read {}", version_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", version_path.display()))?;
        if manifest.schema_version != LIVE_FEEDS_SCHEMA_VERSION {
            continue;
        }
        if manifest.product != product || manifest.version == current_version {
            continue;
        }
        let state_path = live_root.join(safe_relative_path(&manifest.state.url)?);
        if !state_path.exists() {
            continue;
        }
        entries.push((
            modified_time(&version_path),
            manifest.version.clone(),
            LiveFeedCurrentHistoryEntry {
                version: manifest.version,
                version_manifest_url: live_feeds_relative_url(live_root, &version_path)?,
                state_url: Some(manifest.state.url),
                state_sha256: Some(manifest.state.state_sha256),
            },
        ));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    if entries.len() > LIVE_FEED_CURRENT_HISTORY_MAX_ENTRIES {
        entries.drain(0..entries.len() - LIVE_FEED_CURRENT_HISTORY_MAX_ENTRIES);
    }
    Ok(entries.into_iter().map(|(_, _, entry)| entry).collect())
}

pub fn prune_live_feed_publication(
    live_root: &Path,
    policy: &LiveFeedRetentionPolicy,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let Some(current) = read_live_feeds_current(live_root)? else {
        return Ok(());
    };

    let mut retained = BTreeSet::new();
    retained.insert(live_root.join("current.json"));
    for (product, entry) in &current.products {
        let version_manifest_path =
            retain_live_relative_path(live_root, &mut retained, &entry.version_manifest_url)?;
        retain_live_relative_path(live_root, &mut retained, &entry.state_url)?;
        if version_manifest_path.is_file() {
            let manifest =
                retain_version_manifest(live_root, &version_manifest_path, &mut retained)?;
            if let Some(previous) = manifest.previous.as_deref() {
                let previous_manifest_path =
                    live_feed_version_manifest_path(live_root, product, previous)?;
                if previous_manifest_path.is_file() {
                    retain_version_manifest(live_root, &previous_manifest_path, &mut retained)?;
                }
            }
        }
        for history_entry in &entry.history {
            let history_manifest_path = retain_live_relative_path(
                live_root,
                &mut retained,
                &history_entry.version_manifest_url,
            )?;
            if let Some(state_url) = history_entry.state_url.as_deref() {
                retain_live_relative_path(live_root, &mut retained, state_url)?;
            }
            if history_manifest_path.is_file() {
                retain_version_manifest(live_root, &history_manifest_path, &mut retained)?;
            }
        }
    }

    retain_recent_live_feed_versions(live_root, policy, now, &mut retained)?;

    for child in LIVE_FEED_PUBLICATION_DIRS {
        prune_product_children(&live_root.join(child), &retained)?;
    }
    Ok(())
}

fn retain_recent_live_feed_versions(
    live_root: &Path,
    policy: &LiveFeedRetentionPolicy,
    now: DateTime<Utc>,
    retained: &mut BTreeSet<PathBuf>,
) -> anyhow::Result<()> {
    let versions_root = live_root.join("versions");
    if !versions_root.is_dir() {
        return Ok(());
    }
    for product_entry in fs::read_dir(&versions_root)
        .with_context(|| format!("failed to read {}", versions_root.display()))?
    {
        let product_entry =
            product_entry.with_context(|| format!("failed to read {}", versions_root.display()))?;
        let product_path = product_entry.path();
        if !product_entry
            .file_type()
            .with_context(|| format!("failed to stat {}", product_path.display()))?
            .is_dir()
        {
            continue;
        }
        let product = product_entry.file_name().to_string_lossy().into_owned();
        let recent_tail = policy.recent_tail_for(&product);
        for version_entry in fs::read_dir(&product_path)
            .with_context(|| format!("failed to read {}", product_path.display()))?
        {
            let version_entry = version_entry
                .with_context(|| format!("failed to read {}", product_path.display()))?;
            let path = version_entry.path();
            if !version_entry
                .file_type()
                .with_context(|| format!("failed to stat {}", path.display()))?
                .is_file()
                || path.extension().and_then(|extension| extension.to_str()) != Some("json")
            {
                continue;
            }
            if path_modified_within(&path, now, recent_tail) {
                retain_version_manifest(live_root, &path, retained)?;
            }
        }
    }
    Ok(())
}

fn retain_version_manifest(
    live_root: &Path,
    version_manifest_path: &Path,
    retained: &mut BTreeSet<PathBuf>,
) -> anyhow::Result<LiveFeedVersionManifest> {
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
    for delta in &manifest.recent_deltas {
        retain_live_relative_path(live_root, retained, &delta.url)?;
    }
    Ok(manifest)
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

fn live_feed_version_manifest_path(
    live_root: &Path,
    product: &str,
    version: &str,
) -> anyhow::Result<PathBuf> {
    if !is_safe_path_segment(product) || !is_safe_path_segment(version) {
        bail!("invalid live-feed product/version path: {product}/{version}");
    }
    Ok(live_root
        .join("versions")
        .join(product)
        .join(format!("{version}.json")))
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

fn is_safe_path_segment(value: &str) -> bool {
    let mut components = Path::new(value).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn path_modified_within(path: &Path, now: DateTime<Utc>, recent_tail: StdDuration) -> bool {
    let now_system: SystemTime = now.into();
    let modified = modified_time(path);
    match now_system.duration_since(modified) {
        Ok(age) => age <= recent_tail,
        Err(_) => true,
    }
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

fn validate_live_feeds_schema(label: &str, schema_version: u32) -> anyhow::Result<()> {
    if schema_version == LIVE_FEEDS_SCHEMA_VERSION {
        return Ok(());
    }
    bail!("unsupported {label} schema {schema_version}; required {LIVE_FEEDS_SCHEMA_VERSION}")
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
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let bytes = nav_kv_package::decode_xz_if_needed(&bytes)
        .map_err(|err| anyhow::anyhow!("failed to decode {}: {err}", path.display()))?;
    serde_json::from_slice(bytes.as_ref())
        .with_context(|| format!("failed to parse {}", path.display()))
}

pub fn write_json_pretty_file(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("failed to encode JSON")?;
    atomic_write_bytes(path, &bytes)
}

fn write_immutable_json_pretty_file(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("failed to encode immutable JSON")?;
    if path.is_file() {
        let existing = fs::read(path)
            .with_context(|| format!("failed to read immutable {}", path.display()))?;
        if existing != bytes {
            bail!("immutable live-feed manifest changed at {}", path.display());
        }
        return Ok(());
    }
    atomic_write_bytes(path, &bytes)
        .with_context(|| format!("failed to write immutable {}", path.display()))
}

pub fn write_xz_json_pretty_file(path: &Path, value: &impl Serialize) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(value).context("failed to encode JSON")?;
    let encoded = producer_xz_compress_bytes(&bytes)
        .map_err(|err| anyhow::anyhow!("failed to xz-compress {}: {err}", path.display()))?;
    atomic_write_bytes(path, &encoded)
}

fn write_immutable_xz_json_pretty_file(
    path: &Path,
    value: &impl Serialize,
) -> anyhow::Result<Vec<u8>> {
    let json = serde_json::to_vec_pretty(value).context("failed to encode immutable JSON")?;
    let encoded = producer_xz_compress_bytes(&json)
        .map_err(|err| anyhow::anyhow!("failed to xz-compress {}: {err}", path.display()))?;
    if path.is_file() {
        let existing = fs::read(path)
            .with_context(|| format!("failed to read immutable {}", path.display()))?;
        if existing != encoded {
            bail!("immutable live-feed payload changed at {}", path.display());
        }
        return Ok(existing);
    }
    atomic_write_bytes(path, &encoded)
        .with_context(|| format!("failed to write immutable {}", path.display()))?;
    Ok(encoded)
}

fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("{} has no UTF-8 file name", path.display()))?;
    let temp = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));
    {
        let mut file =
            File::create(&temp).with_context(|| format!("failed to create {}", temp.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("failed to write {}", temp.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temp.display()))?;
    }
    fs::rename(&temp, path)
        .with_context(|| format!("failed to promote {} to {}", temp.display(), path.display()))?;
    File::open(parent)
        .with_context(|| format!("failed to open {}", parent.display()))?
        .sync_all()
        .with_context(|| format!("failed to sync {}", parent.display()))
}

fn producer_xz_compress_bytes(bytes: &[u8]) -> Result<Vec<u8>, String> {
    xz_compress_bytes_with_system_xz(bytes).map_err(|err| err.to_string())
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
        } else if file_type.is_symlink() {
            copy_symlink_or_target(&source, &output)?;
        }
    }
    Ok(())
}

fn zip_members_for_dir(source_dir: &Path) -> anyhow::Result<Vec<ZipSource>> {
    let mut members = Vec::new();
    collect_zip_members_for_dir(source_dir, source_dir, &mut members)?;
    Ok(members)
}

fn collect_zip_members_for_dir(
    root: &Path,
    source_dir: &Path,
    members: &mut Vec<ZipSource>,
) -> anyhow::Result<()> {
    let mut entries = fs::read_dir(source_dir)
        .with_context(|| format!("failed to read {}", source_dir.display()))?
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let source = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", source.display()))?;
        if file_type.is_dir() {
            collect_zip_members_for_dir(root, &source, members)?;
        } else if file_type.is_file() {
            let member_name = source
                .strip_prefix(root)
                .with_context(|| format!("{} is not under {}", source.display(), root.display()))?
                .to_string_lossy()
                .replace('\\', "/");
            members.push(ZipSource::new(member_name, source));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_symlink_or_target(source: &Path, output: &Path) -> anyhow::Result<()> {
    if output.exists() {
        return Ok(());
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let target = fs::read_link(source)
        .with_context(|| format!("failed to read link {}", source.display()))?;
    std::os::unix::fs::symlink(&target, output).with_context(|| {
        format!(
            "failed to symlink {} to {}",
            output.display(),
            target.display()
        )
    })
}

#[cfg(not(unix))]
fn copy_symlink_or_target(source: &Path, output: &Path) -> anyhow::Result<()> {
    let metadata =
        fs::metadata(source).with_context(|| format!("failed to stat {}", source.display()))?;
    if metadata.is_dir() {
        hardlink_or_copy_dir_recursive(source, output)
    } else {
        copy_file_if_missing(source, output)
    }
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

fn remove_empty_dir_if_exists(path: &Path) {
    if let Ok(mut entries) = fs::read_dir(path) {
        if entries.next().is_none() {
            let _ = fs::remove_dir(path);
        }
    }
}

fn prune_failed_live_feed_scratch(
    product_scratch_root: &Path,
    retain_count: usize,
    active_scratch_dir: Option<&Path>,
) -> anyhow::Result<()> {
    if !product_scratch_root.is_dir() {
        return Ok(());
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(product_scratch_root)
        .with_context(|| format!("failed to read {}", product_scratch_root.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read {}", product_scratch_root.display()))?;
        let path = entry.path();
        entries.push((modified_time(&path), path));
    }
    entries.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.file_name().cmp(&right.1.file_name()))
    });

    let mut retained = BTreeSet::new();
    if let Some(active_scratch_dir) = active_scratch_dir {
        retained.insert(active_scratch_dir.to_path_buf());
    }
    for (_, path) in entries.iter().take(retain_count) {
        retained.insert(path.clone());
    }
    for (_, path) in entries {
        if !retained.contains(&path) {
            remove_path_if_exists(&path)?;
        }
    }
    remove_empty_dir_if_exists(product_scratch_root);
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

    #[path = "notam_incremental_fixture_test.rs"]
    mod notam_incremental_fixture;

    fn notam_delta_ref(from: &str, to: &str, mutation_count: u64) -> LiveDeltaRef {
        LiveDeltaRef {
            kind: Some("notam_ordered_delta_xz".to_string()),
            from_version: from.to_string(),
            from_state_sha256: from.to_string(),
            to_version: to.to_string(),
            to_state_sha256: to.to_string(),
            url: format!("deltas/notams/{from}__{to}.json.xz"),
            bytes: 1,
            blob_sha256: "a".repeat(64),
            mutation_count: Some(mutation_count),
        }
    }

    #[test]
    fn notam_replay_cost_counts_only_deltas_after_checkpoint() -> anyhow::Result<()> {
        let deltas = vec![
            notam_delta_ref("s0", "s1", 60),
            notam_delta_ref("s1", "s2", 50),
            notam_delta_ref("s2", "s3", 40),
        ];
        validate_notam_delta_chain("s2", &deltas, "s3")?;
        assert_eq!(notam_mutations_after_state("s2", &deltas, "s3")?, 40);
        Ok(())
    }

    #[test]
    fn notam_retention_trims_whole_deltas_and_keeps_checkpoint_reachable() -> anyhow::Result<()> {
        let mut deltas = vec![
            notam_delta_ref("s0", "s1", 60),
            notam_delta_ref("s1", "s2", 50),
            notam_delta_ref("s2", "s3", 40),
        ];
        trim_notam_delta_suffix("s2", &mut deltas)?;
        assert_eq!(
            deltas
                .iter()
                .map(|delta| (&*delta.from_version, &*delta.to_version))
                .collect::<Vec<_>>(),
            vec![("s1", "s2"), ("s2", "s3")]
        );
        validate_notam_delta_chain("s2", &deltas, "s3")?;

        let mut rotated = vec![
            notam_delta_ref("s0", "s1", 60),
            notam_delta_ref("s1", "s2", 50),
            notam_delta_ref("s2", "s3", 40),
        ];
        trim_notam_delta_suffix("s3", &mut rotated)?;
        assert_eq!(rotated.len(), 2);
        assert_eq!(rotated[0].from_version, "s1");

        let mut oversized = vec![notam_delta_ref("a", "b", 140)];
        trim_notam_delta_suffix("b", &mut oversized)?;
        assert_eq!(oversized.len(), 1);
        Ok(())
    }

    #[test]
    fn fresh_notam_store_replaces_unrelated_published_source_epoch() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let live_root = temp.path().join("live-feeds");
        let state_root = temp.path().join("notam-state");
        let publisher = FileLiveFeedPublisher::new(
            live_root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 7, 24, 15, 32, 0).unwrap()),
        );
        let old_state_id = "a".repeat(64);
        write_live_feeds_current_manifest(
            &live_root,
            &LiveFeedsCurrentManifest {
                schema_version: LIVE_FEEDS_SCHEMA_VERSION,
                generated_at_utc: "2026-07-24T15:00:00Z".to_string(),
                products: BTreeMap::from([(
                    NOTAM_PRODUCT_ID.to_string(),
                    LiveFeedCurrentEntry {
                        current: old_state_id.clone(),
                        version_manifest_url: format!(
                            "versions/{NOTAM_PRODUCT_ID}/{old_state_id}.json"
                        ),
                        state_url: format!("states/{NOTAM_PRODUCT_ID}/{old_state_id}.json.xz"),
                        state_sha256: old_state_id,
                        published_at_utc: Some("2026-07-24T15:00:00Z".to_string()),
                        collected_at_utc: Some("2026-07-24T15:00:00Z".to_string()),
                        history: vec![LiveFeedCurrentHistoryEntry {
                            version: "older-source-state".to_string(),
                            version_manifest_url: "versions/notams/older-source-state.json"
                                .to_string(),
                            state_url: None,
                            state_sha256: None,
                        }],
                    },
                )]),
            },
        )?;

        let record = crate::canonicalize_structured_notam_record(serde_json::from_value(
            serde_json::json!({
                "id": "placeholder",
                "nms_id": "1784413572039718",
                "source_type": "D",
                "notam_status": "ACTIVE",
                "location_designator": "AAA",
                "icao_id": "KAAA",
                "location": "AAA",
                "notam_number": "1",
                "notam_year": "2026",
                "notam_type": "N",
                "text": "RWY 01 CLSD."
            }),
        )?)?;
        let store = NotamPersistentStore::new(&state_root);
        let synchronized = store.synchronize_current_records(&[record], "2026-07-24T15:32:00Z")?;
        let update = publisher.publish(BuiltLiveFeedState {
            product: NOTAM_PRODUCT_ID.to_string(),
            version: synchronized.state_id.clone(),
            payload: LiveFeedStatePayload::NotamIncremental {
                state_root: state_root.clone(),
            },
            state_sha256: None,
            state_payload_kind: None,
            status_timestamps: Default::default(),
            delta_policy: DeltaPolicy::None,
            precomputed_delta: None,
            changed_count_if_no_delta: 0,
        })?;

        assert_eq!(update.version, synchronized.state_id);
        assert_eq!(update.changed_count, 1);
        assert_eq!(update.removed_count, 0);
        assert!(update.delta_path.is_none());
        assert!(update.history.is_empty());
        let manifest: LiveFeedVersionManifest =
            serde_json::from_slice(&fs::read(&update.version_manifest_path)?)?;
        assert_eq!(manifest.previous, None);
        assert!(manifest.delta_from_previous.is_none());
        assert_eq!(manifest.state.kind.as_deref(), Some("notam_checkpoint_xz"));
        let current = read_live_feeds_current(&live_root)?.context("missing current manifest")?;
        assert_eq!(current.products[NOTAM_PRODUCT_ID].current, update.version);
        assert!(current.products[NOTAM_PRODUCT_ID].history.is_empty());

        publisher.acknowledge(&update)?;
        let snapshot = store.publication_snapshot()?;
        assert_eq!(
            snapshot.cursor.published_head_state_id.as_deref(),
            Some(update.version.as_str())
        );

        let unrelated_state_id = "b".repeat(64);
        let mut divergent = current;
        divergent
            .products
            .get_mut(NOTAM_PRODUCT_ID)
            .context("missing NOTAM current entry")?
            .current = unrelated_state_id.clone();
        write_live_feeds_current_manifest(&live_root, &divergent)?;
        let error = publisher
            .publish(BuiltLiveFeedState {
                product: NOTAM_PRODUCT_ID.to_string(),
                version: update.version,
                payload: LiveFeedStatePayload::NotamIncremental { state_root },
                state_sha256: None,
                state_payload_kind: None,
                status_timestamps: Default::default(),
                delta_policy: DeltaPolicy::None,
                precomputed_delta: None,
                changed_count_if_no_delta: 0,
            })
            .expect_err("acknowledged publication divergence was silently reset");
        assert!(format!("{error:#}").contains("outside the pending journal"));
        Ok(())
    }

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
    #[ignore = "requires the external three-hour METAR fixture"]
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
    fn canonical_json_hash_is_independent_of_object_insertion_order() -> anyhow::Result<()> {
        let mut left_inner = serde_json::Map::new();
        left_inner.insert("bravo".to_string(), serde_json::json!(2));
        left_inner.insert("alpha".to_string(), serde_json::json!(1));
        let mut left = serde_json::Map::new();
        left.insert("outer_b".to_string(), Value::Object(left_inner));
        left.insert("outer_a".to_string(), serde_json::json!(0));

        let mut right_inner = serde_json::Map::new();
        right_inner.insert("alpha".to_string(), serde_json::json!(1));
        right_inner.insert("bravo".to_string(), serde_json::json!(2));
        let mut right = serde_json::Map::new();
        right.insert("outer_a".to_string(), serde_json::json!(0));
        right.insert("outer_b".to_string(), Value::Object(right_inner));

        assert_eq!(Value::Object(left.clone()), Value::Object(right.clone()));
        assert_eq!(
            canonical_json_sha256(&Value::Object(left))?,
            canonical_json_sha256(&Value::Object(right))?
        );
        Ok(())
    }

    #[test]
    fn file_publisher_writes_json_state_without_delta() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new(
            root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 1, 2, 3).unwrap()),
        );
        let state_path = temp.path().join("winds-aloft.json");
        let state_value = serde_json::json!({
            "schema_version": 1,
            "product_id": "winds-aloft",
            "generated_at_utc": "2026-05-09T06:00:00Z",
            "files": [
                {
                    "forecast_hour": 3,
                    "path": "grib2/gfs_20260509_06_f003.grib2",
                    "size_bytes": 123
                }
            ]
        });
        write_json_pretty_file(&state_path, &state_value)?;

        let result = publisher.publish(BuiltLiveFeedState {
            product: "winds-aloft".to_string(),
            version: "v1".to_string(),
            payload: LiveFeedStatePayload::JsonFile {
                path: state_path,
                value: state_value.clone(),
            },
            state_sha256: None,
            state_payload_kind: None,
            status_timestamps: Default::default(),
            delta_policy: DeltaPolicy::None,
            precomputed_delta: None,
            changed_count_if_no_delta: 1,
        })?;

        assert_eq!(result.product, "winds-aloft");
        assert_eq!(result.version, "v1");
        assert_eq!(result.changed_count, 1);
        assert_eq!(result.delta_path, None);

        let current = read_live_feeds_current(&root)?.expect("current manifest");
        let entry = current
            .products
            .get("winds-aloft")
            .expect("winds-aloft current entry");
        assert_eq!(entry.current, "v1");
        assert_eq!(entry.state_url, "states/winds-aloft/v1.json.xz");
        assert_eq!(entry.state_sha256, canonical_json_sha256(&state_value)?);

        let version_manifest_path = root.join("versions").join("winds-aloft").join("v1.json");
        let version_manifest: LiveFeedVersionManifest =
            serde_json::from_slice(&fs::read(version_manifest_path)?)?;
        assert_eq!(version_manifest.schema_version, LIVE_FEEDS_SCHEMA_VERSION);
        assert_eq!(version_manifest.product, "winds-aloft");
        assert_eq!(version_manifest.previous, None);
        assert_eq!(version_manifest.state.kind.as_deref(), Some("json_xz"));
        assert!(version_manifest.delta_from_previous.is_none());
        Ok(())
    }

    #[test]
    fn file_publisher_labels_directory_manifest_state() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new(
            root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 1, 2, 3).unwrap()),
        );
        let source_root = temp.path().join("nexrad-state");
        fs::create_dir_all(source_root.join("tiles/z00"))?;
        let manifest_path = source_root.join("manifest.json");
        let manifest_value = serde_json::json!({
            "schema_version": 1,
            "product": "nexrad",
            "state_id": "v1",
            "levels": []
        });
        write_json_pretty_file(&manifest_path, &manifest_value)?;
        write_json_pretty_file(
            &source_root.join("tiles/z00/tile.json"),
            &serde_json::json!({"tile": true}),
        )?;

        let result = publisher.publish(BuiltLiveFeedState {
            product: "nexrad".to_string(),
            version: "v1".to_string(),
            payload: LiveFeedStatePayload::Directory {
                root: source_root,
                manifest_path,
                manifest_value: manifest_value.clone(),
            },
            state_sha256: None,
            state_payload_kind: None,
            status_timestamps: Default::default(),
            delta_policy: DeltaPolicy::None,
            precomputed_delta: None,
            changed_count_if_no_delta: 1,
        })?;

        assert_eq!(result.product, "nexrad");
        assert_eq!(result.version, "v1");

        let version_manifest_path = root.join("versions").join("nexrad").join("v1.json");
        let version_manifest: LiveFeedVersionManifest =
            serde_json::from_slice(&fs::read(version_manifest_path)?)?;
        assert_eq!(version_manifest.schema_version, LIVE_FEEDS_SCHEMA_VERSION);
        assert_eq!(version_manifest.state.kind.as_deref(), Some("json"));
        assert_eq!(version_manifest.state.url, "states/nexrad/v1/manifest.json");
        assert_eq!(
            version_manifest
                .install_state
                .as_ref()
                .and_then(|state| state.kind.as_deref()),
            Some("directory_package")
        );
        assert!(version_manifest.delta_from_previous.is_none());
        Ok(())
    }

    #[test]
    fn obstacle_had_delta_round_trips_replaces_adds_and_deletes() -> anyhow::Result<()> {
        let from = vec![
            nav_kv_pair("obstacle/tile/z12/x000001/y000001", "old-a"),
            nav_kv_pair("obstacle/tile/z12/x000001/y000002", "old-b"),
            nav_kv_pair("obstacle/tile/z12/x000001/y000003", "old-c"),
        ];
        let to = vec![
            nav_kv_pair("obstacle/tile/z12/x000001/y000001", "new-a"),
            nav_kv_pair("obstacle/tile/z12/x000001/y000003", "old-c"),
            nav_kv_pair("obstacle/tile/z12/x000001/y000004", "new-d"),
        ];
        let delta = build_nav_kv_delta(&from, &to).map_err(anyhow::Error::msg)?;
        let applied = apply_nav_kv_delta(&from, &delta).map_err(anyhow::Error::msg)?;

        assert_eq!(applied, to);
        assert!(
            applied
                .iter()
                .all(|pair| pair.key != "obstacle/tile/z12/x000001/y000002"),
            "deleted HAD keys must not survive delta application"
        );
        assert_eq!(
            nav_kv_canonical_sha256_from_pairs(&applied),
            nav_kv_canonical_sha256_from_pairs(&to)
        );
        Ok(())
    }

    #[test]
    fn obstacle_live_feed_publishes_had_delta_from_previous_state() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let live_root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new(
            live_root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 1, 2, 3).unwrap()),
        );
        let first_pairs = vec![
            nav_kv_pair("obstacle/tile/z12/x000001/y000001", "old-a"),
            nav_kv_pair("obstacle/tile/z12/x000001/y000002", "old-b"),
        ];
        let second_pairs = vec![
            nav_kv_pair("obstacle/tile/z12/x000001/y000001", "new-a"),
            nav_kv_pair("obstacle/tile/z12/x000001/y000003", "new-c"),
        ];
        let first = write_test_nav_kv_state(temp.path(), "obstacles", "v1", &first_pairs)?;
        let second = write_test_nav_kv_state(temp.path(), "obstacles", "v2", &second_pairs)?;

        publisher.publish(BuiltLiveFeedState {
            product: "obstacles".to_string(),
            version: "v1".to_string(),
            payload: LiveFeedStatePayload::Directory {
                root: first.root,
                manifest_path: first.manifest_path,
                manifest_value: first.manifest_value,
            },
            state_sha256: Some(first.state_sha256),
            state_payload_kind: Some("nav_kv".to_string()),
            status_timestamps: Default::default(),
            delta_policy: DeltaPolicy::NavKv {
                pairs: first_pairs.clone(),
            },
            precomputed_delta: None,
            changed_count_if_no_delta: 2,
        })?;
        let result = publisher.publish(BuiltLiveFeedState {
            product: "obstacles".to_string(),
            version: "v2".to_string(),
            payload: LiveFeedStatePayload::Directory {
                root: second.root,
                manifest_path: second.manifest_path,
                manifest_value: second.manifest_value,
            },
            state_sha256: Some(second.state_sha256.clone()),
            state_payload_kind: Some("nav_kv".to_string()),
            status_timestamps: Default::default(),
            delta_policy: DeltaPolicy::NavKv {
                pairs: second_pairs.clone(),
            },
            precomputed_delta: None,
            changed_count_if_no_delta: 2,
        })?;

        let delta_path = result
            .delta_path
            .expect("second publish should write HAD delta");
        let delta: LiveFeedNavKvDelta = serde_json::from_value(read_json_value(&delta_path)?)?;
        assert_eq!(
            delta
                .entries
                .iter()
                .map(|entry| (entry.key.as_str(), entry.value.is_some()))
                .collect::<Vec<_>>(),
            vec![
                ("obstacle/tile/z12/x000001/y000001", true),
                ("obstacle/tile/z12/x000001/y000002", false),
                ("obstacle/tile/z12/x000001/y000003", true)
            ]
        );
        assert_eq!(
            delta.to_state_sha256,
            nav_kv_canonical_sha256_from_pairs(&second_pairs)
        );
        let version_manifest: LiveFeedVersionManifest =
            serde_json::from_slice(&fs::read(live_root.join("versions/obstacles/v2.json"))?)?;
        assert_eq!(version_manifest.schema_version, LIVE_FEEDS_SCHEMA_VERSION);
        assert_eq!(version_manifest.state.kind.as_deref(), Some("nav_kv"));
        assert_eq!(version_manifest.state.state_sha256, second.state_sha256);
        assert_eq!(
            version_manifest
                .delta_from_previous
                .as_ref()
                .and_then(|delta| delta.kind.as_deref()),
            Some("nav_kv_delta_xz")
        );
        let install_url = version_manifest
            .install_state
            .as_ref()
            .expect("obstacle install package")
            .url
            .clone();
        let package_path = live_root.join(install_url);
        let mut archive = ZipArchive::new(fs::File::open(&package_path)?)?;
        for index in 0..archive.len() {
            let name = archive.by_index(index)?.name().to_string();
            assert!(
                !name.ends_with(".zip"),
                "obstacle install package must not contain nested zip member {name}"
            );
        }
        let mut encoded_page = Vec::new();
        let mut page_member = archive.by_name("page_0000")?;
        assert_eq!(page_member.compression(), CompressionMethod::Stored);
        page_member.read_to_end(&mut encoded_page)?;
        assert!(nav_kv_package::is_xz(&encoded_page));
        Ok(())
    }

    #[test]
    fn file_publisher_uses_precomputed_record_delta() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let live_root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new(
            live_root,
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 1, 2, 3).unwrap()),
        );
        let first_path = temp.path().join("metars-v1.json");
        let second_path = temp.path().join("metars-v2.json");
        let first = serde_json::json!({
            "version_label": "v1",
            "generated_at_utc": "2026-05-18T01:00:00Z",
            "record_count": 1,
            "records": {"A": {"value": 1}}
        });
        let second = serde_json::json!({
            "version_label": "v2",
            "generated_at_utc": "2026-05-18T01:05:00Z",
            "record_count": 2,
            "records": {"A": {"value": 1}, "B": {"value": 2}}
        });
        write_json_pretty_file(&first_path, &first)?;
        write_json_pretty_file(&second_path, &second)?;

        let first_update = publisher.publish(BuiltLiveFeedState {
            product: "metars".to_string(),
            version: "v1".to_string(),
            payload: LiveFeedStatePayload::JsonFile {
                path: first_path,
                value: first,
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
        })?;
        fs::remove_file(first_update.state_path)?;

        let precomputed_delta = LiveFeedRecordDelta {
            schema_version: LIVE_FEEDS_SCHEMA_VERSION,
            product: "metars".to_string(),
            from_version: "v1".to_string(),
            to_version: "v2".to_string(),
            top_level_changed: BTreeMap::from([
                (
                    "generated_at_utc".to_string(),
                    serde_json::json!("2026-05-18T01:05:00Z"),
                ),
                ("record_count".to_string(), serde_json::json!(2)),
            ]),
            top_level_removed: Vec::new(),
            changed: BTreeMap::from([("B".to_string(), serde_json::json!({"value": 2}))]),
            removed: Vec::new(),
        };
        let result = publisher.publish(BuiltLiveFeedState {
            product: "metars".to_string(),
            version: "v2".to_string(),
            payload: LiveFeedStatePayload::JsonFile {
                path: second_path,
                value: second,
            },
            state_sha256: None,
            state_payload_kind: None,
            status_timestamps: Default::default(),
            delta_policy: DeltaPolicy::KeyedRecords {
                records_key: "records".to_string(),
                count_key: Some("record_count".to_string()),
            },
            precomputed_delta: Some(precomputed_delta),
            changed_count_if_no_delta: 2,
        })?;

        let delta_path = result.delta_path.expect("precomputed delta path");
        let delta: LiveFeedRecordDelta = serde_json::from_value(read_json_value(&delta_path)?)?;
        assert_eq!(delta.changed.keys().cloned().collect::<Vec<_>>(), vec!["B"]);
        assert_eq!(result.changed_count, 1);
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
                state_sha256: None,
                state_payload_kind: None,
                status_timestamps: Default::default(),
                delta_policy: DeltaPolicy::KeyedRecords {
                    records_key: "records".to_string(),
                    count_key: Some("record_count".to_string()),
                },
                precomputed_delta: None,
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
                state_sha256: None,
                state_payload_kind: None,
                status_timestamps: Default::default(),
                delta_policy: DeltaPolicy::KeyedRecords {
                    records_key: "records".to_string(),
                    count_key: Some("record_count".to_string()),
                },
                precomputed_delta: None,
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
    fn file_publisher_writes_taf_keyed_record_delta() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new(
            root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 1, 2, 3).unwrap()),
        );

        publisher.publish(json_state_with_count_key(
            temp.path(),
            "tafs",
            "tafs-v1",
            "v1",
            "tafs_by_station",
            "taf_count",
            &[("KSEA", 1), ("KPAE", 2)],
        )?)?;
        let result = publisher.publish(json_state_with_count_key(
            temp.path(),
            "tafs",
            "tafs-v2",
            "v2",
            "tafs_by_station",
            "taf_count",
            &[("KSEA", 3), ("KBFI", 4)],
        )?)?;

        let delta_path = result.delta_path.expect("TAF delta path");
        assert_eq!(
            delta_path,
            root.join("deltas").join("tafs").join("v1__v2.json.xz")
        );
        let delta: LiveFeedRecordDelta = serde_json::from_value(read_json_value(&delta_path)?)?;
        assert_eq!(delta.product, "tafs");
        assert_eq!(
            delta.changed.keys().cloned().collect::<Vec<_>>(),
            vec!["KBFI", "KSEA"]
        );
        assert_eq!(delta.removed, vec!["KPAE"]);

        let version_manifest: LiveFeedVersionManifest =
            serde_json::from_slice(&fs::read(root.join("versions/tafs/v2.json"))?)?;
        assert_eq!(
            version_manifest
                .delta_from_previous
                .as_ref()
                .map(|delta| delta.url.as_str()),
            Some("deltas/tafs/v1__v2.json.xz")
        );
        assert_eq!(
            version_manifest
                .delta_from_previous
                .as_ref()
                .and_then(|delta| delta.kind.as_deref()),
            Some("record_json_delta_xz")
        );
        Ok(())
    }

    #[test]
    fn file_publisher_prunes_old_versions_after_publish() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new_with_retention_policy(
            root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2035, 1, 1, 0, 0, 0).unwrap()),
            LiveFeedRetentionPolicy::new(StdDuration::ZERO),
        );

        publisher.publish(json_state(
            temp.path(),
            "metars",
            "metars-v1",
            "v1",
            "records",
            &[("KSEA", 1)],
        )?)?;
        publisher.publish(json_state(
            temp.path(),
            "metars",
            "metars-v2",
            "v2",
            "records",
            &[("KSEA", 2)],
        )?)?;
        publisher.publish(json_state(
            temp.path(),
            "metars",
            "metars-v3",
            "v3",
            "records",
            &[("KSEA", 3)],
        )?)?;

        assert!(!root.join("versions/metars/v1.json").exists());
        assert!(!root.join("states/metars/v1.json.xz").exists());
        assert!(root.join("versions/metars/v2.json").is_file());
        assert!(root.join("states/metars/v2.json.xz").is_file());
        assert!(root.join("deltas/metars/v2__v3.json.xz").is_file());
        assert!(root.join("versions/metars/v3.json").is_file());
        assert_eq!(
            read_live_feeds_current(&root)?.expect("current").products["metars"].current,
            "v3"
        );
        Ok(())
    }

    #[test]
    fn current_manifest_and_invalidation_history_are_bounded() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new(root.clone(), FixedClock::new(Utc::now()));

        let mut latest_event = None;
        for index in 0..14 {
            let version = format!("v{index:02}");
            let file_stem = format!("metars-{version}");
            let records = [("KSEA", index)];
            let (_, event) = publisher.publish_and_invalidation(json_state(
                temp.path(),
                "metars",
                &file_stem,
                &version,
                "records",
                &records,
            )?)?;
            latest_event = Some(event);
        }

        let current = read_live_feeds_current(&root)?.expect("current");
        let history = &current.products["metars"].history;
        assert_eq!(history.len(), LIVE_FEED_CURRENT_HISTORY_MAX_ENTRIES);
        assert_eq!(
            history.first().map(|entry| entry.version.as_str()),
            Some("v01")
        );
        assert_eq!(
            history.last().map(|entry| entry.version.as_str()),
            Some("v12")
        );
        assert!(!history.iter().any(|entry| entry.version == "v13"));
        assert_eq!(latest_event.expect("latest event").history, *history);
        for entry in history {
            assert!(root.join(&entry.version_manifest_url).is_file());
            assert!(root
                .join(entry.state_url.as_deref().expect("history state url"))
                .is_file());
        }
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
                    previous_source_id: None,
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
        assert!(
            !live_feed_event_scratch_dir(
                &scratch_root,
                "metars",
                &UpstreamEvent {
                    product: "metars".to_string(),
                    source_id: "m1".to_string(),
                    previous_source_id: None,
                    observed_at_utc: Utc.with_ymd_and_hms(2026, 5, 18, 4, 0, 0).unwrap(),
                    payload_path: None,
                }
            )
            .exists(),
            "successful live-feed events should not leave scratch behind"
        );
        Ok(())
    }

    #[test]
    fn upstream_publish_tick_keeps_only_recent_failed_scratch_dirs() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let live_root = temp.path().join("live-feeds");
        let scratch_root = temp.path().join("scratch");
        let publisher = FileLiveFeedPublisher::new(
            live_root,
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 4, 5, 6).unwrap()),
        );
        let broker = RecordingBroker::default();
        let events = (0..7)
            .map(|index| UpstreamEvent {
                product: "metars".to_string(),
                source_id: format!("m{index}"),
                previous_source_id: None,
                observed_at_utc: Utc.with_ymd_and_hms(2026, 5, 18, 4, index, 0).unwrap(),
                payload_path: None,
            })
            .collect();
        let mut tasks = vec![LiveFeedSourceAndBuilder::new(
            StaticSource {
                product: "metars".to_string(),
                events,
            },
            FailingScratchBuilder {
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

        assert_eq!(result.published.len(), 0);
        assert_eq!(
            result
                .failures
                .iter()
                .filter(|failure| failure.phase == LiveFeedTaskPhase::Build)
                .count(),
            7
        );
        let remaining =
            fs::read_dir(scratch_root.join("metars"))?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(remaining.len(), LIVE_FEED_FAILED_SCRATCH_RETAIN_COUNT);
        Ok(())
    }

    #[test]
    fn startup_scratch_prune_bounds_existing_product_dirs() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let scratch_root = temp.path().join("scratch");
        for product in ["metars", "nexrad"] {
            for index in 0..7 {
                let dir = scratch_root.join(product).join(format!("attempt-{index}"));
                fs::create_dir_all(&dir)?;
                fs::write(dir.join("marker"), b"debug")?;
            }
        }

        prune_live_feed_scratch_root(&scratch_root, 5)?;

        for product in ["metars", "nexrad"] {
            let remaining =
                fs::read_dir(scratch_root.join(product))?.collect::<Result<Vec<_>, _>>()?;
            assert_eq!(remaining.len(), 5);
        }
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
    fn repeated_version_with_new_collection_time_updates_current_metadata() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let live_root = temp.path().join("live-feeds");
        let publisher = FileLiveFeedPublisher::new(
            live_root.clone(),
            FixedClock::new(Utc.with_ymd_and_hms(2026, 5, 18, 4, 5, 6).unwrap()),
        );
        let broker = RecordingBroker::default();
        let first_collected = Utc.with_ymd_and_hms(2026, 5, 18, 4, 0, 0).unwrap();
        let second_collected = Utc.with_ymd_and_hms(2026, 5, 18, 4, 5, 0).unwrap();
        let mut first_state = json_state(
            temp.path(),
            "metars",
            "metars-v1",
            "v1",
            "records",
            &[("A", 1)],
        )?;
        first_state.status_timestamps.collected_at_utc = Some(first_collected);
        let mut products = vec![StaticProductTask::state("metars", first_state)];

        let first = run_live_feed_publish_tick(&mut products, &publisher, &broker);
        assert!(first.failures.is_empty());
        assert_eq!(first.published.len(), 1);
        assert!(!first.published[0].unchanged);
        assert_eq!(broker.events().len(), 1);

        let mut second_state = json_state(
            temp.path(),
            "metars",
            "metars-v1-again",
            "v1",
            "records",
            &[("A", 1)],
        )?;
        second_state.status_timestamps.collected_at_utc = Some(second_collected);
        let mut products = vec![StaticProductTask::state("metars", second_state)];
        let second = run_live_feed_publish_tick(&mut products, &publisher, &broker);

        assert!(second.failures.is_empty());
        assert_eq!(second.published.len(), 1);
        assert!(!second.published[0].unchanged);
        assert_eq!(
            broker.events().len(),
            2,
            "metadata-only current changes should still reach clients"
        );
        let current = read_live_feeds_current(&live_root)?.expect("current");
        assert_eq!(
            current.products["metars"].collected_at_utc.as_deref(),
            Some("2026-05-18T04:05:00Z")
        );
        Ok(())
    }

    #[test]
    fn publication_is_acknowledged_only_after_successful_announcement() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let built = json_state(
            temp.path(),
            "notams",
            "notams-v1",
            "n1",
            "records",
            &[("A", 1)],
        )?;
        let publisher = RecordingAckPublisher::default();
        let mut failed_task = vec![StaticProductTask::state("notams", built.clone())];
        let failed = run_live_feed_publish_tick(&mut failed_task, &publisher, &FailingBroker);
        assert!(failed.published.is_empty());
        assert_eq!(failed.failures.len(), 1);
        assert_eq!(failed.failures[0].phase, LiveFeedTaskPhase::Announce);
        assert_eq!(*publisher.acknowledgements.lock().unwrap(), 0);
        assert_eq!(*publisher.maintenance_runs.lock().unwrap(), 0);

        let mut successful_task = vec![StaticProductTask::state("notams", built)];
        let successful = run_live_feed_publish_tick(
            &mut successful_task,
            &publisher,
            &RecordingBroker::default(),
        );
        assert_eq!(successful.published.len(), 1);
        assert!(successful.failures.is_empty());
        assert_eq!(*publisher.acknowledgements.lock().unwrap(), 1);
        assert_eq!(*publisher.maintenance_runs.lock().unwrap(), 1);
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
            previous_source_id: None,
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
            Some(StdDuration::from_secs(NEXRAD_POLL_INTERVAL_SECS))
        );
        assert_eq!(
            default_poll_interval("metars"),
            Some(StdDuration::from_secs(5 * 60))
        );
        assert_eq!(
            default_poll_interval("tafs"),
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
    fn default_retention_keeps_nexrad_animation_tail() {
        let policy = LiveFeedRetentionPolicy::default();
        assert_eq!(
            policy.recent_tail_for("nexrad"),
            StdDuration::from_secs(NEXRAD_CURRENT_HISTORY_TAIL_SECS)
        );
        assert_eq!(
            policy.recent_tail_for("metars"),
            StdDuration::from_secs(3 * 60 * 60)
        );
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

    struct FailingBroker;

    impl SseBroker for FailingBroker {
        fn announce(&self, _event: LiveFeedInvalidation) -> anyhow::Result<()> {
            bail!("intentional announcement failure")
        }
    }

    #[derive(Default)]
    struct RecordingAckPublisher {
        acknowledgements: Mutex<usize>,
        maintenance_runs: Mutex<usize>,
    }

    impl LiveFeedPublisher for RecordingAckPublisher {
        fn publish(&self, built: BuiltLiveFeedState) -> anyhow::Result<PublishedLiveFeedUpdate> {
            Ok(PublishedLiveFeedUpdate {
                product: built.product,
                version: built.version,
                unchanged: false,
                state_path: PathBuf::from("states/notams/n1.json.xz"),
                version_manifest_path: PathBuf::from("versions/notams/n1.json"),
                version_manifest_url: "versions/notams/n1.json".to_string(),
                state_url: "states/notams/n1.json.xz".to_string(),
                state_sha256: "n1".to_string(),
                published_at_utc: None,
                collected_at_utc: None,
                history: Vec::new(),
                delta_path: None,
                changed_count: 1,
                removed_count: 0,
                publication_ack: Some(NotamPublicationAck {
                    state_root: PathBuf::from("not-used-by-mock"),
                    journal_seq: 1,
                    expected_from_state_id: None,
                    to_state_id: "n1".to_string(),
                }),
                notam_compaction: None,
            })
        }

        fn acknowledge(&self, _update: &PublishedLiveFeedUpdate) -> anyhow::Result<()> {
            *self.acknowledgements.lock().unwrap() += 1;
            Ok(())
        }

        fn maintain_after_acknowledgement(
            &self,
            _update: &PublishedLiveFeedUpdate,
        ) -> anyhow::Result<()> {
            *self.maintenance_runs.lock().unwrap() += 1;
            Ok(())
        }
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

    struct FailingScratchBuilder {
        product: String,
    }

    impl ProductBuilder for FailingScratchBuilder {
        fn product_id(&self) -> &str {
            &self.product
        }

        fn build_state(
            &self,
            _event: &UpstreamEvent,
            scratch_dir: &Path,
        ) -> anyhow::Result<BuiltLiveFeedState> {
            fs::create_dir_all(scratch_dir)?;
            fs::write(scratch_dir.join("failure-marker"), b"debug")?;
            bail!("intentional test failure")
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
        json_state_with_count_key(
            root,
            product,
            file_stem,
            version,
            records_key,
            "record_count",
            records,
        )
    }

    fn json_state_with_count_key(
        root: &Path,
        product: &str,
        file_stem: &str,
        version: &str,
        records_key: &str,
        count_key: &str,
        records: &[(&str, i64)],
    ) -> anyhow::Result<BuiltLiveFeedState> {
        let value = serde_json::json!({
            "version_label": version,
            count_key: records.len(),
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
            state_sha256: None,
            state_payload_kind: None,
            status_timestamps: Default::default(),
            delta_policy: DeltaPolicy::KeyedRecords {
                records_key: records_key.to_string(),
                count_key: Some(count_key.to_string()),
            },
            precomputed_delta: None,
            changed_count_if_no_delta: records.len(),
        })
    }

    fn metar_delta_fixture_states() -> anyhow::Result<Vec<Value>> {
        let test_artifacts_root = std::env::var_os("AEROBAG_TEST_ARTIFACTS_ROOT")
            .map(PathBuf::from)
            .context("set AEROBAG_TEST_ARTIFACTS_ROOT to run external fixture tests")?;
        let fixture_root = test_artifacts_root.join("metars").join("delta-three-hour");
        if !fixture_root.is_dir() {
            bail!("missing METAR fixture directory {}", fixture_root.display());
        }
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

    struct TestNavKvState {
        root: PathBuf,
        manifest_path: PathBuf,
        manifest_value: Value,
        state_sha256: String,
    }

    fn nav_kv_pair(key: &str, value: &str) -> NavKvPair {
        NavKvPair {
            key: key.to_string(),
            value: value.as_bytes().to_vec(),
        }
    }

    fn write_test_nav_kv_state(
        root: &Path,
        product: &str,
        version: &str,
        pairs: &[NavKvPair],
    ) -> anyhow::Result<TestNavKvState> {
        let state_dir = root.join(format!("{product}-{version}"));
        fs::create_dir_all(&state_dir)?;
        let built =
            had_nav_kv::build_nav_kv_strict(pairs.to_vec(), 1024).map_err(anyhow::Error::msg)?;
        fs::write(state_dir.join("root"), &built.root_bytes)?;
        for (index, page) in built.pages.iter().enumerate() {
            fs::write(state_dir.join(format!("page_{index:04}")), page)?;
        }
        let state_sha256 = nav_kv_canonical_sha256_from_pairs(pairs);
        let manifest_value = serde_json::json!({
            "schema_version": 1,
            "product_id": product,
            "version_label": version,
            "encoding": format!("had-nav-kv-v{}", had_nav_kv::VERSION),
            "root": "root",
            "page_path_template": "page_{page:04}",
            "page_count": built.pages.len(),
            "page_size": built.page_size,
            "state_sha256": state_sha256
        });
        let manifest_path = state_dir.join("manifest.json");
        write_json_pretty_file(&manifest_path, &manifest_value)?;
        Ok(TestNavKvState {
            root: state_dir,
            manifest_path,
            manifest_value,
            state_sha256,
        })
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
