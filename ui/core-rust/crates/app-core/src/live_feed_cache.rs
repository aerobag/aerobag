use std::{
    collections::BTreeMap,
    io::{Read, Write},
};

use had_nav_kv::{
    apply_nav_kv_delta, build_nav_kv_strict, nav_kv_canonical_sha256_from_pairs, NavKvDelta,
    NavKvDeltaEntry, NavKvRoot, VERSION as NAV_KV_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{AppError, AppErrorKind, AppResult};

const CURRENT_RESOURCE_ID: &str = "live_feed_cache/current";
const CURRENT_ADDRESS: &str = "/live-feeds/current.json";
const LIVE_FEEDS_PREFIX: &str = "/live-feeds/";

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LiveFeedCache {
    current_loaded: bool,
    current: BTreeMap<String, LiveFeedCacheCurrentEntry>,
    versions: BTreeMap<String, LiveFeedCacheVersion>,
    installed: BTreeMap<String, LiveFeedInstalledState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedCacheCurrentEntry {
    pub current: String,
    pub version_manifest_url: String,
    pub state_url: String,
    pub state_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collected_at_utc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveFeedCacheVersion {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    pub state: LiveFeedPayloadRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_state: Option<LiveFeedPayloadRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_from_previous: Option<LiveFeedDeltaRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedPayloadRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub url: String,
    pub bytes: u64,
    pub blob_sha256: String,
    pub state_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedDeltaRef {
    pub from_version: String,
    pub from_state_sha256: String,
    pub to_version: String,
    pub to_state_sha256: String,
    pub url: String,
    pub bytes: u64,
    pub blob_sha256: String,
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
    Current,
    Version {
        product: String,
        version: String,
    },
    Full {
        product: String,
        version: String,
        payload_kind: Option<String>,
    },
    Delta {
        product: String,
        from_version: String,
        to_version: String,
        payload_kind: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedInstalledState {
    pub product: String,
    pub version: String,
    pub state_sha256: String,
    pub payload: LiveFeedInstalledPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedInstalledSummary {
    pub product: String,
    pub version: String,
    pub state_sha256: String,
    pub payload_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveFeedInstalledPayload {
    Json {
        bytes: Vec<u8>,
    },
    NavKv {
        manifest: Vec<u8>,
        root: Vec<u8>,
        pages: Vec<Vec<u8>>,
    },
    Opaque {
        bytes: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveFeedFetchedPayload {
    Bytes(Vec<u8>),
    NavKvMembers {
        manifest: Vec<u8>,
        root: Vec<u8>,
        pages: Vec<Vec<u8>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveFeedProductRegistry {
    drivers: BTreeMap<String, LiveFeedProductDriver>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveFeedProductDriver {
    RecordJson {
        product: String,
        records_key: String,
        count_key: Option<String>,
    },
    NavKv {
        product: String,
    },
    FullJson {
        product: String,
    },
    OpaqueFull {
        product: String,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct CurrentManifest {
    products: BTreeMap<String, LiveFeedCacheCurrentEntry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct LiveFeedCurrentEvent {
    product: String,
    version: String,
    version_manifest_url: String,
    #[serde(default)]
    state_url: Option<String>,
    #[serde(default)]
    state_sha256: Option<String>,
    #[serde(default)]
    published_at_utc: Option<String>,
    #[serde(default)]
    collected_at_utc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct LiveFeedRecordDelta {
    product: String,
    from_version: String,
    to_version: String,
    #[serde(default)]
    top_level_changed: serde_json::Map<String, Value>,
    #[serde(default)]
    top_level_removed: Vec<String>,
    #[serde(default)]
    changed: serde_json::Map<String, Value>,
    #[serde(default)]
    removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct LiveFeedNavKvDelta {
    product: String,
    from_version: String,
    to_version: String,
    from_state_sha256: String,
    to_state_sha256: String,
    entries: Vec<LiveFeedNavKvDeltaEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct LiveFeedNavKvDeltaEntry {
    key: String,
    value: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
struct NavKvInstallManifest {
    product_id: String,
    version_label: String,
    encoding: String,
    page_count: usize,
    state_sha256: String,
}

impl LiveFeedCache {
    pub fn with_installed(installed: impl IntoIterator<Item = LiveFeedInstalledState>) -> Self {
        Self {
            installed: installed
                .into_iter()
                .map(|state| (state.product.clone(), state))
                .collect(),
            ..Self::default()
        }
    }

    pub fn installed(&self, product: &str) -> Option<&LiveFeedInstalledState> {
        self.installed.get(product)
    }

    pub fn installed_states(&self) -> impl Iterator<Item = &LiveFeedInstalledState> {
        self.installed.values()
    }

    pub fn installed_summary(&self, product: &str) -> Option<LiveFeedInstalledSummary> {
        self.installed
            .get(product)
            .map(LiveFeedInstalledState::summary)
    }

    pub fn installed_payload_bytes(&self, product: &str) -> AppResult<Vec<u8>> {
        let installed = self
            .installed
            .get(product)
            .ok_or_else(|| cache_error(format!("{product} is not installed")))?;
        installed.payload_bytes()
    }

    pub fn ingest_installed_payload_bytes(
        &mut self,
        registry: &LiveFeedProductRegistry,
        summary: &LiveFeedInstalledSummary,
        bytes: &[u8],
    ) -> AppResult<()> {
        let driver = registry.required_driver(&summary.product)?;
        let installed = driver.install_persisted(summary, bytes)?;
        self.installed.insert(summary.product.clone(), installed);
        Ok(())
    }

    pub fn ingest_current(&mut self, bytes: &[u8]) -> AppResult<()> {
        let current: CurrentManifest = serde_json::from_slice(bytes).map_err(cache_json_error)?;
        self.current_loaded = true;
        for entry in current.products.values() {
            validate_live_feed_relative_url(&entry.version_manifest_url)?;
            validate_live_feed_relative_url(&entry.state_url)?;
        }
        self.current = current.products;
        self.versions.retain(|product, version| {
            self.current
                .get(product)
                .is_some_and(|entry| entry.current == version.version)
        });
        Ok(())
    }

    pub fn ingest_sse_event(&mut self, event: &crate::LiveFeedSseEvent) -> AppResult<bool> {
        let event_name = event.event.as_deref().unwrap_or("message");
        if !matches!(event_name, "live-feed-current" | "message") {
            return Ok(false);
        }
        let payload: LiveFeedCurrentEvent =
            serde_json::from_str(&event.data).map_err(cache_json_error)?;
        validate_live_feed_relative_url(&payload.version_manifest_url)?;
        let state_url = payload.state_url.unwrap_or_default();
        if !state_url.is_empty() {
            validate_live_feed_relative_url(&state_url)?;
        }
        let state_sha256 = payload.state_sha256.unwrap_or_default();
        let entry = LiveFeedCacheCurrentEntry {
            current: payload.version,
            version_manifest_url: payload.version_manifest_url,
            state_url,
            state_sha256,
            published_at_utc: payload.published_at_utc,
            collected_at_utc: payload.collected_at_utc,
        };
        let changed = self.current.get(&payload.product) != Some(&entry);
        self.current_loaded = true;
        self.current.insert(payload.product.clone(), entry);
        self.versions.retain(|product, version| {
            product != &payload.product || {
                self.current
                    .get(product)
                    .is_some_and(|entry| entry.current == version.version)
            }
        });
        Ok(changed)
    }

    pub fn ingest_version_manifest(
        &mut self,
        product: &str,
        version: &str,
        bytes: &[u8],
    ) -> AppResult<()> {
        let manifest: LiveFeedCacheVersion =
            serde_json::from_slice(bytes).map_err(cache_json_error)?;
        if manifest.product != product || manifest.version != version {
            return Err(cache_error(format!(
                "version manifest for {product}/{version} contained {}/{}",
                manifest.product, manifest.version
            )));
        }
        validate_live_feed_relative_url(&manifest.state.url)?;
        if let Some(install_state) = &manifest.install_state {
            validate_live_feed_relative_url(&install_state.url)?;
        }
        if let Some(delta) = &manifest.delta_from_previous {
            validate_live_feed_relative_url(&delta.url)?;
        }
        self.versions.insert(product.to_string(), manifest);
        Ok(())
    }

    pub fn missing_requests(
        &self,
        registry: &LiveFeedProductRegistry,
    ) -> Vec<LiveFeedCacheRequest> {
        if !self.current_loaded {
            return vec![LiveFeedCacheRequest {
                id: CURRENT_RESOURCE_ID.to_string(),
                url: CURRENT_ADDRESS.to_string(),
                kind: LiveFeedCacheRequestKind::Current,
            }];
        }

        let mut requests = Vec::new();
        for (product, current) in &self.current {
            let Some(driver) = registry.driver(product) else {
                continue;
            };
            if self
                .installed
                .get(product)
                .is_some_and(|installed| installed.version == current.current)
            {
                continue;
            }
            let Some(version) = self.versions.get(product) else {
                requests.push(LiveFeedCacheRequest {
                    id: format!("live_feed_cache/version/{product}/{}", current.current),
                    url: live_feed_address(&current.version_manifest_url),
                    kind: LiveFeedCacheRequestKind::Version {
                        product: product.clone(),
                        version: current.current.clone(),
                    },
                });
                continue;
            };
            if version.version != current.current {
                requests.push(LiveFeedCacheRequest {
                    id: format!("live_feed_cache/version/{product}/{}", current.current),
                    url: live_feed_address(&current.version_manifest_url),
                    kind: LiveFeedCacheRequestKind::Version {
                        product: product.clone(),
                        version: current.current.clone(),
                    },
                });
                continue;
            }
            if let Some(delta) = self.applicable_delta(product, version, driver) {
                requests.push(LiveFeedCacheRequest {
                    id: format!(
                        "live_feed_cache/delta/{}/{}/{}",
                        product, delta.from_version, delta.to_version
                    ),
                    url: live_feed_address(&delta.url),
                    kind: LiveFeedCacheRequestKind::Delta {
                        product: product.clone(),
                        from_version: delta.from_version.clone(),
                        to_version: delta.to_version.clone(),
                        payload_kind: Some(driver.delta_payload_kind().to_string()),
                    },
                });
                continue;
            }
            let full_ref = version.install_state.as_ref().unwrap_or(&version.state);
            requests.push(LiveFeedCacheRequest {
                id: format!("live_feed_cache/full/{product}/{}", current.current),
                url: live_feed_address(&full_ref.url),
                kind: LiveFeedCacheRequestKind::Full {
                    product: product.clone(),
                    version: current.current.clone(),
                    payload_kind: full_ref.kind.clone(),
                },
            });
        }
        requests
    }

    pub fn install_fetched_payload(
        &mut self,
        registry: &LiveFeedProductRegistry,
        request: &LiveFeedCacheRequest,
        payload: LiveFeedFetchedPayload,
    ) -> AppResult<Option<LiveFeedInstalledState>> {
        match &request.kind {
            LiveFeedCacheRequestKind::Current => {
                let LiveFeedFetchedPayload::Bytes(bytes) = payload else {
                    return Err(cache_error("current manifest must be bytes".to_string()));
                };
                self.ingest_current(&bytes)?;
                Ok(None)
            }
            LiveFeedCacheRequestKind::Version { product, version } => {
                let LiveFeedFetchedPayload::Bytes(bytes) = payload else {
                    return Err(cache_error("version manifest must be bytes".to_string()));
                };
                self.ingest_version_manifest(product, version, &bytes)?;
                Ok(None)
            }
            LiveFeedCacheRequestKind::Full {
                product, version, ..
            } => {
                let version_manifest = self.version_for(product, version)?.clone();
                let driver = registry.required_driver(product)?;
                let full_ref = version_manifest
                    .install_state
                    .as_ref()
                    .unwrap_or(&version_manifest.state);
                let installed = driver.install_full(&version_manifest, full_ref, payload)?;
                self.installed.insert(product.clone(), installed.clone());
                Ok(Some(installed))
            }
            LiveFeedCacheRequestKind::Delta {
                product,
                from_version,
                to_version,
                ..
            } => {
                let version_manifest = self.version_for(product, to_version)?.clone();
                let delta = version_manifest
                    .delta_from_previous
                    .as_ref()
                    .ok_or_else(|| {
                        cache_error(format!("version {product}/{to_version} has no delta"))
                    })?;
                if delta.from_version != *from_version || delta.to_version != *to_version {
                    return Err(cache_error(format!(
                        "requested delta {product}/{from_version}/{to_version} does not match manifest"
                    )));
                }
                let LiveFeedFetchedPayload::Bytes(bytes) = payload else {
                    return Err(cache_error("delta payload must be bytes".to_string()));
                };
                verify_blob_sha256("delta", &bytes, &delta.blob_sha256)?;
                let current = self.installed.get(product).ok_or_else(|| {
                    cache_error(format!(
                        "cannot apply {product} delta without installed state"
                    ))
                })?;
                let driver = registry.required_driver(product)?;
                let installed = driver.apply_delta(current, delta, &bytes)?;
                self.installed.insert(product.clone(), installed.clone());
                Ok(Some(installed))
            }
        }
    }

    fn version_for(&self, product: &str, version: &str) -> AppResult<&LiveFeedCacheVersion> {
        let manifest = self.versions.get(product).ok_or_else(|| {
            cache_error(format!("missing version manifest for {product}/{version}"))
        })?;
        if manifest.version != version {
            return Err(cache_error(format!(
                "loaded version manifest for {product} is {}, expected {version}",
                manifest.version
            )));
        }
        Ok(manifest)
    }

    fn applicable_delta<'a>(
        &'a self,
        product: &str,
        version: &'a LiveFeedCacheVersion,
        driver: &LiveFeedProductDriver,
    ) -> Option<&'a LiveFeedDeltaRef> {
        if !driver.supports_delta() {
            return None;
        }
        let installed = self.installed.get(product)?;
        let delta = version.delta_from_previous.as_ref()?;
        if installed.version == delta.from_version
            && installed.state_sha256 == delta.from_state_sha256
            && version.version == delta.to_version
        {
            Some(delta)
        } else {
            None
        }
    }
}

impl LiveFeedInstalledState {
    pub fn summary(&self) -> LiveFeedInstalledSummary {
        LiveFeedInstalledSummary {
            product: self.product.clone(),
            version: self.version.clone(),
            state_sha256: self.state_sha256.clone(),
            payload_kind: self.payload.kind_name().to_string(),
        }
    }

    pub fn payload_bytes(&self) -> AppResult<Vec<u8>> {
        match &self.payload {
            LiveFeedInstalledPayload::Json { bytes }
            | LiveFeedInstalledPayload::Opaque { bytes } => Ok(bytes.clone()),
            LiveFeedInstalledPayload::NavKv {
                manifest,
                root,
                pages,
            } => write_nav_kv_zip_bytes(&manifest, root, pages),
        }
    }
}

impl LiveFeedInstalledPayload {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Json { .. } => "json",
            Self::NavKv { .. } => "nav_kv_package",
            Self::Opaque { .. } => "opaque",
        }
    }
}

impl LiveFeedProductRegistry {
    pub fn new(drivers: impl IntoIterator<Item = LiveFeedProductDriver>) -> Self {
        Self {
            drivers: drivers
                .into_iter()
                .map(|driver| (driver.product().to_string(), driver))
                .collect(),
        }
    }

    pub fn driver(&self, product: &str) -> Option<&LiveFeedProductDriver> {
        self.drivers.get(product)
    }

    pub fn record_json_delta_schema(&self, product: &str) -> Option<(String, Option<String>)> {
        self.driver(product)
            .and_then(LiveFeedProductDriver::record_json_delta_schema)
            .map(|(records_key, count_key)| {
                (records_key.to_string(), count_key.map(str::to_string))
            })
    }

    fn required_driver(&self, product: &str) -> AppResult<&LiveFeedProductDriver> {
        self.driver(product).ok_or_else(|| {
            cache_error(format!(
                "no live feed product driver registered for {product}"
            ))
        })
    }
}

pub fn live_feed_product_registry() -> LiveFeedProductRegistry {
    LiveFeedProductRegistry::new([
        LiveFeedProductDriver::RecordJson {
            product: "metars".to_string(),
            records_key: "metars_by_station".to_string(),
            count_key: Some("metar_count".to_string()),
        },
        LiveFeedProductDriver::RecordJson {
            product: "tafs".to_string(),
            records_key: "tafs_by_station".to_string(),
            count_key: Some("taf_count".to_string()),
        },
        LiveFeedProductDriver::FullJson {
            product: "tfrs".to_string(),
        },
        LiveFeedProductDriver::FullJson {
            product: "winds-aloft".to_string(),
        },
        LiveFeedProductDriver::NavKv {
            product: "obstacles".to_string(),
        },
        LiveFeedProductDriver::OpaqueFull {
            product: "nexrad".to_string(),
        },
    ])
}

impl LiveFeedProductDriver {
    pub fn product(&self) -> &str {
        match self {
            Self::RecordJson { product, .. }
            | Self::NavKv { product }
            | Self::FullJson { product }
            | Self::OpaqueFull { product } => product,
        }
    }

    fn supports_delta(&self) -> bool {
        matches!(self, Self::RecordJson { .. } | Self::NavKv { .. })
    }

    fn record_json_delta_schema(&self) -> Option<(&str, Option<&str>)> {
        match self {
            Self::RecordJson {
                records_key,
                count_key,
                ..
            } => Some((records_key, count_key.as_deref())),
            Self::NavKv { .. } | Self::FullJson { .. } | Self::OpaqueFull { .. } => None,
        }
    }

    fn delta_payload_kind(&self) -> &'static str {
        match self {
            Self::RecordJson { .. } => "record_json_delta",
            Self::NavKv { .. } => "nav_kv_delta",
            Self::FullJson { .. } | Self::OpaqueFull { .. } => "none",
        }
    }

    fn install_full(
        &self,
        version: &LiveFeedCacheVersion,
        payload_ref: &LiveFeedPayloadRef,
        payload: LiveFeedFetchedPayload,
    ) -> AppResult<LiveFeedInstalledState> {
        match self {
            Self::RecordJson { product, .. } | Self::FullJson { product } => {
                let LiveFeedFetchedPayload::Bytes(bytes) = payload else {
                    return Err(cache_error(format!("{product} full state must be bytes")));
                };
                verify_blob_sha256("full state", &bytes, &payload_ref.blob_sha256)?;
                let value: Value = serde_json::from_slice(&bytes).map_err(cache_json_error)?;
                let actual_state_sha256 = canonical_json_sha256(&value)?;
                if actual_state_sha256 != payload_ref.state_sha256 {
                    return Err(cache_error(format!(
                        "{product} full state hash mismatch: expected {}, got {}",
                        payload_ref.state_sha256, actual_state_sha256
                    )));
                }
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: version.version.clone(),
                    state_sha256: actual_state_sha256,
                    payload: LiveFeedInstalledPayload::Json { bytes },
                })
            }
            Self::NavKv { product } => {
                let (manifest, root, pages) = match payload {
                    LiveFeedFetchedPayload::NavKvMembers {
                        manifest,
                        root,
                        pages,
                    } => (manifest, root, pages),
                    LiveFeedFetchedPayload::Bytes(bytes) => {
                        verify_blob_sha256(
                            "nav_kv full package",
                            &bytes,
                            &payload_ref.blob_sha256,
                        )?;
                        read_nav_kv_members_from_zip(product, &bytes)?
                    }
                };
                verify_nav_kv_state(
                    product,
                    &version.version,
                    payload_ref,
                    manifest,
                    root,
                    pages,
                )
            }
            Self::OpaqueFull { product } => {
                let LiveFeedFetchedPayload::Bytes(bytes) = payload else {
                    return Err(cache_error(format!("{product} full state must be bytes")));
                };
                verify_blob_sha256("full state", &bytes, &payload_ref.blob_sha256)?;
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: version.version.clone(),
                    state_sha256: payload_ref.state_sha256.clone(),
                    payload: LiveFeedInstalledPayload::Opaque { bytes },
                })
            }
        }
    }

    fn install_persisted(
        &self,
        summary: &LiveFeedInstalledSummary,
        bytes: &[u8],
    ) -> AppResult<LiveFeedInstalledState> {
        match self {
            Self::RecordJson { product, .. } | Self::FullJson { product } => {
                if summary.product != *product || summary.payload_kind != "json" {
                    return Err(cache_error(format!(
                        "{product} persisted payload metadata is not JSON"
                    )));
                }
                let value: Value = serde_json::from_slice(bytes).map_err(cache_json_error)?;
                let actual_state_sha256 = canonical_json_sha256(&value)?;
                if actual_state_sha256 != summary.state_sha256 {
                    return Err(cache_error(format!(
                        "{product} persisted state hash mismatch: expected {}, got {actual_state_sha256}",
                        summary.state_sha256
                    )));
                }
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: summary.version.clone(),
                    state_sha256: actual_state_sha256,
                    payload: LiveFeedInstalledPayload::Json {
                        bytes: bytes.to_vec(),
                    },
                })
            }
            Self::NavKv { product } => {
                if summary.product != *product || summary.payload_kind != "nav_kv_package" {
                    return Err(cache_error(format!(
                        "{product} persisted payload metadata is not nav_kv"
                    )));
                }
                let (manifest, root, pages) = read_nav_kv_members_from_zip(product, bytes)?;
                let payload_ref = LiveFeedPayloadRef {
                    kind: Some("nav_kv_package".to_string()),
                    url: String::new(),
                    bytes: bytes.len() as u64,
                    blob_sha256: sha256_hex(bytes),
                    state_sha256: summary.state_sha256.clone(),
                };
                verify_nav_kv_state(
                    product,
                    &summary.version,
                    &payload_ref,
                    manifest,
                    root,
                    pages,
                )
            }
            Self::OpaqueFull { product } => {
                if summary.product != *product || summary.payload_kind != "opaque" {
                    return Err(cache_error(format!(
                        "{product} persisted payload metadata is not opaque"
                    )));
                }
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: summary.version.clone(),
                    state_sha256: summary.state_sha256.clone(),
                    payload: LiveFeedInstalledPayload::Opaque {
                        bytes: bytes.to_vec(),
                    },
                })
            }
        }
    }

    fn apply_delta(
        &self,
        installed: &LiveFeedInstalledState,
        delta_ref: &LiveFeedDeltaRef,
        bytes: &[u8],
    ) -> AppResult<LiveFeedInstalledState> {
        match self {
            Self::RecordJson {
                product,
                records_key,
                count_key,
            } => {
                if installed.product != *product
                    || installed.version != delta_ref.from_version
                    || installed.state_sha256 != delta_ref.from_state_sha256
                {
                    return Err(cache_error(format!(
                        "{product} installed state does not match delta source"
                    )));
                }
                let LiveFeedInstalledPayload::Json { bytes: state_bytes } = &installed.payload
                else {
                    return Err(cache_error(format!(
                        "{product} installed state is not JSON"
                    )));
                };
                let from_state: Value =
                    serde_json::from_slice(state_bytes).map_err(cache_json_error)?;
                let delta: LiveFeedRecordDelta =
                    serde_json::from_slice(bytes).map_err(cache_json_error)?;
                let next = apply_record_json_delta(
                    records_key,
                    count_key.as_deref(),
                    &from_state,
                    &delta,
                )?;
                let next_sha256 = canonical_json_sha256(&next)?;
                if next_sha256 != delta_ref.to_state_sha256 {
                    return Err(cache_error(format!(
                        "{product} delta target hash mismatch: expected {}, got {}",
                        delta_ref.to_state_sha256, next_sha256
                    )));
                }
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: delta_ref.to_version.clone(),
                    state_sha256: next_sha256,
                    payload: LiveFeedInstalledPayload::Json {
                        bytes: serde_json::to_vec(&next).map_err(cache_json_error)?,
                    },
                })
            }
            Self::NavKv { product } => {
                if installed.product != *product
                    || installed.version != delta_ref.from_version
                    || installed.state_sha256 != delta_ref.from_state_sha256
                {
                    return Err(cache_error(format!(
                        "{product} installed state does not match delta source"
                    )));
                }
                let LiveFeedInstalledPayload::NavKv {
                    manifest,
                    root,
                    pages,
                } = &installed.payload
                else {
                    return Err(cache_error(format!(
                        "{product} installed state is not nav_kv"
                    )));
                };
                let root = NavKvRoot::parse(root)
                    .map_err(|err| cache_error(format!("failed to parse {product} root: {err}")))?;
                let current_pairs = root
                    .pairs(|page| pages.get(page as usize).cloned())
                    .ok_or_else(|| cache_error(format!("failed to read {product} nav_kv pairs")))?;
                let delta: LiveFeedNavKvDelta =
                    serde_json::from_slice(bytes).map_err(cache_json_error)?;
                if delta.product != *product
                    || delta.from_version != delta_ref.from_version
                    || delta.to_version != delta_ref.to_version
                    || delta.from_state_sha256 != delta_ref.from_state_sha256
                    || delta.to_state_sha256 != delta_ref.to_state_sha256
                {
                    return Err(cache_error(format!(
                        "{product} nav_kv delta metadata does not match manifest"
                    )));
                }
                let nav_delta = NavKvDelta {
                    entries: delta
                        .entries
                        .into_iter()
                        .map(|entry| NavKvDeltaEntry {
                            key: entry.key,
                            value: entry.value,
                        })
                        .collect(),
                };
                let next_pairs = apply_nav_kv_delta(&current_pairs, &nav_delta).map_err(|err| {
                    cache_error(format!("failed to apply {product} delta: {err}"))
                })?;
                let next_sha256 = nav_kv_canonical_sha256_from_pairs(&next_pairs);
                if next_sha256 != delta_ref.to_state_sha256 {
                    return Err(cache_error(format!(
                        "{product} nav_kv delta target hash mismatch: expected {}, got {}",
                        delta_ref.to_state_sha256, next_sha256
                    )));
                }
                let built = build_nav_kv_strict(next_pairs, root.page_size()).map_err(|err| {
                    cache_error(format!("failed to rebuild {product} nav_kv state: {err}"))
                })?;
                let manifest = updated_nav_kv_manifest_bytes(
                    product,
                    &delta_ref.to_version,
                    manifest,
                    &built.root_bytes,
                    &built.pages,
                    &next_sha256,
                )?;
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: delta_ref.to_version.clone(),
                    state_sha256: next_sha256,
                    payload: LiveFeedInstalledPayload::NavKv {
                        manifest,
                        root: built.root_bytes,
                        pages: built.pages,
                    },
                })
            }
            Self::FullJson { product } | Self::OpaqueFull { product } => {
                Err(cache_error(format!("{product} does not support deltas")))
            }
        }
    }
}

fn verify_nav_kv_state(
    product: &str,
    version: &str,
    payload_ref: &LiveFeedPayloadRef,
    manifest: Vec<u8>,
    root: Vec<u8>,
    pages: Vec<Vec<u8>>,
) -> AppResult<LiveFeedInstalledState> {
    let parsed_manifest: NavKvInstallManifest =
        serde_json::from_slice(&manifest).map_err(cache_json_error)?;
    if parsed_manifest.product_id != product || parsed_manifest.version_label != version {
        return Err(cache_error(format!(
            "nav_kv manifest contained {}/{}, expected {product}/{version}",
            parsed_manifest.product_id, parsed_manifest.version_label
        )));
    }
    let expected_encoding = format!("had-nav-kv-v{NAV_KV_VERSION}");
    if parsed_manifest.encoding != expected_encoding {
        return Err(cache_error(format!(
            "unsupported {product} nav_kv encoding {}, expected {expected_encoding}",
            parsed_manifest.encoding
        )));
    }
    if parsed_manifest.state_sha256 != payload_ref.state_sha256 {
        return Err(cache_error(format!(
            "{product} nav_kv manifest hash {} did not match payload ref {}",
            parsed_manifest.state_sha256, payload_ref.state_sha256
        )));
    }
    if pages.len() != parsed_manifest.page_count {
        return Err(cache_error(format!(
            "{product} nav_kv page count mismatch: manifest {}, payload {}",
            parsed_manifest.page_count,
            pages.len()
        )));
    }
    let parsed_root = NavKvRoot::parse(&root)
        .map_err(|err| cache_error(format!("failed to parse {product} root: {err}")))?;
    if parsed_root.page_count() as usize != pages.len() {
        return Err(cache_error(format!(
            "{product} nav_kv root page count mismatch: root {}, payload {}",
            parsed_root.page_count(),
            pages.len()
        )));
    }
    let actual = parsed_root
        .canonical_sha256(|page| pages.get(page as usize).cloned())
        .ok_or_else(|| cache_error(format!("failed to hash {product} nav_kv payload")))?;
    if actual != payload_ref.state_sha256 {
        return Err(cache_error(format!(
            "{product} nav_kv state hash mismatch: expected {}, got {actual}",
            payload_ref.state_sha256
        )));
    }
    Ok(LiveFeedInstalledState {
        product: product.to_string(),
        version: version.to_string(),
        state_sha256: actual,
        payload: LiveFeedInstalledPayload::NavKv {
            manifest,
            root,
            pages,
        },
    })
}

fn updated_nav_kv_manifest_bytes(
    product: &str,
    version: &str,
    prior_manifest: &[u8],
    root: &[u8],
    pages: &[Vec<u8>],
    state_sha256: &str,
) -> AppResult<Vec<u8>> {
    let parsed_root = NavKvRoot::parse(root).map_err(|err| {
        cache_error(format!(
            "failed to parse {product} nav_kv root for manifest update: {err}"
        ))
    })?;
    let mut value: Value = serde_json::from_slice(prior_manifest).map_err(cache_json_error)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| cache_error(format!("{product} nav_kv manifest must be an object")))?;
    object.insert("product_id".to_string(), Value::String(product.to_string()));
    object.insert(
        "version_label".to_string(),
        Value::String(version.to_string()),
    );
    object.insert(
        "encoding".to_string(),
        Value::String(format!("had-nav-kv-v{NAV_KV_VERSION}")),
    );
    object
        .entry("root".to_string())
        .or_insert_with(|| Value::String("root".to_string()));
    object
        .entry("page_path_template".to_string())
        .or_insert_with(|| Value::String("page_{page:04}".to_string()));
    object.insert("page_count".to_string(), serde_json::json!(pages.len()));
    object.insert(
        "page_size".to_string(),
        serde_json::json!(parsed_root.page_size()),
    );
    object.insert(
        "state_sha256".to_string(),
        Value::String(state_sha256.to_string()),
    );
    serde_json::to_vec_pretty(&value).map_err(cache_json_error)
}

fn write_nav_kv_zip_bytes(manifest: &[u8], root: &[u8], pages: &[Vec<u8>]) -> AppResult<Vec<u8>> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    write_zip_member(&mut writer, "manifest.json", manifest)?;
    write_zip_member(&mut writer, "root", root)?;
    for (index, page) in pages.iter().enumerate() {
        write_zip_member(&mut writer, &format!("page_{index:04}"), page)?;
    }
    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|err| cache_error(format!("failed to finish nav_kv zip: {err}")))
}

