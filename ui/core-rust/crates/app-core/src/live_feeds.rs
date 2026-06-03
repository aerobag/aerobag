use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    map_overlay::{MetarProductPayload, MetarRecord},
    AppError, AppErrorKind, AppResult, CoreResourceRequest, HadOperationOutcome, UiInvalidation,
};

const CURRENT_RESOURCE_ID: &str = "live_feeds/current";
const CURRENT_ADDRESS: &str = "/live-feeds/current.json";
const LIVE_FEEDS_PREFIX: &str = "/live-feeds/";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LiveFeedsState {
    products: HashMap<String, LiveFeedProductState>,
    current_loaded: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct LiveFeedProductState {
    current_version: Option<String>,
    loaded_version: Option<String>,
    version_manifest_url: Option<String>,
    state_url: Option<String>,
    expected_state_sha256: Option<String>,
    state_kind: Option<String>,
    delta_from_previous: Option<LiveFeedDeltaRef>,
    version_manifest: Option<Value>,
    state_manifest: Option<Value>,
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

#[derive(Debug, Deserialize)]
struct CurrentManifest {
    products: HashMap<String, CurrentProduct>,
}

#[derive(Debug, Deserialize)]
struct CurrentProduct {
    current: String,
    version_manifest_url: String,
    #[serde(default)]
    state_url: Option<String>,
    #[serde(default)]
    state_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    product: String,
    version: String,
    #[serde(default)]
    delta_from_previous: Option<LiveFeedDeltaRef>,
    state: VersionStateRef,
}

#[derive(Debug, Deserialize)]
struct VersionStateRef {
    #[serde(default)]
    kind: Option<String>,
    url: String,
    state_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct LiveFeedDeltaRef {
    from_version: String,
    from_state_sha256: String,
    to_version: String,
    to_state_sha256: String,
    url: String,
    #[serde(default)]
    blob_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct LiveFeedRecordDelta {
    product: String,
    from_version: String,
    to_version: String,
    top_level_changed: serde_json::Map<String, Value>,
    top_level_removed: Vec<String>,
    changed: serde_json::Map<String, Value>,
    removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedMetarLiveFeedEnvelope {
    pub schema_version: u32,
    pub resource_id: String,
    pub version: String,
    pub state_sha256: String,
    #[serde(default)]
    pub from_version: Option<String>,
    #[serde(default)]
    pub from_state_sha256: Option<String>,
    #[serde(default)]
    pub delta_blob_sha256: Option<String>,
    pub feed: PreparedMetarLiveFeed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedMetarLiveFeed {
    pub schema_version: u32,
    pub version_label: String,
    #[serde(default)]
    pub generated_at_utc: Option<String>,
    #[serde(default)]
    pub observed_at_utc: Option<String>,
    pub records: Vec<MetarRecord>,
    pub tiles: Vec<PreparedMetarTile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreparedMetarTile {
    pub z: u32,
    pub x: u32,
    pub y: u32,
    pub record_indexes: Vec<u32>,
}

#[derive(Debug, Deserialize)]
struct LiveFeedCurrentEvent {
    product: String,
    version: String,
    version_manifest_url: String,
    #[serde(default)]
    state_url: Option<String>,
    #[serde(default)]
    state_sha256: Option<String>,
}

impl LiveFeedsState {
    pub fn sync_outcome(&self) -> HadOperationOutcome {
        let resources = self.missing_resources();
        if resources.is_empty() {
            HadOperationOutcome::complete(
                serde_json::to_value(self.snapshot()).expect("live feed snapshot serializes"),
            )
        } else {
            HadOperationOutcome::NeedResources { resources }
        }
    }

    pub fn sync_product_outcome(&self, product: &str) -> HadOperationOutcome {
        let resources = if self.current_loaded {
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
        let resources = if self.current_loaded {
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
        let mut latest_current_by_product = HashMap::new();
        for event in events {
            if let Some(payload) = parse_sse_current_event(event)? {
                latest_current_by_product.insert(payload.product.clone(), payload);
            }
        }
        let mut affected = latest_current_by_product
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        affected.sort();
        for product in &affected {
            let payload = latest_current_by_product
                .remove(product)
                .expect("affected product came from latest_current_by_product");
            self.register_product(
                payload.product,
                payload.version,
                payload.version_manifest_url,
                payload.state_url,
                payload.state_sha256,
            )?;
        }
        Ok(affected)
    }

    pub fn ingest_resource(&mut self, resource_id: &str, bytes: &[u8]) -> AppResult<()> {
        if resource_id == CURRENT_RESOURCE_ID {
            let current: CurrentManifest =
                serde_json::from_slice(bytes).map_err(invalid_live_feed_json)?;
            self.current_loaded = true;
            let products = current.products;
            self.products
                .retain(|product, _| products.contains_key(product));
            for (product, entry) in products {
                self.register_product(
                    product,
                    entry.current,
                    entry.version_manifest_url,
                    entry.state_url,
                    entry.state_sha256,
                )?;
            }
            return Ok(());
        }
        if let Some(rest) = resource_id.strip_prefix("live_feeds/version/") {
            let (product, version) = split_product_version(resource_id, rest)?;
            let manifest: VersionManifest =
                serde_json::from_slice(bytes).map_err(invalid_live_feed_json)?;
            if manifest.product != product || manifest.version != version {
                return Err(invalid_live_feed(format!(
                    "version resource {resource_id} contained {}:{}",
                    manifest.product, manifest.version
                )));
            }
            let entry = self.products.entry(product).or_default();
            if entry.current_version.as_deref() != Some(version.as_str()) {
                return Ok(());
            }
            entry.state_url = Some(manifest.state.url);
            entry.expected_state_sha256 = Some(manifest.state.state_sha256);
            entry.state_kind = manifest.state.kind;
            entry.delta_from_previous = manifest.delta_from_previous;
            entry.version_manifest =
                Some(serde_json::from_slice(bytes).map_err(invalid_live_feed_json)?);
            return Ok(());
        }
        if let Some(rest) = resource_id.strip_prefix("live_feeds/state/") {
            let (product, version) = split_product_version(resource_id, rest)?;
            let parsed: Value = serde_json::from_slice(bytes).map_err(invalid_live_feed_json)?;
            let entry = self.products.entry(product).or_default();
            if entry.current_version.as_deref() != Some(version.as_str()) {
                return Ok(());
            }
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
            entry.state_manifest = Some(parsed);
            entry.loaded_version = Some(version);
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
            if let Some(expected_blob_sha256) = &delta_ref.blob_sha256 {
                let actual_blob_sha256 = sha256_hex(bytes);
                if &actual_blob_sha256 != expected_blob_sha256 {
                    return Err(invalid_live_feed(format!(
                        "delta blob hash mismatch for {resource_id}: expected {expected_blob_sha256}, got {actual_blob_sha256}"
                    )));
                }
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
            let delta: LiveFeedRecordDelta =
                serde_json::from_slice(bytes).map_err(invalid_live_feed_json)?;
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
            return Ok(());
        }
        Err(invalid_live_feed(format!(
            "unsupported live feed resource id: {resource_id}"
        )))
    }

    pub fn ingest_prepared_metar_live_feed(
        &mut self,
        resource_id: &str,
        envelope: &PreparedMetarLiveFeedEnvelope,
    ) -> AppResult<()> {
        if envelope.schema_version != 1 || envelope.feed.schema_version != 1 {
            return Err(invalid_live_feed(format!(
                "unsupported prepared METAR schema {}/{}",
                envelope.schema_version, envelope.feed.schema_version
            )));
        }
        if envelope.resource_id != resource_id {
            return Err(invalid_live_feed(format!(
                "prepared METAR envelope for {} cannot satisfy {resource_id}",
                envelope.resource_id
            )));
        }
        if let Some(rest) = resource_id.strip_prefix("live_feeds/state/") {
            let (product, version) = split_product_version(resource_id, rest)?;
            if product != "metars" {
                return Err(invalid_live_feed(format!(
                    "prepared METAR full resource used for {product}"
                )));
            }
            let entry = self.products.entry(product).or_default();
            if entry.current_version.as_deref() != Some(version.as_str()) {
                return Ok(());
            }
            if envelope.version != version || envelope.feed.version_label != version {
                return Err(invalid_live_feed(format!(
                    "prepared METAR full resource {resource_id} contained {} / {}",
                    envelope.version, envelope.feed.version_label
                )));
            }
            if let Some(expected) = &entry.expected_state_sha256 {
                if &envelope.state_sha256 != expected {
                    return Err(invalid_live_feed(format!(
                        "prepared METAR state hash mismatch for {resource_id}: expected {expected}, got {}",
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
            if product != "metars" {
                return Err(invalid_live_feed(format!(
                    "prepared METAR delta resource used for {product}"
                )));
            }
            let entry = self.products.entry(product).or_default();
            if entry.current_version.as_deref() != Some(to_version.as_str()) {
                return Ok(());
            }
            if entry.loaded_version.as_deref() != Some(from_version.as_str()) {
                return Err(invalid_live_feed(format!(
                    "cannot install prepared {resource_id}: local version is {:?}",
                    entry.loaded_version
                )));
            }
            let delta_ref = entry.delta_from_previous.as_ref().ok_or_else(|| {
                invalid_live_feed(format!("prepared delta {resource_id} was not expected"))
            })?;
            if delta_ref.from_version != from_version || delta_ref.to_version != to_version {
                return Err(invalid_live_feed(format!(
                    "prepared delta {resource_id} does not match version manifest"
                )));
            }
            if envelope.from_version.as_deref() != Some(from_version.as_str())
                || envelope.version != to_version
                || envelope.feed.version_label != to_version
            {
                return Err(invalid_live_feed(format!(
                    "prepared delta {resource_id} contained {:?} -> {} / {}",
                    envelope.from_version, envelope.version, envelope.feed.version_label
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
            if let Some(expected_blob_sha256) = &delta_ref.blob_sha256 {
                if envelope.delta_blob_sha256.as_deref() != Some(expected_blob_sha256.as_str()) {
                    return Err(invalid_live_feed(format!(
                        "prepared delta {resource_id} blob hash mismatch: expected {expected_blob_sha256}, got {:?}",
                        envelope.delta_blob_sha256
                    )));
                }
            }
            entry.state_manifest = None;
            entry.loaded_version = Some(to_version);
            return Ok(());
        }
        Err(invalid_live_feed(format!(
            "unsupported prepared METAR resource id: {resource_id}"
        )))
    }

    pub fn handles_resource(resource_id: &str) -> bool {
        resource_id == CURRENT_RESOURCE_ID
            || resource_id.starts_with("live_feeds/version/")
            || resource_id.starts_with("live_feeds/state/")
            || resource_id.starts_with("live_feeds/delta/")
    }

    pub fn product_state_manifest(&self, product: &str) -> Option<&Value> {
        let entry = self.products.get(product)?;
        if !entry
            .current_version
            .as_deref()
            .is_some_and(|version| entry.loaded_version.as_deref() == Some(version))
        {
            return None;
        }
        entry.state_manifest.as_ref()
    }

    pub fn loaded_product_state_manifest(&self, product: &str) -> Option<&Value> {
        self.products.get(product)?.state_manifest.as_ref()
    }

    pub fn product_loaded_version(&self, product: &str) -> Option<&str> {
        let entry = self.products.get(product)?;
        if !entry
            .current_version
            .as_deref()
            .is_some_and(|version| entry.loaded_version.as_deref() == Some(version))
        {
            return None;
        }
        entry.loaded_version.as_deref()
    }

    pub fn product_state_url(&self, product: &str) -> Option<&str> {
        let entry = self.products.get(product)?;
        if !entry
            .current_version
            .as_deref()
            .is_some_and(|version| entry.loaded_version.as_deref() == Some(version))
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

    pub fn current_loaded(&self) -> bool {
        self.current_loaded
    }

    fn register_product(
        &mut self,
        product: String,
        version: String,
        version_manifest_url: String,
        state_url: Option<String>,
        state_sha256: Option<String>,
    ) -> AppResult<()> {
        validate_relative_url(&version_manifest_url)?;
        if let Some(url) = &state_url {
            validate_relative_url(url)?;
        }
        let entry = self.products.entry(product).or_default();
        if entry.current_version.as_deref() == Some(version.as_str()) {
            entry.version_manifest_url = Some(version_manifest_url);
            if state_url.is_some() {
                entry.state_url = state_url;
            }
            if state_sha256.is_some() {
                entry.expected_state_sha256 = state_sha256;
            }
            return Ok(());
        }
        entry.current_version = Some(version);
        entry.version_manifest_url = Some(version_manifest_url);
        entry.state_url = state_url;
        entry.expected_state_sha256 = state_sha256;
        entry.state_kind = None;
        entry.delta_from_previous = None;
        entry.version_manifest = None;
        Ok(())
    }

    fn missing_resources(&self) -> Vec<CoreResourceRequest> {
        if !self.current_loaded {
            return vec![CoreResourceRequest::public_url(
                CURRENT_RESOURCE_ID,
                CURRENT_ADDRESS,
                false,
            )];
        }
        let products = self.products.keys().map(String::as_str).collect::<Vec<_>>();
        self.missing_resources_for_products(products)
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
                    resources.push(CoreResourceRequest::public_url(
                        format!("live_feeds/version/{product}/{version}"),
                        live_feed_address(url),
                        false,
                    ));
                    continue;
                }
            }
            if entry.loaded_version.as_deref() == Some(version.as_str()) {
                continue;
            }
            if let Some(delta) = entry.applicable_delta(product) {
                resources.push(CoreResourceRequest::public_url(
                    format!(
                        "live_feeds/delta/{}/{}/{}",
                        product, delta.from_version, delta.to_version
                    ),
                    live_feed_address(&delta.url),
                    false,
                ));
                continue;
            }
            if let Some(url) = &entry.state_url {
                resources.push(CoreResourceRequest::public_url(
                    format!("live_feeds/state/{product}/{version}"),
                    live_feed_address(url),
                    false,
                ));
            }
        }
        resources
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
        if self.current_loaded {
            invalidations.push(UiInvalidation::SessionSnapshot);
            invalidations.push(UiInvalidation::MapOverlay);
            invalidations.push(UiInvalidation::NexradOverlay);
            invalidations.push(UiInvalidation::DebugPanel);
        }
        for (product, entry) in &self.products {
            if !entry
                .current_version
                .as_deref()
                .is_some_and(|version| entry.loaded_version.as_deref() == Some(version))
            {
                continue;
            }
            invalidations.push(UiInvalidation::SessionSnapshot);
            match product.as_str() {
                "nexrad" => {
                    invalidations.push(UiInvalidation::NexradOverlay);
                    invalidations.push(UiInvalidation::DebugPanel);
                }
                "metars" | "tfrs" | "pireps" | "obstacles" => {
                    invalidations.push(UiInvalidation::MapOverlay);
                    invalidations.push(UiInvalidation::DebugPanel);
                }
                _ => {
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
    fn applicable_delta(&self, product: &str) -> Option<&LiveFeedDeltaRef> {
        if !supports_record_delta(product) {
            return None;
        }
        let delta = self.delta_from_previous.as_ref()?;
        if self.loaded_version.as_deref() == Some(delta.from_version.as_str())
            && self.current_version.as_deref() == Some(delta.to_version.as_str())
            && (product == "metars" || self.state_manifest.is_some())
        {
            Some(delta)
        } else {
            None
        }
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

fn parse_sse_current_event(event: LiveFeedSseEvent) -> AppResult<Option<LiveFeedCurrentEvent>> {
    let event_name = event.event.as_deref().unwrap_or("message");
    match event_name {
        "live-feed-current" | "message" => Ok(Some(
            serde_json::from_str(&event.data).map_err(invalid_live_feed_json)?,
        )),
        _ => Ok(None),
    }
}

fn live_feed_address(relative_url: &str) -> String {
    format!(
        "{LIVE_FEEDS_PREFIX}{}",
        relative_url.trim_start_matches('/')
    )
}

fn validate_relative_url(url: &str) -> AppResult<()> {
    if url.starts_with('/') || url.contains("://") || url.split('/').any(|part| part == "..") {
        return Err(invalid_live_feed(format!(
            "live feed URL must be package-relative: {url}"
        )));
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

pub fn prepare_metar_live_feed_state_resource(
    resource_id: &str,
    bytes: &[u8],
) -> AppResult<(Value, Vec<u8>)> {
    let Some(rest) = resource_id.strip_prefix("live_feeds/state/") else {
        return Err(invalid_live_feed(format!(
            "not a live-feed state resource: {resource_id}"
        )));
    };
    let (product, version) = split_product_version(resource_id, rest)?;
    if product != "metars" {
        return Err(invalid_live_feed(format!(
            "cannot prepare METAR state from {product}"
        )));
    }
    let state: Value = serde_json::from_slice(bytes).map_err(invalid_live_feed_json)?;
    let state_sha256 = canonical_json_sha256(&state)?;
    let payload: MetarProductPayload =
        serde_json::from_value(state.clone()).map_err(invalid_live_feed_json)?;
    if payload.version_label != version {
        return Err(invalid_live_feed(format!(
            "METAR state {resource_id} contained version {}",
            payload.version_label
        )));
    }
    let envelope = PreparedMetarLiveFeedEnvelope {
        schema_version: 1,
        resource_id: resource_id.to_string(),
        version,
        state_sha256,
        from_version: None,
        from_state_sha256: None,
        delta_blob_sha256: None,
        feed: prepare_metar_live_feed(&payload),
    };
    let envelope_bytes = postcard::to_allocvec(&envelope).map_err(|err| {
        invalid_live_feed(format!("failed to encode prepared METAR state: {err}"))
    })?;
    Ok((state, envelope_bytes))
}

pub fn prepare_metar_live_feed_delta_resource(
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
    if product != "metars" {
        return Err(invalid_live_feed(format!(
            "cannot prepare METAR delta from {product}"
        )));
    }
    let from_state_sha256 = canonical_json_sha256(current_state)?;
    let delta_blob_sha256 = sha256_hex(bytes);
    let delta: LiveFeedRecordDelta =
        serde_json::from_slice(bytes).map_err(invalid_live_feed_json)?;
    if delta.product != "metars"
        || delta.from_version != from_version
        || delta.to_version != to_version
    {
        return Err(invalid_live_feed(format!(
            "METAR delta {resource_id} contained {} {} -> {}",
            delta.product, delta.from_version, delta.to_version
        )));
    }
    let next_state = apply_live_feed_record_delta(current_state, &delta)?;
    let state_sha256 = canonical_json_sha256(&next_state)?;
    let payload: MetarProductPayload =
        serde_json::from_value(next_state.clone()).map_err(invalid_live_feed_json)?;
    if payload.version_label != to_version {
        return Err(invalid_live_feed(format!(
            "prepared METAR delta {resource_id} produced version {}",
            payload.version_label
        )));
    }
    let envelope = PreparedMetarLiveFeedEnvelope {
        schema_version: 1,
        resource_id: resource_id.to_string(),
        version: to_version,
        state_sha256,
        from_version: Some(from_version),
        from_state_sha256: Some(from_state_sha256),
        delta_blob_sha256: Some(delta_blob_sha256),
        feed: prepare_metar_live_feed(&payload),
    };
    let envelope_bytes = postcard::to_allocvec(&envelope).map_err(|err| {
        invalid_live_feed(format!("failed to encode prepared METAR delta: {err}"))
    })?;
    Ok((next_state, envelope_bytes))
}

pub fn decode_prepared_metar_live_feed(bytes: &[u8]) -> AppResult<PreparedMetarLiveFeedEnvelope> {
    postcard::from_bytes(bytes)
        .map_err(|err| invalid_live_feed(format!("failed to decode prepared METAR feed: {err}")))
}

fn prepare_metar_live_feed(payload: &MetarProductPayload) -> PreparedMetarLiveFeed {
    let mut records = payload
        .metars_by_station
        .values()
        .cloned()
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.station_id.cmp(&right.station_id));
    let mut tiles = std::collections::BTreeMap::<(u32, u32, u32), Vec<u32>>::new();
    for zoom in [5_u32, 6, 7] {
        for (record_index, record) in records.iter().enumerate() {
            let Some((x, y)) = live_feed_metar_tile_xy(record.latitude, record.longitude, zoom)
            else {
                continue;
            };
            tiles
                .entry((zoom, x, y))
                .or_default()
                .push(record_index as u32);
        }
    }
    PreparedMetarLiveFeed {
        schema_version: 1,
        version_label: payload.version_label.clone(),
        generated_at_utc: payload.generated_at_utc.map(|value| value.to_rfc3339()),
        observed_at_utc: payload.observed_at_utc.map(|value| value.to_rfc3339()),
        records,
        tiles: tiles
            .into_iter()
            .map(|((z, x, y), record_indexes)| PreparedMetarTile {
                z,
                x,
                y,
                record_indexes,
            })
            .collect(),
    }
}

fn supports_record_delta(product: &str) -> bool {
    matches!(product, "metars")
}

fn record_delta_schema(product: &str) -> Option<(&'static str, &'static str)> {
    match product {
        "metars" => Some(("metars_by_station", "metar_count")),
        _ => None,
    }
}

fn apply_live_feed_record_delta(
    from_state: &Value,
    delta: &LiveFeedRecordDelta,
) -> AppResult<Value> {
    let (records_key, count_key) = record_delta_schema(&delta.product).ok_or_else(|| {
        invalid_live_feed(format!(
            "unsupported live feed delta product: {}",
            delta.product
        ))
    })?;
    let from_version = from_state
        .get("version_label")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_live_feed("live feed state missing version_label".to_string()))?;
    if from_version != delta.from_version {
        return Err(invalid_live_feed(format!(
            "delta starts at {}, but local state is {}",
            delta.from_version, from_version
        )));
    }
    let mut result = from_state.clone();
    {
        let result_object = result.as_object_mut().ok_or_else(|| {
            invalid_live_feed("live feed state must be a JSON object".to_string())
        })?;
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
            .ok_or_else(|| invalid_live_feed(format!("state missing {records_key} object")))?;
        for station_id in &delta.removed {
            records.remove(station_id);
        }
        for (station_id, record) in &delta.changed {
            records.insert(station_id.clone(), record.clone());
        }
        records.len()
    };
    let version = result
        .get_mut("version_label")
        .ok_or_else(|| invalid_live_feed("live feed state missing version_label".to_string()))?;
    *version = Value::String(delta.to_version.clone());
    if let Some(count) = result.get_mut(count_key) {
        *count = serde_json::json!(record_count);
    }
    Ok(result)
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
    use super::*;

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
            "schema_version": 2,
            "version_label": version,
            "generated_at_utc": "2026-05-18T20:00:00Z",
            "observed_at_utc": "2026-05-18T20:00:00Z",
            "metar_count": metars_by_station.len(),
            "metars_by_station": metars_by_station
        })
    }

    fn metar_delta(from: &Value, to: &Value) -> Value {
        let from_version = from["version_label"].as_str().unwrap();
        let to_version = to["version_label"].as_str().unwrap();
        let from_records = from["metars_by_station"].as_object().unwrap();
        let to_records = to["metars_by_station"].as_object().unwrap();
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
            "schema_version": 1,
            "product": "metars",
            "from_version": from_version,
            "to_version": to_version,
            "top_level_changed": {},
            "top_level_removed": [],
            "changed": changed,
            "removed": removed
        })
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
    fn sync_requests_current_manifest_first() {
        let state = LiveFeedsState::default();
        let HadOperationOutcome::NeedResources { resources } = state.sync_outcome() else {
            panic!("expected current manifest request");
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "live_feeds/current");
        assert_eq!(
            resources[0].source,
            crate::CoreResourceSource::PublicUrl {
                url: "/live-feeds/current.json".to_string(),
            }
        );
    }

    #[test]
    fn current_manifest_drives_version_then_state_requests() {
        let mut state = LiveFeedsState::default();
        state
            .ingest_resource(
                "live_feeds/current",
                br#"{
                    "products": {
                        "nexrad": {
                            "current": "v1",
                            "version_manifest_url": "versions/nexrad/v1.json",
                            "state_url": "states/nexrad/v1/manifest.json",
                            "state_sha256": "unused"
                        }
                    }
                }"#,
            )
            .unwrap();
        let HadOperationOutcome::NeedResources { resources } = state.sync_outcome() else {
            panic!("expected version request");
        };
        assert_eq!(resources[0].id, "live_feeds/version/nexrad/v1");
        assert_eq!(
            resources[0].source,
            crate::CoreResourceSource::PublicUrl {
                url: "/live-feeds/versions/nexrad/v1.json".to_string(),
            }
        );
    }

    #[test]
    fn current_manifest_is_authoritative_for_product_membership() {
        let mut state = LiveFeedsState::default();
        state
            .ingest_resource(
                "live_feeds/current",
                br#"{
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
            .ingest_resource(
                "live_feeds/current",
                br#"{
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
    fn loaded_current_without_overlay_products_still_invalidates_overlays() {
        let mut state = LiveFeedsState::default();
        state
            .ingest_resource("live_feeds/current", br#"{"products": {}}"#)
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
        let mut state = LiveFeedsState::default();
        state
            .ingest_resource(
                "live_feeds/current",
                format!(
                    r#"{{
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
                    "product": "tfrs",
                    "version": "v1",
                    "state": {{
                        "url": "states/tfrs/v1.json",
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
                &serde_json::to_vec(&tfrs).unwrap(),
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
            product: "metars".to_string(),
            from_version: "from".to_string(),
            to_version: "to".to_string(),
            top_level_changed: serde_json::Map::from_iter([
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
            changed: serde_json::Map::new(),
            removed: Vec::new(),
        };

        let applied = apply_live_feed_record_delta(&from, &delta).unwrap();

        assert_eq!(applied, to);
    }

    #[test]
    fn nav_kv_state_manifest_validates_embedded_state_hash() {
        let mut state = LiveFeedsState::default();
        state
            .ingest_resource(
                "live_feeds/current",
                br#"{
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
                    "product": "obstacles",
                    "version": "v1",
                    "state": {
                        "kind": "nav_kv",
                        "url": "states/obstacles/v1/manifest.json",
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
            current_loaded: true,
            ..LiveFeedsState::default()
        };
        let outcome = state
            .ingest_sse_event(LiveFeedSseEvent {
                id: Some("nexrad:v2".to_string()),
                event: Some("live-feed-current".to_string()),
                data: r#"{
                    "product": "nexrad",
                    "version": "v2",
                    "version_manifest_url": "versions/nexrad/v2.json"
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
            current_loaded: true,
            ..LiveFeedsState::default()
        };
        let affected = state
            .ingest_sse_events([
                LiveFeedSseEvent {
                    id: Some("metars:v1".to_string()),
                    event: Some("live-feed-current".to_string()),
                    data: r#"{
                        "product": "metars",
                        "version": "v1",
                        "version_manifest_url": "versions/metars/v1.json"
                    }"#
                    .to_string(),
                },
                LiveFeedSseEvent {
                    id: Some("metars:v2".to_string()),
                    event: Some("live-feed-current".to_string()),
                    data: r#"{
                        "product": "metars",
                        "version": "v2",
                        "version_manifest_url": "versions/metars/v2.json"
                    }"#
                    .to_string(),
                },
                LiveFeedSseEvent {
                    id: Some("nexrad:v7".to_string()),
                    event: Some("live-feed-current".to_string()),
                    data: r#"{
                        "product": "nexrad",
                        "version": "v7",
                        "version_manifest_url": "versions/nexrad/v7.json"
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
            current_loaded: true,
            ..LiveFeedsState::default()
        };
        let events = vec![
            LiveFeedSseEvent {
                id: Some("metars:v1".to_string()),
                event: Some("live-feed-current".to_string()),
                data: r#"{
                    "product": "metars",
                    "version": "v1",
                    "version_manifest_url": "versions/metars/v1.json"
                }"#
                .to_string(),
            },
            LiveFeedSseEvent {
                id: Some("metars:v2".to_string()),
                event: Some("live-feed-current".to_string()),
                data: r#"{
                    "product": "metars",
                    "version": "v2",
                    "version_manifest_url": "versions/metars/v2.json"
                }"#
                .to_string(),
            },
        ];
        state.ingest_sse_events(events.clone()).unwrap();
        state
            .ingest_resource(
                "live_feeds/version/metars/v2",
                br#"{
                    "product": "metars",
                    "version": "v2",
                    "state": {
                        "url": "states/metars/v2.json",
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
        let delta_bytes = serde_json::to_vec(&delta).unwrap();
        let mut state = LiveFeedsState::default();
        state
            .ingest_resource(
                "live_feeds/current",
                format!(
                    r#"{{
                    "products": {{
                        "metars": {{
                            "current": "v1",
                            "version_manifest_url": "versions/metars/v1.json",
                            "state_url": "states/metars/v1.json",
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
                "live_feeds/version/metars/v1",
                format!(
                    r#"{{
                    "product": "metars",
                    "version": "v1",
                    "state": {{
                        "url": "states/metars/v1.json",
                        "state_sha256": "{}"
                    }}
                }}"#,
                    canonical_json_sha256(&v1).unwrap()
                )
                .as_bytes(),
            )
            .unwrap();
        state
            .ingest_resource(
                "live_feeds/state/metars/v1",
                &serde_json::to_vec(&v1).unwrap(),
            )
            .unwrap();

        state
            .ingest_sse_event(LiveFeedSseEvent {
                id: Some("metars:v2".to_string()),
                event: Some("live-feed-current".to_string()),
                data: r#"{
                    "product": "metars",
                    "version": "v2",
                    "version_manifest_url": "versions/metars/v2.json"
                }"#
                .to_string(),
            })
            .unwrap();
        let HadOperationOutcome::NeedResources { resources } = state.sync_outcome() else {
            panic!("expected version request");
        };
        assert_eq!(resources[0].id, "live_feeds/version/metars/v2");

        state
            .ingest_resource(
                "live_feeds/version/metars/v2",
                format!(
                    r#"{{
                    "product": "metars",
                    "version": "v2",
                    "previous": "v1",
                    "state": {{
                        "url": "states/metars/v2.json",
                        "state_sha256": "{}"
                    }},
                    "delta_from_previous": {{
                        "from_version": "v1",
                        "from_state_sha256": "{}",
                        "to_version": "v2",
                        "to_state_sha256": "{}",
                        "url": "deltas/metars/v1__v2.json",
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
            panic!("expected delta request");
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "live_feeds/delta/metars/v1/v2");
        assert_eq!(
            resources[0].source,
            crate::CoreResourceSource::PublicUrl {
                url: "/live-feeds/deltas/metars/v1__v2.json".to_string(),
            }
        );

        state
            .ingest_resource("live_feeds/delta/metars/v1/v2", &delta_bytes)
            .unwrap();
        assert_eq!(state.product_state_manifest("metars"), Some(&v2));
        let outcome = state.sync_outcome_with_invalidations();
        let HadOperationOutcome::Complete { invalidations, .. } = outcome else {
            panic!("expected complete");
        };
        assert!(invalidations.contains(&UiInvalidation::SessionSnapshot));
        assert!(invalidations.contains(&UiInvalidation::MapOverlay));
        assert!(invalidations.contains(&UiInvalidation::DebugPanel));
    }
}
