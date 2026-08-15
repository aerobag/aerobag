// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeMap, io::Read, sync::Arc};

use had_nav_kv::{
    apply_nav_kv_delta, build_nav_kv_strict, nav_kv_canonical_sha256_from_pairs, NavKvDelta,
    NavKvDeltaEntry, NavKvRoot, VERSION as NAV_KV_VERSION,
};
#[cfg(test)]
use notam_state::NotamState;
use notam_state::{NotamApplyWork, NotamCheckpoint, NotamDelta};
use product_contracts::{live_feeds::v3 as live_feeds_v3, versioned_json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    live_feed_runtime_decision, AppError, AppErrorKind, AppResult, LiveFeedDurableInstalledProduct,
    LiveFeedRuntimeDecision, LiveFeedRuntimeInput, LiveFeedRuntimeState, LiveFeedsState,
    NotamProjectionPreparer, PreparedLiveFeedEnvelope, PreparedLiveFeedPayload,
    PreparedNotamPayload,
};

pub use crate::live_feeds::{
    LiveFeedCacheRequest, LiveFeedCacheRequestKind, LiveFeedDeltaRef, LiveFeedPayloadRef,
    NEXRAD_FRAME_WINDOW_SIZE,
};

#[derive(Debug, Default)]
pub struct LiveFeedCache {
    live_feeds: LiveFeedsState,
    runtime: LiveFeedRuntimeState,
    installed: BTreeMap<String, BTreeMap<String, LiveFeedInstalledState>>,
    pending_installed: BTreeMap<String, LiveFeedInstalledState>,
    restoring_resources: BTreeMap<String, RestoringLiveFeedResources>,
    notam_preparer: NotamProjectionPreparer,
    pending_notam_prepared: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedInstalledState {
    pub product: String,
    pub version: String,
    pub state_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collected_at_utc: Option<String>,
    pub payload: LiveFeedInstalledPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedInstalledSummary {
    pub product: String,
    pub version: String,
    pub state_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collected_at_utc: Option<String>,
    pub payload_kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedResourceManifest {
    pub summary: LiveFeedInstalledSummary,
    pub resources: Vec<LiveFeedResourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveFeedResourceRef {
    pub kind: String,
    pub blob_sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RestoringLiveFeedResources {
    manifest: LiveFeedResourceManifest,
    resources: BTreeMap<String, Vec<u8>>,
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
    NexradPackage {
        manifest: Vec<u8>,
        package_blob_sha256: String,
        package_bytes: Option<Arc<Vec<u8>>>,
    },
    NotamResources {
        checkpoint: Arc<Vec<u8>>,
        deltas: Vec<Arc<Vec<u8>>>,
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
    NexradPackage {
        product: String,
    },
    Notam {
        product: String,
    },
}

type LiveFeedRecordDelta = live_feeds_v3::RecordDelta;
type LiveFeedNavKvDelta = live_feeds_v3::NavKvDelta;

#[derive(Debug, Deserialize)]
struct NavKvInstallManifest {
    product_id: String,
    version_label: String,
    encoding: String,
    page_count: usize,
    state_sha256: String,
}

impl LiveFeedCache {
    pub fn with_source_root_url_and_installed(
        source_root_url: &str,
        installed: impl IntoIterator<Item = LiveFeedInstalledState>,
    ) -> AppResult<Self> {
        let mut cache = Self::default();
        cache.live_feeds.set_source_root_url(source_root_url)?;
        for state in installed {
            cache.stage_or_remember_installed_state(state);
        }
        Ok(cache)
    }

    pub fn set_source_root_url(&mut self, source_root_url: &str) -> AppResult<String> {
        self.live_feeds.set_source_root_url(source_root_url)
    }

    pub fn installed(&self, product: &str) -> Option<&LiveFeedInstalledState> {
        let states = self.installed.get(product)?;
        if product == "nexrad" {
            self.live_feeds
                .current_product_version(product)
                .and_then(|version| states.get(version))
                .or_else(|| {
                    states
                        .values()
                        .max_by_key(|state| installed_retention_key(state))
                })
        } else {
            states.last_key_value().map(|(_, state)| state)
        }
    }

    pub fn installed_states(&self) -> impl Iterator<Item = &LiveFeedInstalledState> {
        self.installed.values().flat_map(BTreeMap::values)
    }

    pub fn live_feeds_state(&self) -> &LiveFeedsState {
        &self.live_feeds
    }

    pub fn installed_summary(&self, product: &str) -> Option<LiveFeedInstalledSummary> {
        self.installed(product).map(LiveFeedInstalledState::summary)
    }

    pub fn retained_summaries(&self, product: &str) -> Vec<LiveFeedInstalledSummary> {
        self.installed
            .get(product)
            .into_iter()
            .flat_map(BTreeMap::values)
            .map(LiveFeedInstalledState::summary)
            .collect()
    }

    pub fn release_persisted_payload_bytes(
        &mut self,
        product: &str,
        version: &str,
    ) -> AppResult<()> {
        let installed = self
            .installed
            .get_mut(product)
            .and_then(|states| states.get_mut(version))
            .ok_or_else(|| cache_error(format!("{product}/{version} is not installed")))?;
        if let LiveFeedInstalledPayload::NexradPackage { package_bytes, .. } =
            &mut installed.payload
        {
            *package_bytes = None;
        }
        Ok(())
    }

    pub fn installed_payload_bytes(&self, product: &str, version: &str) -> AppResult<Vec<u8>> {
        let installed = self
            .pending_installed
            .get(product)
            .filter(|installed| installed.version == version)
            .or_else(|| self.installed.get(product)?.get(version))
            .ok_or_else(|| cache_error(format!("{product}/{version} is not installed")))?;
        installed.payload_bytes()
    }

    pub fn resource_manifest(&self, product: &str) -> AppResult<Option<LiveFeedResourceManifest>> {
        let installed = self
            .install_candidate(product)
            .ok_or_else(|| cache_error(format!("{product} is not installed")))?;
        Ok(installed.resource_manifest())
    }

    pub fn resource_bytes(&self, product: &str, blob_sha256: &str) -> AppResult<Vec<u8>> {
        let installed = self
            .install_candidate(product)
            .ok_or_else(|| cache_error(format!("{product} is not installed")))?;
        installed.resource_bytes(blob_sha256).ok_or_else(|| {
            cache_error(format!("{product} has no immutable resource {blob_sha256}"))
        })
    }

    pub fn begin_restoring_resources(
        &mut self,
        manifest: LiveFeedResourceManifest,
    ) -> AppResult<()> {
        validate_resource_manifest(&manifest)?;
        let product = manifest.summary.product.clone();
        if self.restoring_resources.contains_key(&product) {
            return Err(cache_error(format!(
                "resource restoration already active for {product}"
            )));
        }
        self.restoring_resources.insert(
            product,
            RestoringLiveFeedResources {
                manifest,
                resources: BTreeMap::new(),
            },
        );
        Ok(())
    }

    pub fn restore_resource_bytes(
        &mut self,
        product: &str,
        blob_sha256: &str,
        bytes: &[u8],
    ) -> AppResult<()> {
        let restoring = self.restoring_resources.get_mut(product).ok_or_else(|| {
            cache_error(format!("resource restoration is not active for {product}"))
        })?;
        let resource_ref = restoring
            .manifest
            .resources
            .iter()
            .find(|resource| resource.blob_sha256 == blob_sha256)
            .ok_or_else(|| {
                cache_error(format!(
                    "{product} restoration does not reference {blob_sha256}"
                ))
            })?;
        if resource_ref.bytes != bytes.len() as u64 {
            return Err(cache_error(format!(
                "{product} resource {blob_sha256} has {} bytes, expected {}",
                bytes.len(),
                resource_ref.bytes
            )));
        }
        verify_blob_sha256("persisted resource", bytes, blob_sha256)?;
        restoring
            .resources
            .insert(blob_sha256.to_string(), bytes.to_vec());
        Ok(())
    }

    pub fn finish_restoring_resources(
        &mut self,
        registry: &LiveFeedProductRegistry,
        product: &str,
    ) -> AppResult<()> {
        let restoring = self.restoring_resources.remove(product).ok_or_else(|| {
            cache_error(format!("resource restoration is not active for {product}"))
        })?;
        let result = (|| {
            let driver = registry.required_driver(product)?;
            let installed = driver.install_persisted_resources(&restoring)?;
            if product == "notams" {
                self.prepare_restored_notam_candidate(&installed)?;
            }
            self.stage_or_remember_installed_state(installed);
            Ok(())
        })();
        if result.is_err() {
            self.restoring_resources
                .insert(product.to_string(), restoring);
        }
        result
    }

    pub fn install_candidate(&self, product: &str) -> Option<&LiveFeedInstalledState> {
        self.pending_installed
            .get(product)
            .or_else(|| self.installed(product))
    }

    pub fn install_candidate_for_main(
        &self,
        product: &str,
        version: &str,
    ) -> AppResult<LiveFeedInstalledState> {
        let candidate = self.install_candidate_state(product, version)?;
        if product == "notams" {
            return Err(cache_error(
                "NOTAM candidates must cross the session boundary as prepared projections"
                    .to_string(),
            ));
        }
        Ok(candidate)
    }

    pub fn install_candidate_state(
        &self,
        product: &str,
        version: &str,
    ) -> AppResult<LiveFeedInstalledState> {
        self.pending_installed
            .get(product)
            .filter(|candidate| candidate.version == version)
            .or_else(|| self.installed.get(product)?.get(version))
            .cloned()
            .ok_or_else(|| cache_error(format!("{product}/{version} is not installed")))
    }

    pub fn prepared_install_candidate(
        &self,
        product: &str,
        version: &str,
    ) -> AppResult<Option<Vec<u8>>> {
        if product != "notams" {
            return Ok(None);
        }
        let candidate = self
            .pending_installed
            .get(product)
            .filter(|candidate| candidate.version == version)
            .or_else(|| self.installed.get(product)?.get(version))
            .ok_or_else(|| cache_error(format!("{product}/{version} is not installed")))?;
        if candidate.version != version {
            return Err(cache_error(format!(
                "prepared {product} candidate does not match requested version {version}"
            )));
        }
        self.pending_notam_prepared
            .clone()
            .map(Some)
            .ok_or_else(|| {
                cache_error(format!(
                    "prepared {product}/{version} projection is unavailable"
                ))
            })
    }

    pub fn acknowledge_install_candidate(&mut self, product: &str, version: &str) -> AppResult<()> {
        let Some(candidate) = self.pending_installed.remove(product) else {
            return self
                .installed
                .get(product)
                .and_then(|states| states.get(version))
                .map(|_| ())
                .ok_or_else(|| {
                    cache_error(format!(
                        "no pending {product}/{version} live-feed candidate to acknowledge"
                    ))
                });
        };
        if candidate.version != version {
            self.pending_installed
                .insert(product.to_string(), candidate);
            return Err(cache_error(format!(
                "pending {product} candidate does not match acknowledged version {version}"
            )));
        }
        if product == "notams" {
            self.pending_notam_prepared = None;
        }
        self.remember_installed_state(candidate);
        Ok(())
    }

    pub fn reject_install_candidate(&mut self, product: &str) {
        self.pending_installed.remove(product);
        if product == "notams" {
            self.pending_notam_prepared = None;
            self.notam_preparer.reset();
            self.installed.remove(product);
            self.live_feeds.mark_product_no_state(product);
        }
    }

    pub fn ingest_installed_payload_bytes(
        &mut self,
        registry: &LiveFeedProductRegistry,
        summary: &LiveFeedInstalledSummary,
        bytes: &[u8],
    ) -> AppResult<()> {
        let driver = registry.required_driver(&summary.product)?;
        let installed = driver.install_persisted(summary, bytes)?;
        self.stage_or_remember_installed_state(installed);
        Ok(())
    }

    pub fn ingest_current(&mut self, bytes: &[u8]) -> AppResult<()> {
        self.live_feeds
            .ingest_resource("live_feeds/current", bytes)?;
        self.prune_nexrad_to_catalog();
        Ok(())
    }

    pub fn ingest_sse_event(&mut self, event: &crate::LiveFeedSseEvent) -> AppResult<bool> {
        let changed = !self
            .live_feeds
            .ingest_sse_events(std::iter::once(event.clone()))?
            .is_empty();
        self.prune_nexrad_to_catalog();
        Ok(changed)
    }

    pub fn ingest_version_manifest(
        &mut self,
        product: &str,
        version: &str,
        bytes: &[u8],
    ) -> AppResult<()> {
        self.live_feeds
            .ingest_resource(&format!("live_feeds/version/{product}/{version}"), bytes)
    }

    pub fn missing_requests(&self) -> Vec<LiveFeedCacheRequest> {
        self.live_feeds
            .durable_missing_requests(self.installed_states().map(|installed| {
                LiveFeedDurableInstalledProduct {
                    product: installed.product.clone(),
                    version: installed.version.clone(),
                    state_sha256: installed.state_sha256.clone(),
                }
            }))
    }

    pub fn missing_requests_at_epoch_ms(&self, epoch_ms: i64) -> Vec<LiveFeedCacheRequest> {
        self.live_feeds.durable_missing_requests_at_epoch_ms(
            self.installed_states()
                .map(|installed| LiveFeedDurableInstalledProduct {
                    product: installed.product.clone(),
                    version: installed.version.clone(),
                    state_sha256: installed.state_sha256.clone(),
                }),
            epoch_ms,
        )
    }

    pub fn current_refresh_requests_at_epoch_ms(&self, epoch_ms: i64) -> Vec<LiveFeedCacheRequest> {
        self.live_feeds
            .durable_current_refresh_requests_at_epoch_ms(epoch_ms)
    }

    pub fn record_request_failure(&mut self, request_id: &str, epoch_ms: i64) {
        self.live_feeds
            .record_resource_failure(request_id, epoch_ms);
    }

    pub fn runtime_decision(&mut self, input: LiveFeedRuntimeInput) -> LiveFeedRuntimeDecision {
        live_feed_runtime_decision(&mut self.runtime, input)
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
                self.live_feeds
                    .ingest_durable_request_resource(request, &bytes)?;
                Ok(None)
            }
            LiveFeedCacheRequestKind::Version { product, version } => {
                let LiveFeedFetchedPayload::Bytes(bytes) = payload else {
                    return Err(cache_error("version manifest must be bytes".to_string()));
                };
                let _ = (product, version);
                self.live_feeds
                    .ingest_durable_request_resource(request, &bytes)?;
                Ok(None)
            }
            LiveFeedCacheRequestKind::Full {
                product, version, ..
            } => {
                let driver = registry.required_driver(product)?;
                let full_ref = self
                    .live_feeds
                    .durable_full_payload_ref_for_request(product, version)?;
                let mut installed = driver.install_full(product, version, full_ref, payload)?;
                installed.collected_at_utc = self
                    .live_feeds
                    .product_collected_at_utc_for_version(product, version)
                    .map(str::to_string);
                if product == "notams" {
                    self.prepare_full_notam_candidate(&installed)?;
                }
                self.stage_fetched_installed_state(installed.clone());
                Ok(Some(installed))
            }
            LiveFeedCacheRequestKind::Delta {
                product,
                from_version,
                to_version,
                ..
            } => {
                let delta = self.live_feeds.durable_delta_ref_for_request(
                    product,
                    from_version,
                    to_version,
                )?;
                let LiveFeedFetchedPayload::Bytes(bytes) = payload else {
                    return Err(cache_error("delta payload must be bytes".to_string()));
                };
                verify_blob_sha256("delta", &bytes, &delta.blob_sha256)?;
                let current = self.installed(product).ok_or_else(|| {
                    cache_error(format!(
                        "cannot apply {product} delta without installed state"
                    ))
                })?;
                let driver = registry.required_driver(product)?;
                let mut installed = driver.apply_delta(current, delta, &bytes)?;
                installed.collected_at_utc = self
                    .live_feeds
                    .product_collected_at_utc_for_version(product, to_version)
                    .map(str::to_string);
                if product == "notams" {
                    self.prepare_notam_delta_candidate(&installed, &bytes)?;
                }
                self.stage_fetched_installed_state(installed.clone());
                Ok(Some(installed))
            }
        }
    }

    fn remember_installed_state(&mut self, installed: LiveFeedInstalledState) {
        self.live_feeds.mark_durable_product_loaded(
            installed.product.clone(),
            installed.version.clone(),
            installed.state_sha256.clone(),
            installed.collected_at_utc.clone(),
            state_manifest_for_installed(&installed),
        );
        let product = installed.product.clone();
        let states = self.installed.entry(product.clone()).or_default();
        if product != "nexrad" {
            states.clear();
        }
        states.insert(installed.version.clone(), installed);
        while states.len() > durable_retention_count(&product) {
            let Some(oldest) = states
                .values()
                .min_by_key(|state| installed_retention_key(state))
                .map(|state| state.version.clone())
            else {
                break;
            };
            states.remove(&oldest);
        }
        self.prune_nexrad_to_catalog();
    }

    fn stage_or_remember_installed_state(&mut self, installed: LiveFeedInstalledState) {
        if installed.product == "notams" {
            self.pending_installed
                .insert(installed.product.clone(), installed);
        } else {
            self.remember_installed_state(installed);
        }
    }

    fn stage_fetched_installed_state(&mut self, installed: LiveFeedInstalledState) {
        if installed.product == "nexrad" || installed.product == "notams" {
            self.pending_installed
                .insert(installed.product.clone(), installed);
        } else {
            self.remember_installed_state(installed);
        }
    }

    fn prune_nexrad_to_catalog(&mut self) {
        if !self.live_feeds.current_loaded() {
            return;
        }
        let retained = self
            .live_feeds
            .durable_retained_versions("nexrad")
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        if let Some(states) = self.installed.get_mut("nexrad") {
            states.retain(|version, _| retained.contains(version));
        }
    }

    fn prepare_full_notam_candidate(
        &mut self,
        installed: &LiveFeedInstalledState,
    ) -> AppResult<()> {
        let LiveFeedInstalledPayload::NotamResources { checkpoint, deltas } = &installed.payload
        else {
            return Err(cache_error(
                "NOTAM candidate is not an immutable resource chain".to_string(),
            ));
        };
        if !deltas.is_empty() {
            return Err(cache_error(
                "fresh NOTAM checkpoint unexpectedly contains deltas".to_string(),
            ));
        }
        let checkpoint = decode_notam_checkpoint(Some("notam_checkpoint_xz"), checkpoint)?;
        self.notam_preparer.reset();
        let payload = self
            .notam_preparer
            .install_checkpoint(checkpoint, &mut NotamApplyWork::default())
            .map_err(|error| cache_error(format!("invalid NOTAM checkpoint: {error}")))?;
        self.require_prepared_notam_target(installed)?;
        self.pending_notam_prepared = Some(encode_cached_notam_envelope(
            format!("live_feeds/state/notams/{}", installed.version),
            installed,
            None,
            None,
            payload,
        )?);
        Ok(())
    }

    fn prepare_notam_delta_candidate(
        &mut self,
        installed: &LiveFeedInstalledState,
        delta_bytes: &[u8],
    ) -> AppResult<()> {
        let delta = decode_notam_delta(Some("notam_ordered_delta_xz"), delta_bytes)?;
        let from_version = delta.from_state_id.clone();
        let payload = self
            .notam_preparer
            .apply_delta(delta, &mut NotamApplyWork::default())
            .map_err(|error| cache_error(format!("invalid NOTAM delta: {error}")))?;
        self.require_prepared_notam_target(installed)?;
        self.pending_notam_prepared = Some(encode_cached_notam_envelope(
            format!(
                "live_feeds/delta/notams/{from_version}/{}",
                installed.version
            ),
            installed,
            Some(from_version.clone()),
            Some(from_version),
            payload,
        )?);
        Ok(())
    }

    fn prepare_restored_notam_candidate(
        &mut self,
        installed: &LiveFeedInstalledState,
    ) -> AppResult<()> {
        let LiveFeedInstalledPayload::NotamResources { checkpoint, deltas } = &installed.payload
        else {
            return Err(cache_error(
                "restored NOTAM candidate is not an immutable resource chain".to_string(),
            ));
        };
        self.notam_preparer.reset();
        let checkpoint = decode_notam_checkpoint(Some("notam_checkpoint_xz"), checkpoint)?;
        self.notam_preparer
            .install_checkpoint(checkpoint, &mut NotamApplyWork::default())
            .map_err(|error| cache_error(format!("invalid persisted NOTAM checkpoint: {error}")))?;
        for bytes in deltas {
            let delta = decode_notam_delta(Some("notam_ordered_delta_xz"), bytes)?;
            self.notam_preparer
                .apply_delta(delta, &mut NotamApplyWork::default())
                .map_err(|error| cache_error(format!("invalid persisted NOTAM delta: {error}")))?;
        }
        self.require_prepared_notam_target(installed)?;
        let checkpoint = self
            .notam_preparer
            .projection_checkpoint()
            .ok_or_else(|| cache_error("restored NOTAM projection is unavailable".to_string()))?;
        self.pending_notam_prepared = Some(encode_cached_notam_envelope(
            format!("live_feeds/state/notams/{}", installed.version),
            installed,
            None,
            None,
            PreparedNotamPayload::InstallAirportCheckpoint(checkpoint),
        )?);
        Ok(())
    }

    fn require_prepared_notam_target(&self, installed: &LiveFeedInstalledState) -> AppResult<()> {
        let actual = self.notam_preparer.state_id().ok_or_else(|| {
            cache_error("canonical NOTAM worker state is unavailable".to_string())
        })?;
        if actual != installed.version || actual != installed.state_sha256 {
            return Err(cache_error(format!(
                "prepared NOTAM state ended at {actual}, expected {}/{}",
                installed.version, installed.state_sha256
            )));
        }
        Ok(())
    }
}

fn encode_cached_notam_envelope(
    resource_id: String,
    installed: &LiveFeedInstalledState,
    from_version: Option<String>,
    from_state_sha256: Option<String>,
    payload: PreparedNotamPayload,
) -> AppResult<Vec<u8>> {
    postcard::to_allocvec(&PreparedLiveFeedEnvelope {
        schema_version: 1,
        resource_id,
        product: installed.product.clone(),
        version: installed.version.clone(),
        state_sha256: installed.state_sha256.clone(),
        from_version,
        from_state_sha256,
        delta_blob_sha256: None,
        payload: PreparedLiveFeedPayload::Notams(payload),
    })
    .map_err(|error| {
        cache_error(format!(
            "failed to encode prepared NOTAM projection: {error}"
        ))
    })
}

fn installed_retention_key(installed: &LiveFeedInstalledState) -> (Option<String>, String) {
    let observed_at_utc = match &installed.payload {
        LiveFeedInstalledPayload::NexradPackage { manifest, .. } => {
            serde_json::from_slice::<Value>(manifest)
                .ok()
                .and_then(|manifest| {
                    manifest
                        .get("observed_at_utc")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
        }
        _ => None,
    };
    (observed_at_utc, installed.version.clone())
}

fn durable_retention_count(product: &str) -> usize {
    if product == "nexrad" {
        NEXRAD_FRAME_WINDOW_SIZE
    } else {
        1
    }
}

impl LiveFeedInstalledState {
    pub fn summary(&self) -> LiveFeedInstalledSummary {
        LiveFeedInstalledSummary {
            product: self.product.clone(),
            version: self.version.clone(),
            state_sha256: self.state_sha256.clone(),
            collected_at_utc: self.collected_at_utc.clone(),
            payload_kind: self.payload.kind_name().to_string(),
            blob_sha256: match &self.payload {
                LiveFeedInstalledPayload::NexradPackage {
                    package_blob_sha256,
                    ..
                } => Some(package_blob_sha256.clone()),
                _ => None,
            },
        }
    }

    pub fn payload_bytes(&self) -> AppResult<Vec<u8>> {
        match &self.payload {
            LiveFeedInstalledPayload::Json { bytes } => Ok(bytes.clone()),
            LiveFeedInstalledPayload::NexradPackage {
                package_bytes: Some(bytes),
                ..
            } => Ok(bytes.as_ref().clone()),
            LiveFeedInstalledPayload::NexradPackage {
                package_bytes: None,
                ..
            } => Err(cache_error(
                "NEXRAD package bytes have been released after persistence".to_string(),
            )),
            LiveFeedInstalledPayload::NavKv {
                manifest,
                root,
                pages,
            } => write_nav_kv_zip_bytes(&manifest, root, pages),
            LiveFeedInstalledPayload::NotamResources { .. } => Err(cache_error(
                "NOTAM resources must be persisted as immutable blobs".to_string(),
            )),
        }
    }

    fn resource_manifest(&self) -> Option<LiveFeedResourceManifest> {
        let LiveFeedInstalledPayload::NotamResources { checkpoint, deltas } = &self.payload else {
            return None;
        };
        let mut resources = Vec::with_capacity(1 + deltas.len());
        resources.push(resource_ref("notam_checkpoint_xz", checkpoint));
        resources.extend(
            deltas
                .iter()
                .map(|delta| resource_ref("notam_ordered_delta_xz", delta)),
        );
        Some(LiveFeedResourceManifest {
            summary: self.summary(),
            resources,
        })
    }

    fn resource_bytes(&self, blob_sha256: &str) -> Option<Vec<u8>> {
        let LiveFeedInstalledPayload::NotamResources { checkpoint, deltas } = &self.payload else {
            return None;
        };
        if sha256_hex(checkpoint) == blob_sha256 {
            return Some(checkpoint.as_ref().clone());
        }
        deltas
            .iter()
            .find(|delta| sha256_hex(delta) == blob_sha256)
            .map(|delta| delta.as_ref().clone())
    }
}

fn resource_ref(kind: &str, bytes: &[u8]) -> LiveFeedResourceRef {
    LiveFeedResourceRef {
        kind: kind.to_string(),
        blob_sha256: sha256_hex(bytes),
        bytes: bytes.len() as u64,
    }
}

fn validate_resource_manifest(manifest: &LiveFeedResourceManifest) -> AppResult<()> {
    if manifest.summary.payload_kind != "notam_resources" {
        return Err(cache_error(format!(
            "{} does not use immutable resource persistence",
            manifest.summary.product
        )));
    }
    if manifest.resources.is_empty() {
        return Err(cache_error(
            "immutable resource manifest is empty".to_string(),
        ));
    }
    if manifest.resources[0].kind != "notam_checkpoint_xz" {
        return Err(cache_error(
            "immutable NOTAM resources must begin with a checkpoint".to_string(),
        ));
    }
    if manifest.resources[1..]
        .iter()
        .any(|resource| resource.kind != "notam_ordered_delta_xz")
    {
        return Err(cache_error(
            "immutable NOTAM resources contain an unsupported delta kind".to_string(),
        ));
    }
    let mut hashes = std::collections::BTreeSet::new();
    if manifest
        .resources
        .iter()
        .any(|resource| !hashes.insert(resource.blob_sha256.as_str()))
    {
        return Err(cache_error(
            "immutable resource manifest contains duplicate blob hashes".to_string(),
        ));
    }
    Ok(())
}

fn state_manifest_for_installed(installed: &LiveFeedInstalledState) -> Option<Value> {
    match &installed.payload {
        LiveFeedInstalledPayload::Json { bytes } => serde_json::from_slice(bytes).ok(),
        LiveFeedInstalledPayload::NavKv { manifest, .. } => serde_json::from_slice(manifest).ok(),
        LiveFeedInstalledPayload::NexradPackage { manifest, .. } => {
            serde_json::from_slice(manifest).ok()
        }
        LiveFeedInstalledPayload::NotamResources { .. } => None,
    }
}

impl LiveFeedInstalledPayload {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Json { .. } => "json",
            Self::NavKv { .. } => "nav_kv_package",
            Self::NexradPackage { .. } => "nexrad_package",
            Self::NotamResources { .. } => "notam_resources",
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
        LiveFeedProductDriver::RecordJson {
            product: "pireps".to_string(),
            records_key: "pireps_by_id".to_string(),
            count_key: Some("pirep_count".to_string()),
        },
        LiveFeedProductDriver::Notam {
            product: "notams".to_string(),
        },
        LiveFeedProductDriver::FullJson {
            product: "tfrs".to_string(),
        },
        LiveFeedProductDriver::NavKv {
            product: "winds-aloft".to_string(),
        },
        LiveFeedProductDriver::NavKv {
            product: "obstacles".to_string(),
        },
        LiveFeedProductDriver::NexradPackage {
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
            | Self::NexradPackage { product }
            | Self::Notam { product } => product,
        }
    }

    fn record_json_delta_schema(&self) -> Option<(&str, Option<&str>)> {
        match self {
            Self::RecordJson {
                records_key,
                count_key,
                ..
            } => Some((records_key, count_key.as_deref())),
            Self::NavKv { .. }
            | Self::FullJson { .. }
            | Self::NexradPackage { .. }
            | Self::Notam { .. } => None,
        }
    }

    fn install_full(
        &self,
        product_id: &str,
        version: &str,
        payload_ref: &LiveFeedPayloadRef,
        payload: LiveFeedFetchedPayload,
    ) -> AppResult<LiveFeedInstalledState> {
        match self {
            Self::RecordJson { product, .. } | Self::FullJson { product } => {
                if product != product_id {
                    return Err(cache_error(format!(
                        "live-feed driver {product} cannot install {product_id}"
                    )));
                }
                let LiveFeedFetchedPayload::Bytes(bytes) = payload else {
                    return Err(cache_error(format!("{product} full state must be bytes")));
                };
                verify_blob_sha256(
                    "full state",
                    &bytes,
                    required_blob_sha256("full state", product, payload_ref)?,
                )?;
                let decoded_bytes =
                    decode_live_feed_cache_payload(payload_ref.kind.as_deref(), &bytes)?;
                let value: Value =
                    serde_json::from_slice(decoded_bytes.as_ref()).map_err(cache_json_error)?;
                let actual_state_sha256 = canonical_json_sha256(&value)?;
                if actual_state_sha256 != payload_ref.state_sha256 {
                    return Err(cache_error(format!(
                        "{product} full state hash mismatch: expected {}, got {}",
                        payload_ref.state_sha256, actual_state_sha256
                    )));
                }
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: version.to_string(),
                    state_sha256: actual_state_sha256,
                    collected_at_utc: None,
                    payload: LiveFeedInstalledPayload::Json {
                        bytes: decoded_bytes.into_owned(),
                    },
                })
            }
            Self::NavKv { product } => {
                if product != product_id {
                    return Err(cache_error(format!(
                        "live-feed driver {product} cannot install {product_id}"
                    )));
                }
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
                            required_blob_sha256("nav_kv full package", product, payload_ref)?,
                        )?;
                        read_nav_kv_members_from_zip(product, &bytes)?
                    }
                };
                verify_nav_kv_state(product, version, payload_ref, manifest, root, pages)
            }
            Self::Notam { product } => {
                if product != product_id {
                    return Err(cache_error(format!(
                        "live-feed driver {product} cannot install {product_id}"
                    )));
                }
                let LiveFeedFetchedPayload::Bytes(bytes) = payload else {
                    return Err(cache_error("NOTAM checkpoint must be bytes".to_string()));
                };
                verify_blob_sha256(
                    "NOTAM checkpoint",
                    &bytes,
                    required_blob_sha256("checkpoint", product, payload_ref)?,
                )?;
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: version.to_string(),
                    state_sha256: version.to_string(),
                    collected_at_utc: None,
                    payload: LiveFeedInstalledPayload::NotamResources {
                        checkpoint: Arc::new(bytes),
                        deltas: Vec::new(),
                    },
                })
            }
            Self::NexradPackage { product } => {
                if product != product_id {
                    return Err(cache_error(format!(
                        "live-feed driver {product} cannot install {product_id}"
                    )));
                }
                let LiveFeedFetchedPayload::Bytes(bytes) = payload else {
                    return Err(cache_error(format!("{product} full state must be bytes")));
                };
                verify_blob_sha256(
                    "full state",
                    &bytes,
                    required_blob_sha256("full state", product, payload_ref)?,
                )?;
                let manifest = read_nexrad_package_manifest(&bytes)?;
                let manifest_value: Value =
                    serde_json::from_slice(&manifest).map_err(cache_json_error)?;
                if manifest_value.get("state_id").and_then(Value::as_str) != Some(version) {
                    return Err(cache_error(format!(
                        "NEXRAD package manifest state_id does not match {version}"
                    )));
                }
                let actual_state_sha256 = canonical_json_sha256(&manifest_value)?;
                if actual_state_sha256 != payload_ref.state_sha256 {
                    return Err(cache_error(format!(
                        "NEXRAD manifest hash mismatch: expected {}, got {actual_state_sha256}",
                        payload_ref.state_sha256
                    )));
                }
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: version.to_string(),
                    state_sha256: actual_state_sha256,
                    collected_at_utc: None,
                    payload: LiveFeedInstalledPayload::NexradPackage {
                        manifest,
                        package_blob_sha256: sha256_hex(&bytes),
                        package_bytes: Some(Arc::new(bytes)),
                    },
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
                    collected_at_utc: summary.collected_at_utc.clone(),
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
                let mut installed = verify_nav_kv_state(
                    product,
                    &summary.version,
                    &payload_ref,
                    manifest,
                    root,
                    pages,
                )?;
                installed.collected_at_utc = summary.collected_at_utc.clone();
                Ok(installed)
            }
            Self::NexradPackage { product } => {
                if summary.product != *product || summary.payload_kind != "nexrad_package" {
                    return Err(cache_error(format!(
                        "{product} persisted payload metadata is not a NEXRAD package"
                    )));
                }
                let expected_blob_sha256 = summary.blob_sha256.as_deref().ok_or_else(|| {
                    cache_error("persisted NEXRAD package metadata has no blob hash".to_string())
                })?;
                verify_blob_sha256("persisted NEXRAD package", bytes, expected_blob_sha256)?;
                let manifest = read_nexrad_package_manifest(bytes)?;
                let manifest_value: Value =
                    serde_json::from_slice(&manifest).map_err(cache_json_error)?;
                let actual_state_sha256 = canonical_json_sha256(&manifest_value)?;
                if manifest_value.get("state_id").and_then(Value::as_str)
                    != Some(summary.version.as_str())
                    || actual_state_sha256 != summary.state_sha256
                {
                    return Err(cache_error(format!(
                        "persisted NEXRAD package does not match {}/{}",
                        summary.version, summary.state_sha256
                    )));
                }
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: summary.version.clone(),
                    state_sha256: actual_state_sha256,
                    collected_at_utc: summary.collected_at_utc.clone(),
                    payload: LiveFeedInstalledPayload::NexradPackage {
                        manifest,
                        package_blob_sha256: expected_blob_sha256.to_string(),
                        package_bytes: Some(Arc::new(bytes.to_vec())),
                    },
                })
            }
            Self::Notam { product } => {
                let _ = (product, summary, bytes);
                Err(cache_error(
                    "NOTAM cache requires immutable resource restoration".to_string(),
                ))
            }
        }
    }

    fn install_persisted_resources(
        &self,
        restoring: &RestoringLiveFeedResources,
    ) -> AppResult<LiveFeedInstalledState> {
        let Self::Notam { product } = self else {
            return Err(cache_error(format!(
                "{} does not support immutable resource persistence",
                self.product()
            )));
        };
        if restoring.manifest.summary.product != *product {
            return Err(cache_error(format!(
                "resource manifest is for {}, not {product}",
                restoring.manifest.summary.product
            )));
        }
        let ordered = restoring
            .manifest
            .resources
            .iter()
            .map(|resource| {
                restoring
                    .resources
                    .get(&resource.blob_sha256)
                    .cloned()
                    .ok_or_else(|| {
                        cache_error(format!(
                            "missing persisted {product} resource {}",
                            resource.blob_sha256
                        ))
                    })
            })
            .collect::<AppResult<Vec<_>>>()?;
        let (checkpoint, deltas) = ordered
            .split_first()
            .ok_or_else(|| cache_error("immutable resource manifest is empty".to_string()))?;
        let summary = &restoring.manifest.summary;
        Ok(LiveFeedInstalledState {
            product: product.clone(),
            version: summary.version.clone(),
            state_sha256: summary.state_sha256.clone(),
            collected_at_utc: summary.collected_at_utc.clone(),
            payload: LiveFeedInstalledPayload::NotamResources {
                checkpoint: Arc::new(checkpoint.clone()),
                deltas: deltas.iter().cloned().map(Arc::new).collect(),
            },
        })
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
                let decoded_bytes =
                    decode_live_feed_cache_payload(delta_ref.kind.as_deref(), bytes)?;
                let delta = versioned_json::decode_exact::<LiveFeedRecordDelta>(
                    "live-feed record delta",
                    decoded_bytes.as_ref(),
                    product_contracts::LIVE_FEEDS_SCHEMA_VERSION,
                )
                .map_err(|error| cache_error(error.to_string()))?;
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
                    collected_at_utc: None,
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
                let decoded_bytes =
                    decode_live_feed_cache_payload(delta_ref.kind.as_deref(), bytes)?;
                let delta = versioned_json::decode_exact::<LiveFeedNavKvDelta>(
                    "live-feed nav_kv delta",
                    decoded_bytes.as_ref(),
                    product_contracts::LIVE_FEEDS_SCHEMA_VERSION,
                )
                .map_err(|error| cache_error(error.to_string()))?;
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
                    collected_at_utc: None,
                    payload: LiveFeedInstalledPayload::NavKv {
                        manifest,
                        root: built.root_bytes,
                        pages: built.pages,
                    },
                })
            }
            Self::Notam { product } => {
                if installed.product != *product
                    || installed.version != delta_ref.from_version
                    || installed.state_sha256 != delta_ref.from_state_sha256
                {
                    return Err(cache_error(
                        "NOTAM installed head does not match delta source".to_string(),
                    ));
                }
                let LiveFeedInstalledPayload::NotamResources { checkpoint, deltas } =
                    &installed.payload
                else {
                    return Err(cache_error(
                        "NOTAM installed payload is not a resource chain".to_string(),
                    ));
                };
                let delta = decode_notam_delta(delta_ref.kind.as_deref(), bytes)?;
                if delta.from_state_id != delta_ref.from_state_sha256
                    || delta.to_state_id != delta_ref.to_state_sha256
                    || delta.from_state_id != delta_ref.from_version
                    || delta.to_state_id != delta_ref.to_version
                    || delta_ref.mutation_count != Some(delta.mutations.len() as u64)
                {
                    return Err(cache_error(
                        "NOTAM delta payload does not match manifest reference".to_string(),
                    ));
                }
                let mut next_deltas = deltas.clone();
                next_deltas.push(Arc::new(bytes.to_vec()));
                Ok(LiveFeedInstalledState {
                    product: product.clone(),
                    version: delta.to_state_id.clone(),
                    state_sha256: delta.to_state_id,
                    collected_at_utc: None,
                    payload: LiveFeedInstalledPayload::NotamResources {
                        checkpoint: checkpoint.clone(),
                        deltas: next_deltas,
                    },
                })
            }
            Self::FullJson { product } | Self::NexradPackage { product } => {
                Err(cache_error(format!("{product} does not support deltas")))
            }
        }
    }
}

