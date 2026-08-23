// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, HashMap};

use notam_state::{NotamApplyWork, NotamCheckpoint, NotamDelta, NotamState};
use product_contracts::{
    live_feeds::v3::{self as live_feeds_v3, CATALOG_EVENT_NAME, PRODUCT_EVENT_NAME},
    versioned_json, LIVE_FEED_FAILED_RESOURCE_RETRY_DELAY_MS,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    map_overlay::{
        notam_display_checkpoint, notam_display_delta, MetarProductPayload, NotamDisplayCheckpoint,
        NotamDisplayDelta, PirepProductPayload, TafProductPayload, TfrProductPayload,
    },
    AppError, AppErrorKind, AppResult, CoreResourceRequest, HadOperationOutcome, UiInvalidation,
};

pub use product_contracts::LIVE_FEEDS_SCHEMA_VERSION;
pub const LIVE_FEEDS_BASE_PATH: &str = "/live-feeds/v3";
pub const LIVE_FEEDS_EVENTS_PATH: &str = "/live-feeds/v3/events";
const LIVE_FEEDS_PREFIX: &str = "/live-feeds/v3/";
const LIVE_FEEDS_STATUS_PATH: &str = "/live-feeds/status.html";
pub const LIVE_FEED_HISTORY_MAX_ENTRIES: usize = 12;
pub const NEXRAD_FRAME_WINDOW_SIZE: usize = product_contracts::NEXRAD_CLIENT_HISTORY_ENTRIES + 1;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LiveFeedsState {
    source_root_url: Option<String>,
    products: HashMap<String, LiveFeedProductState>,
    catalog_loaded: bool,
    resource_failure_retry_after_epoch_ms: HashMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct LiveFeedProductState {
    current_version: Option<String>,
    loaded_version: Option<String>,
    version_manifest_url: Option<String>,
    state_url: Option<String>,
    expected_state_sha256: Option<String>,
    published_at_utc: Option<String>,
    collected_at_utc: Option<String>,
    state_kind: Option<String>,
    state_ref: Option<LiveFeedPayloadRef>,
    install_state_ref: Option<LiveFeedPayloadRef>,
    install_profile_refs: BTreeMap<String, LiveFeedPayloadRef>,
    delta_from_previous: Option<LiveFeedDeltaRef>,
    recent_deltas: Vec<LiveFeedDeltaRef>,
    version_manifest: Option<Value>,
    state_manifest: Option<Value>,
    catalog_state_manifest: Option<Value>,
    history: Vec<LiveFeedProductHistoryState>,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct LiveFeedProductHistoryState {
    version: String,
    version_manifest_url: Option<String>,
    state_url: Option<String>,
    expected_state_sha256: Option<String>,
    published_at_utc: Option<String>,
    collected_at_utc: Option<String>,
    state_kind: Option<String>,
    state_ref: Option<LiveFeedPayloadRef>,
    install_state_ref: Option<LiveFeedPayloadRef>,
    install_profile_refs: BTreeMap<String, LiveFeedPayloadRef>,
    version_manifest: Option<Value>,
    state_manifest: Option<Value>,
}

pub struct LiveFeedLoadedStateManifest<'a> {
    pub version: &'a str,
    pub manifest: &'a Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveFeedSseEvent {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveFeedsSnapshot {
    pub products: Vec<LiveFeedProductSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveFeedProductSnapshot {
    pub product: String,
    pub current_version: Option<String>,
    pub version_manifest_loaded: bool,
    pub state_manifest_loaded: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct CurrentProductHistoryEntry {
    version: String,
    version_manifest_url: String,
    state_url: Option<String>,
    state_sha256: Option<String>,
    published_at_utc: Option<String>,
    collected_at_utc: Option<String>,
}

#[derive(Clone)]
struct ProductRegistration {
    product: String,
    version: String,
    version_manifest_url: String,
    state_url: String,
    state_sha256: String,
    published_at_utc: Option<String>,
    collected_at_utc: Option<String>,
    history: Vec<CurrentProductHistoryEntry>,
}

pub type LiveFeedPayloadRef = live_feeds_v3::PayloadRef;
pub type LiveFeedDeltaRef = live_feeds_v3::DeltaRef;
type VersionManifest = live_feeds_v3::VersionManifest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedDurableInstalledProduct {
    pub product: String,
    pub version: String,
    pub state_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedCacheRequest {
    pub id: String,
    pub url: String,
    pub kind: LiveFeedCacheRequestKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveFeedCacheRequestKind {
    Version {
        product: String,
        version: String,
    },
    Full {
        product: String,
        version: String,
        payload_kind: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        install_profile: Option<String>,
    },
    State {
        product: String,
        version: String,
    },
    Delta {
        product: String,
        from_version: String,
        to_version: String,
        payload_kind: Option<String>,
    },
}

type LiveFeedRecordDelta = live_feeds_v3::RecordDelta;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedLiveFeedEnvelope {
    pub schema_version: u32,
    pub resource_id: String,
    pub product: String,
    pub version: String,
    pub state_sha256: String,
    #[serde(default)]
    pub from_version: Option<String>,
    #[serde(default)]
    pub from_state_sha256: Option<String>,
    #[serde(default)]
    pub delta_blob_sha256: Option<String>,
    pub payload: PreparedLiveFeedPayload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PreparedLiveFeedPayload {
    Metars(PreparedMetarLiveFeed),
    Tafs(TafProductPayload),
    Tfrs(TfrProductPayload),
    Notams(PreparedNotamPayload),
    Pireps(PreparedPirepLiveFeed),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreparedNotamPayload {
    InstallDisplayCheckpoint(NotamDisplayCheckpoint),
    ApplyDisplayDelta(NotamDisplayDelta),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BackgroundNotamWork {
    pub compressed_bytes_read: u64,
    pub json_bytes_decoded: u64,
    pub records_decoded: u64,
    pub postcard_bytes_written: u64,
}

#[derive(Debug, Default)]
pub struct NotamProjectionPreparer {
    state: Option<NotamState>,
}

impl NotamProjectionPreparer {
    pub fn install_checkpoint(
        &mut self,
        checkpoint: NotamCheckpoint,
        work: &mut NotamApplyWork,
    ) -> Result<PreparedNotamPayload, notam_state::NotamStateError> {
        let state = NotamState::from_checkpoint(checkpoint, work)?;
        let projection = notam_display_checkpoint(&state);
        self.state = Some(state);
        Ok(PreparedNotamPayload::InstallDisplayCheckpoint(projection))
    }

    pub fn apply_delta(
        &mut self,
        delta: NotamDelta,
        work: &mut NotamApplyWork,
    ) -> Result<PreparedNotamPayload, notam_state::NotamStateError> {
        let mut state = self.state.take().ok_or_else(|| {
            notam_state::NotamStateError::Invariant(
                "cannot prepare NOTAM delta without canonical worker state".to_string(),
            )
        })?;
        let projection = notam_display_delta(&state, &delta)?;
        state.apply_delta(delta, work)?;
        self.state = Some(state);
        Ok(PreparedNotamPayload::ApplyDisplayDelta(projection))
    }

    pub fn state_id(&self) -> Option<&str> {
        self.state.as_ref().map(NotamState::state_id)
    }

    pub fn projection_checkpoint(&self) -> Option<NotamDisplayCheckpoint> {
        self.state.as_ref().map(notam_display_checkpoint)
    }

    pub fn canonical_checkpoint(&self) -> Option<NotamCheckpoint> {
        self.state.as_ref().map(NotamState::checkpoint)
    }

    pub fn reset(&mut self) {
        self.state = None;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedMetarLiveFeed {
    pub schema_version: u32,
    pub payload: MetarProductPayload,
    pub tiles: Vec<PreparedMetarTile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedMetarTile {
    pub z: u32,
    pub x: u32,
    pub y: u32,
    pub station_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedPirepLiveFeed {
    pub schema_version: u32,
    pub payload: PirepProductPayload,
    pub tiles: Vec<PreparedPirepTile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedPirepTile {
    pub z: u32,
    pub x: u32,
    pub y: u32,
    pub pirep_ids: Vec<String>,
}

impl PreparedLiveFeedPayload {
    fn product(&self) -> &'static str {
        match self {
            Self::Metars(_) => "metars",
            Self::Tafs(_) => "tafs",
            Self::Tfrs(_) => "tfrs",
            Self::Notams(_) => "notams",
            Self::Pireps(_) => "pireps",
        }
    }

    fn version_label(&self) -> &str {
        match self {
            Self::Metars(feed) => &feed.payload.version_label,
            Self::Tafs(payload) => &payload.version_label,
            Self::Tfrs(payload) => &payload.version_label,
            Self::Notams(PreparedNotamPayload::InstallDisplayCheckpoint(checkpoint)) => {
                &checkpoint.state_id
            }
            Self::Notams(PreparedNotamPayload::ApplyDisplayDelta(delta)) => &delta.to_state_id,
            Self::Pireps(feed) => &feed.payload.version_label,
        }
    }
}

impl LiveFeedsState {
    pub fn set_source_root_url(&mut self, source_root_url: &str) -> AppResult<String> {
        let normalized = normalize_live_feed_source_root_url(source_root_url)?;
        self.source_root_url = Some(normalized.clone());
        Ok(normalized)
    }

    pub fn source_root_url(&self) -> Option<&str> {
        self.source_root_url.as_deref()
    }

    pub fn sync_outcome(&self) -> HadOperationOutcome {
        let resources = self.missing_resources();
        self.outcome_for_resources(resources)
    }

    pub fn sync_outcome_at_epoch_ms(&self, epoch_ms: i64) -> HadOperationOutcome {
        let resources = self.retryable_resources(self.missing_resources(), epoch_ms);
        self.outcome_for_resources(resources)
    }

    pub fn complete_outcome_with_invalidations(&self) -> HadOperationOutcome {
        self.outcome_for_resources_with_invalidations(Vec::new())
    }

    pub fn sync_product_outcome_at_epoch_ms(
        &self,
        product: &str,
        epoch_ms: i64,
    ) -> HadOperationOutcome {
        let resources = if self.catalog_loaded {
            self.missing_resources_for_products(std::iter::once(product))
        } else {
            self.missing_resources()
        };
        self.outcome_for_resources(self.retryable_resources(resources, epoch_ms))
    }

    fn outcome_for_resources(&self, resources: Vec<CoreResourceRequest>) -> HadOperationOutcome {
        if resources.is_empty() {
            HadOperationOutcome::complete(
                serde_json::to_value(self.snapshot()).expect("live feed snapshot serializes"),
            )
        } else {
            HadOperationOutcome::NeedResources { resources }
        }
    }

    pub fn sync_product_outcome(&self, product: &str) -> HadOperationOutcome {
        let resources = if self.catalog_loaded {
            self.missing_resources_for_products(std::iter::once(product))
        } else {
            self.missing_resources()
        };
        if resources.is_empty() {
            HadOperationOutcome::complete(
                serde_json::to_value(self.snapshot()).expect("live feed snapshot serializes"),
            )
        } else {
            HadOperationOutcome::NeedResources { resources }
        }
    }

    pub fn sync_outcome_with_invalidations(&self) -> HadOperationOutcome {
        let resources = self.missing_resources();
        self.outcome_for_resources_with_invalidations(resources)
    }

    pub fn sync_outcome_with_invalidations_at_epoch_ms(
        &self,
        epoch_ms: i64,
    ) -> HadOperationOutcome {
        let resources = self.retryable_resources(self.missing_resources(), epoch_ms);
        self.outcome_for_resources_with_invalidations(resources)
    }

    fn outcome_for_resources_with_invalidations(
        &self,
        resources: Vec<CoreResourceRequest>,
    ) -> HadOperationOutcome {
        if resources.is_empty() {
            HadOperationOutcome::complete_with_invalidations(
                serde_json::to_value(self.snapshot()).expect("live feed snapshot serializes"),
                self.invalidations(),
            )
        } else {
            HadOperationOutcome::NeedResources { resources }
        }
    }

    pub fn sync_products_outcome_with_invalidations<'a>(
        &self,
        products: impl IntoIterator<Item = &'a str>,
    ) -> HadOperationOutcome {
        let resources = if self.catalog_loaded {
            self.missing_resources_for_products(products)
        } else {
            self.missing_resources()
        };
        if resources.is_empty() {
            HadOperationOutcome::complete_with_invalidations(
                serde_json::to_value(self.snapshot()).expect("live feed snapshot serializes"),
                self.invalidations(),
            )
        } else {
            HadOperationOutcome::NeedResources { resources }
        }
    }

    pub fn ingest_sse_event(&mut self, event: LiveFeedSseEvent) -> AppResult<HadOperationOutcome> {
        let affected = self.ingest_sse_events(std::iter::once(event))?;
        Ok(self.sync_products_outcome_with_invalidations(affected.iter().map(String::as_str)))
    }

    pub fn ingest_sse_events(
        &mut self,
        events: impl IntoIterator<Item = LiveFeedSseEvent>,
    ) -> AppResult<Vec<String>> {
        let mut catalog = None;
        let mut latest_products = HashMap::new();
        for event in events {
            match parse_live_feed_sse_event(event)? {
                Some(ParsedLiveFeedSseEvent::Catalog(catalog_event)) => {
                    latest_products.clear();
                    catalog = Some(catalog_event);
                }
                Some(ParsedLiveFeedSseEvent::Product(payload)) => {
                    latest_products.insert(payload.product.clone(), payload);
                }
                None => {}
            }
        }

        let catalog_registrations = catalog.map(validated_catalog_registrations).transpose()?;
        let mut product_registrations = latest_products
            .into_values()
            .map(product_registration_from_event)
            .map(validate_product_registration)
            .collect::<AppResult<Vec<_>>>()?;
        product_registrations.sort_by(|left, right| left.product.cmp(&right.product));

        let mut affected = Vec::new();
        if let Some(registrations) = catalog_registrations {
            affected.extend(self.install_validated_catalog(registrations));
        }
        for registration in product_registrations {
            affected.push(registration.product.clone());
            self.register_validated_product(registration);
        }
        affected.sort();
        affected.dedup();
        Ok(affected)
    }

    pub(crate) fn ingest_catalog_bytes(&mut self, bytes: &[u8]) -> AppResult<Vec<String>> {
        let catalog = versioned_json::decode_exact::<live_feeds_v3::Catalog>(
            "live-feed catalog",
            bytes,
            LIVE_FEEDS_SCHEMA_VERSION,
        )
        .map_err(|error| invalid_live_feed(error.to_string()))?;
        self.install_catalog(catalog)
    }

    fn install_catalog(&mut self, catalog: live_feeds_v3::Catalog) -> AppResult<Vec<String>> {
        Ok(self.install_validated_catalog(validated_catalog_registrations(catalog)?))
    }

    fn install_validated_catalog(
        &mut self,
        registrations: Vec<ProductRegistration>,
    ) -> Vec<String> {
        self.catalog_loaded = true;
        let mut affected = self.products.keys().cloned().collect::<Vec<_>>();
        let products = registrations
            .iter()
            .map(|registration| registration.product.as_str())
            .collect::<std::collections::HashSet<_>>();
        self.products
            .retain(|product, _| products.contains(product.as_str()));
        for registration in registrations {
            affected.push(registration.product.clone());
            self.register_validated_product(registration);
        }
        affected.sort();
        affected.dedup();
        affected
    }

    pub fn ingest_resource(&mut self, resource_id: &str, bytes: &[u8]) -> AppResult<()> {
        if let Some(rest) = resource_id.strip_prefix("live_feeds/version/") {
            let (product, version) = split_product_version(resource_id, rest)?;
            let manifest = versioned_json::decode_exact::<live_feeds_v3::VersionManifest>(
                "live-feed version manifest",
                bytes,
                LIVE_FEEDS_SCHEMA_VERSION,
            )
            .map_err(|error| invalid_live_feed(error.to_string()))?;
            let manifest_json = serde_json::to_value(&manifest).map_err(invalid_live_feed_json)?;
            if manifest.product != product || manifest.version != version {
                return Err(invalid_live_feed(format!(
                    "version resource {resource_id} contained {}:{}",
                    manifest.product, manifest.version
                )));
            }
            if manifest.temporal_coverage.as_ref().is_some_and(|coverage| {
                coverage.valid_from_epoch_ms > coverage.valid_through_epoch_ms
            }) {
                return Err(invalid_live_feed(format!(
                    "version resource {resource_id} temporal coverage ends before it begins"
                )));
            }
            if manifest.state.kind.is_none() {
                return Err(invalid_live_feed(format!(
                    "version resource {resource_id} state missing kind"
                )));
            }
            validate_relative_url(&manifest.state.url)?;
            if let Some(install_state) = &manifest.install_state {
                if install_state.kind.is_none() {
                    return Err(invalid_live_feed(format!(
                        "version resource {resource_id} install state missing kind"
                    )));
                }
                validate_relative_url(&install_state.url)?;
            }
            for (profile, install_state) in &manifest.install_profiles {
                if install_state.kind.is_none() {
                    return Err(invalid_live_feed(format!(
                        "version resource {resource_id} install profile {profile} missing kind"
                    )));
                }
                validate_relative_url(&install_state.url)?;
            }
            if let Some(delta) = &manifest.delta_from_previous {
                validate_relative_url(&delta.url)?;
                if delta.kind.is_none() {
                    return Err(invalid_live_feed(format!(
                        "version resource {resource_id} delta missing kind"
                    )));
                }
            }
            for delta in &manifest.recent_deltas {
                validate_relative_url(&delta.url)?;
                if delta.kind.is_none() {
                    return Err(invalid_live_feed(format!(
                        "version resource {resource_id} retained delta missing kind"
                    )));
                }
            }
            if product == "notams" {
                let expected_head = self
                    .products
                    .get(&product)
                    .and_then(|entry| entry.current_version.as_deref())
                    .ok_or_else(|| {
                        invalid_live_feed(
                            "NOTAM version manifest arrived before current manifest".to_string(),
                        )
                    })?;
                validate_notam_version_manifest(&manifest, expected_head)?;
            }
            let entry = self.products.entry(product.clone()).or_default();
            if entry.current_version.as_deref() != Some(version.as_str()) {
                let Some(history) = entry
                    .history
                    .iter_mut()
                    .find(|entry| entry.version == version)
                else {
                    self.resource_failure_retry_after_epoch_ms
                        .remove(resource_id);
                    return Ok(());
                };
                history.state_url = Some(manifest.state.url.clone());
                history.expected_state_sha256 = Some(manifest.state.state_sha256.clone());
                history.state_kind = manifest.state.kind.clone();
                history.state_ref = Some(manifest.state);
                history.install_state_ref = manifest.install_state;
                history.install_profile_refs = manifest.install_profiles;
                history.version_manifest = Some(manifest_json);
                self.resource_failure_retry_after_epoch_ms
                    .remove(resource_id);
                return Ok(());
            }
            entry.state_url = Some(manifest.state.url.clone());
            if product != "notams" {
                entry.expected_state_sha256 = Some(manifest.state.state_sha256.clone());
            }
            entry.state_kind = manifest.state.kind.clone();
            entry.state_ref = Some(manifest.state);
            entry.install_state_ref = manifest.install_state;
            entry.install_profile_refs = manifest.install_profiles;
            entry.delta_from_previous = manifest.delta_from_previous;
            entry.recent_deltas = manifest.recent_deltas;
            entry.version_manifest = Some(manifest_json);
            self.resource_failure_retry_after_epoch_ms
                .remove(resource_id);
            return Ok(());
        }
        if let Some(rest) = resource_id.strip_prefix("live_feeds/state/") {
            let (product, version) = split_product_version(resource_id, rest)?;
            let entry = self.products.entry(product).or_default();
            if entry.current_version.as_deref() != Some(version.as_str()) {
                let Some(history) = entry
                    .history
                    .iter_mut()
                    .find(|entry| entry.version == version)
                else {
                    self.resource_failure_retry_after_epoch_ms
                        .remove(resource_id);
                    return Ok(());
                };
                let decoded_bytes = decode_live_feed_payload(history.state_kind.as_deref(), bytes)?;
                let parsed: Value = serde_json::from_slice(decoded_bytes.as_ref())
                    .map_err(invalid_live_feed_json)?;
                if let Some(expected) = &history.expected_state_sha256 {
                    let actual = canonical_json_sha256(&parsed)?;
                    if &actual != expected {
                        return Err(invalid_live_feed(format!(
                            "state hash mismatch for {resource_id}: expected {expected}, got {actual}"
                        )));
                    }
                }
                history.state_manifest = Some(parsed);
                self.resource_failure_retry_after_epoch_ms
                    .remove(resource_id);
                return Ok(());
            }
            let decoded_bytes = decode_live_feed_payload(entry.state_kind.as_deref(), bytes)?;
            let parsed: Value =
                serde_json::from_slice(decoded_bytes.as_ref()).map_err(invalid_live_feed_json)?;
            if let Some(expected) = &entry.expected_state_sha256 {
                let actual = if entry.state_kind.as_deref() == Some("nav_kv") {
                    parsed
                        .get("state_sha256")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            invalid_live_feed(format!(
                                "nav_kv state {resource_id} missing state_sha256"
                            ))
                        })?
                        .to_string()
                } else {
                    canonical_json_sha256(&parsed)?
                };
                if &actual != expected {
                    return Err(invalid_live_feed(format!(
                        "state hash mismatch for {resource_id}: expected {expected}, got {actual}"
                    )));
                }
            }
            entry.catalog_state_manifest = Some(parsed.clone());
            entry.state_manifest = Some(parsed);
            entry.loaded_version = Some(version);
            self.resource_failure_retry_after_epoch_ms
                .remove(resource_id);
            return Ok(());
        }
        if let Some(rest) = resource_id.strip_prefix("live_feeds/delta/") {
            let (product, from_version, to_version) = split_product_from_to(resource_id, rest)?;
            if !supports_record_delta(&product) {
                return Err(invalid_live_feed(format!(
                    "unsupported live feed delta product: {product}"
                )));
            }
            let entry = self.products.entry(product.clone()).or_default();
            if entry.current_version.as_deref() != Some(to_version.as_str()) {
                self.resource_failure_retry_after_epoch_ms
                    .remove(resource_id);
                return Ok(());
            }
            let delta_ref = entry.delta_from_previous.clone().ok_or_else(|| {
                invalid_live_feed(format!("delta resource {resource_id} was not expected"))
            })?;
            if delta_ref.from_version != from_version || delta_ref.to_version != to_version {
                return Err(invalid_live_feed(format!(
                    "delta resource {resource_id} does not match version manifest"
                )));
            }
            let actual_blob_sha256 = sha256_hex(bytes);
            if actual_blob_sha256 != delta_ref.blob_sha256 {
                return Err(invalid_live_feed(format!(
                    "delta blob hash mismatch for {resource_id}: expected {}, got {actual_blob_sha256}",
                    delta_ref.blob_sha256
                )));
            }
            let current_state = entry.state_manifest.as_ref().ok_or_else(|| {
                invalid_live_feed(format!(
                    "cannot apply {resource_id}: local {product} state is missing"
                ))
            })?;
            if entry.loaded_version.as_deref() != Some(from_version.as_str()) {
                return Err(invalid_live_feed(format!(
                    "cannot apply {resource_id}: local version is {:?}",
                    entry.loaded_version
                )));
            }
            let current_state_sha256 = canonical_json_sha256(current_state)?;
            if current_state_sha256 != delta_ref.from_state_sha256 {
                return Err(invalid_live_feed(format!(
                    "local state hash mismatch for {from_version}: expected {}, got {}",
                    delta_ref.from_state_sha256, current_state_sha256
                )));
            }
            let decoded_bytes = decode_live_feed_payload(delta_ref.kind.as_deref(), bytes)?;
            let delta = versioned_json::decode_exact::<LiveFeedRecordDelta>(
                "live-feed record delta",
                decoded_bytes.as_ref(),
                LIVE_FEEDS_SCHEMA_VERSION,
            )
            .map_err(|error| invalid_live_feed(error.to_string()))?;
            let next_state = apply_live_feed_record_delta(current_state, &delta)?;
            let next_state_sha256 = canonical_json_sha256(&next_state)?;
            if next_state_sha256 != delta_ref.to_state_sha256 {
                return Err(invalid_live_feed(format!(
                    "delta target hash mismatch for {to_version}: expected {}, got {}",
                    delta_ref.to_state_sha256, next_state_sha256
                )));
            }
            entry.state_manifest = Some(next_state);
            entry.loaded_version = Some(to_version);
            self.resource_failure_retry_after_epoch_ms
                .remove(resource_id);
            return Ok(());
        }
        Err(invalid_live_feed(format!(
            "unsupported live feed resource id: {resource_id}"
        )))
    }

    pub fn record_resource_failure(&mut self, resource_id: &str, epoch_ms: i64) {
        if Self::handles_resource(resource_id) {
            self.resource_failure_retry_after_epoch_ms.insert(
                resource_id.to_string(),
                epoch_ms + LIVE_FEED_FAILED_RESOURCE_RETRY_DELAY_MS,
            );
        }
    }

    pub fn next_resource_retry_delay_ms(&self, now_ms: i64) -> Option<i64> {
        self.resource_failure_retry_after_epoch_ms
            .values()
            .copied()
            .min()
            .map(|retry_after_ms| retry_after_ms.saturating_sub(now_ms).max(0))
    }

    pub fn ingest_prepared_live_feed(
        &mut self,
        resource_id: &str,
        envelope: &PreparedLiveFeedEnvelope,
    ) -> AppResult<()> {
        if envelope.schema_version != 1 {
            return Err(invalid_live_feed(format!(
                "unsupported prepared live-feed schema {}",
                envelope.schema_version
            )));
        }
        if envelope.resource_id != resource_id {
            return Err(invalid_live_feed(format!(
                "prepared live-feed envelope for {} cannot satisfy {resource_id}",
                envelope.resource_id
            )));
        }
        if envelope.payload.product() != envelope.product {
            return Err(invalid_live_feed(format!(
                "prepared {} envelope contained {} payload",
                envelope.product,
                envelope.payload.product()
            )));
        }
        if let Some(rest) = resource_id.strip_prefix("live_feeds/state/") {
            let (product, version) = split_product_version(resource_id, rest)?;
            if product != envelope.product {
                return Err(invalid_live_feed(format!(
                    "prepared {} full resource used for {product}",
                    envelope.product
                )));
            }
            let entry = self.products.entry(product.clone()).or_default();
            if product == "notams" {
                let state_ref = entry.state_ref.as_ref().ok_or_else(|| {
                    invalid_live_feed("NOTAM version manifest has no checkpoint".to_string())
                })?;
                if state_ref.state_sha256 != version {
                    return Err(invalid_live_feed(format!(
                        "prepared NOTAM checkpoint {version} does not match manifest {}",
                        state_ref.state_sha256
                    )));
                }
                if envelope.version != version
                    || envelope.payload.version_label() != version
                    || envelope.state_sha256 != version
                {
                    return Err(invalid_live_feed(format!(
                        "prepared NOTAM checkpoint {resource_id} contained {} / {} / {}",
                        envelope.version,
                        envelope.payload.version_label(),
                        envelope.state_sha256
                    )));
                }
                if envelope.delta_blob_sha256.as_deref() != Some(state_ref.blob_sha256.as_str()) {
                    return Err(invalid_live_feed(format!(
                        "prepared NOTAM checkpoint {resource_id} blob hash mismatch: expected {}, got {:?}",
                        state_ref.blob_sha256, envelope.delta_blob_sha256
                    )));
                }
                entry.state_manifest = None;
                entry.loaded_version = Some(version);
                return Ok(());
            }
            if entry.current_version.as_deref() != Some(version.as_str()) {
                return Ok(());
            }
            if envelope.version != version || envelope.payload.version_label() != version {
                return Err(invalid_live_feed(format!(
                    "prepared {product} full resource {resource_id} contained {} / {}",
                    envelope.version,
                    envelope.payload.version_label()
                )));
            }
            if let Some(expected) = &entry.expected_state_sha256 {
                if &envelope.state_sha256 != expected {
                    return Err(invalid_live_feed(format!(
                        "prepared {product} state hash mismatch for {resource_id}: expected {expected}, got {}",
                        envelope.state_sha256
                    )));
                }
            }
            entry.state_manifest = None;
            entry.loaded_version = Some(version);
            return Ok(());
        }
        if let Some(rest) = resource_id.strip_prefix("live_feeds/delta/") {
            let (product, from_version, to_version) = split_product_from_to(resource_id, rest)?;
            if product != envelope.product || !supports_live_feed_delta(&product) {
                return Err(invalid_live_feed(format!(
                    "prepared {} delta resource used for {product}",
                    envelope.product
                )));
            }
            let entry = self.products.entry(product.clone()).or_default();
            if product != "notams" && entry.current_version.as_deref() != Some(to_version.as_str())
            {
                return Ok(());
            }
            if entry.loaded_version.as_deref() != Some(from_version.as_str()) {
                return Err(invalid_live_feed(format!(
                    "cannot install prepared {resource_id}: local version is {:?}",
                    entry.loaded_version
                )));
            }
            let delta_ref = if product == "notams" {
                entry.notam_delta(&from_version, &to_version)
            } else {
                entry.delta_from_previous.as_ref()
            }
            .ok_or_else(|| {
                invalid_live_feed(format!("prepared delta {resource_id} was not expected"))
            })?;
            if delta_ref.from_version != from_version || delta_ref.to_version != to_version {
                return Err(invalid_live_feed(format!(
                    "prepared delta {resource_id} does not match version manifest"
                )));
            }
            if envelope.from_version.as_deref() != Some(from_version.as_str())
                || envelope.version != to_version
                || envelope.payload.version_label() != to_version
            {
                return Err(invalid_live_feed(format!(
                    "prepared {product} delta {resource_id} contained {:?} -> {} / {}",
                    envelope.from_version,
                    envelope.version,
                    envelope.payload.version_label()
                )));
            }
            if envelope.from_state_sha256.as_deref() != Some(delta_ref.from_state_sha256.as_str()) {
                return Err(invalid_live_feed(format!(
                    "prepared delta {resource_id} source hash mismatch: expected {}, got {:?}",
                    delta_ref.from_state_sha256, envelope.from_state_sha256
                )));
            }
            if envelope.state_sha256 != delta_ref.to_state_sha256 {
                return Err(invalid_live_feed(format!(
                    "prepared delta {resource_id} target hash mismatch: expected {}, got {}",
                    delta_ref.to_state_sha256, envelope.state_sha256
                )));
            }
            if envelope.delta_blob_sha256.as_deref() != Some(delta_ref.blob_sha256.as_str()) {
                return Err(invalid_live_feed(format!(
                    "prepared delta {resource_id} blob hash mismatch: expected {}, got {:?}",
                    delta_ref.blob_sha256, envelope.delta_blob_sha256
                )));
            }
            entry.state_manifest = None;
            entry.loaded_version = Some(to_version);
            return Ok(());
        }
        Err(invalid_live_feed(format!(
            "unsupported prepared live-feed resource id: {resource_id}"
        )))
    }

    pub fn handles_resource(resource_id: &str) -> bool {
        resource_id.starts_with("live_feeds/version/")
            || resource_id.starts_with("live_feeds/state/")
            || resource_id.starts_with("live_feeds/delta/")
    }

    pub fn product_state_manifest(&self, product: &str) -> Option<&Value> {
        let entry = self.products.get(product)?;
        if entry
            .current_version
            .as_deref()
            .is_none_or(|version| entry.loaded_version.as_deref() != Some(version))
        {
            return None;
        }
        entry.state_manifest.as_ref()
    }

    pub fn loaded_product_state_manifest(&self, product: &str) -> Option<&Value> {
        self.products.get(product)?.state_manifest.as_ref()
    }

    pub fn current_product_catalog_state_manifest(&self, product: &str) -> Option<&Value> {
        self.products.get(product)?.catalog_state_manifest.as_ref()
    }

    pub fn current_product_temporal_coverage(
        &self,
        product: &str,
    ) -> Option<live_feeds_v3::TemporalCoverage> {
        let manifest = self.products.get(product)?.version_manifest.as_ref()?;
        serde_json::from_value(manifest.get("temporal_coverage")?.clone()).ok()
    }

    pub fn current_product_install_bytes(&self, product: &str) -> Option<u64> {
        self.products
            .get(product)?
            .install_state_ref
            .as_ref()
            .map(|payload| payload.bytes)
    }

    pub fn product_loaded_version(&self, product: &str) -> Option<&str> {
        let entry = self.products.get(product)?;
        if entry
            .current_version
            .as_deref()
            .is_none_or(|version| entry.loaded_version.as_deref() != Some(version))
        {
            return None;
        }
        entry.loaded_version.as_deref()
    }

    pub(crate) fn product_staged_version(&self, product: &str) -> Option<&str> {
        self.products.get(product)?.loaded_version.as_deref()
    }

    pub fn product_published_at_utc(&self, product: &str) -> Option<&str> {
        let entry = self.products.get(product)?;
        if entry
            .current_version
            .as_deref()
            .is_none_or(|version| entry.loaded_version.as_deref() != Some(version))
        {
            return None;
        }
        entry.published_at_utc.as_deref()
    }

    pub fn product_collected_at_utc(&self, product: &str) -> Option<&str> {
        let entry = self.products.get(product)?;
        if entry
            .current_version
            .as_deref()
            .is_none_or(|version| entry.loaded_version.as_deref() != Some(version))
        {
            return None;
        }
        entry.collected_at_utc.as_deref()
    }

    pub(crate) fn product_collected_at_utc_for_version(
        &self,
        product: &str,
        version: &str,
    ) -> Option<&str> {
        let entry = self.products.get(product)?;
        if entry.current_version.as_deref() == Some(version) {
            return entry.collected_at_utc.as_deref();
        }
        entry
            .history
            .iter()
            .find(|history| history.version == version)
            .and_then(|history| history.collected_at_utc.as_deref())
    }

    pub fn product_state_url(&self, product: &str) -> Option<&str> {
        let entry = self.products.get(product)?;
        if entry
            .current_version
            .as_deref()
            .is_none_or(|version| entry.loaded_version.as_deref() != Some(version))
        {
            return None;
        }
        entry.state_url.as_deref()
    }

    pub fn has_product_current_version(&self, product: &str) -> bool {
        self.products
            .get(product)
            .and_then(|entry| entry.current_version.as_ref())
            .is_some()
    }

    pub fn current_product_version(&self, product: &str) -> Option<&str> {
        self.products.get(product)?.current_version.as_deref()
    }

    pub fn catalog_loaded(&self) -> bool {
        self.catalog_loaded
    }

    pub fn client_retained_versions(&self, product: &str) -> Vec<String> {
        let Some(entry) = self.products.get(product) else {
            return Vec::new();
        };
        let Some(current) = entry.current_version.as_ref() else {
            return Vec::new();
        };
        if product != "nexrad" {
            return vec![current.clone()];
        }
        let retained_history_count = NEXRAD_FRAME_WINDOW_SIZE.saturating_sub(1);
        let retained_history_start = entry.history.len().saturating_sub(retained_history_count);
        entry.history[retained_history_start..]
            .iter()
            .map(|history| history.version.clone())
            .chain(std::iter::once(current.clone()))
            .collect()
    }

    pub fn product_loaded_state_manifests(
        &self,
        product: &str,
    ) -> Vec<LiveFeedLoadedStateManifest<'_>> {
        let Some(entry) = self.products.get(product) else {
            return Vec::new();
        };
        let mut manifests = Vec::new();
        for history in &entry.history {
            if let Some(manifest) = &history.state_manifest {
                manifests.push(LiveFeedLoadedStateManifest {
                    version: &history.version,
                    manifest,
                });
            }
        }
        if entry
            .current_version
            .as_deref()
            .is_some_and(|version| entry.loaded_version.as_deref() == Some(version))
        {
            if let (Some(version), Some(manifest)) = (
                entry.current_version.as_deref(),
                entry.state_manifest.as_ref(),
            ) {
                manifests.push(LiveFeedLoadedStateManifest { version, manifest });
            }
        }
        manifests
    }

    pub fn missing_history_resources_for_product_at_epoch_ms(
        &self,
        product: &str,
        epoch_ms: i64,
    ) -> Vec<CoreResourceRequest> {
        let resources = self.missing_history_resources_for_product(product);
        self.retryable_resources(resources, epoch_ms)
    }

    fn register_validated_product(&mut self, registration: ProductRegistration) {
        let ProductRegistration {
            product,
            version,
            version_manifest_url,
            state_url,
            state_sha256,
            published_at_utc,
            collected_at_utc,
            history,
        } = registration;
        let entry = self.products.entry(product).or_default();
        if entry.current_version.as_deref() == Some(version.as_str()) {
            entry.version_manifest_url = Some(version_manifest_url);
            entry.state_url = Some(state_url);
            entry.expected_state_sha256 = Some(state_sha256);
            entry.published_at_utc = published_at_utc;
            entry.collected_at_utc = collected_at_utc;
            entry.sync_history(history);
            if entry.loaded_version.as_deref() == Some(version.as_str()) {
                entry.loaded_version = Some(version);
            }
            return;
        }
        entry.current_version = Some(version);
        entry.version_manifest_url = Some(version_manifest_url);
        entry.state_url = Some(state_url);
        entry.expected_state_sha256 = Some(state_sha256);
        entry.published_at_utc = published_at_utc;
        entry.collected_at_utc = collected_at_utc;
        entry.state_kind = None;
        entry.state_ref = None;
        entry.install_state_ref = None;
        entry.install_profile_refs.clear();
        entry.delta_from_previous = None;
        entry.recent_deltas.clear();
        entry.version_manifest = None;
        entry.catalog_state_manifest = None;
        entry.sync_history(history);
    }

    pub fn mark_durable_product_loaded(
        &mut self,
        product: String,
        version: String,
        state_sha256: String,
        collected_at_utc: Option<String>,
        state_manifest: Option<Value>,
    ) {
        let entry = self.products.entry(product).or_default();
        if entry.current_version.is_none() {
            entry.current_version = Some(version.clone());
        }
        if self.catalog_loaded && entry.current_version.as_deref() != Some(version.as_str()) {
            return;
        }
        if self.catalog_loaded
            && entry
                .expected_state_sha256
                .as_deref()
                .is_some_and(|expected| expected != state_sha256)
        {
            return;
        }
        entry.loaded_version = Some(version);
        if entry.expected_state_sha256.is_none() {
            entry.expected_state_sha256 = Some(state_sha256);
        }
        if collected_at_utc.is_some() {
            entry.collected_at_utc = collected_at_utc;
        }
        if let Some(state_manifest) = state_manifest {
            entry.catalog_state_manifest = Some(state_manifest.clone());
            entry.state_manifest = Some(state_manifest);
        }
    }

    pub(crate) fn mark_product_no_state(&mut self, product: &str) {
        if let Some(entry) = self.products.get_mut(product) {
            entry.loaded_version = None;
            entry.state_manifest = None;
        }
    }

    pub fn merge_catalog_from(&mut self, source: &Self) {
        if source.catalog_loaded {
            self.catalog_loaded = true;
            self.products
                .retain(|product, _| source.products.contains_key(product));
        }
        for (product, source_entry) in &source.products {
            let entry = self.products.entry(product.clone()).or_default();
            entry.current_version = source_entry.current_version.clone();
            entry.version_manifest_url = source_entry.version_manifest_url.clone();
            entry.state_url = source_entry.state_url.clone();
            entry.expected_state_sha256 = source_entry.expected_state_sha256.clone();
            entry.published_at_utc = source_entry.published_at_utc.clone();
            entry.collected_at_utc = source_entry.collected_at_utc.clone();
            entry.state_kind = source_entry.state_kind.clone();
            entry.state_ref = source_entry.state_ref.clone();
            entry.install_state_ref = source_entry.install_state_ref.clone();
            entry.install_profile_refs = source_entry.install_profile_refs.clone();
            entry.delta_from_previous = source_entry.delta_from_previous.clone();
            entry.recent_deltas = source_entry.recent_deltas.clone();
            entry.version_manifest = source_entry.version_manifest.clone();
            entry.catalog_state_manifest = source_entry.catalog_state_manifest.clone();
            entry.history = source_entry.history.clone();
            if product == "nexrad" && source_entry.state_manifest.is_some() {
                entry.loaded_version = source_entry.loaded_version.clone();
                entry.state_manifest = source_entry.state_manifest.clone();
            }
        }
    }

    pub fn durable_missing_requests(
        &self,
        installed: impl IntoIterator<Item = LiveFeedDurableInstalledProduct>,
    ) -> Vec<LiveFeedCacheRequest> {
        self.durable_missing_requests_with_nexrad_profile(installed, "offline_0")
    }

    pub fn durable_missing_requests_with_nexrad_profile(
        &self,
        installed: impl IntoIterator<Item = LiveFeedDurableInstalledProduct>,
        nexrad_profile: &str,
    ) -> Vec<LiveFeedCacheRequest> {
        if !self.catalog_loaded {
            return Vec::new();
        }

        let installed = installed.into_iter().collect::<Vec<_>>();
        let installed_by_product = installed
            .iter()
            .cloned()
            .map(|installed| (installed.product.clone(), installed))
            .collect::<HashMap<_, _>>();
        let installed_by_product_version = installed
            .into_iter()
            .map(|installed| {
                (
                    (installed.product.clone(), installed.version.clone()),
                    installed,
                )
            })
            .collect::<HashMap<_, _>>();
        let mut requests = Vec::new();
        let mut products = self.products.keys().map(String::as_str).collect::<Vec<_>>();
        products.sort();
        products.dedup();
        for product in products {
            let Some(entry) = self.products.get(product) else {
                continue;
            };
            let Some(version) = &entry.current_version else {
                continue;
            };
            if product == "nexrad" {
                append_durable_nexrad_request(
                    self,
                    &mut requests,
                    DurableNexradRequest {
                        product,
                        version,
                        version_manifest: entry.version_manifest.as_ref(),
                        version_manifest_url: entry.version_manifest_url.as_deref(),
                        full_ref: entry
                            .install_profile_refs
                            .get(nexrad_profile)
                            .or(entry.install_state_ref.as_ref())
                            .or(entry.state_ref.as_ref()),
                        install_profile: entry
                            .install_profile_refs
                            .contains_key(nexrad_profile)
                            .then(|| nexrad_profile.to_string()),
                        expected_state_sha256: entry.expected_state_sha256.as_deref(),
                    },
                    &installed_by_product_version,
                );
                let retained_history_count = NEXRAD_FRAME_WINDOW_SIZE.saturating_sub(1);
                let retained_history_start =
                    entry.history.len().saturating_sub(retained_history_count);
                for history in &entry.history[retained_history_start..] {
                    append_durable_nexrad_request(
                        self,
                        &mut requests,
                        DurableNexradRequest {
                            product,
                            version: &history.version,
                            version_manifest: history.version_manifest.as_ref(),
                            version_manifest_url: history.version_manifest_url.as_deref(),
                            full_ref: history
                                .install_profile_refs
                                .get(nexrad_profile)
                                .or(history.install_state_ref.as_ref())
                                .or(history.state_ref.as_ref()),
                            install_profile: history
                                .install_profile_refs
                                .contains_key(nexrad_profile)
                                .then(|| nexrad_profile.to_string()),
                            expected_state_sha256: history.expected_state_sha256.as_deref(),
                        },
                        &installed_by_product_version,
                    );
                }
                continue;
            }
            if installed_by_product
                .get(product)
                .is_some_and(|installed| entry.installed_product_is_current(installed))
            {
                continue;
            }
            if entry.version_manifest.is_none() {
                if let Some(url) = &entry.version_manifest_url {
                    requests.push(LiveFeedCacheRequest {
                        id: format!("live_feeds/version/{product}/{version}"),
                        url: self.required_live_feed_url(url),
                        kind: LiveFeedCacheRequestKind::Version {
                            product: product.to_string(),
                            version: version.clone(),
                        },
                    });
                }
                continue;
            }
            if product == "notams" {
                if let Some(installed) = installed_by_product.get(product) {
                    if let Some(delta) = entry.notam_delta_from(&installed.version) {
                        requests.push(LiveFeedCacheRequest {
                            id: format!(
                                "live_feeds/delta/{}/{}/{}",
                                product, delta.from_version, delta.to_version
                            ),
                            url: self.required_live_feed_url(&delta.url),
                            kind: LiveFeedCacheRequestKind::Delta {
                                product: product.to_string(),
                                from_version: delta.from_version.clone(),
                                to_version: delta.to_version.clone(),
                                payload_kind: delta.kind.clone(),
                            },
                        });
                        continue;
                    }
                }
                if let Some(state) = entry.state_ref.as_ref() {
                    requests.push(LiveFeedCacheRequest {
                        id: format!("live_feeds/full/{product}/{}", state.state_sha256),
                        url: self.required_live_feed_url(&state.url),
                        kind: LiveFeedCacheRequestKind::Full {
                            product: product.to_string(),
                            version: state.state_sha256.clone(),
                            payload_kind: state.kind.clone(),
                            install_profile: None,
                        },
                    });
                }
                continue;
            }
            let full_ref = entry.durable_full_payload_ref(product);
            if product != "winds-aloft" {
                if let Some(delta) =
                    entry.durable_applicable_delta(product, installed_by_product.get(product))
                {
                    if delta_is_preferred(delta, full_ref) {
                        requests.push(LiveFeedCacheRequest {
                            id: format!(
                                "live_feeds/delta/{}/{}/{}",
                                product, delta.from_version, delta.to_version
                            ),
                            url: self.required_live_feed_url(&delta.url),
                            kind: LiveFeedCacheRequestKind::Delta {
                                product: product.to_string(),
                                from_version: delta.from_version.clone(),
                                to_version: delta.to_version.clone(),
                                payload_kind: delta.kind.clone().or_else(|| {
                                    Some(durable_delta_payload_kind(product).to_string())
                                }),
                            },
                        });
                        continue;
                    }
                }
            }
            if let Some(full_ref) = full_ref {
                requests.push(LiveFeedCacheRequest {
                    id: format!("live_feeds/full/{product}/{version}"),
                    url: self.required_live_feed_url(&full_ref.url),
                    kind: LiveFeedCacheRequestKind::Full {
                        product: product.to_string(),
                        version: version.clone(),
                        payload_kind: full_ref.kind.clone(),
                        install_profile: None,
                    },
                });
            }
        }
        requests
    }

    pub fn durable_missing_requests_at_epoch_ms(
        &self,
        installed: impl IntoIterator<Item = LiveFeedDurableInstalledProduct>,
        epoch_ms: i64,
    ) -> Vec<LiveFeedCacheRequest> {
        let requests = self.durable_missing_requests(installed);
        self.retryable_cache_requests(requests, epoch_ms)
    }

    pub(crate) fn retryable_cache_requests(
        &self,
        requests: Vec<LiveFeedCacheRequest>,
        epoch_ms: i64,
    ) -> Vec<LiveFeedCacheRequest> {
        requests
            .into_iter()
            .filter(
                |request| match self.resource_failure_retry_after_epoch_ms.get(&request.id) {
                    Some(retry_after) => *retry_after <= epoch_ms,
                    None => true,
                },
            )
            .collect()
    }

    pub fn ingest_durable_request_resource(
        &mut self,
        request: &LiveFeedCacheRequest,
        bytes: &[u8],
    ) -> AppResult<()> {
        match &request.kind {
            LiveFeedCacheRequestKind::Version { product, version } => {
                self.ingest_resource(&format!("live_feeds/version/{product}/{version}"), bytes)
            }
            LiveFeedCacheRequestKind::State { product, version } => {
                self.ingest_resource(&format!("live_feeds/state/{product}/{version}"), bytes)
            }
            LiveFeedCacheRequestKind::Full { .. } | LiveFeedCacheRequestKind::Delta { .. } => {
                Ok(())
            }
        }
    }

    pub fn durable_full_payload_ref_for_request(
        &self,
        product: &str,
        version: &str,
        install_profile: Option<&str>,
    ) -> AppResult<&LiveFeedPayloadRef> {
        let entry = self
            .products
            .get(product)
            .ok_or_else(|| invalid_live_feed(format!("missing live-feed product {product}")))?;
        if product == "notams" {
            return entry
                .state_ref
                .as_ref()
                .filter(|state| state.state_sha256 == version)
                .ok_or_else(|| {
                    invalid_live_feed(format!(
                        "NOTAM checkpoint {version} does not match version manifest"
                    ))
                });
        }
        if entry.current_version.as_deref() != Some(version) && product == "nexrad" {
            return entry
                .history
                .iter()
                .find(|history| history.version == version)
                .and_then(|history| {
                    install_profile
                        .and_then(|profile| history.install_profile_refs.get(profile))
                        .or(history
                            .install_state_ref
                            .as_ref()
                            .or(history.state_ref.as_ref()))
                })
                .ok_or_else(|| {
                    invalid_live_feed(format!(
                        "{product}/{version} does not advertise an installable durable payload"
                    ))
                });
        }
        if entry.current_version.as_deref() != Some(version) {
            return Err(invalid_live_feed(format!(
                "{product} current version is {:?}, expected {version}",
                entry.current_version
            )));
        }
        install_profile
            .and_then(|profile| entry.install_profile_refs.get(profile))
            .or_else(|| entry.durable_full_payload_ref(product))
            .ok_or_else(|| {
                invalid_live_feed(format!(
                    "{product}/{version} does not advertise an installable durable payload"
                ))
            })
    }

    pub fn nexrad_install_profile_bytes(&self) -> BTreeMap<String, u64> {
        self.products
            .get("nexrad")
            .map(|entry| {
                entry
                    .install_profile_refs
                    .iter()
                    .map(|(profile, payload)| (profile.clone(), payload.bytes))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn nexrad_state_manifest_cache_requests(&self) -> Vec<LiveFeedCacheRequest> {
        let Some(entry) = self.products.get("nexrad") else {
            return Vec::new();
        };
        let mut requests = Vec::new();
        if let Some(version) = entry.current_version.as_deref() {
            append_nexrad_state_manifest_cache_request(self, &mut requests, version, entry);
        }
        let retained_history_count = NEXRAD_FRAME_WINDOW_SIZE.saturating_sub(1);
        let retained_history_start = entry.history.len().saturating_sub(retained_history_count);
        for history in &entry.history[retained_history_start..] {
            append_nexrad_history_state_manifest_cache_request(self, &mut requests, history);
        }
        requests
    }

    pub fn current_state_manifest_cache_request(
        &self,
        product: &str,
    ) -> Option<LiveFeedCacheRequest> {
        let entry = self.products.get(product)?;
        let version = entry.current_version.as_deref()?;
        if entry.version_manifest.is_none() || entry.catalog_state_manifest.is_some() {
            return None;
        }
        let state_ref = entry.state_ref.as_ref()?;
        Some(LiveFeedCacheRequest {
            id: format!("live_feeds/state/{product}/{version}"),
            url: self.required_live_feed_url(&state_ref.url),
            kind: LiveFeedCacheRequestKind::State {
                product: product.to_string(),
                version: version.to_string(),
            },
        })
    }

    pub fn durable_delta_ref_for_request(
        &self,
        product: &str,
        from_version: &str,
        to_version: &str,
    ) -> AppResult<&LiveFeedDeltaRef> {
        let entry = self
            .products
            .get(product)
            .ok_or_else(|| invalid_live_feed(format!("missing live-feed product {product}")))?;
        let delta = (if product == "notams" {
            entry.notam_delta(from_version, to_version)
        } else {
            entry.delta_from_previous.as_ref()
        })
        .ok_or_else(|| invalid_live_feed(format!("{product}/{to_version} has no delta")))?;
        if delta.from_version != from_version || delta.to_version != to_version {
            return Err(invalid_live_feed(format!(
                "requested delta {product}/{from_version}/{to_version} does not match manifest"
            )));
        }
        Ok(delta)
    }

    fn missing_resources(&self) -> Vec<CoreResourceRequest> {
        if !self.catalog_loaded {
            return Vec::new();
        }
        let products = self.products.keys().map(String::as_str).collect::<Vec<_>>();
        self.missing_resources_for_products(products)
    }

    pub(crate) fn public_live_feed_resource(
        &self,
        id: impl Into<String>,
        live_feed_relative_url: &str,
        optional: bool,
    ) -> CoreResourceRequest {
        let id = id.into();
        match self.live_feed_url(live_feed_relative_url) {
            Ok(url) => CoreResourceRequest::public_url(id, url, optional),
            Err(err) => CoreResourceRequest::unavailable(id, err.to_string(), optional),
        }
    }

    fn live_feed_url(&self, live_feed_relative_url: &str) -> AppResult<String> {
        let source_root_url = self.source_root_url.as_deref().ok_or_else(|| {
            invalid_live_feed("live-feed source root is not configured".to_string())
        })?;
        Ok(live_feed_resource_url(
            source_root_url,
            live_feed_relative_url,
        ))
    }

    fn required_live_feed_url(&self, live_feed_relative_url: &str) -> String {
        self.live_feed_url(live_feed_relative_url).expect(
            "live-feed source root must be configured before requesting live-feed resources",
        )
    }

    fn missing_resources_for_products<'a>(
        &self,
        products: impl IntoIterator<Item = &'a str>,
    ) -> Vec<CoreResourceRequest> {
        let mut resources = Vec::new();
        let mut products = products.into_iter().collect::<Vec<_>>();
        products.sort();
        products.dedup();
        for product in products {
            let Some(entry) = self.products.get(product) else {
                continue;
            };
            let Some(version) = &entry.current_version else {
                continue;
            };
            if entry.version_manifest.is_none() {
                if let Some(url) = &entry.version_manifest_url {
                    resources.push(self.public_live_feed_resource(
                        format!("live_feeds/version/{product}/{version}"),
                        url,
                        false,
                    ));
                    continue;
                }
            }
            if entry.loaded_version.as_deref() == Some(version.as_str()) {
                continue;
            }
            if product == "notams" {
                if let Some(delta) = entry.applicable_notam_delta() {
                    resources.push(self.public_live_feed_resource(
                        format!(
                            "live_feeds/delta/{}/{}/{}",
                            product, delta.from_version, delta.to_version
                        ),
                        &delta.url,
                        false,
                    ));
                    continue;
                }
                if let Some(state) = entry.state_ref.as_ref() {
                    resources.push(self.public_live_feed_resource(
                        format!("live_feeds/state/{product}/{}", state.state_sha256),
                        &state.url,
                        false,
                    ));
                }
                continue;
            }
            if let Some(delta) = entry
                .applicable_delta(product)
                .filter(|delta| delta_is_preferred(delta, entry.state_ref.as_ref()))
            {
                resources.push(self.public_live_feed_resource(
                    format!(
                        "live_feeds/delta/{}/{}/{}",
                        product, delta.from_version, delta.to_version
                    ),
                    &delta.url,
                    false,
                ));
                continue;
            }
            if let Some(url) = &entry.state_url {
                resources.push(self.public_live_feed_resource(
                    format!("live_feeds/state/{product}/{version}"),
                    url,
                    false,
                ));
            }
        }
        resources
    }

    fn missing_history_resources_for_product(&self, product: &str) -> Vec<CoreResourceRequest> {
        let Some(entry) = self.products.get(product) else {
            return Vec::new();
        };
        let mut resources = Vec::new();
        for history in &entry.history {
            if history.state_manifest.is_some() {
                continue;
            }
            if history.version_manifest.is_none() {
                if let Some(url) = &history.version_manifest_url {
                    resources.push(self.public_live_feed_resource(
                        format!("live_feeds/version/{product}/{}", history.version),
                        url,
                        true,
                    ));
                    continue;
                }
            }
            if let Some(url) = &history.state_url {
                resources.push(self.public_live_feed_resource(
                    format!("live_feeds/state/{product}/{}", history.version),
                    url,
                    true,
                ));
            }
        }
        resources
    }

    fn retryable_resources(
        &self,
        resources: Vec<CoreResourceRequest>,
        epoch_ms: i64,
    ) -> Vec<CoreResourceRequest> {
        resources
            .into_iter()
            .filter(
                |resource| match self.resource_failure_retry_after_epoch_ms.get(&resource.id) {
                    Some(retry_after) => *retry_after <= epoch_ms,
                    None => true,
                },
            )
            .collect()
    }

    fn snapshot(&self) -> LiveFeedsSnapshot {
        let mut products: Vec<_> = self
            .products
            .iter()
            .map(|(product, entry)| LiveFeedProductSnapshot {
                product: product.clone(),
                current_version: entry.current_version.clone(),
                version_manifest_loaded: entry.version_manifest.is_some(),
                state_manifest_loaded: entry
                    .current_version
                    .as_deref()
                    .is_some_and(|version| entry.loaded_version.as_deref() == Some(version)),
            })
            .collect();
        products.sort_by(|left, right| left.product.cmp(&right.product));
        LiveFeedsSnapshot { products }
    }

    fn invalidations(&self) -> Vec<UiInvalidation> {
        let mut invalidations = Vec::new();
        if self.catalog_loaded {
            invalidations.push(UiInvalidation::SessionSnapshot);
            invalidations.push(UiInvalidation::MapOverlay);
            invalidations.push(UiInvalidation::NexradOverlay);
            invalidations.push(UiInvalidation::DebugPanel);
        }
        for (product, entry) in &self.products {
            if entry
                .current_version
                .as_deref()
                .is_none_or(|version| entry.loaded_version.as_deref() != Some(version))
            {
                continue;
            }
            invalidations.push(UiInvalidation::SessionSnapshot);
            match product_contracts::live_feed_product_policy(product)
                .map(|policy| policy.ui_invalidation)
            {
                Some(product_contracts::LiveFeedUiInvalidationPolicy::NexradOverlay) => {
                    invalidations.push(UiInvalidation::NexradOverlay);
                    invalidations.push(UiInvalidation::DebugPanel);
                }
                Some(product_contracts::LiveFeedUiInvalidationPolicy::MapOverlay) => {
                    invalidations.push(UiInvalidation::MapOverlay);
                    invalidations.push(UiInvalidation::DebugPanel);
                }
                Some(product_contracts::LiveFeedUiInvalidationPolicy::None) | None => {
                    invalidations.push(UiInvalidation::DebugPanel);
                }
            }
        }
        invalidations.sort_by_key(|invalidation| format!("{invalidation:?}"));
        invalidations.dedup();
        invalidations
    }
}

impl LiveFeedProductState {
    fn applicable_notam_delta(&self) -> Option<&LiveFeedDeltaRef> {
        let loaded = self.loaded_version.as_deref()?;
        self.recent_deltas
            .iter()
            .find(|delta| delta.from_version == loaded)
            .or_else(|| {
                self.delta_from_previous
                    .as_ref()
                    .filter(|delta| delta.from_version == loaded)
            })
    }

    fn notam_delta(&self, from_version: &str, to_version: &str) -> Option<&LiveFeedDeltaRef> {
        self.recent_deltas
            .iter()
            .find(|delta| delta.from_version == from_version && delta.to_version == to_version)
            .or_else(|| {
                self.delta_from_previous.as_ref().filter(|delta| {
                    delta.from_version == from_version && delta.to_version == to_version
                })
            })
    }

    fn notam_delta_from(&self, from_version: &str) -> Option<&LiveFeedDeltaRef> {
        self.recent_deltas
            .iter()
            .find(|delta| delta.from_version == from_version)
            .or_else(|| {
                self.delta_from_previous
                    .as_ref()
                    .filter(|delta| delta.from_version == from_version)
            })
    }

    fn sync_history(&mut self, history: Vec<CurrentProductHistoryEntry>) {
        let mut existing = std::mem::take(&mut self.history)
            .into_iter()
            .map(|entry| (entry.version.clone(), entry))
            .collect::<HashMap<_, _>>();
        self.history = history
            .into_iter()
            .map(|entry| {
                let mut state = existing.remove(&entry.version).unwrap_or_default();
                let state_sha256_changed =
                    entry.state_sha256.as_ref().is_some_and(|state_sha256| {
                        state.expected_state_sha256.as_ref() != Some(state_sha256)
                    });
                let version_manifest_url_changed = state
                    .version_manifest_url
                    .as_ref()
                    .is_some_and(|existing| existing != &entry.version_manifest_url);
                if state_sha256_changed || version_manifest_url_changed {
                    state.state_kind = None;
                    state.state_ref = None;
                    state.install_state_ref = None;
                    state.install_profile_refs.clear();
                    state.version_manifest = None;
                    state.state_manifest = None;
                }
                state.version = entry.version;
                state.version_manifest_url = Some(entry.version_manifest_url);
                if entry.state_url.is_some() {
                    state.state_url = entry.state_url;
                }
                if entry.state_sha256.is_some() {
                    state.expected_state_sha256 = entry.state_sha256;
                }
                state.published_at_utc = entry.published_at_utc;
                state.collected_at_utc = entry.collected_at_utc;
                state
            })
            .collect();
    }

    fn applicable_delta(&self, product: &str) -> Option<&LiveFeedDeltaRef> {
        if !supports_record_delta(product) {
            return None;
        }
        let delta = self.delta_from_previous.as_ref()?;
        if self.loaded_version.as_deref() == Some(delta.from_version.as_str())
            && self.current_version.as_deref() == Some(delta.to_version.as_str())
            && (self.state_manifest.is_some()
                || product_supports_prepared_record_delta_without_raw_state(product))
        {
            Some(delta)
        } else {
            None
        }
    }

    fn installed_product_is_current(&self, installed: &LiveFeedDurableInstalledProduct) -> bool {
        self.current_version
            .as_deref()
            .is_some_and(|version| installed.version == version)
            && self
                .expected_state_sha256
                .as_deref()
                .is_none_or(|expected| installed.state_sha256 == expected)
    }

    fn durable_applicable_delta(
        &self,
        product: &str,
        installed: Option<&LiveFeedDurableInstalledProduct>,
    ) -> Option<&LiveFeedDeltaRef> {
        if !supports_durable_delta(product) {
            return None;
        }
        let installed = installed?;
        let delta = self.delta_from_previous.as_ref()?;
        if installed.version == delta.from_version
            && installed.state_sha256 == delta.from_state_sha256
            && self.current_version.as_deref() == Some(delta.to_version.as_str())
        {
            Some(delta)
        } else {
            None
        }
    }

    fn durable_full_payload_ref(&self, product: &str) -> Option<&LiveFeedPayloadRef> {
        if product == "obstacles" || self.state_ref.as_ref()?.kind.as_deref() == Some("nav_kv") {
            self.install_state_ref.as_ref()
        } else {
            self.install_state_ref.as_ref().or(self.state_ref.as_ref())
        }
    }
}

struct DurableNexradRequest<'a> {
    product: &'a str,
    version: &'a str,
    version_manifest: Option<&'a Value>,
    version_manifest_url: Option<&'a str>,
    full_ref: Option<&'a LiveFeedPayloadRef>,
    install_profile: Option<String>,
    expected_state_sha256: Option<&'a str>,
}

fn append_nexrad_state_manifest_cache_request(
    state: &LiveFeedsState,
    requests: &mut Vec<LiveFeedCacheRequest>,
    version: &str,
    entry: &LiveFeedProductState,
) {
    if entry.version_manifest.is_none() || entry.state_manifest.is_some() {
        return;
    }
    let Some(state_ref) = entry.state_ref.as_ref() else {
        return;
    };
    requests.push(LiveFeedCacheRequest {
        id: format!("live_feeds/state/nexrad/{version}"),
        url: state.required_live_feed_url(&state_ref.url),
        kind: LiveFeedCacheRequestKind::State {
            product: "nexrad".to_string(),
            version: version.to_string(),
        },
    });
}

fn append_nexrad_history_state_manifest_cache_request(
    state: &LiveFeedsState,
    requests: &mut Vec<LiveFeedCacheRequest>,
    entry: &LiveFeedProductHistoryState,
) {
    if entry.version_manifest.is_none() || entry.state_manifest.is_some() {
        return;
    }
    let Some(state_ref) = entry.state_ref.as_ref() else {
        return;
    };
    requests.push(LiveFeedCacheRequest {
        id: format!("live_feeds/state/nexrad/{}", entry.version),
        url: state.required_live_feed_url(&state_ref.url),
        kind: LiveFeedCacheRequestKind::State {
            product: "nexrad".to_string(),
            version: entry.version.clone(),
        },
    });
}

fn append_durable_nexrad_request(
    state: &LiveFeedsState,
    requests: &mut Vec<LiveFeedCacheRequest>,
    request: DurableNexradRequest<'_>,
    installed: &HashMap<(String, String), LiveFeedDurableInstalledProduct>,
) {
    let DurableNexradRequest {
        product,
        version,
        version_manifest,
        version_manifest_url,
        full_ref,
        install_profile,
        expected_state_sha256,
    } = request;
    if installed
        .get(&(product.to_string(), version.to_string()))
        .is_some_and(|installed| {
            expected_state_sha256.is_none_or(|expected| installed.state_sha256 == expected)
        })
    {
        return;
    }
    if version_manifest.is_none() {
        if let Some(url) = version_manifest_url {
            requests.push(LiveFeedCacheRequest {
                id: format!("live_feeds/version/{product}/{version}"),
                url: state.required_live_feed_url(url),
                kind: LiveFeedCacheRequestKind::Version {
                    product: product.to_string(),
                    version: version.to_string(),
                },
            });
        }
        return;
    }
    if let Some(full_ref) = full_ref {
        requests.push(LiveFeedCacheRequest {
            id: format!("live_feeds/full/{product}/{version}"),
            url: state.required_live_feed_url(&full_ref.url),
            kind: LiveFeedCacheRequestKind::Full {
                product: product.to_string(),
                version: version.to_string(),
                payload_kind: full_ref.kind.clone(),
                install_profile,
            },
        });
    }
}

fn delta_is_preferred(delta: &LiveFeedDeltaRef, full_ref: Option<&LiveFeedPayloadRef>) -> bool {
    let Some(full_ref) = full_ref else {
        return true;
    };
    delta.bytes <= full_ref.bytes
}

fn supports_durable_delta(product: &str) -> bool {
    product_contracts::live_feed_product_policy(product).is_some_and(|policy| {
        matches!(
            policy.delta,
            product_contracts::LiveFeedDeltaPolicy::RecordJson
                | product_contracts::LiveFeedDeltaPolicy::NavKv
        )
    })
}

fn durable_delta_payload_kind(product: &str) -> &'static str {
    match product_contracts::live_feed_product_policy(product).map(|policy| policy.delta) {
        Some(product_contracts::LiveFeedDeltaPolicy::NavKv) => "nav_kv_delta_xz",
        Some(product_contracts::LiveFeedDeltaPolicy::RecordJson) => "record_json_delta_xz",
        _ => panic!("live-feed product {product} does not support a durable delta"),
    }
}

fn split_product_version(resource_id: &str, rest: &str) -> AppResult<(String, String)> {
    let Some((product, version)) = rest.split_once('/') else {
        return Err(invalid_live_feed(format!(
            "invalid live feed resource id: {resource_id}"
        )));
    };
    Ok((product.to_string(), version.to_string()))
}

fn split_product_from_to(resource_id: &str, rest: &str) -> AppResult<(String, String, String)> {
    let parts = rest.split('/').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
        return Err(invalid_live_feed(format!(
            "invalid live feed resource id: {resource_id}"
        )));
    }
    Ok((
        parts[0].to_string(),
        parts[1].to_string(),
        parts[2].to_string(),
    ))
}

fn normalize_product_history(
    product: &str,
    history: Vec<CurrentProductHistoryEntry>,
) -> AppResult<Vec<CurrentProductHistoryEntry>> {
    if history.len() > LIVE_FEED_HISTORY_MAX_ENTRIES {
        return Err(invalid_live_feed(format!(
            "{product} live-feed history has {} entries; max is {LIVE_FEED_HISTORY_MAX_ENTRIES}",
            history.len()
        )));
    }
    let mut versions = Vec::new();
    let mut normalized = Vec::new();
    for entry in history {
        if entry.version.is_empty() {
            return Err(invalid_live_feed(format!(
                "{product} live-feed history contains an empty version"
            )));
        }
        if versions.iter().any(|version| version == &entry.version) {
            return Err(invalid_live_feed(format!(
                "{product} live-feed history repeats version {}",
                entry.version
            )));
        }
        validate_relative_url(&entry.version_manifest_url)?;
        if let Some(url) = &entry.state_url {
            validate_relative_url(url)?;
        }
        versions.push(entry.version.clone());
        normalized.push(entry);
    }
    Ok(normalized)
}

fn project_current_history(
    history: Vec<live_feeds_v3::CurrentHistoryEntry>,
) -> Vec<CurrentProductHistoryEntry> {
    history
        .into_iter()
        .map(|entry| CurrentProductHistoryEntry {
            version: entry.version,
            version_manifest_url: entry.version_manifest_url,
            state_url: entry.state_url,
            state_sha256: entry.state_sha256,
            published_at_utc: None,
            collected_at_utc: None,
        })
        .collect()
}

fn product_registration_from_event(payload: live_feeds_v3::CurrentEvent) -> ProductRegistration {
    ProductRegistration {
        product: payload.product,
        version: payload.version,
        version_manifest_url: payload.version_manifest_url,
        state_url: payload.state_url,
        state_sha256: payload.state_sha256,
        published_at_utc: payload.published_at_utc,
        collected_at_utc: payload.collected_at_utc,
        history: project_current_history(payload.history),
    }
}

fn validated_catalog_registrations(
    catalog: live_feeds_v3::Catalog,
) -> AppResult<Vec<ProductRegistration>> {
    catalog
        .products
        .into_iter()
        .map(|(product, entry)| {
            validate_product_registration(ProductRegistration {
                product,
                version: entry.current,
                version_manifest_url: entry.version_manifest_url,
                state_url: entry.state_url,
                state_sha256: entry.state_sha256,
                published_at_utc: entry.published_at_utc,
                collected_at_utc: entry.collected_at_utc,
                history: project_current_history(entry.history),
            })
        })
        .collect()
}

fn validate_product_registration(
    mut registration: ProductRegistration,
) -> AppResult<ProductRegistration> {
    validate_relative_url(&registration.version_manifest_url)?;
    validate_relative_url(&registration.state_url)?;
    registration.history = normalize_product_history(&registration.product, registration.history)?;
    Ok(registration)
}

enum ParsedLiveFeedSseEvent {
    Catalog(live_feeds_v3::Catalog),
    Product(live_feeds_v3::CurrentEvent),
}

fn parse_live_feed_sse_event(event: LiveFeedSseEvent) -> AppResult<Option<ParsedLiveFeedSseEvent>> {
    let event_name = event.event.as_deref().unwrap_or("message");
    match event_name {
        CATALOG_EVENT_NAME => {
            let catalog = versioned_json::decode_exact::<live_feeds_v3::Catalog>(
                "live-feed SSE catalog",
                event.data.as_bytes(),
                LIVE_FEEDS_SCHEMA_VERSION,
            )
            .map_err(|error| invalid_live_feed(error.to_string()))?;
            Ok(Some(ParsedLiveFeedSseEvent::Catalog(catalog)))
        }
        PRODUCT_EVENT_NAME | "message" => {
            let payload = versioned_json::decode_exact::<live_feeds_v3::CurrentEvent>(
                "live-feed SSE event",
                event.data.as_bytes(),
                LIVE_FEEDS_SCHEMA_VERSION,
            )
            .map_err(|error| invalid_live_feed(error.to_string()))?;
            Ok(Some(ParsedLiveFeedSseEvent::Product(payload)))
        }
        _ => Ok(None),
    }
}

fn decode_live_feed_payload<'a>(
    payload_kind: Option<&str>,
    bytes: &'a [u8],
) -> AppResult<std::borrow::Cow<'a, [u8]>> {
    match payload_kind {
        Some("json_xz")
        | Some("record_json_delta_xz")
        | Some("nav_kv_delta_xz")
        | Some("notam_checkpoint_xz")
        | Some("notam_ordered_delta_xz") => {
            nav_kv_package::decode_xz_if_needed(bytes).map_err(invalid_live_feed)
        }
        _ => Ok(std::borrow::Cow::Borrowed(bytes)),
    }
}

pub fn normalize_live_feed_source_root_url(source_root_url: &str) -> AppResult<String> {
    let mut normalized = source_root_url.trim().trim_end_matches('/').to_string();
    for suffix in [
        "/live-feeds/v3/events",
        "/live-feeds/v3",
        "/live-feeds/status.html",
        "/live-feeds",
    ] {
        if normalized.ends_with(suffix) {
            normalized.truncate(normalized.len() - suffix.len());
            break;
        }
    }
    if normalized.is_empty()
        || !(normalized.starts_with("http://") || normalized.starts_with("https://"))
    {
        return Err(invalid_live_feed(format!(
            "live-feed source root must be an absolute http(s) URL: {source_root_url}"
        )));
    }
    Ok(normalized)
}

pub fn live_feed_events_url(source_root_url: &str) -> AppResult<String> {
    Ok(format!(
        "{}{}",
        normalize_live_feed_source_root_url(source_root_url)?,
        LIVE_FEEDS_EVENTS_PATH
    ))
}

pub fn live_feed_status_url(source_root_url: &str) -> AppResult<String> {
    Ok(format!(
        "{}{}",
        normalize_live_feed_source_root_url(source_root_url)?,
        LIVE_FEEDS_STATUS_PATH
    ))
}

fn live_feed_resource_url(source_root_url: &str, live_feed_relative_url: &str) -> String {
    let path = if live_feed_relative_url.starts_with('/') {
        live_feed_relative_url.to_string()
    } else {
        format!(
            "{LIVE_FEEDS_PREFIX}{}",
            live_feed_relative_url.trim_start_matches('/')
        )
    };
    format!("{}{}", source_root_url.trim_end_matches('/'), path)
}

fn validate_relative_url(url: &str) -> AppResult<()> {
    if url.starts_with('/') || url.contains("://") || url.split('/').any(|part| part == "..") {
        return Err(invalid_live_feed(format!(
            "live feed URL must be package-relative: {url}"
        )));
    }
    Ok(())
}

fn validate_notam_version_manifest(
    manifest: &VersionManifest,
    expected_head: &str,
) -> AppResult<()> {
    if manifest.version != expected_head {
        return Err(invalid_live_feed(format!(
            "NOTAM manifest head {} does not match current {expected_head}",
            manifest.version
        )));
    }
    if manifest.state.kind.as_deref() != Some("notam_checkpoint_xz") {
        return Err(invalid_live_feed(
            "NOTAM manifest state is not a checkpoint".to_string(),
        ));
    }
    if manifest.recent_deltas.is_empty() {
        if manifest.state.state_sha256 != expected_head {
            return Err(invalid_live_feed(format!(
                "NOTAM checkpoint is {}, but head is {expected_head} with no deltas",
                manifest.state.state_sha256
            )));
        }
        if manifest.delta_from_previous.is_some() {
            return Err(invalid_live_feed(
                "NOTAM checkpoint-only manifest unexpectedly has a latest delta".to_string(),
            ));
        }
        return Ok(());
    }

    let mut state_id = manifest.recent_deltas[0].from_state_sha256.as_str();
    let mut checkpoint_reachable = state_id == manifest.state.state_sha256;
    for delta in &manifest.recent_deltas {
        if delta.kind.as_deref() != Some("notam_ordered_delta_xz")
            || delta.from_version != state_id
            || delta.from_state_sha256 != state_id
            || delta.to_version != delta.to_state_sha256
            || delta.mutation_count.is_none()
        {
            return Err(invalid_live_feed(format!(
                "NOTAM retained delta chain is invalid at {}",
                delta.url
            )));
        }
        state_id = &delta.to_state_sha256;
        checkpoint_reachable |= state_id == manifest.state.state_sha256;
    }
    if state_id != expected_head {
        return Err(invalid_live_feed(format!(
            "NOTAM retained delta chain ends at {state_id}, expected {expected_head}"
        )));
    }
    if !checkpoint_reachable {
        return Err(invalid_live_feed(format!(
            "NOTAM checkpoint {} is outside the retained delta chain",
            manifest.state.state_sha256
        )));
    }
    if let Some(delta) = manifest.delta_from_previous.as_ref() {
        if delta.kind.as_deref() != Some("notam_ordered_delta_xz")
            || delta.to_version != expected_head
            || delta.to_state_sha256 != expected_head
            || delta.mutation_count.is_none()
        {
            return Err(invalid_live_feed(
                "NOTAM latest delta metadata is invalid".to_string(),
            ));
        }
        if manifest.recent_deltas.last() != Some(delta) {
            return Err(invalid_live_feed(
                "NOTAM latest delta is not the retained chain tail".to_string(),
            ));
        }
    } else {
        return Err(invalid_live_feed(
            "NOTAM delta-chain manifest has no latest delta".to_string(),
        ));
    }
    Ok(())
}

fn canonical_json_sha256(value: &Value) -> AppResult<String> {
    let bytes = serde_json::to_vec(value).map_err(invalid_live_feed_json)?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn prepare_live_feed_state_resource(
    resource_id: &str,
    bytes: &[u8],
) -> AppResult<(Value, Vec<u8>)> {
    let Some(rest) = resource_id.strip_prefix("live_feeds/state/") else {
        return Err(invalid_live_feed(format!(
            "not a live-feed state resource: {resource_id}"
        )));
    };
    let (product, version) = split_product_version(resource_id, rest)?;
    if product == "notams" {
        return Err(invalid_live_feed(
            "NOTAM checkpoints require the stateful canonical NOTAM preparer".to_string(),
        ));
    }
    let decoded = nav_kv_package::decode_xz_if_needed(bytes).map_err(invalid_live_feed)?;
    let state: Value = serde_json::from_slice(decoded.as_ref()).map_err(invalid_live_feed_json)?;
    let state_sha256 = canonical_json_sha256(&state)?;
    let payload = prepare_live_feed_payload(&product, state.clone())?;
    if payload.version_label() != version {
        return Err(invalid_live_feed(format!(
            "{product} state {resource_id} contained version {}",
            payload.version_label()
        )));
    }
    let envelope = PreparedLiveFeedEnvelope {
        schema_version: 1,
        resource_id: resource_id.to_string(),
        product,
        version,
        state_sha256,
        from_version: None,
        from_state_sha256: None,
        delta_blob_sha256: None,
        payload,
    };
    let envelope_bytes = postcard::to_allocvec(&envelope).map_err(|err| {
        invalid_live_feed(format!("failed to encode prepared live-feed state: {err}"))
    })?;
    Ok((state, envelope_bytes))
}

pub fn prepare_notam_live_feed_state_resource_with_work(
    resource_id: &str,
    bytes: &[u8],
    preparer: &mut NotamProjectionPreparer,
    background_work: &mut BackgroundNotamWork,
    apply_work: &mut NotamApplyWork,
) -> AppResult<Vec<u8>> {
    let Some(rest) = resource_id.strip_prefix("live_feeds/state/") else {
        return Err(invalid_live_feed(format!(
            "not a live-feed state resource: {resource_id}"
        )));
    };
    let (product, version) = split_product_version(resource_id, rest)?;
    if product != "notams" {
        return Err(invalid_live_feed(format!(
            "not a NOTAM state resource: {resource_id}"
        )));
    }
    let decoded = nav_kv_package::decode_xz_if_needed(bytes).map_err(invalid_live_feed)?;
    background_work.compressed_bytes_read += bytes.len() as u64;
    background_work.json_bytes_decoded += decoded.len() as u64;
    let checkpoint: NotamCheckpoint =
        serde_json::from_slice(decoded.as_ref()).map_err(invalid_live_feed_json)?;
    background_work.records_decoded += checkpoint.records.len() as u64;
    if checkpoint.state_id != version {
        return Err(invalid_live_feed(format!(
            "NOTAM checkpoint {resource_id} contained state {}",
            checkpoint.state_id
        )));
    }
    let state_sha256 = checkpoint.state_id.clone();
    let payload = preparer
        .install_checkpoint(checkpoint, apply_work)
        .map_err(|error| invalid_live_feed(error.to_string()))?;
    let envelope = PreparedLiveFeedEnvelope {
        schema_version: 1,
        resource_id: resource_id.to_string(),
        product,
        version,
        state_sha256,
        from_version: None,
        from_state_sha256: None,
        delta_blob_sha256: Some(sha256_hex(bytes)),
        payload: PreparedLiveFeedPayload::Notams(payload),
    };
    encode_prepared_notam_envelope(&envelope, background_work)
}

pub fn prepare_notam_live_feed_delta_resource_with_work(
    resource_id: &str,
    bytes: &[u8],
    preparer: &mut NotamProjectionPreparer,
    background_work: &mut BackgroundNotamWork,
    apply_work: &mut NotamApplyWork,
) -> AppResult<Vec<u8>> {
    let Some(rest) = resource_id.strip_prefix("live_feeds/delta/") else {
        return Err(invalid_live_feed(format!(
            "not a live-feed delta resource: {resource_id}"
        )));
    };
    let (product, from_version, to_version) = split_product_from_to(resource_id, rest)?;
    if product != "notams" {
        return Err(invalid_live_feed(format!(
            "not a NOTAM delta resource: {resource_id}"
        )));
    }
    let decoded = nav_kv_package::decode_xz_if_needed(bytes).map_err(invalid_live_feed)?;
    background_work.compressed_bytes_read += bytes.len() as u64;
    background_work.json_bytes_decoded += decoded.len() as u64;
    let delta: NotamDelta =
        serde_json::from_slice(decoded.as_ref()).map_err(invalid_live_feed_json)?;
    background_work.records_decoded += delta.mutations.len() as u64;
    if delta.from_state_id != from_version || delta.to_state_id != to_version {
        return Err(invalid_live_feed(format!(
            "NOTAM delta {resource_id} contained {} -> {}",
            delta.from_state_id, delta.to_state_id
        )));
    }
    let from_state_sha256 = delta.from_state_id.clone();
    let state_sha256 = delta.to_state_id.clone();
    let payload = preparer
        .apply_delta(delta, apply_work)
        .map_err(|error| invalid_live_feed(error.to_string()))?;
    let envelope = PreparedLiveFeedEnvelope {
        schema_version: 1,
        resource_id: resource_id.to_string(),
        product,
        version: to_version,
        state_sha256,
        from_version: Some(from_version),
        from_state_sha256: Some(from_state_sha256),
        delta_blob_sha256: Some(sha256_hex(bytes)),
        payload: PreparedLiveFeedPayload::Notams(payload),
    };
    encode_prepared_notam_envelope(&envelope, background_work)
}

fn encode_prepared_notam_envelope(
    envelope: &PreparedLiveFeedEnvelope,
    work: &mut BackgroundNotamWork,
) -> AppResult<Vec<u8>> {
    let envelope_bytes = postcard::to_allocvec(envelope).map_err(|error| {
        invalid_live_feed(format!("failed to encode prepared NOTAM payload: {error}"))
    })?;
    work.postcard_bytes_written += envelope_bytes.len() as u64;
    Ok(envelope_bytes)
}

pub fn prepare_live_feed_delta_resource(
    resource_id: &str,
    current_state: &Value,
    bytes: &[u8],
) -> AppResult<(Value, Vec<u8>)> {
    let Some(rest) = resource_id.strip_prefix("live_feeds/delta/") else {
        return Err(invalid_live_feed(format!(
            "not a live-feed delta resource: {resource_id}"
        )));
    };
    let (product, from_version, to_version) = split_product_from_to(resource_id, rest)?;
    if !supports_live_feed_delta(&product) || !supports_prepared_live_feed(&product) {
        return Err(invalid_live_feed(format!(
            "cannot prepare live-feed delta from {product}"
        )));
    }
    if product == "notams" {
        return Err(invalid_live_feed(
            "NOTAM deltas require the stateful canonical NOTAM preparer".to_string(),
        ));
    }
    let delta_blob_sha256 = sha256_hex(bytes);
    let decoded = nav_kv_package::decode_xz_if_needed(bytes).map_err(invalid_live_feed)?;
    let from_state_sha256 = canonical_json_sha256(current_state)?;
    let delta = versioned_json::decode_exact::<LiveFeedRecordDelta>(
        "live-feed record delta",
        decoded.as_ref(),
        LIVE_FEEDS_SCHEMA_VERSION,
    )
    .map_err(|error| invalid_live_feed(error.to_string()))?;
    if delta.product != product
        || delta.from_version != from_version
        || delta.to_version != to_version
    {
        return Err(invalid_live_feed(format!(
            "live-feed delta {resource_id} contained {} {} -> {}",
            delta.product, delta.from_version, delta.to_version
        )));
    }
    let next_state = apply_live_feed_record_delta(current_state, &delta)?;
    let state_sha256 = canonical_json_sha256(&next_state)?;
    let payload = prepare_live_feed_payload(&product, next_state.clone())?;
    if payload.version_label() != to_version {
        return Err(invalid_live_feed(format!(
            "prepared {product} delta {resource_id} produced version {}",
            payload.version_label()
        )));
    }
    let envelope = PreparedLiveFeedEnvelope {
        schema_version: 1,
        resource_id: resource_id.to_string(),
        product,
        version: to_version,
        state_sha256,
        from_version: Some(from_version),
        from_state_sha256: Some(from_state_sha256),
        delta_blob_sha256: Some(delta_blob_sha256),
        payload,
    };
    let envelope_bytes = postcard::to_allocvec(&envelope).map_err(|err| {
        invalid_live_feed(format!("failed to encode prepared live-feed delta: {err}"))
    })?;
    Ok((next_state, envelope_bytes))
}

pub fn decode_prepared_live_feed(bytes: &[u8]) -> AppResult<PreparedLiveFeedEnvelope> {
    postcard::from_bytes(bytes)
        .map_err(|err| invalid_live_feed(format!("failed to decode prepared live feed: {err}")))
}

pub fn supports_prepared_live_feed(product: &str) -> bool {
    product_contracts::live_feed_product_policy(product)
        .is_some_and(|policy| policy.preparation.is_prepared())
}

pub fn should_prepare_live_feed_resource(resource_id: &str) -> bool {
    let parts = resource_id.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["live_feeds", "state", product, _] | ["live_feeds", "delta", product, _, _] => {
            supports_prepared_live_feed(product)
        }
        _ => false,
    }
}

pub(crate) fn prepare_live_feed_payload(
    product: &str,
    state: Value,
) -> AppResult<PreparedLiveFeedPayload> {
    use product_contracts::LiveFeedPreparationPolicy as Preparation;

    let preparation = product_contracts::live_feed_product_policy(product)
        .map(|policy| policy.preparation)
        .unwrap_or(Preparation::None);
    match preparation {
        Preparation::Metars => {
            let payload: MetarProductPayload =
                serde_json::from_value(state).map_err(invalid_live_feed_json)?;
            Ok(PreparedLiveFeedPayload::Metars(prepare_metar_live_feed(
                payload,
            )))
        }
        Preparation::Tafs => serde_json::from_value(state)
            .map(PreparedLiveFeedPayload::Tafs)
            .map_err(invalid_live_feed_json),
        Preparation::Pireps => {
            let payload: PirepProductPayload =
                serde_json::from_value(state).map_err(invalid_live_feed_json)?;
            Ok(PreparedLiveFeedPayload::Pireps(prepare_pirep_live_feed(
                payload,
            )))
        }
        Preparation::Tfrs => serde_json::from_value(state)
            .map(PreparedLiveFeedPayload::Tfrs)
            .map_err(invalid_live_feed_json),
        Preparation::Notams => Err(invalid_live_feed(
            "NOTAMs require checkpoint or ordered-delta preparation".to_string(),
        )),
        Preparation::None => Err(invalid_live_feed(format!(
            "unsupported prepared live-feed product: {product}"
        ))),
    }
}

fn prepare_metar_live_feed(payload: MetarProductPayload) -> PreparedMetarLiveFeed {
    let mut records = payload.metars_by_station.values().collect::<Vec<_>>();
    records.sort_by(|left, right| left.station_id.cmp(&right.station_id));
    let mut tiles = std::collections::BTreeMap::<(u32, u32, u32), Vec<String>>::new();
    for zoom in [5_u32, 6, 7] {
        for record in &records {
            let Some((x, y)) = live_feed_metar_tile_xy(record.latitude, record.longitude, zoom)
            else {
                continue;
            };
            tiles
                .entry((zoom, x, y))
                .or_default()
                .push(record.station_id.clone());
        }
    }
    PreparedMetarLiveFeed {
        schema_version: 1,
        payload,
        tiles: tiles
            .into_iter()
            .map(|((z, x, y), station_ids)| PreparedMetarTile {
                z,
                x,
                y,
                station_ids,
            })
            .collect(),
    }
}

fn prepare_pirep_live_feed(payload: PirepProductPayload) -> PreparedPirepLiveFeed {
    let mut records = payload.pireps_by_id.values().collect::<Vec<_>>();
    records.sort_by(|left, right| left.id.cmp(&right.id));
    let mut tiles = std::collections::BTreeMap::<(u32, u32, u32), Vec<String>>::new();
    for zoom in [5_u32, 6, 7] {
        for record in &records {
            let Some((x, y)) = live_feed_metar_tile_xy(record.latitude, record.longitude, zoom)
            else {
                continue;
            };
            tiles
                .entry((zoom, x, y))
                .or_default()
                .push(record.id.clone());
        }
    }
    PreparedPirepLiveFeed {
        schema_version: 1,
        payload,
        tiles: tiles
            .into_iter()
            .map(|((z, x, y), pirep_ids)| PreparedPirepTile { z, x, y, pirep_ids })
            .collect(),
    }
}

fn supports_record_delta(product: &str) -> bool {
    product_contracts::live_feed_product_policy(product)
        .is_some_and(|policy| policy.delta == product_contracts::LiveFeedDeltaPolicy::RecordJson)
}

fn supports_live_feed_delta(product: &str) -> bool {
    product_contracts::live_feed_product_policy(product).is_some_and(|policy| {
        matches!(
            policy.delta,
            product_contracts::LiveFeedDeltaPolicy::RecordJson
                | product_contracts::LiveFeedDeltaPolicy::Notam
        )
    })
}

fn product_supports_prepared_record_delta_without_raw_state(product: &str) -> bool {
    supports_prepared_live_feed(product) && supports_record_delta(product)
}

fn record_delta_schema(product: &str) -> Option<(String, Option<String>, Option<String>)> {
    crate::live_feed_product_registry().record_json_delta_schema(product)
}

fn apply_live_feed_record_delta(
    from_state: &Value,
    delta: &LiveFeedRecordDelta,
) -> AppResult<Value> {
    let (records_key, record_id_key, count_key) =
        record_delta_schema(&delta.product).ok_or_else(|| {
            invalid_live_feed(format!(
                "unsupported live feed delta product: {}",
                delta.product
            ))
        })?;
    crate::live_feed_cache::apply_record_json_delta(
        &records_key,
        record_id_key.as_deref(),
        count_key.as_deref(),
        from_state,
        delta,
    )
}

fn live_feed_metar_tile_xy(lat: f64, lon: f64, zoom: u32) -> Option<(u32, u32)> {
    if !lat.is_finite() || !lon.is_finite() {
        return None;
    }
    let scale = 2_u32.checked_pow(zoom)?;
    let scale_f64 = scale as f64;
    let x = (((lon + 180.0) / 360.0) * scale_f64).floor();
    let clamped_lat = lat.clamp(-85.0511287798066, 85.0511287798066);
    let y = ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI) / 2.0
        * scale_f64)
        .floor();
    Some((
        live_feed_positive_mod_i64(x as i64, scale as i64) as u32,
        (y as i64).clamp(0, scale as i64 - 1) as u32,
    ))
}

fn live_feed_positive_mod_i64(value: i64, modulus: i64) -> i64 {
    ((value % modulus) + modulus) % modulus
}

fn invalid_live_feed_json(err: impl std::fmt::Display) -> AppError {
    invalid_live_feed(err.to_string())
}

fn invalid_live_feed(message: String) -> AppError {
    AppError {
        kind: AppErrorKind::InvalidManifest,
        message,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    const TEST_LIVE_FEED_ROOT: &str = "http://live.test";

    fn live_feeds_state() -> LiveFeedsState {
        let mut state = LiveFeedsState::default();
        state.set_source_root_url(TEST_LIVE_FEED_ROOT).unwrap();
        state
    }

    fn test_live_feed_url(path: &str) -> String {
        live_feed_resource_url(TEST_LIVE_FEED_ROOT, path)
    }

    fn metar_state(version: &str, stations: &[(&str, &str)]) -> Value {
        let mut metars_by_station = serde_json::Map::new();
        for (station_id, raw_text) in stations {
            metars_by_station.insert(
                (*station_id).to_string(),
                serde_json::json!({
                    "raw_text": raw_text,
                    "station_id": station_id,
                    "latitude": 47.0,
                    "longitude": -122.0,
                    "flight_category": "VFR"
                }),
            );
        }
        serde_json::json!({
            "schema_version": 3,
            "version_label": version,
            "generated_at_utc": "2026-05-18T20:00:00Z",
            "observed_at_utc": "2026-05-18T20:00:00Z",
            "metar_count": metars_by_station.len(),
            "metars_by_station": metars_by_station
        })
    }

    fn taf_state(version: &str, stations: &[(&str, &str)]) -> Value {
        let mut tafs_by_station = serde_json::Map::new();
        for (station_id, raw_text) in stations {
            tafs_by_station.insert(
                (*station_id).to_string(),
                serde_json::json!({
                    "raw_text": raw_text,
                    "station_id": station_id,
                    "issued_at_utc": "2026-05-18T20:00:00Z",
                    "latitude": 47.0,
                    "longitude": -122.0
                }),
            );
        }
        serde_json::json!({
            "schema_version": 1,
            "version_label": version,
            "generated_at_utc": "2026-05-18T20:00:00Z",
            "taf_count": tafs_by_station.len(),
            "tafs_by_station": tafs_by_station
        })
    }

    fn metar_delta(from: &Value, to: &Value) -> Value {
        record_delta("metars", "metars_by_station", from, to)
    }

    fn taf_delta(from: &Value, to: &Value) -> Value {
        record_delta("tafs", "tafs_by_station", from, to)
    }

    fn record_delta(product: &str, records_key: &str, from: &Value, to: &Value) -> Value {
        let from_version = from["version_label"].as_str().unwrap();
        let to_version = to["version_label"].as_str().unwrap();
        let from_records = from[records_key].as_object().unwrap();
        let to_records = to[records_key].as_object().unwrap();
        let changed = to_records
            .iter()
            .filter(|(station_id, record)| from_records.get(*station_id) != Some(*record))
            .map(|(station_id, record)| (station_id.clone(), record.clone()))
            .collect::<serde_json::Map<_, _>>();
        let removed = from_records
            .keys()
            .filter(|station_id| !to_records.contains_key(*station_id))
            .cloned()
            .collect::<Vec<_>>();
        serde_json::json!({
            "schema_version": LIVE_FEEDS_SCHEMA_VERSION,
            "product": product,
            "from_version": from_version,
            "to_version": to_version,
            "top_level_changed": {},
            "top_level_removed": [],
            "changed": changed,
            "removed": removed
        })
    }

    fn test_notam_record(id: &str, airport_id: &str, text: &str) -> notam_state::NotamRecord {
        notam_state::NotamRecord {
            id: id.to_string(),
            airport_id: Some(airport_id.to_string()),
            airport_effects: [product_contracts::AirportNotamEffect::RoutineAdvisory]
                .into_iter()
                .collect(),
            procedure_rendezvous_keys: Default::default(),
            notam_keyword: Some("AD".to_string()),
            effective_start_utc: Some("2026-07-23T00:00:00Z".to_string()),
            effective_end_utc: None,
            text: Some(text.to_string()),
            local_text: None,
            icao_text: None,
        }
    }

    fn test_notam_delta_ref(from: &str, to: &str) -> LiveFeedDeltaRef {
        LiveFeedDeltaRef {
            kind: Some("notam_ordered_delta_xz".to_string()),
            from_version: from.to_string(),
            from_state_sha256: from.to_string(),
            to_version: to.to_string(),
            to_state_sha256: to.to_string(),
            url: format!("deltas/notams/{from}__{to}.json.xz"),
            bytes: 1,
            blob_sha256: "a".repeat(64),
            mutation_count: Some(1),
        }
    }

    #[test]
    fn shared_core_owns_background_preparation_routing() {
        for policy in product_contracts::LIVE_FEED_PRODUCT_POLICIES {
            assert_eq!(
                should_prepare_live_feed_resource(&format!(
                    "live_feeds/state/{}/v1",
                    policy.product_id
                )),
                policy.preparation.is_prepared(),
                "preparation routing for {}",
                policy.product_id
            );
        }
        assert!(should_prepare_live_feed_resource(
            "live_feeds/delta/notams/v1/v2"
        ));
        assert!(!should_prepare_live_feed_resource(
            "live_feeds/state/obstacles/v1"
        ));
    }

    #[test]
    fn notam_manifest_accepts_checkpoint_inside_retained_suffix() {
        let latest = test_notam_delta_ref("s1", "s2");
        let manifest = VersionManifest {
            schema_version: LIVE_FEEDS_SCHEMA_VERSION,
            product: "notams".to_string(),
            version: "s2".to_string(),
            previous: None,
            temporal_coverage: None,
            install_state: None,
            install_profiles: std::collections::BTreeMap::new(),
            delta_from_previous: Some(latest.clone()),
            recent_deltas: vec![test_notam_delta_ref("s0", "s1"), latest],
            state: LiveFeedPayloadRef {
                kind: Some("notam_checkpoint_xz".to_string()),
                url: "states/notams/s1.json.xz".to_string(),
                bytes: 1,
                blob_sha256: "b".repeat(64),
                state_sha256: "s1".to_string(),
            },
        };
        validate_notam_version_manifest(&manifest, "s2").unwrap();
    }

    #[test]
    fn prepared_live_feed_states_decode_xz_and_encode_typed_postcard_payloads() {
        let states = vec![
            ("metars", metar_state("v1", &[("KSEA", "METAR KSEA")])),
            ("tafs", taf_state("v1", &[("KSEA", "TAF KSEA")])),
            (
                "tfrs",
                serde_json::json!({
                    "schema_version": 2,
                    "version_label": "v1",
                    "generated_at_utc": "2026-05-18T20:00:00Z",
                    "notam_count": 1,
                    "area_group_count": 1,
                    "areas": [{
                        "area_id": "6/7042:0123456789abcdef",
                        "notam_id": "6/7042",
                        "area_index": 0,
                        "schedule_fragments": [],
                        "upper_limit": { "value_text": "5000", "unit": "FT" },
                        "lower_limit": { "value_text": "SFC", "unit": "" },
                        "polygon": [
                            { "lat": 47.0, "lon": -122.0 },
                            { "lat": 47.1, "lon": -122.0 },
                            { "lat": 47.0, "lon": -121.9 }
                        ],
                        "summary_text": "TFR:: fixture"
                    }]
                }),
            ),
        ];

        for (product, state) in states {
            let resource_id = format!("live_feeds/state/{product}/v1");
            let encoded =
                nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&state).unwrap())
                    .unwrap();
            let (decoded_state, prepared_bytes) =
                prepare_live_feed_state_resource(&resource_id, &encoded).unwrap();
            let envelope = decode_prepared_live_feed(&prepared_bytes).unwrap();

            assert_eq!(decoded_state, state);
            assert_eq!(envelope.resource_id, resource_id);
            assert_eq!(envelope.product, product);
            assert_eq!(envelope.version, "v1");
            assert_eq!(envelope.payload.product(), product);
            assert_eq!(envelope.payload.version_label(), "v1");
            assert_eq!(
                envelope.state_sha256,
                canonical_json_sha256(&state).unwrap()
            );
        }
    }

    #[test]
    fn prepared_live_feed_delta_carries_early_indexed_target_state() {
        let first = taf_state("v1", &[("KSEA", "TAF KSEA OLD")]);
        let second = taf_state("v2", &[("KSEA", "TAF KSEA NEW"), ("KPAE", "TAF KPAE")]);
        let delta = taf_delta(&first, &second);
        let encoded_delta =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&delta).unwrap())
                .unwrap();

        let (next_state, prepared_bytes) =
            prepare_live_feed_delta_resource("live_feeds/delta/tafs/v1/v2", &first, &encoded_delta)
                .unwrap();
        let envelope = decode_prepared_live_feed(&prepared_bytes).unwrap();

        assert_eq!(next_state, second);
        assert_eq!(envelope.product, "tafs");
        assert_eq!(envelope.from_version.as_deref(), Some("v1"));
        assert_eq!(envelope.version, "v2");
        let PreparedLiveFeedPayload::Tafs(payload) = envelope.payload else {
            panic!("expected prepared TAF payload");
        };
        assert_eq!(payload.tafs_by_station.len(), 2);
        assert_eq!(payload.tafs_by_station["KSEA"].raw_text, "TAF KSEA NEW");
    }

    #[test]
    fn notam_checkpoint_then_one_record_delta_stays_incremental_end_to_end() {
        let mut producer_state = notam_state::NotamState::empty();
        for index in 0..256 {
            producer_state
                .apply_mutation(
                    notam_state::NotamMutation::Upsert {
                        record: test_notam_record(
                            &format!("N{index:04}"),
                            "KSEA",
                            &format!("initial text {index}"),
                        ),
                    },
                    &mut notam_state::NotamApplyWork::default(),
                )
                .unwrap();
        }
        let checkpoint = producer_state.checkpoint();
        let checkpoint_id = checkpoint.state_id.clone();
        let mutation = notam_state::NotamMutation::Upsert {
            record: test_notam_record("N0042", "KPAE", "one changed record"),
        };
        producer_state
            .apply_mutation(
                mutation.clone(),
                &mut notam_state::NotamApplyWork::default(),
            )
            .unwrap();
        let head_id = producer_state.state_id().to_string();
        let delta = notam_state::NotamDelta::new(
            checkpoint_id.clone(),
            head_id.clone(),
            producer_state.counters(),
            vec![mutation],
        );
        let checkpoint_bytes =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&checkpoint).unwrap())
                .unwrap();
        let delta_bytes =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&delta).unwrap())
                .unwrap();
        let checkpoint_blob_sha256 = sha256_hex(&checkpoint_bytes);
        let delta_blob_sha256 = sha256_hex(&delta_bytes);

        let mut feeds = live_feeds_state();
        feeds
            .ingest_catalog_bytes(
                &serde_json::to_vec(&serde_json::json!({
                    "schema_version": LIVE_FEEDS_SCHEMA_VERSION,
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {
                        "notams": {
                            "current": head_id,
                            "version_manifest_url": format!("versions/notams/{head_id}.json"),
                            "state_url": format!("states/notams/{checkpoint_id}.json.xz"),
                            "state_sha256": head_id
                        }
                    }
                }))
                .unwrap(),
            )
            .unwrap();
        let delta_url = format!("deltas/notams/{checkpoint_id}__{head_id}.json.xz");
        let delta_ref = serde_json::json!({
            "kind": "notam_ordered_delta_xz",
            "from_version": checkpoint_id,
            "from_state_sha256": checkpoint_id,
            "to_version": head_id,
            "to_state_sha256": head_id,
            "url": delta_url,
            "bytes": delta_bytes.len(),
            "blob_sha256": delta_blob_sha256,
            "mutation_count": 1
        });
        feeds
            .ingest_resource(
                &format!("live_feeds/version/notams/{head_id}"),
                &serde_json::to_vec(&serde_json::json!({
                    "schema_version": LIVE_FEEDS_SCHEMA_VERSION,
                    "product": "notams",
                    "version": head_id,
                    "previous": checkpoint_id,
                    "state": {
                        "kind": "notam_checkpoint_xz",
                        "url": format!("states/notams/{checkpoint_id}.json.xz"),
                        "bytes": checkpoint_bytes.len(),
                        "blob_sha256": checkpoint_blob_sha256,
                        "state_sha256": checkpoint_id
                    },
                    "delta_from_previous": delta_ref,
                    "recent_deltas": [delta_ref]
                }))
                .unwrap(),
            )
            .unwrap();

        let HadOperationOutcome::NeedResources { resources } = feeds.sync_outcome() else {
            panic!("cold NOTAM client should request checkpoint");
        };
        assert_eq!(
            resources[0].id,
            format!("live_feeds/state/notams/{checkpoint_id}")
        );
        let mut notam_preparer = NotamProjectionPreparer::default();
        let mut checkpoint_prepare_work = BackgroundNotamWork::default();
        let checkpoint_postcard = prepare_notam_live_feed_state_resource_with_work(
            &resources[0].id,
            &checkpoint_bytes,
            &mut notam_preparer,
            &mut checkpoint_prepare_work,
            &mut NotamApplyWork::default(),
        )
        .unwrap();
        let checkpoint_envelope = decode_prepared_live_feed(&checkpoint_postcard).unwrap();
        feeds
            .ingest_prepared_live_feed(&resources[0].id, &checkpoint_envelope)
            .unwrap();

        let HadOperationOutcome::NeedResources { resources } = feeds.sync_outcome() else {
            panic!("checkpoint client should request the next NOTAM delta");
        };
        assert_eq!(
            resources[0].id,
            format!("live_feeds/delta/notams/{checkpoint_id}/{head_id}")
        );
        let mut delta_prepare_work = BackgroundNotamWork::default();
        let delta_postcard = prepare_notam_live_feed_delta_resource_with_work(
            &resources[0].id,
            &delta_bytes,
            &mut notam_preparer,
            &mut delta_prepare_work,
            &mut NotamApplyWork::default(),
        )
        .unwrap();
        assert!(delta_postcard.len() * 10 < checkpoint_postcard.len());
        assert_eq!(checkpoint_prepare_work.records_decoded, 256);
        assert_eq!(delta_prepare_work.records_decoded, 1);
        assert_eq!(
            delta_prepare_work.compressed_bytes_read,
            delta_bytes.len() as u64
        );
        assert_eq!(
            delta_prepare_work.postcard_bytes_written,
            delta_postcard.len() as u64
        );
        let delta_envelope = decode_prepared_live_feed(&delta_postcard).unwrap();
        feeds
            .ingest_prepared_live_feed(&resources[0].id, &delta_envelope)
            .unwrap();
        assert_eq!(
            feeds.product_loaded_version("notams"),
            Some(head_id.as_str())
        );

        let PreparedLiveFeedPayload::Notams(PreparedNotamPayload::InstallDisplayCheckpoint(
            checkpoint,
        )) = checkpoint_envelope.payload
        else {
            panic!("expected prepared NOTAM checkpoint");
        };
        assert_eq!(checkpoint.records.len(), 256);
        let mut client = crate::NotamDisplayIndex::from_projection_checkpoint(checkpoint).unwrap();
        let PreparedLiveFeedPayload::Notams(PreparedNotamPayload::ApplyDisplayDelta(delta)) =
            delta_envelope.payload
        else {
            panic!("expected prepared NOTAM delta");
        };
        assert_eq!(delta.mutations.len(), 1);
        client.apply_projection_delta(delta).unwrap();
        assert_eq!(client.state_id(), producer_state.state_id());
        assert_eq!(notam_preparer.state_id(), Some(producer_state.state_id()));
    }

    #[test]
    fn prepared_record_delta_advances_core_without_retaining_raw_state() {
        let first = taf_state("v1", &[("KSEA", "TAF KSEA OLD")]);
        let second = taf_state("v2", &[("KSEA", "TAF KSEA NEW")]);
        let delta = taf_delta(&first, &second);
        let encoded_first =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&first).unwrap())
                .unwrap();
        let encoded_delta =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&delta).unwrap())
                .unwrap();
        let (_, first_prepared) =
            prepare_live_feed_state_resource("live_feeds/state/tafs/v1", &encoded_first).unwrap();
        let first_envelope = decode_prepared_live_feed(&first_prepared).unwrap();
        let (_, delta_prepared) =
            prepare_live_feed_delta_resource("live_feeds/delta/tafs/v1/v2", &first, &encoded_delta)
                .unwrap();
        let delta_envelope = decode_prepared_live_feed(&delta_prepared).unwrap();
        let mut state = live_feeds_state();
        state
            .ingest_catalog_bytes(
                format!(
                    r#"{{
                        "schema_version": {LIVE_FEEDS_SCHEMA_VERSION},
                        "generated_at_utc": "2026-08-04T00:00:00Z",
                        "products": {{
                            "tafs": {{
                                "current": "v1",
                                "version_manifest_url": "versions/tafs/v1.json",
                                "state_url": "states/tafs/v1.json.xz",
                                "state_sha256": "{}"
                            }}
                        }}
                    }}"#,
                    canonical_json_sha256(&first).unwrap()
                )
                .as_bytes(),
            )
            .unwrap();
        state
            .ingest_prepared_live_feed("live_feeds/state/tafs/v1", &first_envelope)
            .unwrap();
        assert_eq!(state.product_loaded_version("tafs"), Some("v1"));
        assert_eq!(state.product_state_manifest("tafs"), None);

        state
            .ingest_sse_event(LiveFeedSseEvent {
                id: Some("tafs:v2".to_string()),
                event: Some("live-feed-current".to_string()),
                data: format!(
                    r#"{{
                        "schema_version": {LIVE_FEEDS_SCHEMA_VERSION},
                        "product": "tafs",
                        "version": "v2",
                        "version_manifest_url": "versions/tafs/v2.json",
                        "state_url": "states/tafs/v2.json.xz",
                        "state_sha256": "{}"
                    }}"#,
                    canonical_json_sha256(&second).unwrap()
                ),
            })
            .unwrap();
        state
            .ingest_resource(
                "live_feeds/version/tafs/v2",
                format!(
                    r#"{{
                        "schema_version": {LIVE_FEEDS_SCHEMA_VERSION},
                        "product": "tafs",
                        "version": "v2",
                        "state": {{
                            "kind": "json_xz",
                            "url": "states/tafs/v2.json.xz",
                            "bytes": 1,
                            "blob_sha256": "unused",
                            "state_sha256": "{}"
                        }},
                        "delta_from_previous": {{
                            "kind": "record_json_delta_xz",
                            "from_version": "v1",
                            "from_state_sha256": "{}",
                            "to_version": "v2",
                            "to_state_sha256": "{}",
                            "url": "deltas/tafs/v1__v2.json.xz",
                            "bytes": 1,
                            "blob_sha256": "{}"
                        }}
                    }}"#,
                    canonical_json_sha256(&second).unwrap(),
                    canonical_json_sha256(&first).unwrap(),
                    canonical_json_sha256(&second).unwrap(),
                    sha256_hex(&encoded_delta),
                )
                .as_bytes(),
            )
            .unwrap();
        state
            .ingest_prepared_live_feed("live_feeds/delta/tafs/v1/v2", &delta_envelope)
            .unwrap();

        assert_eq!(state.product_loaded_version("tafs"), Some("v2"));
        assert_eq!(state.product_state_manifest("tafs"), None);
    }

    fn assert_record_sync_installs_product(
        product: &str,
        v1: Value,
        v2: Value,
        delta: Value,
        full_advertised_bytes: u64,
        delta_advertised_bytes: u64,
    ) {
        let v1_bytes =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&v1).unwrap()).unwrap();
        let v2_bytes =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&v2).unwrap()).unwrap();
        let delta_bytes =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&delta).unwrap())
                .unwrap();
        let mut state = live_feeds_state();
        state
            .ingest_catalog_bytes(
                format!(
                    r#"{{
                    "schema_version": {LIVE_FEEDS_SCHEMA_VERSION},
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {{
                        "{product}": {{
                            "current": "v1",
                            "version_manifest_url": "versions/{product}/v1.json",
                            "state_url": "states/{product}/v1.json",
                            "state_sha256": "{}"
                        }}
                    }}
                }}"#,
                    canonical_json_sha256(&v1).unwrap()
                )
                .as_bytes(),
            )
            .unwrap();
        state
            .ingest_resource(
                &format!("live_feeds/version/{product}/v1"),
                format!(
                    r#"{{
                    "schema_version": {LIVE_FEEDS_SCHEMA_VERSION},
                    "product": "{product}",
                    "version": "v1",
                    "state": {{
                        "kind": "json_xz",
                        "url": "states/{product}/v1.json.xz",
                        "bytes": 1,
                        "blob_sha256": "unused",
                        "state_sha256": "{}"
                    }}
                }}"#,
                    canonical_json_sha256(&v1).unwrap()
                )
                .as_bytes(),
            )
            .unwrap();
        state
            .ingest_resource(&format!("live_feeds/state/{product}/v1"), &v1_bytes)
            .unwrap();

        state
            .ingest_sse_event(LiveFeedSseEvent {
                id: Some(format!("{product}:v2")),
                event: Some("live-feed-current".to_string()),
                data: format!(
                    r#"{{
                    "schema_version": {LIVE_FEEDS_SCHEMA_VERSION},
                    "product": "{product}",
                    "version": "v2",
                    "version_manifest_url": "versions/{product}/v2.json",
                    "state_url": "states/{product}/v2.json.xz",
                    "state_sha256": "{}"
                }}"#,
                    canonical_json_sha256(&v2).unwrap()
                ),
            })
            .unwrap();
        let HadOperationOutcome::NeedResources { resources } = state.sync_outcome() else {
            panic!("expected version request");
        };
        assert_eq!(resources[0].id, format!("live_feeds/version/{product}/v2"));

        state
            .ingest_resource(
                &format!("live_feeds/version/{product}/v2"),
                format!(
                    r#"{{
                    "schema_version": {LIVE_FEEDS_SCHEMA_VERSION},
                    "product": "{product}",
                    "version": "v2",
                    "previous": "v1",
                    "state": {{
                        "kind": "json_xz",
                        "url": "states/{product}/v2.json.xz",
                        "bytes": {full_advertised_bytes},
                        "blob_sha256": "unused",
                        "state_sha256": "{}"
                    }},
                    "delta_from_previous": {{
                        "kind": "record_json_delta_xz",
                        "from_version": "v1",
                        "from_state_sha256": "{}",
                        "to_version": "v2",
                        "to_state_sha256": "{}",
                        "url": "deltas/{product}/v1__v2.json.xz",
                        "bytes": {delta_advertised_bytes},
                        "blob_sha256": "{}"
                    }}
                }}"#,
                    canonical_json_sha256(&v2).unwrap(),
                    canonical_json_sha256(&v1).unwrap(),
                    canonical_json_sha256(&v2).unwrap(),
                    sha256_hex(&delta_bytes),
                )
                .as_bytes(),
            )
            .unwrap();
        let HadOperationOutcome::NeedResources { resources } = state.sync_outcome() else {
            panic!("expected update request");
        };
        let use_delta = delta_advertised_bytes <= full_advertised_bytes;
        let resource_id = if use_delta {
            format!("live_feeds/delta/{product}/v1/v2")
        } else {
            format!("live_feeds/state/{product}/v2")
        };
        let source_path = if use_delta {
            format!("deltas/{product}/v1__v2.json.xz")
        } else {
            format!("states/{product}/v2.json.xz")
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, resource_id);
        assert_eq!(
            resources[0].source,
            crate::CoreResourceSource::PublicUrl {
                url: test_live_feed_url(&source_path),
            }
        );

        state
            .ingest_resource(
                &resource_id,
                if use_delta { &delta_bytes } else { &v2_bytes },
            )
            .unwrap();
        assert_eq!(state.product_state_manifest(product), Some(&v2));
        let outcome = state.sync_outcome_with_invalidations();
        let HadOperationOutcome::Complete { invalidations, .. } = outcome else {
            panic!("expected complete");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        assert!(invalidations.contains(&UiInvalidation::MapOverlay));
        assert!(invalidations.contains(&UiInvalidation::DebugPanel));
    }

    #[test]
    fn canonical_json_hash_is_independent_of_object_insertion_order() {
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
            canonical_json_sha256(&Value::Object(left)).unwrap(),
            canonical_json_sha256(&Value::Object(right)).unwrap()
        );
    }

    #[test]
    fn sync_waits_for_stream_catalog_without_requesting_a_resource() {
        let state = live_feeds_state();
        assert!(matches!(
            state.sync_outcome(),
            HadOperationOutcome::Complete { .. }
        ));
    }

    #[test]
    fn stream_catalog_drives_version_then_state_requests() {
        let mut state = live_feeds_state();
        state
            .ingest_sse_event(LiveFeedSseEvent {
                id: Some("catalog:2026-08-04T00:00:00Z".to_string()),
                event: Some(CATALOG_EVENT_NAME.to_string()),
                data: r#"{
                    "schema_version": 3,
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {
                        "nexrad": {
                            "current": "v1",
                            "version_manifest_url": "versions/nexrad/v1.json",
                            "state_url": "states/nexrad/v1/manifest.json",
                            "state_sha256": "unused"
                        }
                    }
                }"#
                .to_string(),
            })
            .unwrap();
        let HadOperationOutcome::NeedResources { resources } = state.sync_outcome() else {
            panic!("expected version request");
        };
        assert_eq!(resources[0].id, "live_feeds/version/nexrad/v1");
        assert_eq!(
            resources[0].source,
            crate::CoreResourceSource::PublicUrl {
                url: test_live_feed_url("versions/nexrad/v1.json"),
            }
        );
    }

    #[test]
    fn stream_catalog_history_is_bounded_and_loaded_separately() {
        let mut state = live_feeds_state();
        let history_manifest = serde_json::json!({
            "product": "nexrad",
            "state_id": "nexrad-v1",
            "observed_at_utc": "2026-05-18T20:00:00Z",
            "source_grid": {"geo_transform": [-123.0, 0.01, 0.0, 48.0, 0.0, -0.01]},
            "levels": [],
            "tile_size": 256,
            "tile_path_template": "tiles/res{res}/{x}/{y}.png"
        });
        let history_sha = canonical_json_sha256(&history_manifest).expect("history hash");
        state
            .ingest_catalog_bytes(
                format!(
                    r#"{{
                    "schema_version": 3,
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {{
                        "nexrad": {{
                            "current": "v2",
                            "version_manifest_url": "versions/nexrad/v2.json",
                            "state_url": "states/nexrad/v2/manifest.json",
                            "state_sha256": "current-unused",
                            "history": [{{
                                "version": "v1",
                                "version_manifest_url": "versions/nexrad/v1.json",
                                "state_url": "states/nexrad/v1/manifest.json",
                                "state_sha256": "{history_sha}"
                            }}]
                        }}
                    }}
                }}"#
                )
                .as_bytes(),
            )
            .expect("ingest stream catalog");

        let resources = state.missing_history_resources_for_product_at_epoch_ms("nexrad", 0);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "live_feeds/version/nexrad/v1");
        assert!(resources[0].optional);

        state
            .ingest_resource(
                "live_feeds/version/nexrad/v1",
                format!(
                    r#"{{
                    "schema_version": 3,
                    "product": "nexrad",
                    "version": "v1",
                    "state": {{
                        "kind": "json",
                        "url": "states/nexrad/v1/manifest.json",
                        "bytes": 1,
                        "blob_sha256": "history-unused",
                        "state_sha256": "{history_sha}"
                    }}
                }}"#
                )
                .as_bytes(),
            )
            .expect("ingest history version");
        let resources = state.missing_history_resources_for_product_at_epoch_ms("nexrad", 0);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "live_feeds/state/nexrad/v1");
        assert!(resources[0].optional);

        state
            .ingest_resource(
                "live_feeds/state/nexrad/v1",
                &serde_json::to_vec(&history_manifest).expect("history json"),
            )
            .expect("ingest history state");
        let loaded = state.product_loaded_state_manifests("nexrad");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].version, "v1");
        assert_eq!(loaded[0].manifest["state_id"], "nexrad-v1");
    }

    #[test]
    fn stream_catalog_rejects_unbounded_history() {
        let mut state = live_feeds_state();
        let history = (0..=LIVE_FEED_HISTORY_MAX_ENTRIES)
            .map(|index| {
                format!(
                    r#"{{
                    "version": "v{index}",
                    "version_manifest_url": "versions/nexrad/v{index}.json"
                }}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let err = state
            .ingest_catalog_bytes(
                format!(
                    r#"{{
                    "schema_version": 3,
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {{
                        "nexrad": {{
                            "current": "v-current",
                            "version_manifest_url": "versions/nexrad/v-current.json",
                            "state_url": "states/nexrad/v-current/manifest.json",
                            "state_sha256": "current-unused",
                            "history": [{history}]
                        }}
                    }}
                }}"#
                )
                .as_bytes(),
            )
            .expect_err("unbounded history should fail");
        assert!(err.message.contains("max is"));
    }

    #[test]
    fn stream_catalog_is_authoritative_for_product_membership() {
        let mut state = live_feeds_state();
        state
            .ingest_catalog_bytes(
                br#"{
                    "schema_version": 3,
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {
                        "metars": {
                            "current": "v1",
                            "version_manifest_url": "versions/metars/v1.json",
                            "state_url": "states/metars/v1.json",
                            "state_sha256": "unused"
                        },
                        "obstacles": {
                            "current": "v1",
                            "version_manifest_url": "versions/obstacles/v1.json",
                            "state_url": "states/obstacles/v1/manifest.json",
                            "state_sha256": "had-hash"
                        }
                    }
                }"#,
            )
            .unwrap();
        assert!(state.has_product_current_version("metars"));
        assert!(state.has_product_current_version("obstacles"));

        state
            .ingest_catalog_bytes(
                br#"{
                    "schema_version": 3,
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {
                        "metars": {
                            "current": "v2",
                            "version_manifest_url": "versions/metars/v2.json",
                            "state_url": "states/metars/v2.json",
                            "state_sha256": "unused"
                        }
                    }
                }"#,
            )
            .unwrap();

        assert!(state.has_product_current_version("metars"));
        assert!(!state.has_product_current_version("obstacles"));
    }

    #[test]
    fn invalid_stream_catalog_does_not_partially_replace_the_previous_catalog() {
        let mut state = live_feeds_state();
        state
            .ingest_catalog_bytes(
                br#"{
                    "schema_version": 3,
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {
                        "metars": {
                            "current": "v1",
                            "version_manifest_url": "versions/metars/v1.json",
                            "state_url": "states/metars/v1.json",
                            "state_sha256": "v1"
                        }
                    }
                }"#,
            )
            .unwrap();

        let error = state
            .ingest_sse_event(LiveFeedSseEvent {
                id: Some("catalog:bad".to_string()),
                event: Some(CATALOG_EVENT_NAME.to_string()),
                data: r#"{
                    "schema_version": 3,
                    "generated_at_utc": "2026-08-04T00:01:00Z",
                    "products": {
                        "metars": {
                            "current": "v2",
                            "version_manifest_url": "versions/metars/v2.json",
                            "state_url": "states/metars/v2.json",
                            "state_sha256": "v2"
                        },
                        "obstacles": {
                            "current": "bad",
                            "version_manifest_url": "https://wrong.example/bad.json",
                            "state_url": "states/obstacles/bad.json",
                            "state_sha256": "bad"
                        }
                    }
                }"#
                .to_string(),
            })
            .expect_err("invalid catalog");

        assert!(error.message.contains("relative"));
        assert_eq!(state.current_product_version("metars"), Some("v1"));
        assert!(!state.has_product_current_version("obstacles"));
    }

    #[test]
    fn reconnect_catalog_supersedes_old_queued_events_before_later_updates() {
        let mut state = LiveFeedsState {
            catalog_loaded: true,
            ..live_feeds_state()
        };
        let event = |version: &str| LiveFeedSseEvent {
            id: Some(format!("metars:{version}")),
            event: Some(PRODUCT_EVENT_NAME.to_string()),
            data: serde_json::json!({
                "schema_version": LIVE_FEEDS_SCHEMA_VERSION,
                "product": "metars",
                "version": version,
                "version_manifest_url": format!("versions/metars/{version}.json"),
                "state_url": format!("states/metars/{version}.json"),
                "state_sha256": version,
            })
            .to_string(),
        };
        let catalog = LiveFeedSseEvent {
            id: Some("catalog:v3".to_string()),
            event: Some(CATALOG_EVENT_NAME.to_string()),
            data: serde_json::json!({
                "schema_version": LIVE_FEEDS_SCHEMA_VERSION,
                "generated_at_utc": "2026-08-04T00:03:00Z",
                "products": {
                    "metars": {
                        "current": "v3",
                        "version_manifest_url": "versions/metars/v3.json",
                        "state_url": "states/metars/v3.json",
                        "state_sha256": "v3",
                    }
                },
            })
            .to_string(),
        };

        state
            .ingest_sse_events([event("v2"), catalog, event("v4")])
            .unwrap();

        assert_eq!(state.current_product_version("metars"), Some("v4"));
    }

    #[test]
    fn durable_loaded_product_cannot_override_current_state_hash() {
        let mut state = live_feeds_state();
        state
            .ingest_catalog_bytes(
                br#"{
                    "schema_version": 3,
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {
                        "tafs": {
                            "current": "v1",
                            "version_manifest_url": "versions/tafs/v1.json",
                            "state_url": "states/tafs/v1.json",
                            "state_sha256": "expected"
                        }
                    }
                }"#,
            )
            .expect("stream catalog");

        state.mark_durable_product_loaded(
            "tafs".to_string(),
            "v1".to_string(),
            "wrong".to_string(),
            None,
            Some(serde_json::json!({"version_label": "v1"})),
        );

        assert_eq!(state.product_loaded_version("tafs"), None);
        assert_eq!(state.product_state_manifest("tafs"), None);
    }

    #[test]
    fn loaded_current_without_overlay_products_still_invalidates_overlays() {
        let mut state = live_feeds_state();
        state
            .ingest_catalog_bytes(
                br#"{
                    "schema_version": 3,
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {}
                }"#,
            )
            .unwrap();

        let HadOperationOutcome::Complete { invalidations, .. } =
            state.sync_outcome_with_invalidations()
        else {
            panic!("expected complete live-feed sync");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        assert!(invalidations.contains(&UiInvalidation::MapOverlay));
        assert!(invalidations.contains(&UiInvalidation::NexradOverlay));
        assert!(invalidations.contains(&UiInvalidation::DebugPanel));
    }

    #[test]
    fn tfr_live_feed_uses_full_state_and_invalidates_overlay() {
        let tfrs = serde_json::json!({
            "schema_version": 1,
            "version_label": "v1",
            "notam_count": 1,
            "area_group_count": 0,
            "areas": []
        });
        let state_sha256 = canonical_json_sha256(&tfrs).unwrap();
        let mut state = live_feeds_state();
        state
            .ingest_catalog_bytes(
                format!(
                    r#"{{
                    "schema_version": {LIVE_FEEDS_SCHEMA_VERSION},
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {{
                        "tfrs": {{
                            "current": "v1",
                            "version_manifest_url": "versions/tfrs/v1.json",
                            "state_url": "states/tfrs/v1.json",
                            "state_sha256": "{state_sha256}"
                        }}
                    }}
                }}"#
                )
                .as_bytes(),
            )
            .unwrap();
        let HadOperationOutcome::NeedResources { resources } = state.sync_outcome() else {
            panic!("expected version request");
        };
        assert_eq!(resources[0].id, "live_feeds/version/tfrs/v1");

        state
            .ingest_resource(
                "live_feeds/version/tfrs/v1",
                format!(
                    r#"{{
                    "schema_version": {LIVE_FEEDS_SCHEMA_VERSION},
                    "product": "tfrs",
                    "version": "v1",
                    "state": {{
                        "kind": "json_xz",
                        "url": "states/tfrs/v1.json",
                        "bytes": 1,
                        "blob_sha256": "unused",
                        "state_sha256": "{state_sha256}"
                    }}
                }}"#
                )
                .as_bytes(),
            )
            .unwrap();
        let HadOperationOutcome::NeedResources { resources } = state.sync_outcome() else {
            panic!("expected state request");
        };
        assert_eq!(resources[0].id, "live_feeds/state/tfrs/v1");

        state
            .ingest_resource(
                "live_feeds/state/tfrs/v1",
                &nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&tfrs).unwrap())
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(state.product_state_manifest("tfrs"), Some(&tfrs));
        let outcome = state.sync_outcome_with_invalidations();
        let HadOperationOutcome::Complete { invalidations, .. } = outcome else {
            panic!("expected complete");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        assert!(invalidations.contains(&UiInvalidation::MapOverlay));
        assert!(invalidations.contains(&UiInvalidation::DebugPanel));
    }

    #[test]
    fn record_delta_round_trips_changed_top_level_metar_fields() {
        let from = serde_json::json!({
            "schema_version": 3,
            "version_label": "from",
            "metar_count": 1,
            "generated_at_utc": "2026-05-18T20:00:00Z",
            "observed_at_utc": "2026-05-18T20:00:00Z",
            "metars_by_station": {
                "KAAA": {"station_id": "KAAA", "raw_text": "same"}
            }
        });
        let to = serde_json::json!({
            "schema_version": 3,
            "version_label": "to",
            "metar_count": 1,
            "generated_at_utc": "2026-05-18T20:05:00Z",
            "observed_at_utc": "2026-05-18T20:05:00Z",
            "metars_by_station": {
                "KAAA": {"station_id": "KAAA", "raw_text": "same"}
            }
        });
        let delta = LiveFeedRecordDelta {
            schema_version: LIVE_FEEDS_SCHEMA_VERSION,
            product: "metars".to_string(),
            from_version: "from".to_string(),
            to_version: "to".to_string(),
            top_level_changed: BTreeMap::from([
                (
                    "generated_at_utc".to_string(),
                    serde_json::json!("2026-05-18T20:05:00Z"),
                ),
                (
                    "observed_at_utc".to_string(),
                    serde_json::json!("2026-05-18T20:05:00Z"),
                ),
            ]),
            top_level_removed: Vec::new(),
            changed: BTreeMap::new(),
            removed: Vec::new(),
        };

        let applied = apply_live_feed_record_delta(&from, &delta).unwrap();

        assert_eq!(applied, to);
    }

    #[test]
    fn record_delta_round_trips_keyed_tfr_area_array() {
        let from = serde_json::json!({
            "schema_version": 2,
            "version_label": "from",
            "notam_count": 1,
            "area_group_count": 1,
            "areas": [
                {"area_id": "6/7042:a", "notam_id": "6/7042", "summary_text": "old"}
            ]
        });
        let to = serde_json::json!({
            "schema_version": 2,
            "version_label": "to",
            "notam_count": 2,
            "area_group_count": 2,
            "areas": [
                {"area_id": "6/7042:a", "notam_id": "6/7042", "summary_text": "new"},
                {"area_id": "6/7043:b", "notam_id": "6/7043", "summary_text": "added"}
            ]
        });
        let delta = LiveFeedRecordDelta {
            schema_version: LIVE_FEEDS_SCHEMA_VERSION,
            product: "tfrs".to_string(),
            from_version: "from".to_string(),
            to_version: "to".to_string(),
            top_level_changed: BTreeMap::from([
                ("notam_count".to_string(), serde_json::json!(2)),
                ("area_group_count".to_string(), serde_json::json!(2)),
            ]),
            top_level_removed: Vec::new(),
            changed: BTreeMap::from([
                (
                    "6/7042:a".to_string(),
                    serde_json::json!({
                        "area_id": "6/7042:a",
                        "notam_id": "6/7042",
                        "summary_text": "new"
                    }),
                ),
                (
                    "6/7043:b".to_string(),
                    serde_json::json!({
                        "area_id": "6/7043:b",
                        "notam_id": "6/7043",
                        "summary_text": "added"
                    }),
                ),
            ]),
            removed: Vec::new(),
        };

        assert_eq!(apply_live_feed_record_delta(&from, &delta).unwrap(), to);
    }

    #[test]
    fn nav_kv_state_manifest_validates_embedded_state_hash() {
        let mut state = live_feeds_state();
        state
            .ingest_catalog_bytes(
                br#"{
                    "schema_version": 3,
                    "generated_at_utc": "2026-08-04T00:00:00Z",
                    "products": {
                        "obstacles": {
                            "current": "v1",
                            "version_manifest_url": "versions/obstacles/v1.json",
                            "state_url": "states/obstacles/v1/manifest.json",
                            "state_sha256": "had-hash"
                        }
                    }
                }"#,
            )
            .unwrap();
        state
            .ingest_resource(
                "live_feeds/version/obstacles/v1",
                br#"{
                    "schema_version": 3,
                    "product": "obstacles",
                    "version": "v1",
                    "state": {
                        "kind": "nav_kv",
                        "url": "states/obstacles/v1/manifest.json",
                        "bytes": 1,
                        "blob_sha256": "unused",
                        "state_sha256": "had-hash"
                    }
                }"#,
            )
            .unwrap();

        state
            .ingest_resource(
                "live_feeds/state/obstacles/v1",
                br#"{
                    "schema_version": 1,
                    "product_id": "obstacles",
                    "version_label": "v1",
                    "encoding": "had-nav-kv-v1",
                    "root": "root",
                    "page_path_template": "page_{page:04}",
                    "state_sha256": "had-hash"
                }"#,
            )
            .unwrap();
        assert_eq!(
            state.product_state_manifest("obstacles").unwrap()["state_sha256"],
            "had-hash"
        );
    }

    #[test]
    fn sse_event_updates_product_without_platform_contract_logic() {
        let mut state = LiveFeedsState {
            catalog_loaded: true,
            ..live_feeds_state()
        };
        let outcome = state
            .ingest_sse_event(LiveFeedSseEvent {
                id: Some("nexrad:v2".to_string()),
                event: Some("live-feed-current".to_string()),
                data: r#"{
                    "schema_version": 3,
                    "product": "nexrad",
                    "version": "v2",
                    "version_manifest_url": "versions/nexrad/v2.json",
                    "state_url": "states/nexrad/v2/manifest.json",
                    "state_sha256": "nexrad-v2"
                }"#
                .to_string(),
            })
            .unwrap();
        let HadOperationOutcome::NeedResources { resources } = outcome else {
            panic!("expected version request");
        };
        assert_eq!(resources[0].id, "live_feeds/version/nexrad/v2");
    }

    #[test]
    fn batched_sse_events_fetch_only_the_latest_version_per_product() {
        let mut state = LiveFeedsState {
            catalog_loaded: true,
            ..live_feeds_state()
        };
        let affected = state
            .ingest_sse_events([
                LiveFeedSseEvent {
                    id: Some("metars:v1".to_string()),
                    event: Some("live-feed-current".to_string()),
                    data: r#"{
                        "schema_version": 3,
                        "product": "metars",
                        "version": "v1",
                        "version_manifest_url": "versions/metars/v1.json",
                        "state_url": "states/metars/v1.json",
                        "state_sha256": "metars-v1"
                    }"#
                    .to_string(),
                },
                LiveFeedSseEvent {
                    id: Some("metars:v2".to_string()),
                    event: Some("live-feed-current".to_string()),
                    data: r#"{
                        "schema_version": 3,
                        "product": "metars",
                        "version": "v2",
                        "version_manifest_url": "versions/metars/v2.json",
                        "state_url": "states/metars/v2.json",
                        "state_sha256": "metars-v2"
                    }"#
                    .to_string(),
                },
                LiveFeedSseEvent {
                    id: Some("nexrad:v7".to_string()),
                    event: Some("live-feed-current".to_string()),
                    data: r#"{
                        "schema_version": 3,
                        "product": "nexrad",
                        "version": "v7",
                        "version_manifest_url": "versions/nexrad/v7.json",
                        "state_url": "states/nexrad/v7/manifest.json",
                        "state_sha256": "nexrad-v7"
                    }"#
                    .to_string(),
                },
            ])
            .unwrap();
        let HadOperationOutcome::NeedResources { resources } =
            state.sync_products_outcome_with_invalidations(affected.iter().map(String::as_str))
        else {
            panic!("expected version requests");
        };

        let ids = resources
            .iter()
            .map(|resource| resource.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                "live_feeds/version/metars/v2",
                "live_feeds/version/nexrad/v7"
            ]
        );
    }

    #[test]
    fn replayed_batched_sse_events_do_not_forget_loaded_version_manifest() {
        let mut state = LiveFeedsState {
            catalog_loaded: true,
            ..live_feeds_state()
        };
        let events = vec![
            LiveFeedSseEvent {
                id: Some("metars:v1".to_string()),
                event: Some("live-feed-current".to_string()),
                data: r#"{
                    "schema_version": 3,
                    "product": "metars",
                    "version": "v1",
                    "version_manifest_url": "versions/metars/v1.json",
                    "state_url": "states/metars/v1.json",
                    "state_sha256": "metars-v1"
                }"#
                .to_string(),
            },
            LiveFeedSseEvent {
                id: Some("metars:v2".to_string()),
                event: Some("live-feed-current".to_string()),
                data: r#"{
                    "schema_version": 3,
                    "product": "metars",
                    "version": "v2",
                    "version_manifest_url": "versions/metars/v2.json",
                    "state_url": "states/metars/v2.json",
                    "state_sha256": "metars-v2"
                }"#
                .to_string(),
            },
        ];
        state.ingest_sse_events(events.clone()).unwrap();
        state
            .ingest_resource(
                "live_feeds/version/metars/v2",
                br#"{
                    "schema_version": 3,
                    "product": "metars",
                    "version": "v2",
                    "state": {
                        "kind": "json_xz",
                        "url": "states/metars/v2.json",
                        "bytes": 1,
                        "blob_sha256": "unused",
                        "state_sha256": "abc123"
                    }
                }"#,
            )
            .unwrap();

        let affected = state.ingest_sse_events(events).unwrap();
        let HadOperationOutcome::NeedResources { resources } =
            state.sync_products_outcome_with_invalidations(affected.iter().map(String::as_str))
        else {
            panic!("expected state request");
        };

        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "live_feeds/state/metars/v2");
    }

    #[test]
    fn metar_live_feed_prefers_applicable_delta_and_invalidates_overlay() {
        let v1 = metar_state("v1", &[("KAAA", "METAR KAAA 010000Z AUTO")]);
        let v2 = metar_state(
            "v2",
            &[
                ("KAAA", "METAR KAAA 010005Z AUTO"),
                ("KBBB", "METAR KBBB 010005Z AUTO"),
            ],
        );
        let delta = metar_delta(&v1, &v2);
        assert_record_sync_installs_product("metars", v1, v2, delta, 2, 1);
    }

    #[test]
    fn taf_live_feed_prefers_smaller_full_state_and_invalidates_overlay() {
        let v1 = taf_state("v1", &[("KSEA", "TAF KSEA 010000Z OLD")]);
        let v2 = taf_state(
            "v2",
            &[
                ("KSEA", "TAF KSEA 010600Z NEW"),
                ("KBFI", "TAF KBFI 010600Z NEW"),
            ],
        );
        let delta = taf_delta(&v1, &v2);
        assert_record_sync_installs_product("tafs", v1, v2, delta, 1, 2);
    }
}
