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
    version_manifest_url: Option<String>,
    state_url: Option<String>,
    expected_state_sha256: Option<String>,
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
    state: VersionStateRef,
}

#[derive(Debug, Deserialize)]
struct VersionStateRef {
    url: String,
    state_sha256: String,
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
        self.products
            .get(product)
            .and_then(|entry| entry.state_manifest.as_ref())
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
        entry.version_manifest = None;
        entry.state_manifest = None;
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
            if entry.state_manifest.is_none() {
                if let Some(url) = &entry.state_url {
                    resources.push(CoreResourceRequest {
                        id: format!("live_feeds/state/{product}/{version}"),
                        address: live_feed_address(url),
                        optional: false,
                    });
                }
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
                state_manifest_loaded: entry.state_manifest.is_some(),
            })
            .collect();
        products.sort_by(|left, right| left.product.cmp(&right.product));
        LiveFeedsSnapshot { products }
    }

    fn invalidations(&self) -> Vec<UiInvalidation> {
        let mut invalidations = Vec::new();
        for (product, entry) in &self.products {
            if entry.state_manifest.is_none() {
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

fn split_product_version(resource_id: &str, rest: &str) -> AppResult<(String, String)> {
    let Some((product, version)) = rest.split_once('/') else {
        return Err(invalid_live_feed(format!(
            "invalid live feed resource id: {resource_id}"
        )));
    };
    Ok((product.to_string(), version.to_string()))
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
    Ok(format!("{:x}", Sha256::digest(bytes)))
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
}
