use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
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
struct MetarStationDelta {
    product: String,
    from_version: String,
    to_version: String,
    changed: serde_json::Map<String, Value>,
    removed: Vec<String>,
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

    pub fn ingest_sse_event(&mut self, event: LiveFeedSseEvent) -> AppResult<HadOperationOutcome> {
        let event_name = event.event.as_deref().unwrap_or("message");
        match event_name {
            "live-feed-current" | "message" => {
                let payload: LiveFeedCurrentEvent =
                    serde_json::from_str(&event.data).map_err(invalid_live_feed_json)?;
                self.register_product(
                    payload.product,
                    payload.version,
                    payload.version_manifest_url,
                    payload.state_url,
                    payload.state_sha256,
                )?;
            }
            _ => {}
        }
        Ok(self.sync_outcome_with_invalidations())
    }

    pub fn ingest_resource(&mut self, resource_id: &str, bytes: &[u8]) -> AppResult<()> {
        if resource_id == CURRENT_RESOURCE_ID {
            let current: CurrentManifest =
                serde_json::from_slice(bytes).map_err(invalid_live_feed_json)?;
            self.current_loaded = true;
            for (product, entry) in current.products {
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
                let actual = canonical_json_sha256(&parsed)?;
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
            if product != "metars" {
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
                    "cannot apply {resource_id}: local METAR state is missing"
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
            let delta: MetarStationDelta =
                serde_json::from_slice(bytes).map_err(invalid_live_feed_json)?;
            let next_state = apply_metar_station_delta(current_state, &delta)?;
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

    pub fn handles_resource(resource_id: &str) -> bool {
        resource_id == CURRENT_RESOURCE_ID || resource_id.starts_with("live_feeds/")
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

    pub fn has_product_current_version(&self, product: &str) -> bool {
        self.products
            .get(product)
            .and_then(|entry| entry.current_version.as_ref())
            .is_some()
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
        entry.delta_from_previous = None;
        entry.version_manifest = None;
        Ok(())
    }

    fn missing_resources(&self) -> Vec<CoreResourceRequest> {
        if !self.current_loaded {
            return vec![CoreResourceRequest {
                id: CURRENT_RESOURCE_ID.to_string(),
                address: CURRENT_ADDRESS.to_string(),
                optional: false,
            }];
        }
        let mut resources = Vec::new();
        let mut products: Vec<_> = self.products.iter().collect();
        products.sort_by(|(left, _), (right, _)| left.cmp(right));
        for (product, entry) in products {
            let Some(version) = &entry.current_version else {
                continue;
            };
            if entry.version_manifest.is_none() {
                if let Some(url) = &entry.version_manifest_url {
                    resources.push(CoreResourceRequest {
                        id: format!("live_feeds/version/{product}/{version}"),
                        address: live_feed_address(url),
                        optional: false,
                    });
                    continue;
                }
            }
            if entry.loaded_version.as_deref() == Some(version.as_str()) {
                continue;
            }
            if let Some(delta) = entry.applicable_delta(product) {
                resources.push(CoreResourceRequest {
                    id: format!(
                        "live_feeds/delta/{}/{}/{}",
                        product, delta.from_version, delta.to_version
                    ),
                    address: live_feed_address(&delta.url),
                    optional: false,
                });
                continue;
            }
            if let Some(url) = &entry.state_url {
                resources.push(CoreResourceRequest {
                    id: format!("live_feeds/state/{product}/{version}"),
                    address: live_feed_address(url),
                    optional: false,
                });
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
        for (product, entry) in &self.products {
            if !entry
                .current_version
                .as_deref()
                .is_some_and(|version| entry.loaded_version.as_deref() == Some(version))
            {
                continue;
            }
            match product.as_str() {
                "nexrad" => {
                    invalidations.push(UiInvalidation::NexradOverlay);
                    invalidations.push(UiInvalidation::DebugPanel);
                }
                "metars" | "tfrs" | "pireps" => {
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
        if product != "metars" {
            return None;
        }
        let delta = self.delta_from_previous.as_ref()?;
        if self.loaded_version.as_deref() == Some(delta.from_version.as_str())
            && self.current_version.as_deref() == Some(delta.to_version.as_str())
            && self.state_manifest.is_some()
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

fn apply_metar_station_delta(from_state: &Value, delta: &MetarStationDelta) -> AppResult<Value> {
    if delta.product != "metars" {
        return Err(invalid_live_feed(format!(
            "METAR delta had product {}",
            delta.product
        )));
    }
    let from_version = from_state
        .get("version_label")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_live_feed("METAR state missing version_label".to_string()))?;
    if from_version != delta.from_version {
        return Err(invalid_live_feed(format!(
            "delta starts at {}, but local METAR state is {}",
            delta.from_version, from_version
        )));
    }
    let mut result = from_state.clone();
    let record_count = {
        let records = result
            .get_mut("metars_by_station")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| {
                invalid_live_feed("METAR state missing metars_by_station object".to_string())
            })?;
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
        .ok_or_else(|| invalid_live_feed("METAR state missing version_label".to_string()))?;
    *version = Value::String(delta.to_version.clone());
    if let Some(count) = result.get_mut("metar_count") {
        *count = serde_json::json!(record_count);
    }
    Ok(result)
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
            "changed": changed,
            "removed": removed
        })
    }

    #[test]
    fn sync_requests_current_manifest_first() {
        let state = LiveFeedsState::default();
        let HadOperationOutcome::NeedResources { resources } = state.sync_outcome() else {
            panic!("expected current manifest request");
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "live_feeds/current");
        assert_eq!(resources[0].address, "/live-feeds/current.json");
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
        assert_eq!(resources[0].address, "/live-feeds/versions/nexrad/v1.json");
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
        assert!(invalidations.contains(&UiInvalidation::MapOverlay));
        assert!(invalidations.contains(&UiInvalidation::DebugPanel));
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
            resources[0].address,
            "/live-feeds/deltas/metars/v1__v2.json"
        );

        state
            .ingest_resource("live_feeds/delta/metars/v1/v2", &delta_bytes)
            .unwrap();
        assert_eq!(state.product_state_manifest("metars"), Some(&v2));
        let outcome = state.sync_outcome_with_invalidations();
        let HadOperationOutcome::Complete { invalidations, .. } = outcome else {
            panic!("expected complete");
        };
        assert!(invalidations.contains(&UiInvalidation::MapOverlay));
        assert!(invalidations.contains(&UiInvalidation::DebugPanel));
    }
}