fn read_nexrad_package_manifest(bytes: &[u8]) -> AppResult<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| cache_error(format!("failed to open NEXRAD package: {error}")))?;
    let mut member = archive
        .by_name("manifest.json")
        .map_err(|error| cache_error(format!("NEXRAD package has no manifest.json: {error}")))?;
    let mut manifest = Vec::new();
    member
        .read_to_end(&mut manifest)
        .map_err(|error| cache_error(format!("failed to read NEXRAD manifest: {error}")))?;
    Ok(manifest)
}

fn decode_notam_checkpoint(payload_kind: Option<&str>, bytes: &[u8]) -> AppResult<NotamCheckpoint> {
    if payload_kind != Some("notam_checkpoint_xz") {
        return Err(cache_error(format!(
            "unsupported NOTAM checkpoint kind {payload_kind:?}"
        )));
    }
    let decoded = nav_kv_package::decode_xz_if_needed(bytes).map_err(cache_error)?;
    let checkpoint: NotamCheckpoint =
        serde_json::from_slice(decoded.as_ref()).map_err(cache_json_error)?;
    checkpoint
        .validate_contract()
        .map_err(|error| cache_error(error.to_string()))?;
    Ok(checkpoint)
}

fn decode_notam_delta(payload_kind: Option<&str>, bytes: &[u8]) -> AppResult<NotamDelta> {
    if payload_kind != Some("notam_ordered_delta_xz") {
        return Err(cache_error(format!(
            "unsupported NOTAM delta kind {payload_kind:?}"
        )));
    }
    let decoded = nav_kv_package::decode_xz_if_needed(bytes).map_err(cache_error)?;
    let delta: NotamDelta = serde_json::from_slice(decoded.as_ref()).map_err(cache_json_error)?;
    delta
        .validate_contract()
        .map_err(|error| cache_error(error.to_string()))?;
    Ok(delta)
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
        collected_at_utc: None,
        payload: LiveFeedInstalledPayload::NavKv {
            manifest,
            root,
            pages,
        },
    })
}