fn write_zip_member<W: std::io::Write + std::io::Seek>(
    writer: &mut zip::ZipWriter<W>,
    name: &str,
    bytes: &[u8],
) -> AppResult<()> {
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    writer
        .start_file(name, options)
        .map_err(|err| cache_error(format!("failed to add {name} to zip: {err}")))?;
    writer
        .write_all(bytes)
        .map_err(|err| cache_error(format!("failed to write {name} to zip: {err}")))
}

fn read_nav_kv_members_from_zip(
    product: &str,
    bytes: &[u8],
) -> AppResult<(Vec<u8>, Vec<u8>, Vec<Vec<u8>>)> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|err| cache_error(format!("failed to read {product} nav_kv zip: {err}")))?;
    let manifest = read_zip_member(&mut archive, "manifest.json")?;
    let root = read_zip_member(&mut archive, "root")?;
    let manifest_value: serde_json::Value =
        serde_json::from_slice(&manifest).map_err(cache_json_error)?;
    let page_count = manifest_value
        .get("page_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| cache_error(format!("{product} nav_kv manifest missing page_count")))?;
    let mut pages = Vec::new();
    for page in 0..page_count {
        pages.push(read_zip_member(&mut archive, &format!("page_{page:04}"))?);
    }
    Ok((manifest, root, pages))
}

