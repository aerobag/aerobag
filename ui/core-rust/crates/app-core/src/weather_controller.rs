// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use app_ui_contracts::{
    nexrad::{NexradOverlayAnimation, NexradOverlayAnimationPhase},
    session::FlightDataCellAction,
};
use chrono::{DateTime, Utc};

use crate::{
    freshness::{format_age, parse_utc_instant},
    live_feed_runtime::{
        LiveFeedConnectionEvent, LiveFeedConnectionEventKind, LiveFeedNetworkStatus,
        LiveFeedRuntimeDecision, LiveFeedRuntimeInput, LiveFeedRuntimeState,
    },
    live_feeds::NEXRAD_FRAME_WINDOW_SIZE,
    map_overlay::WeatherStationAirportAliases,
    AppResult, DataStatusRecord, LiveFeedsState, MetarProductPayload, MetarTilePayload, NavKvStore,
    NotamDisplayIndex, PointTilePayload, PreparedMetarTile, TafProductPayload, TfrProductPayload,
};

pub(crate) const NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS: i64 = 1_000;
pub(crate) const NEXRAD_ANIMATION_CURRENT_FRAME_DWELL_MS: i64 = 2_500;
pub(crate) const NEXRAD_ANIMATION_BLANK_DWELL_MS: i64 = 500;
pub(crate) const NEXRAD_MAX_DISPLAY_AGE_MS: i64 = 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum LiveFeedConnectionMode {
    #[default]
    Unknown,
    Connecting,
    Connected,
    Error,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct LiveFeedConnectionState {
    pub mode: LiveFeedConnectionMode,
    pub runtime: LiveFeedRuntimeState,
    pub source_url: Option<String>,
    pub status_url: Option<String>,
    pub last_state_change_epoch_ms: Option<i64>,
    pub last_heard_epoch_ms: Option<i64>,
    pub last_error_epoch_ms: Option<i64>,
    pub last_error_message: Option<String>,
    pub last_resource_error_epoch_ms: Option<i64>,
    pub last_resource_error_message: Option<String>,
    pub network_status: Option<LiveFeedNetworkStatus>,
}

#[derive(Clone)]
pub(crate) struct LiveNexradInstalledState {
    pub version: String,
    pub package_blob_sha256: String,
    pub manifest: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(crate) struct LiveNavKvSource {
    pub product: String,
    pub version: String,
    pub state_url: String,
    pub root_member_path: String,
    pub page_path_template: String,
    pub page_count: u32,
    pub state_sha256: String,
    pub package_blob_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct LiveObstacleHadState {
    pub source: LiveNavKvSource,
    pub store: Option<NavKvStore>,
}

#[derive(Debug, Clone)]
pub(crate) struct LiveForecastAtmosphereState {
    pub source: LiveNavKvSource,
    pub manifest: product_contracts::AtmosphereManifest,
}

#[derive(Clone, Default)]
struct WeatherModel {
    live_feeds: Arc<LiveFeedsState>,
    connection: LiveFeedConnectionState,
    nexrad_animation_mode: NexradAnimationMode,
    revision: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum NexradAnimationMode {
    #[default]
    Animating,
    HoldLatest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WeatherProjectionInput {
    pub nexrad_visible: bool,
    pub wall_clock_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WeatherProjection {
    pub nexrad_age_banner_value: String,
    pub nexrad_action: Option<FlightDataCellAction>,
}

pub(crate) struct WeatherProjectionResult {
    pub projection: WeatherProjection,
    pub rebuilt: bool,
}

#[derive(Clone)]
struct WeatherProjectionCache {
    revision: u64,
    input: WeatherProjectionInput,
    projection: WeatherProjection,
}

#[derive(Default)]
pub(crate) struct WeatherRuntime {
    pub metar_tile_cache: HashMap<String, MetarTilePayload>,
    pub metar_payload: Option<MetarProductPayload>,
    pub prepared_metar_tiles: Option<Vec<PreparedMetarTile>>,
    pub pirep_payload: Option<crate::PirepProductPayload>,
    pub prepared_pirep_tiles: Option<Vec<crate::PreparedPirepTile>>,
    pub important_metar_station_ids: Option<HashSet<String>>,
    pub metar_station_importance_status: Option<DataStatusRecord>,
    pub weather_station_airport_aliases: Option<WeatherStationAirportAliases>,
    pub obstacle_had: Option<LiveObstacleHadState>,
    pub forecast_atmosphere_state: Option<LiveForecastAtmosphereState>,
    pub forecast_atmosphere: Option<crate::InstalledForecastAtmosphere>,
    pub obstacle_tile_cache: HashMap<String, PointTilePayload>,
    pub taf_payload: Option<TafProductPayload>,
    pub notam_display_index: Option<NotamDisplayIndex>,
    pub tfr_payload: Option<TfrProductPayload>,
    pub nexrad_installed: BTreeMap<String, LiveNexradInstalledState>,
    pub nexrad_tile_cache: HashMap<String, Vec<u8>>,
}

#[derive(Clone)]
pub(crate) struct WeatherModelCheckpoint {
    model: WeatherModel,
}

#[derive(Default)]
pub(crate) struct WeatherController {
    model: WeatherModel,
    runtime: WeatherRuntime,
    projection_cache: Option<WeatherProjectionCache>,
}

impl WeatherController {
    pub fn revision(&self) -> u64 {
        self.model.revision
    }

    pub fn runtime(&self) -> &WeatherRuntime {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut WeatherRuntime {
        self.note_change();
        &mut self.runtime
    }

    pub fn nexrad_animation_mode(&self) -> NexradAnimationMode {
        self.model.nexrad_animation_mode
    }

    pub fn toggle_nexrad_animation_mode(&mut self) {
        self.model.nexrad_animation_mode = match self.model.nexrad_animation_mode {
            NexradAnimationMode::Animating => NexradAnimationMode::HoldLatest,
            NexradAnimationMode::HoldLatest => NexradAnimationMode::Animating,
        };
        self.note_change();
    }

    pub fn checkpoint_model(&self) -> WeatherModelCheckpoint {
        WeatherModelCheckpoint {
            model: self.model.clone(),
        }
    }

    pub fn rollback_model(&mut self, checkpoint: WeatherModelCheckpoint) {
        self.model = checkpoint.model;
        self.projection_cache = None;
    }

    pub fn live_feeds(&self) -> &LiveFeedsState {
        &self.model.live_feeds
    }

    pub fn live_feeds_mut(&mut self) -> &mut LiveFeedsState {
        self.note_change();
        Arc::make_mut(&mut self.model.live_feeds)
    }

    #[cfg(test)]
    pub fn live_feeds_arc(&self) -> &Arc<LiveFeedsState> {
        &self.model.live_feeds
    }

    pub fn replace_live_feeds(&mut self, live_feeds: LiveFeedsState) {
        self.model.live_feeds = Arc::new(live_feeds);
        self.note_change();
    }

    pub fn connection(&self) -> &LiveFeedConnectionState {
        &self.model.connection
    }

    pub fn set_source_root_url(&mut self, source_root_url: &str) -> AppResult<String> {
        let mut live_feeds = (*self.model.live_feeds).clone();
        let normalized = live_feeds.set_source_root_url(source_root_url)?;
        let status_url = crate::live_feed_status_url(&normalized)?;
        self.model.live_feeds = Arc::new(live_feeds);
        self.model.connection.source_url = Some(normalized.clone());
        self.model.connection.status_url = Some(status_url);
        self.note_change();
        Ok(normalized)
    }

    pub fn runtime_decision(&mut self, input: LiveFeedRuntimeInput) -> LiveFeedRuntimeDecision {
        self.note_change();
        let now_ms = input.now_ms;
        let mut decision =
            crate::live_feed_runtime_decision(&mut self.model.connection.runtime, input);
        if let Some(delay_ms) = self.model.live_feeds.next_resource_retry_delay_ms(now_ms) {
            decision
                .commands
                .push(crate::LiveFeedRuntimeCommand::RetryResources { delay_ms });
        }
        decision
    }

    pub fn record_resource_error(&mut self, epoch_ms: i64, message: String) {
        self.model.connection.last_resource_error_epoch_ms = Some(epoch_ms);
        self.model.connection.last_resource_error_message = Some(message);
        self.note_change();
    }

    pub fn clear_resource_error(&mut self) {
        let had_epoch = self
            .model
            .connection
            .last_resource_error_epoch_ms
            .take()
            .is_some();
        let had_message = self
            .model
            .connection
            .last_resource_error_message
            .take()
            .is_some();
        if had_epoch || had_message {
            self.note_change();
        }
    }

    pub fn record_connection_event(&mut self, event: LiveFeedConnectionEvent, epoch_ms: i64) {
        if let Some(source_url) = event.source_url {
            match Arc::make_mut(&mut self.model.live_feeds).set_source_root_url(&source_url) {
                Ok(normalized) => {
                    self.model.connection.source_url = Some(normalized);
                }
                Err(err) => {
                    self.model.connection.source_url = Some(source_url);
                    self.model.connection.last_resource_error_epoch_ms = Some(epoch_ms);
                    self.model.connection.last_resource_error_message = Some(err.to_string());
                }
            }
        }
        if event.status_url.is_some() {
            self.model.connection.status_url = event.status_url;
        }
        if event.network_status.is_some() {
            self.model.connection.network_status = event.network_status;
        }
        match event.kind {
            LiveFeedConnectionEventKind::Connecting => {
                self.model.connection.mode = LiveFeedConnectionMode::Connecting;
                self.model.connection.last_state_change_epoch_ms = Some(epoch_ms);
            }
            LiveFeedConnectionEventKind::Connected => {
                self.model.connection.mode = LiveFeedConnectionMode::Connected;
                self.model.connection.last_state_change_epoch_ms = Some(epoch_ms);
                self.model.connection.last_error_message = None;
            }
            LiveFeedConnectionEventKind::Message => {
                self.model.connection.mode = LiveFeedConnectionMode::Connected;
                self.model.connection.last_state_change_epoch_ms = self
                    .model
                    .connection
                    .last_state_change_epoch_ms
                    .or(Some(epoch_ms));
                self.model.connection.last_heard_epoch_ms = Some(epoch_ms);
                self.model.connection.last_error_message = None;
            }
            LiveFeedConnectionEventKind::Error => {
                self.model.connection.mode = LiveFeedConnectionMode::Error;
                self.model.connection.last_state_change_epoch_ms = Some(epoch_ms);
                self.model.connection.last_error_epoch_ms = Some(epoch_ms);
                self.model.connection.last_error_message = event.message;
            }
            LiveFeedConnectionEventKind::Closed => {
                self.model.connection.mode = LiveFeedConnectionMode::Closed;
                self.model.connection.last_state_change_epoch_ms = Some(epoch_ms);
            }
            LiveFeedConnectionEventKind::NetworkStatus => {}
        }
        self.note_change();
    }

    pub fn invalidate_nav_data(&mut self) {
        self.runtime.important_metar_station_ids = None;
        self.runtime.metar_station_importance_status = None;
        self.runtime.weather_station_airport_aliases = None;
        self.note_change();
    }

    pub fn project(&mut self, input: WeatherProjectionInput) -> WeatherProjectionResult {
        if let Some(cache) = self.projection_cache.as_ref() {
            if cache.revision == self.model.revision && cache.input == input {
                return WeatherProjectionResult {
                    projection: cache.projection.clone(),
                    rebuilt: false,
                };
            }
        }
        let projection = WeatherProjection {
            nexrad_age_banner_value: nexrad_frame_age_banner_value(self, input),
            nexrad_action: nexrad_animation_action(self, input),
        };
        self.projection_cache = Some(WeatherProjectionCache {
            revision: self.model.revision,
            input,
            projection: projection.clone(),
        });
        WeatherProjectionResult {
            projection,
            rebuilt: true,
        }
    }

    fn note_change(&mut self) {
        self.model.revision = self.model.revision.wrapping_add(1);
        self.projection_cache = None;
    }
}

#[derive(Clone)]
pub(crate) struct NexradFrameCandidate {
    pub version: String,
    pub manifest: serde_json::Value,
    pub observed_at_utc: Option<DateTime<Utc>>,
}

pub(crate) fn nexrad_retained_frame_candidates(
    weather: &WeatherController,
) -> Vec<NexradFrameCandidate> {
    let mut frames = Vec::new();
    let mut identities = HashSet::new();
    for installed in weather.runtime.nexrad_installed.values() {
        let identity = nexrad_manifest_identity(&installed.version, &installed.manifest);
        if !identities.insert(identity) {
            continue;
        }
        frames.push(NexradFrameCandidate {
            version: installed.version.clone(),
            observed_at_utc: json_observed_at_utc(&installed.manifest),
            manifest: installed.manifest.clone(),
        });
    }
    for loaded in weather
        .live_feeds()
        .product_loaded_state_manifests("nexrad")
    {
        let identity = nexrad_manifest_identity(loaded.version, loaded.manifest);
        if identities.insert(identity) {
            frames.push(NexradFrameCandidate {
                version: loaded.version.to_string(),
                observed_at_utc: json_observed_at_utc(loaded.manifest),
                manifest: loaded.manifest.clone(),
            });
        }
    }
    frames.sort_by(|left, right| {
        left.observed_at_utc
            .cmp(&right.observed_at_utc)
            .then_with(|| left.version.cmp(&right.version))
    });
    if frames.len() > NEXRAD_FRAME_WINDOW_SIZE {
        frames.drain(0..frames.len() - NEXRAD_FRAME_WINDOW_SIZE);
    }
    frames
}

pub(crate) fn nexrad_displayable_frame_candidates(
    weather: &WeatherController,
    epoch_ms: i64,
) -> Vec<NexradFrameCandidate> {
    let oldest_displayable_epoch_ms = epoch_ms.saturating_sub(NEXRAD_MAX_DISPLAY_AGE_MS);
    nexrad_retained_frame_candidates(weather)
        .into_iter()
        .filter(|frame| {
            frame
                .observed_at_utc
                .is_some_and(|observed| observed.timestamp_millis() >= oldest_displayable_epoch_ms)
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn nexrad_animation_for_frames(
    frames: &[NexradFrameCandidate],
    epoch_ms: i64,
) -> NexradOverlayAnimation {
    nexrad_animation_for_frames_in_mode(frames, epoch_ms, NexradAnimationMode::Animating)
}

pub(crate) fn nexrad_animation_for_frames_in_mode(
    frames: &[NexradFrameCandidate],
    epoch_ms: i64,
    mode: NexradAnimationMode,
) -> NexradOverlayAnimation {
    if frames.is_empty() {
        return NexradOverlayAnimation::idle();
    }
    let age_labels = nexrad_frame_age_labels(frames, epoch_ms);
    let age_summary = if age_labels.is_empty() {
        "---".to_string()
    } else {
        age_labels.join(", ")
    };
    if mode == NexradAnimationMode::HoldLatest {
        return NexradOverlayAnimation {
            phase: NexradOverlayAnimationPhase::Frame,
            selected_frame_index: Some(frames.len() - 1),
            frame_count: frames.len(),
            age_labels,
            age_summary,
            next_update_delay_ms: None,
            next_update_epoch_ms: None,
        };
    }
    if frames.len() == 1 {
        return NexradOverlayAnimation {
            phase: NexradOverlayAnimationPhase::Frame,
            selected_frame_index: Some(0),
            frame_count: 1,
            age_labels,
            age_summary,
            next_update_delay_ms: None,
            next_update_epoch_ms: None,
        };
    }
    let cycle_ms = nexrad_animation_cycle_ms(frames.len());
    let offset_ms = epoch_ms.rem_euclid(cycle_ms);
    let mut phase_start_ms = 0;
    for index in 0..frames.len() {
        let dwell_ms = nexrad_animation_frame_dwell_ms(index, frames.len());
        let phase_end_ms = phase_start_ms + dwell_ms;
        if offset_ms < phase_end_ms {
            return NexradOverlayAnimation {
                phase: NexradOverlayAnimationPhase::Frame,
                selected_frame_index: Some(index),
                frame_count: frames.len(),
                age_labels,
                age_summary,
                next_update_delay_ms: Some((phase_end_ms - offset_ms) as u32),
                next_update_epoch_ms: Some(epoch_ms + (phase_end_ms - offset_ms)),
            };
        }
        phase_start_ms = phase_end_ms;
    }
    NexradOverlayAnimation {
        phase: NexradOverlayAnimationPhase::Blank,
        selected_frame_index: None,
        frame_count: frames.len(),
        age_labels,
        age_summary,
        next_update_delay_ms: Some((cycle_ms - offset_ms) as u32),
        next_update_epoch_ms: Some(epoch_ms + (cycle_ms - offset_ms)),
    }
}

pub(crate) fn nexrad_animation_cycle_ms(frame_count: usize) -> i64 {
    if frame_count <= 1 {
        return 0;
    }
    (frame_count.saturating_sub(1) as i64 * NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS)
        + NEXRAD_ANIMATION_CURRENT_FRAME_DWELL_MS
        + NEXRAD_ANIMATION_BLANK_DWELL_MS
}

fn nexrad_animation_frame_dwell_ms(index: usize, frame_count: usize) -> i64 {
    if index + 1 == frame_count {
        NEXRAD_ANIMATION_CURRENT_FRAME_DWELL_MS
    } else {
        NEXRAD_ANIMATION_PRECEDING_FRAME_DWELL_MS
    }
}

pub(crate) fn nexrad_frame_age_labels(
    frames: &[NexradFrameCandidate],
    epoch_ms: i64,
) -> Vec<String> {
    nexrad_frame_age_values(frames, epoch_ms)
        .into_iter()
        .map(|value| {
            if value == "unknown" {
                value
            } else {
                format!("{value} ago")
            }
        })
        .collect()
}

pub(crate) fn nexrad_frame_age_values(
    frames: &[NexradFrameCandidate],
    epoch_ms: i64,
) -> Vec<String> {
    frames
        .iter()
        .map(|frame| match frame.observed_at_utc {
            Some(observed_at_utc) => {
                format_age(epoch_ms.saturating_sub(observed_at_utc.timestamp_millis()))
            }
            None => "unknown".to_string(),
        })
        .collect()
}

pub(crate) fn nexrad_frame_age_summary(
    weather: &WeatherController,
    input: WeatherProjectionInput,
) -> String {
    if !input.nexrad_visible {
        return "off".to_string();
    }
    let labels = nexrad_frame_age_labels(
        &nexrad_displayable_frame_candidates(weather, input.wall_clock_epoch_ms),
        input.wall_clock_epoch_ms,
    );
    if labels.is_empty() {
        "inop".to_string()
    } else {
        labels.join(", ")
    }
}

pub(crate) fn nexrad_freshest_frame_observed_at_utc(
    weather: &WeatherController,
) -> Option<DateTime<Utc>> {
    nexrad_retained_frame_candidates(weather)
        .into_iter()
        .filter_map(|frame| frame.observed_at_utc)
        .max()
}

fn nexrad_frame_age_banner_value(
    weather: &WeatherController,
    input: WeatherProjectionInput,
) -> String {
    if !input.nexrad_visible {
        return "off".to_string();
    }
    let frames = nexrad_displayable_frame_candidates(weather, input.wall_clock_epoch_ms);
    if frames.is_empty() {
        return "inop".to_string();
    }
    let animation = nexrad_animation_for_frames_in_mode(
        &frames,
        input.wall_clock_epoch_ms,
        weather.nexrad_animation_mode(),
    );
    let Some(index) = animation.selected_frame_index else {
        return "---".to_string();
    };
    nexrad_frame_age_values(&frames, input.wall_clock_epoch_ms)
        .get(index)
        .cloned()
        .unwrap_or_else(|| "inop".to_string())
}

fn nexrad_animation_action(
    weather: &WeatherController,
    input: WeatherProjectionInput,
) -> Option<FlightDataCellAction> {
    input.nexrad_visible.then(|| {
        let (action_id, accessibility_label, symbol_id) = match weather.nexrad_animation_mode() {
            NexradAnimationMode::Animating => (
                "pause_nexrad_animation",
                "Hold latest NEXRAD frame",
                "pause_nexrad_animation",
            ),
            NexradAnimationMode::HoldLatest => (
                "resume_nexrad_animation",
                "Animate NEXRAD history",
                "resume_nexrad_animation",
            ),
        };
        FlightDataCellAction {
            action_id: action_id.to_string(),
            accessibility_label: accessibility_label.to_string(),
            symbol_id: Some(symbol_id.to_string()),
        }
    })
}

fn nexrad_manifest_identity(version: &str, manifest: &serde_json::Value) -> String {
    manifest
        .get("state_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(version)
        .to_string()
}

fn json_observed_at_utc(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    value
        .get("observed_at_utc")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_utc_instant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displayable_nexrad_frames_include_one_hour_but_not_older() {
        let mut controller = WeatherController::default();
        let now = 1_800_000_000_000_i64;
        for (version, observed_at_epoch_ms) in [
            ("too-old", now - NEXRAD_MAX_DISPLAY_AGE_MS - 1),
            ("one-hour", now - NEXRAD_MAX_DISPLAY_AGE_MS),
            ("recent", now - 2 * 60 * 1_000),
        ] {
            controller.runtime.nexrad_installed.insert(
                version.to_string(),
                LiveNexradInstalledState {
                    version: version.to_string(),
                    package_blob_sha256: format!("blob-{version}"),
                    manifest: serde_json::json!({
                        "state_id": version,
                        "observed_at_utc": DateTime::<Utc>::from_timestamp_millis(
                            observed_at_epoch_ms
                        )
                        .expect("valid test instant")
                        .to_rfc3339(),
                    }),
                },
            );
        }

        assert_eq!(
            nexrad_displayable_frame_candidates(&controller, now)
                .iter()
                .map(|frame| frame.version.as_str())
                .collect::<Vec<_>>(),
            vec!["one-hour", "recent"]
        );
    }

    #[test]
    fn hold_latest_nexrad_mode_selects_the_freshest_frame_without_a_timer() {
        let now = 1_800_000_000_000_i64;
        let frames = [5_i64, 3, 1]
            .into_iter()
            .enumerate()
            .map(|(index, age_minutes)| NexradFrameCandidate {
                version: format!("frame-{index}"),
                observed_at_utc: DateTime::<Utc>::from_timestamp_millis(now - age_minutes * 60_000),
                manifest: serde_json::json!({"state_id": format!("frame-{index}")}),
            })
            .collect::<Vec<_>>();

        let animation =
            nexrad_animation_for_frames_in_mode(&frames, now, NexradAnimationMode::HoldLatest);

        assert_eq!(animation.phase, NexradOverlayAnimationPhase::Frame);
        assert_eq!(animation.selected_frame_index, Some(2));
        assert_eq!(animation.frame_count, 3);
        assert_eq!(animation.next_update_delay_ms, None);
        assert_eq!(animation.next_update_epoch_ms, None);
    }

    #[test]
    fn nexrad_animation_mode_is_core_owned_and_toggles() {
        let mut controller = WeatherController::default();

        assert_eq!(
            controller.nexrad_animation_mode(),
            NexradAnimationMode::Animating
        );
        controller.toggle_nexrad_animation_mode();
        assert_eq!(
            controller.nexrad_animation_mode(),
            NexradAnimationMode::HoldLatest
        );
        controller.toggle_nexrad_animation_mode();
        assert_eq!(
            controller.nexrad_animation_mode(),
            NexradAnimationMode::Animating
        );
    }

    #[test]
    fn model_checkpoint_rolls_back_protocol_state_without_replacing_runtime() {
        let mut controller = WeatherController::default();
        controller
            .runtime
            .nexrad_tile_cache
            .insert("seed".to_string(), vec![1, 2, 3]);
        let runtime_address = controller.runtime.nexrad_tile_cache["seed"].as_ptr();
        let checkpoint = controller.checkpoint_model();

        controller.record_resource_error(42, "failed".to_string());
        assert_eq!(controller.revision(), 1);
        controller.rollback_model(checkpoint);

        assert_eq!(controller.connection().last_resource_error_message, None);
        assert_eq!(controller.revision(), 0);
        assert_eq!(
            controller.runtime.nexrad_tile_cache["seed"].as_ptr(),
            runtime_address
        );
    }

    #[test]
    fn projection_cache_tracks_revision_visibility_and_clock_inputs() {
        let mut controller = WeatherController::default();
        let hidden = WeatherProjectionInput {
            nexrad_visible: false,
            wall_clock_epoch_ms: 1_000,
        };
        let first = controller.project(hidden);
        assert!(first.rebuilt);
        assert_eq!(first.projection.nexrad_age_banner_value, "off");
        assert!(!controller.project(hidden).rebuilt);

        let visible = WeatherProjectionInput {
            nexrad_visible: true,
            ..hidden
        };
        let visible_projection = controller.project(visible);
        assert!(visible_projection.rebuilt);
        assert_eq!(
            visible_projection.projection.nexrad_age_banner_value,
            "inop"
        );

        controller.record_resource_error(42, "failed".to_string());
        assert!(controller.project(visible).rebuilt);
        assert!(!controller.project(visible).rebuilt);
        assert!(
            controller
                .project(WeatherProjectionInput {
                    wall_clock_epoch_ms: 2_000,
                    ..visible
                })
                .rebuilt
        );
    }

    #[test]
    fn clearing_resource_error_removes_timestamp_and_message_together() {
        let mut controller = WeatherController::default();
        controller.record_resource_error(42, "failed".to_string());

        controller.clear_resource_error();

        assert_eq!(controller.connection().last_resource_error_epoch_ms, None);
        assert_eq!(controller.connection().last_resource_error_message, None);
    }
}