fn required_blob_sha256<'a>(
    _label: &str,
    _product: &str,
    payload_ref: &'a LiveFeedPayloadRef,
) -> AppResult<&'a str> {
    Ok(payload_ref.blob_sha256.as_str())
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
    nav_kv_package::write_stored_xz_framed_package_bytes(manifest, root, pages).map_err(cache_error)
}

fn read_nav_kv_members_from_zip(
    product: &str,
    bytes: &[u8],
) -> AppResult<(Vec<u8>, Vec<u8>, Vec<Vec<u8>>)> {
    let members = nav_kv_package::read_package_bytes(product, bytes).map_err(cache_error)?;
    Ok((members.manifest, members.root, members.pages))
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

fn canonical_json_sha256(value: &Value) -> AppResult<String> {
    let bytes = serde_json::to_vec(value).map_err(cache_json_error)?;
    Ok(sha256_hex(&bytes))
}

fn decode_live_feed_cache_payload<'a>(
    payload_kind: Option<&str>,
    bytes: &'a [u8],
) -> AppResult<std::borrow::Cow<'a, [u8]>> {
    match payload_kind {
        Some("json_xz") | Some("record_json_delta_xz") | Some("nav_kv_delta_xz") => {
            nav_kv_package::decode_xz_if_needed(bytes).map_err(cache_error)
        }
        _ => Ok(std::borrow::Cow::Borrowed(bytes)),
    }
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
    use std::io::{Cursor, Read, Write};
    use zip::CompressionMethod;

    const TEST_LIVE_FEED_ROOT: &str = "http://live.test";

    fn live_feed_cache() -> LiveFeedCache {
        LiveFeedCache::with_source_root_url_and_installed(TEST_LIVE_FEED_ROOT, std::iter::empty())
            .unwrap()
    }

    #[test]
    fn explicitly_adopted_single_state_product_replaces_newer_timestamp_from_old_source() {
        let mut cache = live_feed_cache();
        cache.remember_installed_state(LiveFeedInstalledState {
            product: "tafs".to_string(),
            version: "old-source".to_string(),
            state_sha256: "old-source".to_string(),
            collected_at_utc: Some("2026-07-27T00:00:00Z".to_string()),
            payload: LiveFeedInstalledPayload::Json {
                bytes: b"old".to_vec(),
            },
        });
        cache.remember_installed_state(LiveFeedInstalledState {
            product: "tafs".to_string(),
            version: "current-source".to_string(),
            state_sha256: "current-source".to_string(),
            collected_at_utc: Some("2026-07-26T00:00:00Z".to_string()),
            payload: LiveFeedInstalledPayload::Json {
                bytes: b"current".to_vec(),
            },
        });

        assert!(cache.installed_payload_bytes("tafs", "old-source").is_err());
        assert_eq!(
            cache
                .installed_payload_bytes("tafs", "current-source")
                .unwrap(),
            b"current"
        );
    }

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
        current_manifest_at(product, version, state_sha256, None)
    }

    fn current_manifest_at(
        product: &str,
        version: &str,
        state_sha256: &str,
        collected_at_utc: Option<&str>,
    ) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
            "generated_at_utc": "2026-08-04T00:00:00Z",
            "products": {
                product: {
                    "current": version,
                    "version_manifest_url": format!("versions/{product}/{version}.json"),
                    "state_url": format!("states/{product}/{version}.json.xz"),
                    "state_sha256": state_sha256,
                    "collected_at_utc": collected_at_utc
                }
            }
        }))
        .unwrap()
    }

    fn nexrad_current_manifest(current: &str, history: &[&str]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
            "generated_at_utc": "2026-08-04T00:00:00Z",
            "products": {
                "nexrad": {
                    "current": current,
                    "version_manifest_url": format!("versions/nexrad/{current}.json"),
                    "state_url": format!("states/nexrad/{current}.json"),
                    "state_sha256": nexrad_state_sha256(current),
                    "history": history.iter().map(|version| serde_json::json!({
                        "version": version,
                        "version_manifest_url": format!("versions/nexrad/{version}.json"),
                        "state_sha256": nexrad_state_sha256(version)
                    })).collect::<Vec<_>>()
                }
            }
        }))
        .unwrap()
    }

    fn nexrad_state_manifest(version: &str) -> Value {
        serde_json::json!({
            "state_id": version,
            "observed_at_utc": "2026-07-26T00:00:00Z",
            "source_grid": {
                "geo_transform": [-123.0, 0.01, 0.0, 48.0, 0.0, -0.01]
            },
            "levels": [],
            "tile_size": 256,
            "tile_path_template": "tiles/res{res}/{x}/{y}.png"
        })
    }

    fn nexrad_state_sha256(version: &str) -> String {
        canonical_json_sha256(&nexrad_state_manifest(version)).unwrap()
    }

    fn nexrad_version_manifest(version: &str) -> (Vec<u8>, Vec<u8>) {
        let state_manifest = nexrad_state_manifest(version);
        let state_manifest_bytes = serde_json::to_vec(&state_manifest).unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default());
        writer.start_file("manifest.json", options).unwrap();
        writer.write_all(&state_manifest_bytes).unwrap();
        let package = writer.finish().unwrap().into_inner();
        let state_sha256 = canonical_json_sha256(&state_manifest).unwrap();
        let manifest = serde_json::json!({
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
            "product": "nexrad",
            "version": version,
            "state": {
                "kind": "json",
                "url": format!("states/nexrad/{version}/manifest.json"),
                "bytes": state_manifest_bytes.len(),
                "blob_sha256": sha256_hex(&state_manifest_bytes),
                "state_sha256": state_sha256
            },
            "install_state": {
                "kind": "opaque",
                "url": format!("install/nexrad/{version}.zip"),
                "bytes": package.len(),
                "blob_sha256": sha256_hex(&package),
                "state_sha256": state_sha256
            }
        });
        (serde_json::to_vec(&manifest).unwrap(), package)
    }

    fn xz_json_bytes(value: &Value) -> Vec<u8> {
        nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(value).unwrap()).unwrap()
    }

    fn notam_record(id: &str, text: &str) -> notam_state::NotamRecord {
        notam_state::NotamRecord {
            id: id.to_string(),
            airport_id: Some("KSEA".to_string()),
            airport_effects: [product_contracts::AirportNotamEffect::RoutineAdvisory]
                .into_iter()
                .collect(),
            notam_keyword: Some("AD".to_string()),
            effective_start_utc: None,
            effective_end_utc: None,
            text: Some(text.to_string()),
            local_text: None,
            icao_text: None,
        }
    }

    fn installed_notam_checkpoint(
        driver: &LiveFeedProductDriver,
        record_id: &str,
        text: &str,
    ) -> (LiveFeedInstalledState, Vec<u8>) {
        let mut source = NotamState::empty();
        source
            .apply_mutation(
                notam_state::NotamMutation::Upsert {
                    record: notam_record(record_id, text),
                },
                &mut NotamApplyWork::default(),
            )
            .unwrap();
        let checkpoint = source.checkpoint();
        let checkpoint_id = checkpoint.state_id.clone();
        let checkpoint_bytes =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&checkpoint).unwrap())
                .unwrap();
        let checkpoint_ref = LiveFeedPayloadRef {
            kind: Some("notam_checkpoint_xz".to_string()),
            url: format!("states/notams/{checkpoint_id}.json.xz"),
            bytes: checkpoint_bytes.len() as u64,
            blob_sha256: sha256_hex(&checkpoint_bytes),
            state_sha256: checkpoint_id.clone(),
        };
        let installed = driver
            .install_full(
                "notams",
                &checkpoint_id,
                &checkpoint_ref,
                LiveFeedFetchedPayload::Bytes(checkpoint_bytes.clone()),
            )
            .unwrap();
        (installed, checkpoint_bytes)
    }

    #[test]
    fn notam_cache_stages_until_main_ack_and_persists_immutable_resource_chain() {
        let registry = live_feed_product_registry();
        let driver = registry.required_driver("notams").unwrap();
        let mut source = NotamState::empty();
        for index in 0..128 {
            source
                .apply_mutation(
                    notam_state::NotamMutation::Upsert {
                        record: notam_record(&format!("N{index:04}"), "initial"),
                    },
                    &mut NotamApplyWork::default(),
                )
                .unwrap();
        }
        let checkpoint = source.checkpoint();
        let checkpoint_id = checkpoint.state_id.clone();
        let checkpoint_bytes =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&checkpoint).unwrap())
                .unwrap();
        let checkpoint_ref = LiveFeedPayloadRef {
            kind: Some("notam_checkpoint_xz".to_string()),
            url: format!("states/notams/{checkpoint_id}.json.xz"),
            bytes: checkpoint_bytes.len() as u64,
            blob_sha256: sha256_hex(&checkpoint_bytes),
            state_sha256: checkpoint_id.clone(),
        };
        let installed = driver
            .install_full(
                "notams",
                &checkpoint_id,
                &checkpoint_ref,
                LiveFeedFetchedPayload::Bytes(checkpoint_bytes.clone()),
            )
            .unwrap();
        let mut cache = live_feed_cache();
        cache.prepare_full_notam_candidate(&installed).unwrap();
        cache.stage_or_remember_installed_state(installed);
        assert!(cache.installed("notams").is_none());
        assert_eq!(
            cache
                .install_candidate("notams")
                .map(|state| state.version.as_str()),
            Some(checkpoint_id.as_str())
        );
        let initial_for_main = cache
            .prepared_install_candidate("notams", &checkpoint_id)
            .unwrap();
        let initial_envelope =
            crate::decode_prepared_live_feed(initial_for_main.as_ref().unwrap()).unwrap();
        let PreparedLiveFeedPayload::Notams(PreparedNotamPayload::InstallAirportCheckpoint(
            initial_checkpoint,
        )) = initial_envelope.payload
        else {
            panic!("initial NOTAM candidate should be an airport projection");
        };
        assert_eq!(initial_checkpoint.state_id, checkpoint_id);
        assert_eq!(initial_checkpoint.records.len(), 128);
        cache
            .acknowledge_install_candidate("notams", &checkpoint_id)
            .unwrap();

        let mutation = notam_state::NotamMutation::Upsert {
            record: notam_record("N0042", "changed"),
        };
        source
            .apply_mutation(mutation.clone(), &mut NotamApplyWork::default())
            .unwrap();
        let head_id = source.state_id().to_string();
        let delta = NotamDelta::new(
            checkpoint_id.clone(),
            head_id.clone(),
            source.counters(),
            vec![mutation],
        );
        let delta_bytes =
            nav_kv_package::xz_frame_uncompressed_bytes(&serde_json::to_vec(&delta).unwrap())
                .unwrap();
        let delta_ref = LiveFeedDeltaRef {
            kind: Some("notam_ordered_delta_xz".to_string()),
            from_version: checkpoint_id.clone(),
            from_state_sha256: checkpoint_id.clone(),
            to_version: head_id.clone(),
            to_state_sha256: head_id.clone(),
            url: format!("deltas/notams/{checkpoint_id}__{head_id}.json.xz"),
            bytes: delta_bytes.len() as u64,
            blob_sha256: sha256_hex(&delta_bytes),
            mutation_count: Some(1),
        };
        let installed_checkpoint = match &cache.installed("notams").unwrap().payload {
            LiveFeedInstalledPayload::NotamResources { checkpoint, .. } => checkpoint.clone(),
            _ => unreachable!(),
        };
        let next = driver
            .apply_delta(cache.installed("notams").unwrap(), &delta_ref, &delta_bytes)
            .unwrap();
        let LiveFeedInstalledPayload::NotamResources {
            checkpoint: retained_checkpoint,
            deltas,
        } = &next.payload
        else {
            panic!("NOTAM cache should retain immutable resources");
        };
        assert!(Arc::ptr_eq(&installed_checkpoint, retained_checkpoint));
        assert_eq!(retained_checkpoint.as_ref(), &checkpoint_bytes);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].as_ref(), &delta_bytes);
        cache
            .prepare_notam_delta_candidate(&next, &delta_bytes)
            .unwrap();
        cache.stage_or_remember_installed_state(next);
        assert_eq!(
            cache
                .installed("notams")
                .map(|state| state.version.as_str()),
            Some(checkpoint_id.as_str())
        );
        let delta_for_main = cache
            .prepared_install_candidate("notams", &head_id)
            .unwrap();
        let delta_envelope =
            crate::decode_prepared_live_feed(delta_for_main.as_ref().unwrap()).unwrap();
        let PreparedLiveFeedPayload::Notams(PreparedNotamPayload::ApplyAirportDelta(delta)) =
            delta_envelope.payload
        else {
            panic!("incremental NOTAM candidate should be a projected delta");
        };
        assert_eq!(delta.from_state_id, checkpoint_id);
        assert_eq!(delta.to_state_id, head_id);
        assert_eq!(delta.mutations.len(), 1);
        let manifest = cache.resource_manifest("notams").unwrap().unwrap();
        assert_eq!(manifest.resources.len(), 2);
        assert_eq!(manifest.resources[0].kind, "notam_checkpoint_xz");
        assert_eq!(manifest.resources[1].kind, "notam_ordered_delta_xz");
        let persisted_delta = cache
            .resource_bytes("notams", &manifest.resources[1].blob_sha256)
            .unwrap();
        assert_eq!(persisted_delta.as_slice(), delta_bytes.as_slice());
        assert!(cache.installed_payload_bytes("notams", &head_id).is_err());
        cache
            .acknowledge_install_candidate("notams", &head_id)
            .unwrap();
        let mut restored_cache = live_feed_cache();
        restored_cache
            .begin_restoring_resources(manifest.clone())
            .unwrap();
        for resource in &manifest.resources {
            let bytes = cache
                .resource_bytes("notams", &resource.blob_sha256)
                .unwrap();
            restored_cache
                .restore_resource_bytes("notams", &resource.blob_sha256, &bytes)
                .unwrap();
        }
        restored_cache
            .finish_restoring_resources(&registry, "notams")
            .unwrap();
        let restored = restored_cache.install_candidate("notams").unwrap();
        assert_eq!(restored.version, head_id);
        assert_eq!(restored.state_sha256, source.state_id());
        let restored_prepared = restored_cache
            .prepared_install_candidate("notams", &head_id)
            .unwrap()
            .unwrap();
        let restored_envelope = crate::decode_prepared_live_feed(&restored_prepared).unwrap();
        assert!(matches!(
            restored_envelope.payload,
            PreparedLiveFeedPayload::Notams(PreparedNotamPayload::InstallAirportCheckpoint(_))
        ));
    }

    #[test]
    fn notam_cache_can_replace_an_acknowledged_chain_with_a_new_checkpoint() {
        let registry = live_feed_product_registry();
        let driver = registry.required_driver("notams").unwrap();
        let mut cache = live_feed_cache();

        let (old_installed, _) = installed_notam_checkpoint(driver, "OLD", "old checkpoint");
        let old_id = old_installed.version.clone();
        cache.prepare_full_notam_candidate(&old_installed).unwrap();
        cache.stage_or_remember_installed_state(old_installed);
        cache
            .acknowledge_install_candidate("notams", &old_id)
            .unwrap();

        let (new_installed, new_bytes) =
            installed_notam_checkpoint(driver, "NEW", "replacement checkpoint");
        let new_id = new_installed.version.clone();
        cache.prepare_full_notam_candidate(&new_installed).unwrap();
        cache.stage_or_remember_installed_state(new_installed);

        let replacement = cache
            .prepared_install_candidate("notams", &new_id)
            .unwrap()
            .unwrap();
        let replacement = crate::decode_prepared_live_feed(&replacement).unwrap();
        assert_eq!(replacement.version, new_id);
        let PreparedLiveFeedPayload::Notams(PreparedNotamPayload::InstallAirportCheckpoint(
            checkpoint,
        )) = replacement.payload
        else {
            panic!("replacement NOTAM candidate should be an airport projection");
        };
        assert_eq!(checkpoint.state_id, new_id);
        assert_eq!(checkpoint.records.len(), 1);
        assert!(!new_bytes.is_empty());
    }

    fn json_version_manifest(
        product: &str,
        version: &str,
        state: &Value,
        delta: Option<LiveFeedDeltaRef>,
    ) -> (Vec<u8>, Vec<u8>, String) {
        let state_bytes = xz_json_bytes(state);
        let state_sha256 = canonical_json_sha256(state).unwrap();
        let manifest = serde_json::json!({
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
            "product": product,
            "version": version,
            "state": {
                "kind": "json_xz",
                "url": format!("states/{product}/{version}.json.xz"),
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

    fn version_manifest_with_state_bytes(manifest: &[u8], bytes: u64) -> Vec<u8> {
        let mut value: Value = serde_json::from_slice(manifest).unwrap();
        value["state"]["bytes"] = serde_json::json!(bytes);
        serde_json::to_vec(&value).unwrap()
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
        assert_eq!(
            registry.record_json_delta_schema("pireps"),
            Some(("pireps_by_id".to_string(), Some("pirep_count".to_string())))
        );
        assert_eq!(registry.record_json_delta_schema("notams"), None);
        assert_eq!(registry.record_json_delta_schema("tfrs"), None);
        assert!(matches!(
            registry.driver("winds-aloft"),
            Some(LiveFeedProductDriver::NavKv { .. })
        ));
    }

    #[test]
    fn durable_reconnect_refresh_requests_current_after_catalog_loaded() {
        let mut cache = live_feed_cache();
        cache
            .ingest_current(&current_manifest("metars", "v1", "abc"))
            .unwrap();

        let requests = cache.current_refresh_requests_at_epoch_ms(1_000);

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].id, "live_feeds/current");
    }

    #[test]
    fn durable_nexrad_retains_the_complete_animation_window() {
        let registry = live_feed_product_registry();
        let mut cache = live_feed_cache();
        let published_versions = ["v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9"];
        let retained_versions = ["v3", "v4", "v5", "v6", "v7", "v8", "v9"];
        cache
            .ingest_current(&nexrad_current_manifest("v9", &published_versions[..8]))
            .unwrap();

        let version_requests = cache.missing_requests();
        assert_eq!(version_requests.len(), NEXRAD_FRAME_WINDOW_SIZE);
        for request in version_requests {
            let LiveFeedCacheRequestKind::Version { version, .. } = &request.kind else {
                panic!("expected NEXRAD version manifest request");
            };
            let (manifest, _) = nexrad_version_manifest(version);
            assert!(cache
                .install_fetched_payload(
                    &registry,
                    &request,
                    LiveFeedFetchedPayload::Bytes(manifest),
                )
                .unwrap()
                .is_none());
        }

        let package_requests = cache.missing_requests();
        assert_eq!(package_requests.len(), NEXRAD_FRAME_WINDOW_SIZE);
        for request in package_requests {
            let LiveFeedCacheRequestKind::Full { version, .. } = &request.kind else {
                panic!("expected complete NEXRAD package request");
            };
            let (_, package) = nexrad_version_manifest(version);
            let installed = cache
                .install_fetched_payload(
                    &registry,
                    &request,
                    LiveFeedFetchedPayload::Bytes(package),
                )
                .unwrap()
                .expect("installed NEXRAD package");
            cache
                .acknowledge_install_candidate("nexrad", &installed.version)
                .unwrap();
        }
        assert!(cache.missing_requests().is_empty());
        assert_eq!(
            cache
                .retained_summaries("nexrad")
                .into_iter()
                .map(|summary| summary.version)
                .collect::<Vec<_>>(),
            retained_versions
        );

        cache
            .ingest_current(&nexrad_current_manifest(
                "v10",
                &["v4", "v5", "v6", "v7", "v8", "v9"],
            ))
            .unwrap();
        let request = cache
            .missing_requests()
            .into_iter()
            .find(|request| {
                matches!(
                    &request.kind,
                    LiveFeedCacheRequestKind::Version { version, .. } if version == "v10"
                )
            })
            .expect("v10 version request");
        let (manifest, _) = nexrad_version_manifest("v10");
        cache
            .install_fetched_payload(&registry, &request, LiveFeedFetchedPayload::Bytes(manifest))
            .unwrap();
        let request = cache
            .missing_requests()
            .into_iter()
            .find(|request| {
                matches!(
                    &request.kind,
                    LiveFeedCacheRequestKind::Full { version, .. } if version == "v10"
                )
            })
            .expect("v10 package request");
        let (_, package) = nexrad_version_manifest("v10");
        let installed = cache
            .install_fetched_payload(&registry, &request, LiveFeedFetchedPayload::Bytes(package))
            .unwrap()
            .expect("installed v10 package");
        cache
            .acknowledge_install_candidate("nexrad", &installed.version)
            .unwrap();

        assert_eq!(
            cache
                .retained_summaries("nexrad")
                .into_iter()
                .map(|summary| summary.version)
                .collect::<Vec<_>>(),
            ["v10", "v4", "v5", "v6", "v7", "v8", "v9"]
        );
        cache
            .release_persisted_payload_bytes("nexrad", "v10")
            .expect("release persisted NEXRAD package bytes");
        assert!(cache.installed_payload_bytes("nexrad", "v10").is_err());
        assert!(cache
            .retained_summaries("nexrad")
            .iter()
            .any(|summary| summary.version == "v10"));
        assert_eq!(
            cache
                .installed_summary("nexrad")
                .expect("latest NEXRAD summary")
                .version,
            "v10"
        );
    }

    #[test]
    fn durable_request_failures_are_retry_gated_in_core() {
        let mut cache = live_feed_cache();
        cache.record_request_failure("live_feeds/current", 1_000);

        assert!(cache.current_refresh_requests_at_epoch_ms(1_001).is_empty());
        assert_eq!(
            cache.current_refresh_requests_at_epoch_ms(301_000)[0].id,
            "live_feeds/current"
        );
    }

    #[test]
    fn runtime_backoff_is_cache_owned() {
        let mut cache = live_feed_cache();
        let first = cache.runtime_decision(crate::LiveFeedRuntimeInput {
            kind: crate::LiveFeedRuntimeEventKind::Error,
            message: Some("boom".to_string()),
            source_url: None,
            status_url: None,
            network_status: None,
        });
        let second = cache.runtime_decision(crate::LiveFeedRuntimeInput {
            kind: crate::LiveFeedRuntimeEventKind::Error,
            message: Some("boom".to_string()),
            source_url: None,
            status_url: None,
            network_status: None,
        });

        assert_eq!(first.reconnect_delay_ms, Some(5_000));
        assert_eq!(second.reconnect_delay_ms, Some(10_000));
    }

    #[test]
    fn record_json_cache_installs_full_then_delta() {
        let registry = live_feed_product_registry();
        let v1 = metar_state("v1", &[("KSEA", "old"), ("KOLM", "old")]);
        let (v1_manifest, v1_bytes, v1_sha) = json_version_manifest("metars", "v1", &v1, None);
        let mut cache = live_feed_cache();
        cache
            .ingest_current(&current_manifest("metars", "v1", &v1_sha))
            .unwrap();
        assert_eq!(
            cache.missing_requests()[0].kind,
            LiveFeedCacheRequestKind::Version {
                product: "metars".to_string(),
                version: "v1".to_string()
            }
        );
        cache
            .ingest_version_manifest("metars", "v1", &v1_manifest)
            .unwrap();
        let request = cache.missing_requests().remove(0);
        cache
            .install_fetched_payload(&registry, &request, LiveFeedFetchedPayload::Bytes(v1_bytes))
            .unwrap();
        assert_eq!(cache.installed("metars").unwrap().version, "v1");

        let v2 = metar_state("v2", &[("KSEA", "new"), ("KPAE", "new")]);
        let v2_sha = canonical_json_sha256(&v2).unwrap();
        let delta = serde_json::json!({
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
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
        let delta_bytes = xz_json_bytes(&delta);
        let delta_ref = LiveFeedDeltaRef {
            kind: Some("record_json_delta_xz".to_string()),
            from_version: "v1".to_string(),
            from_state_sha256: v1_sha,
            to_version: "v2".to_string(),
            to_state_sha256: v2_sha.clone(),
            url: "deltas/metars/v1__v2.json.xz".to_string(),
            bytes: delta_bytes.len() as u64,
            blob_sha256: sha256_hex(&delta_bytes),
            mutation_count: None,
        };
        let (v2_manifest, _, _) =
            json_version_manifest("metars", "v2", &v2, Some(delta_ref.clone()));
        let v2_manifest =
            version_manifest_with_state_bytes(&v2_manifest, delta_bytes.len() as u64 + 1024);
        cache
            .ingest_current(&current_manifest("metars", "v2", &v2_sha))
            .unwrap();
        cache
            .ingest_version_manifest("metars", "v2", &v2_manifest)
            .unwrap();
        let request = cache.missing_requests().remove(0);
        assert_eq!(
            request.kind,
            LiveFeedCacheRequestKind::Delta {
                product: "metars".to_string(),
                from_version: "v1".to_string(),
                to_version: "v2".to_string(),
                payload_kind: Some("record_json_delta_xz".to_string())
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
    fn cached_product_restore_preserves_source_collection_time() {
        const COLLECTED_AT_UTC: &str = "2026-07-26T16:15:00Z";

        let registry = live_feed_product_registry();
        let state = metar_state("v1", &[("KSEA", "test")]);
        let (manifest, state_bytes, state_sha256) =
            json_version_manifest("metars", "v1", &state, None);
        let mut cache = live_feed_cache();
        cache
            .ingest_current(&current_manifest_at(
                "metars",
                "v1",
                &state_sha256,
                Some(COLLECTED_AT_UTC),
            ))
            .unwrap();
        cache
            .ingest_version_manifest("metars", "v1", &manifest)
            .unwrap();
        let request = cache.missing_requests().remove(0);
        cache
            .install_fetched_payload(
                &registry,
                &request,
                LiveFeedFetchedPayload::Bytes(state_bytes),
            )
            .unwrap();

        let summary_json = serde_json::to_vec(&cache.installed_summary("metars").unwrap()).unwrap();
        let summary: LiveFeedInstalledSummary = serde_json::from_slice(&summary_json).unwrap();
        let payload = cache
            .installed_payload_bytes("metars", &summary.version)
            .unwrap();
        let mut restored = live_feed_cache();
        restored
            .ingest_installed_payload_bytes(&registry, &summary, &payload)
            .unwrap();

        assert_eq!(
            restored
                .live_feeds_state()
                .product_collected_at_utc("metars"),
            Some(COLLECTED_AT_UTC)
        );
    }

    #[test]
    fn record_json_cache_prefers_full_state_when_smaller_than_delta() {
        let registry = live_feed_product_registry();
        let v1 = metar_state("v1", &[("KSEA", "old")]);
        let (v1_manifest, v1_bytes, v1_sha) = json_version_manifest("metars", "v1", &v1, None);
        let mut cache = live_feed_cache();
        cache
            .ingest_current(&current_manifest("metars", "v1", &v1_sha))
            .unwrap();
        cache
            .ingest_version_manifest("metars", "v1", &v1_manifest)
            .unwrap();
        let request = cache.missing_requests().remove(0);
        cache
            .install_fetched_payload(&registry, &request, LiveFeedFetchedPayload::Bytes(v1_bytes))
            .unwrap();

        let v2 = metar_state("v2", &[("KSEA", "new")]);
        let v2_sha = canonical_json_sha256(&v2).unwrap();
        let delta = serde_json::json!({
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
            "product": "metars",
            "from_version": "v1",
            "to_version": "v2",
            "top_level_changed": {},
            "top_level_removed": [],
            "changed": {
                "KSEA": {"station_id": "KSEA", "raw_text": "new"}
            },
            "removed": []
        });
        let delta_bytes = xz_json_bytes(&delta);
        let delta_ref = LiveFeedDeltaRef {
            kind: Some("record_json_delta_xz".to_string()),
            from_version: "v1".to_string(),
            from_state_sha256: v1_sha,
            to_version: "v2".to_string(),
            to_state_sha256: v2_sha.clone(),
            url: "deltas/metars/v1__v2.json.xz".to_string(),
            bytes: 10_000_000,
            blob_sha256: sha256_hex(&delta_bytes),
            mutation_count: None,
        };
        let (v2_manifest, _, _) =
            json_version_manifest("metars", "v2", &v2, Some(delta_ref.clone()));
        cache
            .ingest_current(&current_manifest("metars", "v2", &v2_sha))
            .unwrap();
        cache
            .ingest_version_manifest("metars", "v2", &v2_manifest)
            .unwrap();

        let request = cache.missing_requests().remove(0);
        assert_eq!(
            request.kind,
            LiveFeedCacheRequestKind::Full {
                product: "metars".to_string(),
                version: "v2".to_string(),
                payload_kind: Some("json_xz".to_string()),
            }
        );
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
        let mut cache = live_feed_cache();
        cache
            .ingest_current(&current_manifest("tafs", "v1", &v1_sha))
            .unwrap();
        cache
            .ingest_version_manifest("tafs", "v1", &v1_manifest)
            .unwrap();
        let request = cache.missing_requests().remove(0);
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
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
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
        let delta_bytes = xz_json_bytes(&delta);
        let delta_ref = LiveFeedDeltaRef {
            kind: Some("record_json_delta_xz".to_string()),
            from_version: "v1".to_string(),
            from_state_sha256: v1_sha,
            to_version: "v2".to_string(),
            to_state_sha256: v2_sha.clone(),
            url: "deltas/tafs/v1__v2.json.xz".to_string(),
            bytes: delta_bytes.len() as u64,
            blob_sha256: sha256_hex(&delta_bytes),
            mutation_count: None,
        };
        let (v2_manifest, _, _) = json_version_manifest("tafs", "v2", &v2, Some(delta_ref));
        let v2_manifest =
            version_manifest_with_state_bytes(&v2_manifest, delta_bytes.len() as u64 + 1024);
        cache
            .ingest_current(&current_manifest("tafs", "v2", &v2_sha))
            .unwrap();
        cache
            .ingest_version_manifest("tafs", "v2", &v2_manifest)
            .unwrap();
        let request = cache.missing_requests().remove(0);
        assert_eq!(
            request.kind,
            LiveFeedCacheRequestKind::Delta {
                product: "tafs".to_string(),
                from_version: "v1".to_string(),
                to_version: "v2".to_string(),
                payload_kind: Some("record_json_delta_xz".to_string())
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
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
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
        let mut cache = live_feed_cache();
        cache
            .ingest_current(&current_manifest("obstacles", "v1", &first_sha))
            .unwrap();
        cache
            .ingest_version_manifest("obstacles", "v1", &version_manifest)
            .unwrap();
        let request = cache.missing_requests().remove(0);
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
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
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
        let delta_bytes = xz_json_bytes(&delta_value);
        let second_manifest = serde_json::to_vec(&serde_json::json!({
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
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
                "bytes": delta_bytes.len() + 1024,
                "blob_sha256": "unused",
                "state_sha256": second_sha
            },
            "delta_from_previous": {
                "kind": "nav_kv_delta_xz",
                "from_version": "v1",
                "from_state_sha256": first_sha,
                "to_version": "v2",
                "to_state_sha256": second_sha,
                "url": "deltas/obstacles/v1__v2.nav-kv-delta.json.xz",
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
        let request = cache.missing_requests().remove(0);
        assert_eq!(
            request.kind,
            LiveFeedCacheRequestKind::Delta {
                product: "obstacles".to_string(),
                from_version: "v1".to_string(),
                to_version: "v2".to_string(),
                payload_kind: Some("nav_kv_delta_xz".to_string())
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
        let summary = installed.summary();
        let persisted = installed.payload_bytes().unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(&persisted)).unwrap();
        let mut encoded_page = Vec::new();
        let mut page_member = archive.by_name("page_0000").unwrap();
        assert_eq!(page_member.compression(), CompressionMethod::Stored);
        page_member.read_to_end(&mut encoded_page).unwrap();
        assert!(nav_kv_package::is_xz(&encoded_page));

        let LiveFeedInstalledPayload::NavKv {
            manifest,
            root,
            pages,
        } = &installed.payload
        else {
            panic!("expected nav_kv payload");
        };
        let manifest: Value = serde_json::from_slice(manifest).unwrap();
        assert_eq!(manifest["product_id"], "obstacles");
        assert_eq!(manifest["version_label"], "v2");
        assert_eq!(manifest["state_sha256"], second_sha);
        let root = NavKvRoot::parse(root).unwrap();
        let pairs = root
            .pairs(|page| pages.get(page as usize).cloned())
            .unwrap();
        assert_eq!(pairs, second_pairs);

        let mut reloaded = live_feed_cache();
        reloaded
            .ingest_installed_payload_bytes(&registry, &summary, &persisted)
            .unwrap();
        let reinstalled = reloaded.installed("obstacles").unwrap();
        let LiveFeedInstalledPayload::NavKv { root, pages, .. } = &reinstalled.payload else {
            panic!("expected reloaded nav_kv payload");
        };
        let root = NavKvRoot::parse(root).unwrap();
        let reloaded_pairs = root
            .pairs(|page| pages.get(page as usize).cloned())
            .unwrap();
        assert_eq!(reloaded_pairs, second_pairs);
    }

    #[test]
    fn nav_kv_cache_prefers_full_package_when_smaller_than_delta() {
        let registry = live_feed_product_registry();
        let first_pairs = vec![NavKvPair {
            key: "obstacle/tile/z01/x000001/y000001".to_string(),
            value: b"old-a".to_vec(),
        }];
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
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
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
        let mut cache = live_feed_cache();
        cache
            .ingest_current(&current_manifest("obstacles", "v1", &first_sha))
            .unwrap();
        cache
            .ingest_version_manifest("obstacles", "v1", &version_manifest)
            .unwrap();
        let request = cache.missing_requests().remove(0);
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

        let second_pairs = vec![NavKvPair {
            key: "obstacle/tile/z01/x000001/y000001".to_string(),
            value: b"new-a".to_vec(),
        }];
        let second_sha = nav_kv_canonical_sha256_from_pairs(&second_pairs);
        let delta_value = serde_json::json!({
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
            "product": "obstacles",
            "from_version": "v1",
            "to_version": "v2",
            "from_state_sha256": first_sha,
            "to_state_sha256": second_sha,
            "entries": [{
                "key": "obstacle/tile/z01/x000001/y000001",
                "value": b"new-a".to_vec(),
            }]
        });
        let delta_bytes = xz_json_bytes(&delta_value);
        let second_manifest = serde_json::to_vec(&serde_json::json!({
            "schema_version": crate::live_feeds::LIVE_FEEDS_SCHEMA_VERSION,
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
                "bytes": 10,
                "blob_sha256": "unused",
                "state_sha256": second_sha
            },
            "delta_from_previous": {
                "kind": "nav_kv_delta_xz",
                "from_version": "v1",
                "from_state_sha256": first_sha,
                "to_version": "v2",
                "to_state_sha256": second_sha,
                "url": "deltas/obstacles/v1__v2.nav-kv-delta.json.xz",
                "bytes": 10_000_000,
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

        let request = cache.missing_requests().remove(0);
        assert_eq!(
            request.kind,
            LiveFeedCacheRequestKind::Full {
                product: "obstacles".to_string(),
                version: "v2".to_string(),
                payload_kind: Some("nav_kv_package".to_string()),
            }
        );
    }
}