fn read_zip_member<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> AppResult<Vec<u8>> {
    let mut member = archive
        .by_name(name)
        .map_err(|err| cache_error(format!("zip missing {name}: {err}")))?;
    let mut bytes = Vec::new();
    member
        .read_to_end(&mut bytes)
        .map_err(|err| cache_error(format!("failed to read zip member {name}: {err}")))?;
    Ok(bytes)
}

fn apply_record_json_delta(
    records_key: &str,
    count_key: Option<&str>,
    from_state: &Value,
    delta: &LiveFeedRecordDelta,
) -> AppResult<Value> {
    let from_version = from_state
        .get("version_label")
        .and_then(Value::as_str)
        .ok_or_else(|| cache_error("live feed state missing version_label".to_string()))?;
    if from_version != delta.from_version {
        return Err(cache_error(format!(
            "delta starts at {}, but local state is {from_version}",
            delta.from_version
        )));
    }
    let mut result = from_state.clone();
    {
        let result_object = result
            .as_object_mut()
            .ok_or_else(|| cache_error("live feed state must be a JSON object".to_string()))?;
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
            .ok_or_else(|| cache_error(format!("state missing {records_key} object")))?;
        for record_id in &delta.removed {
            records.remove(record_id);
        }
        for (record_id, record) in &delta.changed {
            records.insert(record_id.clone(), record.clone());
        }
        records.len()
    };
    let version = result
        .get_mut("version_label")
        .ok_or_else(|| cache_error("live feed state missing version_label".to_string()))?;
    *version = Value::String(delta.to_version.clone());
    if let Some(count_key) = count_key {
        if let Some(count) = result.get_mut(count_key) {
            *count = serde_json::json!(record_count);
        }
    }
    Ok(result)
}

fn verify_blob_sha256(label: &str, bytes: &[u8], expected: &str) -> AppResult<()> {
    let actual = sha256_hex(bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(cache_error(format!(
            "{label} blob hash mismatch: expected {expected}, got {actual}"
        )))
    }
}

fn live_feed_address(relative_url: &str) -> String {
    format!(
        "{LIVE_FEEDS_PREFIX}{}",
        relative_url.trim_start_matches('/')
    )
}

fn validate_live_feed_relative_url(url: &str) -> AppResult<()> {
    if url.starts_with('/') || url.contains("://") || url.split('/').any(|part| part == "..") {
        return Err(cache_error(format!(
            "live feed URL must be package-relative: {url}"
        )));
    }
    Ok(())
}

fn canonical_json_sha256(value: &Value) -> AppResult<String> {
    let bytes = serde_json::to_vec(value).map_err(cache_json_error)?;
    Ok(sha256_hex(&bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn cache_json_error(err: impl std::fmt::Display) -> AppError {
    cache_error(err.to_string())
}

fn cache_error(message: String) -> AppError {
    AppError {
        kind: AppErrorKind::InvalidManifest,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use had_nav_kv::NavKvPair;

    fn metar_state(version: &str, records: &[(&str, &str)]) -> Value {
        let mut metars = serde_json::Map::new();
        for (station, raw_text) in records {
            metars.insert(
                station.to_string(),
                serde_json::json!({
                    "station_id": station,
                    "raw_text": raw_text
                }),
            );
        }
        serde_json::json!({
            "version_label": version,
            "metar_count": metars.len(),
            "metars_by_station": metars
        })
    }

    fn current_manifest(product: &str, version: &str, state_sha256: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "products": {
                product: {
                    "current": version,
                    "version_manifest_url": format!("versions/{product}/{version}.json"),
                    "state_url": format!("states/{product}/{version}.json"),
                    "state_sha256": state_sha256
                }
            }
        }))
        .unwrap()
    }

    fn json_version_manifest(
        product: &str,
        version: &str,
        state: &Value,
        delta: Option<LiveFeedDeltaRef>,
    ) -> (Vec<u8>, Vec<u8>, String) {
        let state_bytes = serde_json::to_vec(state).unwrap();
        let state_sha256 = canonical_json_sha256(state).unwrap();
        let manifest = serde_json::json!({
            "schema_version": 1,
            "product": product,
            "version": version,
            "state": {
                "url": format!("states/{product}/{version}.json"),
                "bytes": state_bytes.len(),
                "blob_sha256": sha256_hex(&state_bytes),
                "state_sha256": state_sha256
            },
            "delta_from_previous": delta
        });
        (
            serde_json::to_vec(&manifest).unwrap(),
            state_bytes,
            state_sha256,
        )
    }

    #[test]
    fn product_registry_describes_record_json_delta_schema() {
        let registry = live_feed_product_registry();
        assert_eq!(
            registry.record_json_delta_schema("metars"),
            Some((
                "metars_by_station".to_string(),
                Some("metar_count".to_string())
            ))
        );
        assert_eq!(
            registry.record_json_delta_schema("tafs"),
            Some(("tafs_by_station".to_string(), Some("taf_count".to_string())))
        );
        assert_eq!(registry.record_json_delta_schema("tfrs"), None);
    }

    #[test]
    fn record_json_cache_installs_full_then_delta() {
        let registry = live_feed_product_registry();
        let v1 = metar_state("v1", &[("KSEA", "old"), ("KOLM", "old")]);
        let (v1_manifest, v1_bytes, v1_sha) = json_version_manifest("metars", "v1", &v1, None);
        let mut cache = LiveFeedCache::default();
        cache
            .ingest_current(&current_manifest("metars", "v1", &v1_sha))
            .unwrap();
        assert_eq!(
            cache.missing_requests(&registry)[0].kind,
            LiveFeedCacheRequestKind::Version {
                product: "metars".to_string(),
                version: "v1".to_string()
            }
        );
        cache
            .ingest_version_manifest("metars", "v1", &v1_manifest)
            .unwrap();
        let request = cache.missing_requests(&registry).remove(0);
        cache
            .install_fetched_payload(&registry, &request, LiveFeedFetchedPayload::Bytes(v1_bytes))
            .unwrap();
        assert_eq!(cache.installed("metars").unwrap().version, "v1");

        let v2 = metar_state("v2", &[("KSEA", "new"), ("KPAE", "new")]);
        let v2_sha = canonical_json_sha256(&v2).unwrap();
        let delta = serde_json::json!({
            "schema_version": 1,
            "product": "metars",
            "from_version": "v1",
            "to_version": "v2",
            "top_level_changed": {},
            "top_level_removed": [],
            "changed": {
                "KSEA": {"station_id": "KSEA", "raw_text": "new"},
                "KPAE": {"station_id": "KPAE", "raw_text": "new"}
            },
            "removed": ["KOLM"]
        });
        let delta_bytes = serde_json::to_vec(&delta).unwrap();
        let delta_ref = LiveFeedDeltaRef {
            from_version: "v1".to_string(),
            from_state_sha256: v1_sha,
            to_version: "v2".to_string(),
            to_state_sha256: v2_sha.clone(),
            url: "deltas/metars/v1__v2.json".to_string(),
            bytes: delta_bytes.len() as u64,
            blob_sha256: sha256_hex(&delta_bytes),
        };
        let (v2_manifest, _, _) =
            json_version_manifest("metars", "v2", &v2, Some(delta_ref.clone()));
        cache
            .ingest_current(&current_manifest("metars", "v2", &v2_sha))
            .unwrap();
        cache
            .ingest_version_manifest("metars", "v2", &v2_manifest)
            .unwrap();
        let request = cache.missing_requests(&registry).remove(0);
        assert_eq!(
            request.kind,
            LiveFeedCacheRequestKind::Delta {
                product: "metars".to_string(),
                from_version: "v1".to_string(),
                to_version: "v2".to_string(),
                payload_kind: Some("record_json_delta".to_string())
            }
        );
        let installed = cache
            .install_fetched_payload(
                &registry,
                &request,
                LiveFeedFetchedPayload::Bytes(delta_bytes),
            )
            .unwrap()
            .unwrap();
        assert_eq!(installed.version, "v2");
        assert_eq!(installed.state_sha256, v2_sha);
    }

    #[test]
    fn taf_record_json_cache_installs_full_then_delta() {
        let registry = live_feed_product_registry();
        let v1 = serde_json::json!({
            "version_label": "v1",
            "taf_count": 1,
            "tafs_by_station": {
                "KSEA": {"station_id": "KSEA", "raw_text": "old"}
            }
        });
        let (v1_manifest, v1_bytes, v1_sha) = json_version_manifest("tafs", "v1", &v1, None);
        let mut cache = LiveFeedCache::default();
        cache
            .ingest_current(&current_manifest("tafs", "v1", &v1_sha))
            .unwrap();
        cache
            .ingest_version_manifest("tafs", "v1", &v1_manifest)
            .unwrap();
        let request = cache.missing_requests(&registry).remove(0);
        cache
            .install_fetched_payload(&registry, &request, LiveFeedFetchedPayload::Bytes(v1_bytes))
            .unwrap();

        let v2 = serde_json::json!({
            "version_label": "v2",
            "taf_count": 1,
            "tafs_by_station": {
                "KSEA": {"station_id": "KSEA", "raw_text": "new"}
            }
        });
        let v2_sha = canonical_json_sha256(&v2).unwrap();
        let delta = serde_json::json!({
            "schema_version": 1,
            "product": "tafs",
            "from_version": "v1",
            "to_version": "v2",
            "top_level_changed": {},
            "top_level_removed": [],
            "changed": {
                "KSEA": {"station_id": "KSEA", "raw_text": "new"}
            },
            "removed": []
        });
        let delta_bytes = serde_json::to_vec(&delta).unwrap();
        let delta_ref = LiveFeedDeltaRef {
            from_version: "v1".to_string(),
            from_state_sha256: v1_sha,
            to_version: "v2".to_string(),
            to_state_sha256: v2_sha.clone(),
            url: "deltas/tafs/v1__v2.json".to_string(),
            bytes: delta_bytes.len() as u64,
            blob_sha256: sha256_hex(&delta_bytes),
        };
        let (v2_manifest, _, _) = json_version_manifest("tafs", "v2", &v2, Some(delta_ref));
        cache
            .ingest_current(&current_manifest("tafs", "v2", &v2_sha))
            .unwrap();
        cache
            .ingest_version_manifest("tafs", "v2", &v2_manifest)
            .unwrap();
        let request = cache.missing_requests(&registry).remove(0);
        assert_eq!(
            request.kind,
            LiveFeedCacheRequestKind::Delta {
                product: "tafs".to_string(),
                from_version: "v1".to_string(),
                to_version: "v2".to_string(),
                payload_kind: Some("record_json_delta".to_string())
            }
        );
        let installed = cache
            .install_fetched_payload(
                &registry,
                &request,
                LiveFeedFetchedPayload::Bytes(delta_bytes),
            )
            .unwrap()
            .unwrap();
        assert_eq!(installed.version, "v2");
        assert_eq!(installed.state_sha256, v2_sha);
    }

    #[test]
    fn nav_kv_cache_installs_full_then_delta_with_deletes() {
        let registry = live_feed_product_registry();
        let first_pairs = vec![
            NavKvPair {
                key: "obstacle/tile/z01/x000001/y000001".to_string(),
                value: b"old-a".to_vec(),
            },
            NavKvPair {
                key: "obstacle/tile/z01/x000001/y000002".to_string(),
                value: b"old-b".to_vec(),
            },
        ];
        let second_pairs = vec![
            NavKvPair {
                key: "obstacle/tile/z01/x000001/y000001".to_string(),
                value: b"new-a".to_vec(),
            },
            NavKvPair {
                key: "obstacle/tile/z01/x000001/y000003".to_string(),
                value: b"new-c".to_vec(),
            },
        ];
        let first = build_nav_kv_strict(first_pairs.clone(), 1024).unwrap();
        let first_sha = nav_kv_canonical_sha256_from_pairs(&first_pairs);
        let first_manifest = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "product_id": "obstacles",
            "version_label": "v1",
            "encoding": format!("had-nav-kv-v{NAV_KV_VERSION}"),
            "page_count": first.pages.len(),
            "state_sha256": first_sha
        }))
        .unwrap();
        let version_manifest = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "product": "obstacles",
            "version": "v1",
            "state": {
                "kind": "nav_kv",
                "url": "states/obstacles/v1/manifest.json",
                "bytes": 123,
                "blob_sha256": "unused",
                "state_sha256": first_sha
            },
            "install_state": {
                "kind": "nav_kv_package",
                "url": "packages/obstacles/v1.zip",
                "bytes": 123,
                "blob_sha256": "unused",
                "state_sha256": first_sha
            }
        }))
        .unwrap();
        let mut cache = LiveFeedCache::default();
        cache
            .ingest_current(&current_manifest("obstacles", "v1", &first_sha))
            .unwrap();
        cache
            .ingest_version_manifest("obstacles", "v1", &version_manifest)
            .unwrap();
        let request = cache.missing_requests(&registry).remove(0);
        assert_eq!(
            request.kind,
            LiveFeedCacheRequestKind::Full {
                product: "obstacles".to_string(),
                version: "v1".to_string(),
                payload_kind: Some("nav_kv_package".to_string())
            }
        );
        cache
            .install_fetched_payload(
                &registry,
                &request,
                LiveFeedFetchedPayload::NavKvMembers {
                    manifest: first_manifest,
                    root: first.root_bytes,
                    pages: first.pages,
                },
            )
            .unwrap();

        let nav_delta = had_nav_kv::build_nav_kv_delta(&first_pairs, &second_pairs).unwrap();
        let second_sha = nav_kv_canonical_sha256_from_pairs(&second_pairs);
        let delta_value = serde_json::json!({
            "schema_version": 1,
            "product": "obstacles",
            "from_version": "v1",
            "to_version": "v2",
            "from_state_sha256": first_sha,
            "to_state_sha256": second_sha,
            "entries": nav_delta.entries.iter().map(|entry| {
                serde_json::json!({
                    "key": entry.key,
                    "value": entry.value
                })
            }).collect::<Vec<_>>()
        });
        let delta_bytes = serde_json::to_vec(&delta_value).unwrap();
        let second_manifest = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "product": "obstacles",
            "version": "v2",
            "state": {
                "kind": "nav_kv",
                "url": "states/obstacles/v2/manifest.json",
                "bytes": 123,
                "blob_sha256": "unused",
                "state_sha256": second_sha
            },
            "install_state": {
                "kind": "nav_kv_package",
                "url": "packages/obstacles/v2.zip",
                "bytes": 123,
                "blob_sha256": "unused",
                "state_sha256": second_sha
            },
            "delta_from_previous": {
                "from_version": "v1",
                "from_state_sha256": first_sha,
                "to_version": "v2",
                "to_state_sha256": second_sha,
                "url": "deltas/obstacles/v1__v2.nav-kv-delta.json",
                "bytes": delta_bytes.len(),
                "blob_sha256": sha256_hex(&delta_bytes)
            }
        }))
        .unwrap();
        cache
            .ingest_current(&current_manifest("obstacles", "v2", &second_sha))
            .unwrap();
        cache
            .ingest_version_manifest("obstacles", "v2", &second_manifest)
            .unwrap();
        let request = cache.missing_requests(&registry).remove(0);
        assert_eq!(
            request.kind,
            LiveFeedCacheRequestKind::Delta {
                product: "obstacles".to_string(),
                from_version: "v1".to_string(),
                to_version: "v2".to_string(),
                payload_kind: Some("nav_kv_delta".to_string())
            }
        );
        let installed = cache
            .install_fetched_payload(
                &registry,
                &request,
                LiveFeedFetchedPayload::Bytes(delta_bytes),
            )
            .unwrap()
            .unwrap();
        assert_eq!(installed.version, "v2");
        assert_eq!(installed.state_sha256, second_sha);
        let LiveFeedInstalledPayload::NavKv {
            manifest,
            root,
            pages,
        } = installed.payload
        else {
            panic!("expected nav_kv payload");
        };
        let manifest: Value = serde_json::from_slice(&manifest).unwrap();
        assert_eq!(manifest["product_id"], "obstacles");
        assert_eq!(manifest["version_label"], "v2");
        assert_eq!(manifest["state_sha256"], second_sha);
        let root = NavKvRoot::parse(&root).unwrap();
        let pairs = root
            .pairs(|page| pages.get(page as usize).cloned())
            .unwrap();
        assert_eq!(pairs, second_pairs);
    }
}
