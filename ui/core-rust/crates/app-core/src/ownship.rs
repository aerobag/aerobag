// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::geometry::LatLon;

const DEFAULT_STALE_AFTER_MS: i64 = 5_000;
const FEET_PER_NAUTICAL_MILE: f64 = 6076.12;
const FEET_PER_METER: f64 = 3.280_839_895_013_123;
const VERTICAL_SPEED_HISTORY_RETENTION_MS: i64 = 20_000;
const VERTICAL_SPEED_RECENCY_HALF_LIFE_MS: f64 = 6_000.0;
const VERTICAL_SPEED_MIN_SPAN_MS: i64 = 2_000;
const VERTICAL_SPEED_MIN_SAMPLES: usize = 3;
const VERTICAL_SPEED_DEFAULT_ACCURACY_M: f64 = 10.0;
const VERTICAL_SPEED_MIN_ACCURACY_M: f64 = 3.0;
const VERTICAL_SPEED_MAX_ACCURACY_M: f64 = 30.0;
const VERTICAL_SPEED_ROBUST_ITERATIONS: usize = 2;
const VERTICAL_SPEED_MIN_OUTLIER_THRESHOLD_FT: f64 = 3.0 * FEET_PER_METER;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SituationRingCandidate {
    pub radius_nm: f64,
    pub label: String,
}

pub fn situation_ring_candidates() -> Vec<SituationRingCandidate> {
    const FEET_CANDIDATES: [u32; 7] = [500, 800, 1_000, 1_500, 2_000, 3_000, 5_000];
    const NM_CANDIDATES: [f64; 18] = [
        1.0, 1.5, 2.0, 3.0, 5.0, 8.0, 10.0, 15.0, 20.0, 30.0, 50.0, 80.0, 100.0, 150.0, 200.0,
        300.0, 500.0, 800.0,
    ];

    FEET_CANDIDATES
        .into_iter()
        .map(|feet| SituationRingCandidate {
            radius_nm: f64::from(feet) / FEET_PER_NAUTICAL_MILE,
            label: format!("{feet}ft"),
        })
        .chain(NM_CANDIDATES.into_iter().map(|nm| SituationRingCandidate {
            radius_nm: nm,
            label: format_nm_label(nm),
        }))
        .collect()
}

fn format_nm_label(nm: f64) -> String {
    if nm.fract() == 0.0 {
        format!("{}nm", nm as u32)
    } else {
        format!("{nm}nm")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OwnshipSourceId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnshipSourceKind {
    DeviceGps,
    ExternalGps,
    ExternalAhrs,
    GpxPlayback,
    AdsbTrackPlayback,
    LiveNetworkTrack,
    FlightPlanSimulator,
    BadAutopilot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnshipMode {
    None,
    Live,
    Replay,
    Simulated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnshipBannerSeverity {
    Info,
    Caution,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnshipControlTone {
    Ready,
    Unavailable,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnshipLauncherTextTone {
    Normal,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceConnectionState {
    Unavailable,
    Searching,
    Connected,
    Stale,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SituationKinematics {
    pub position: LatLon,
    pub track_deg_true: Option<f64>,
    pub heading_deg_true: Option<f64>,
    pub ground_speed_kt: Option<f64>,
    pub altitude_msl_ft: Option<f64>,
    pub pressure_altitude_ft: Option<f64>,
    pub vertical_speed_fpm: Option<f64>,
    pub event_time_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SituationSample {
    pub source_id: OwnshipSourceId,
    pub source_kind: OwnshipSourceKind,
    pub event_time_epoch_ms: i64,
    pub received_time_epoch_ms: i64,
    pub position: Option<LatLon>,
    #[serde(default)]
    pub horizontal_accuracy_m: Option<f64>,
    #[serde(default)]
    pub vertical_accuracy_m: Option<f64>,
    pub track_deg_true: Option<f64>,
    pub heading_deg_true: Option<f64>,
    pub ground_speed_kt: Option<f64>,
    pub altitude_msl_ft: Option<f64>,
    pub pressure_altitude_ft: Option<f64>,
    #[serde(default)]
    pub vertical_speed_fpm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipSourceStatus {
    pub source_id: OwnshipSourceId,
    pub source_kind: OwnshipSourceKind,
    pub display_name: String,
    pub connection_state: SourceConnectionState,
    pub last_event_time_epoch_ms: Option<i64>,
    pub last_received_time_epoch_ms: Option<i64>,
    pub stale_after_ms: i64,
    pub selectable: bool,
    pub enabled: bool,
    pub auto_eligible: bool,
    pub active: bool,
    pub provides_position: bool,
    pub provides_heading: bool,
    pub provides_track: bool,
    pub provides_speed: bool,
    pub provides_altitude: bool,
    pub status_label: String,
    pub latest_sample: Option<SituationSample>,
    #[serde(default)]
    pub recent_samples: Vec<SituationSample>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnshipSelectionPolicy {
    Auto,
    Manual { source_id: OwnshipSourceId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnshipSelectionCommand {
    Auto,
    Source { source_id: OwnshipSourceId },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipPolicy {
    pub selection: OwnshipSelectionPolicy,
    pub source_priority: Vec<OwnshipSourceId>,
    pub allow_auto_replay: bool,
    pub allow_auto_simulated: bool,
}

impl Default for OwnshipPolicy {
    fn default() -> Self {
        Self {
            selection: OwnshipSelectionPolicy::Auto,
            source_priority: Vec::new(),
            allow_auto_replay: false,
            allow_auto_simulated: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedOwnshipState {
    pub mode: OwnshipMode,
    pub active_source_id: Option<OwnshipSourceId>,
    pub active_source_kind: Option<OwnshipSourceKind>,
    pub kinematics: Option<SituationKinematics>,
    pub banner_text: String,
    pub banner_severity: OwnshipBannerSeverity,
    pub guidance_enabled: bool,
    pub sequencing_enabled: bool,
}

impl Default for ResolvedOwnshipState {
    fn default() -> Self {
        Self::none()
    }
}

impl ResolvedOwnshipState {
    pub fn none() -> Self {
        Self {
            mode: OwnshipMode::None,
            active_source_id: None,
            active_source_kind: None,
            kinematics: None,
            banner_text: "NO GPS POSITION".to_string(),
            banner_severity: OwnshipBannerSeverity::Warning,
            guidance_enabled: false,
            sequencing_enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipRenderState {
    pub mode: OwnshipMode,
    pub banner_text: String,
    pub banner_severity: OwnshipBannerSeverity,
    pub draw_aircraft: bool,
    pub draw_predictor: bool,
    pub draw_cdi: bool,
    pub position: Option<LatLon>,
    pub orientation_deg: Option<f64>,
    pub magnetic_variation_deg: Option<f64>,
    pub speed_kt: Option<f64>,
    pub altitude_msl_ft: Option<f64>,
    pub pressure_altitude_ft: Option<f64>,
    pub terrain_altitude_bucket_ft: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipSourceMenuItem {
    pub source_id: OwnshipSourceId,
    pub source_kind: OwnshipSourceKind,
    pub label: String,
    pub launcher_label: String,
    pub tone: OwnshipControlTone,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
    pub active: bool,
    pub status_label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SituationControlMenuItem {
    pub input: SituationControlInput,
    pub label: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipControlModel {
    pub mode: OwnshipMode,
    pub selection: OwnshipSelectionCommand,
    pub launcher_label: String,
    pub launcher_tone: OwnshipControlTone,
    pub launcher_text_tone: OwnshipLauncherTextTone,
    pub sources: Vec<OwnshipSourceMenuItem>,
    pub situation_controls: Vec<SituationControlMenuItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipUiState {
    pub render: OwnshipRenderState,
    pub controls: OwnshipControlModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipState {
    pub policy: OwnshipPolicy,
    pub resolved: ResolvedOwnshipState,
    pub render: OwnshipRenderState,
    pub controls: OwnshipControlModel,
    pub sources: Vec<OwnshipSourceStatus>,
}

impl Default for OwnshipState {
    fn default() -> Self {
        let resolved = ResolvedOwnshipState::default();
        Self {
            policy: OwnshipPolicy::default(),
            render: project_render_state(&resolved),
            controls: OwnshipControlModel {
                mode: resolved.mode,
                selection: OwnshipSelectionCommand::Auto,
                launcher_label: "No GPS".to_string(),
                launcher_tone: OwnshipControlTone::Unavailable,
                launcher_text_tone: OwnshipLauncherTextTone::Unavailable,
                sources: Vec::new(),
                situation_controls: situation_control_handler_for_mode(OwnshipMode::None)
                    .menu_items(),
            },
            resolved,
            sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipSourceRegistration {
    pub source_id: OwnshipSourceId,
    pub source_kind: OwnshipSourceKind,
    pub display_name: String,
    pub selectable: bool,
    pub auto_eligible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipSourceStatusUpdate {
    pub source_id: OwnshipSourceId,
    pub connection_state: SourceConnectionState,
    pub enabled: bool,
    pub status_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SituationControlInput {
    SkipBackward,
    FastRewind,
    FastForward,
    SkipForward,
}

pub fn register_source(
    state: &OwnshipState,
    registration: OwnshipSourceRegistration,
) -> OwnshipState {
    let mut next = state.clone();
    match next
        .sources
        .iter_mut()
        .find(|source| source.source_id == registration.source_id)
    {
        Some(existing) => {
            existing.source_kind = registration.source_kind;
            existing.display_name = registration.display_name;
            existing.selectable = registration.selectable;
            existing.auto_eligible = registration.auto_eligible;
        }
        None => next.sources.push(OwnshipSourceStatus {
            source_id: registration.source_id,
            source_kind: registration.source_kind,
            display_name: registration.display_name,
            connection_state: SourceConnectionState::Unavailable,
            last_event_time_epoch_ms: None,
            last_received_time_epoch_ms: None,
            stale_after_ms: DEFAULT_STALE_AFTER_MS,
            selectable: registration.selectable,
            enabled: true,
            auto_eligible: registration.auto_eligible,
            active: false,
            provides_position: true,
            provides_heading: matches!(
                registration.source_kind,
                OwnshipSourceKind::ExternalAhrs
                    | OwnshipSourceKind::FlightPlanSimulator
                    | OwnshipSourceKind::BadAutopilot
            ),
            provides_track: !matches!(registration.source_kind, OwnshipSourceKind::ExternalAhrs),
            provides_speed: true,
            provides_altitude: matches!(
                registration.source_kind,
                OwnshipSourceKind::ExternalAhrs
                    | OwnshipSourceKind::FlightPlanSimulator
                    | OwnshipSourceKind::BadAutopilot
            ),
            status_label: "Unavailable".to_string(),
            latest_sample: None,
            recent_samples: Vec::new(),
        }),
    }
    refresh(&next)
}

pub fn update_source_status(
    state: &OwnshipState,
    update: OwnshipSourceStatusUpdate,
) -> OwnshipState {
    let mut next = state.clone();
    if let Some(source) = next
        .sources
        .iter_mut()
        .find(|source| source.source_id == update.source_id)
    {
        source.connection_state = update.connection_state;
        source.enabled = update.enabled;
        source.status_label = update.status_label;
    }
    refresh(&next)
}

pub fn set_policy(state: &OwnshipState, policy: OwnshipPolicy) -> OwnshipState {
    let mut next = state.clone();
    next.policy = policy;
    refresh(&next)
}

pub fn select_source(state: &OwnshipState, selection: OwnshipSelectionCommand) -> OwnshipState {
    let mut next = state.clone();
    next.policy.selection = match selection {
        OwnshipSelectionCommand::Auto => OwnshipSelectionPolicy::Auto,
        OwnshipSelectionCommand::Source { source_id } => {
            OwnshipSelectionPolicy::Manual { source_id }
        }
    };
    refresh(&next)
}

pub fn push_sample(state: &OwnshipState, sample: SituationSample) -> OwnshipState {
    let mut next = state.clone();
    match next
        .sources
        .iter_mut()
        .find(|source| source.source_id == sample.source_id)
    {
        Some(source) => apply_sample(source, sample),
        None => {
            let mut source = OwnshipSourceStatus {
                source_id: sample.source_id.clone(),
                source_kind: sample.source_kind,
                display_name: sample.source_id.0.clone(),
                connection_state: SourceConnectionState::Connected,
                last_event_time_epoch_ms: None,
                last_received_time_epoch_ms: None,
                stale_after_ms: DEFAULT_STALE_AFTER_MS,
                selectable: true,
                enabled: true,
                auto_eligible: true,
                active: false,
                provides_position: sample.position.is_some(),
                provides_heading: sample.heading_deg_true.is_some(),
                provides_track: sample.track_deg_true.is_some(),
                provides_speed: sample.ground_speed_kt.is_some(),
                provides_altitude: sample.altitude_msl_ft.is_some()
                    || sample.pressure_altitude_ft.is_some(),
                status_label: "Connected".to_string(),
                latest_sample: None,
                recent_samples: Vec::new(),
            };
            apply_sample(&mut source, sample);
            next.sources.push(source);
        }
    }
    refresh(&next)
}

fn apply_sample(source: &mut OwnshipSourceStatus, sample: SituationSample) {
    source.source_kind = sample.source_kind;
    source.last_event_time_epoch_ms = Some(sample.event_time_epoch_ms);
    source.last_received_time_epoch_ms = Some(sample.received_time_epoch_ms);
    source.connection_state = SourceConnectionState::Connected;
    source.status_label = "Connected".to_string();
    source.provides_position = sample.position.is_some();
    source.provides_heading = sample.heading_deg_true.is_some();
    source.provides_track = sample.track_deg_true.is_some();
    source.provides_speed = sample.ground_speed_kt.is_some();
    source.provides_altitude =
        sample.altitude_msl_ft.is_some() || sample.pressure_altitude_ft.is_some();
    source.latest_sample = Some(sample.clone());
    source.recent_samples.push(sample);
    prune_recent_samples(source);
}

fn prune_recent_samples(source: &mut OwnshipSourceStatus) {
    let Some(latest_time) = source
        .latest_sample
        .as_ref()
        .map(|sample| sample.event_time_epoch_ms)
    else {
        source.recent_samples.clear();
        return;
    };
    source.recent_samples.retain(|sample| {
        latest_time
            .checked_sub(sample.event_time_epoch_ms)
            .is_some_and(|age_ms| (0..=VERTICAL_SPEED_HISTORY_RETENTION_MS).contains(&age_ms))
    });
}

fn refresh(state: &OwnshipState) -> OwnshipState {
    let mut next = state.clone();
    next.resolved = resolve_state(&next.policy, &mut next.sources);
    next.render = project_render_state(&next.resolved);
    next.controls = project_controls(&next.policy, &next.sources, next.resolved.mode);
    next
}

fn resolve_state(
    policy: &OwnshipPolicy,
    sources: &mut [OwnshipSourceStatus],
) -> ResolvedOwnshipState {
    let now_epoch_ms = sources
        .iter()
        .filter_map(|source| source.last_received_time_epoch_ms)
        .max()
        .unwrap_or(0);

    for source in sources.iter_mut() {
        source.active = false;
        if source.enabled && source.latest_sample.is_some() && !is_fresh(source, now_epoch_ms) {
            source.connection_state = SourceConnectionState::Stale;
            source.status_label = "Stale".to_string();
        }
    }

    let selected = match &policy.selection {
        OwnshipSelectionPolicy::Manual { source_id } => sources.iter_mut().find(|source| {
            source.source_id == *source_id && is_manual_candidate(source, now_epoch_ms)
        }),
        OwnshipSelectionPolicy::Auto => {
            if let Some(found) =
                pick_by_priority(sources, &policy.source_priority, now_epoch_ms, policy)
            {
                Some(found)
            } else {
                sources
                    .iter_mut()
                    .find(|source| is_candidate(source, now_epoch_ms, policy))
            }
        }
    };

    let Some(source) = selected else {
        return ResolvedOwnshipState::none();
    };

    source.active = true;
    let sample = match source.latest_sample.as_ref() {
        Some(sample) => sample,
        None => return ResolvedOwnshipState::none(),
    };
    let position = match sample.position {
        Some(position) => position,
        None => return ResolvedOwnshipState::none(),
    };

    let mode = mode_for_kind(source.source_kind);
    let banner_text = match mode {
        OwnshipMode::None => "NO GPS POSITION".to_string(),
        OwnshipMode::Live => "LIVE POSITION".to_string(),
        OwnshipMode::Replay => "REPLAY".to_string(),
        OwnshipMode::Simulated => "SIMULATED POSITION".to_string(),
    };
    let banner_severity = match mode {
        OwnshipMode::None => OwnshipBannerSeverity::Warning,
        OwnshipMode::Live => OwnshipBannerSeverity::Info,
        OwnshipMode::Replay | OwnshipMode::Simulated => OwnshipBannerSeverity::Caution,
    };
    let kinematics = SituationKinematics {
        position,
        track_deg_true: sample.track_deg_true,
        heading_deg_true: sample.heading_deg_true,
        ground_speed_kt: sample.ground_speed_kt,
        altitude_msl_ft: sample.altitude_msl_ft,
        pressure_altitude_ft: sample.pressure_altitude_ft,
        vertical_speed_fpm: sample
            .vertical_speed_fpm
            .filter(|vertical_speed_fpm| vertical_speed_fpm.is_finite())
            .or_else(|| filtered_vertical_speed_fpm(source, sample)),
        event_time_epoch_ms: sample.event_time_epoch_ms,
    };

    ResolvedOwnshipState {
        mode,
        active_source_id: Some(source.source_id.clone()),
        active_source_kind: Some(source.source_kind),
        kinematics: Some(kinematics),
        banner_text,
        banner_severity,
        guidance_enabled: mode != OwnshipMode::None,
        sequencing_enabled: mode != OwnshipMode::None,
    }
}

fn pick_by_priority<'a>(
    sources: &'a mut [OwnshipSourceStatus],
    priority: &[OwnshipSourceId],
    now_epoch_ms: i64,
    policy: &OwnshipPolicy,
) -> Option<&'a mut OwnshipSourceStatus> {
    for source_id in priority {
        if let Some(index) = sources.iter().position(|source| {
            source.source_id == *source_id && is_candidate(source, now_epoch_ms, policy)
        }) {
            return sources.get_mut(index);
        }
    }
    None
}

fn is_manual_candidate(source: &OwnshipSourceStatus, now_epoch_ms: i64) -> bool {
    source.enabled
        && source.selectable
        && source.connection_state == SourceConnectionState::Connected
        && source
            .latest_sample
            .as_ref()
            .is_some_and(|sample| sample.position.is_some())
        && is_fresh(source, now_epoch_ms)
}

fn is_candidate(source: &OwnshipSourceStatus, now_epoch_ms: i64, policy: &OwnshipPolicy) -> bool {
    source.enabled
        && source.auto_eligible
        && source.connection_state == SourceConnectionState::Connected
        && source
            .latest_sample
            .as_ref()
            .is_some_and(|sample| sample.position.is_some())
        && is_fresh(source, now_epoch_ms)
        && match mode_for_kind(source.source_kind) {
            OwnshipMode::Replay => policy.allow_auto_replay,
            OwnshipMode::Simulated => policy.allow_auto_simulated,
            OwnshipMode::Live | OwnshipMode::None => true,
        }
}

fn is_fresh(source: &OwnshipSourceStatus, now_epoch_ms: i64) -> bool {
    if source.source_kind == OwnshipSourceKind::FlightPlanSimulator {
        return true;
    }
    source
        .last_received_time_epoch_ms
        .is_some_and(|received| now_epoch_ms - received <= source.stale_after_ms)
}

fn mode_for_kind(kind: OwnshipSourceKind) -> OwnshipMode {
    match kind {
        OwnshipSourceKind::DeviceGps
        | OwnshipSourceKind::ExternalGps
        | OwnshipSourceKind::ExternalAhrs => OwnshipMode::Live,
        OwnshipSourceKind::GpxPlayback
        | OwnshipSourceKind::AdsbTrackPlayback
        | OwnshipSourceKind::LiveNetworkTrack => OwnshipMode::Replay,
        OwnshipSourceKind::FlightPlanSimulator | OwnshipSourceKind::BadAutopilot => {
            OwnshipMode::Simulated
        }
    }
}

fn project_render_state(resolved: &ResolvedOwnshipState) -> OwnshipRenderState {
    let orientation_deg = resolved
        .kinematics
        .as_ref()
        .and_then(|kinematics| kinematics.heading_deg_true.or(kinematics.track_deg_true));
    let speed_kt = resolved
        .kinematics
        .as_ref()
        .and_then(|kinematics| kinematics.ground_speed_kt);
    let altitude_msl_ft = resolved
        .kinematics
        .as_ref()
        .and_then(|kinematics| kinematics.altitude_msl_ft);
    let pressure_altitude_ft = resolved
        .kinematics
        .as_ref()
        .and_then(|kinematics| kinematics.pressure_altitude_ft);
    let terrain_altitude_bucket_ft =
        crate::terrain_altitude_bucket_ft(altitude_msl_ft.or(pressure_altitude_ft));

    OwnshipRenderState {
        mode: resolved.mode,
        banner_text: resolved.banner_text.clone(),
        banner_severity: resolved.banner_severity,
        draw_aircraft: resolved.kinematics.is_some(),
        draw_predictor: resolved.kinematics.is_some() && speed_kt.is_some(),
        draw_cdi: resolved.guidance_enabled,
        position: resolved
            .kinematics
            .as_ref()
            .map(|kinematics| kinematics.position),
        orientation_deg,
        magnetic_variation_deg: None,
        speed_kt,
        altitude_msl_ft,
        pressure_altitude_ft,
        terrain_altitude_bucket_ft,
    }
}

fn filtered_vertical_speed_fpm(
    source: &OwnshipSourceStatus,
    latest: &SituationSample,
) -> Option<f64> {
    sample_altitude_ft(latest).filter(|altitude_ft| altitude_ft.is_finite())?;
    let points = source
        .recent_samples
        .iter()
        .filter_map(|sample| {
            let age_ms = latest
                .event_time_epoch_ms
                .checked_sub(sample.event_time_epoch_ms)?;
            if !(0..=VERTICAL_SPEED_HISTORY_RETENTION_MS).contains(&age_ms) {
                return None;
            }
            let altitude_ft =
                sample_altitude_ft(sample).filter(|altitude_ft| altitude_ft.is_finite())?;
            let accuracy_m = sample
                .vertical_accuracy_m
                .filter(|accuracy_m| accuracy_m.is_finite() && *accuracy_m > 0.0)
                .unwrap_or(VERTICAL_SPEED_DEFAULT_ACCURACY_M)
                .clamp(VERTICAL_SPEED_MIN_ACCURACY_M, VERTICAL_SPEED_MAX_ACCURACY_M);
            let recency_weight =
                2.0_f64.powf(-(age_ms as f64) / VERTICAL_SPEED_RECENCY_HALF_LIFE_MS);
            Some(VerticalSpeedPoint {
                time_seconds: -(age_ms as f64) / 1_000.0,
                altitude_ft,
                base_weight: recency_weight / accuracy_m.powi(2),
            })
        })
        .collect::<Vec<_>>();
    if points.len() < VERTICAL_SPEED_MIN_SAMPLES {
        return None;
    }
    let span_seconds = points.iter().map(|point| point.time_seconds).fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(minimum, maximum), time| (minimum.min(time), maximum.max(time)),
    );
    if (span_seconds.1 - span_seconds.0) * 1_000.0 < VERTICAL_SPEED_MIN_SPAN_MS as f64 {
        return None;
    }

    let mut weights = points
        .iter()
        .map(|point| point.base_weight)
        .collect::<Vec<_>>();
    let mut fit = weighted_vertical_speed_fit(&points, &weights)?;
    for _ in 0..VERTICAL_SPEED_ROBUST_ITERATIONS {
        let residuals = points
            .iter()
            .map(|point| {
                (point.altitude_ft
                    - (fit.intercept_ft + fit.slope_ft_per_second * point.time_seconds))
                    .abs()
            })
            .collect::<Vec<_>>();
        let threshold_ft = (2.5 * median(&residuals)?).max(VERTICAL_SPEED_MIN_OUTLIER_THRESHOLD_FT);
        weights = points
            .iter()
            .zip(residuals)
            .map(|(point, residual_ft)| {
                point.base_weight * (threshold_ft / residual_ft.max(f64::EPSILON)).min(1.0)
            })
            .collect();
        fit = weighted_vertical_speed_fit(&points, &weights)?;
    }
    Some(fit.slope_ft_per_second * 60.0)
}

fn sample_altitude_ft(sample: &SituationSample) -> Option<f64> {
    sample.altitude_msl_ft.or(sample.pressure_altitude_ft)
}

struct VerticalSpeedPoint {
    time_seconds: f64,
    altitude_ft: f64,
    base_weight: f64,
}

struct VerticalSpeedFit {
    intercept_ft: f64,
    slope_ft_per_second: f64,
}

fn weighted_vertical_speed_fit(
    points: &[VerticalSpeedPoint],
    weights: &[f64],
) -> Option<VerticalSpeedFit> {
    if points.len() != weights.len() || points.is_empty() {
        return None;
    }
    let weight_sum = weights.iter().sum::<f64>();
    if !weight_sum.is_finite() || weight_sum <= 0.0 {
        return None;
    }
    let mean_time_seconds = points
        .iter()
        .zip(weights)
        .map(|(point, weight)| point.time_seconds * weight)
        .sum::<f64>()
        / weight_sum;
    let mean_altitude_ft = points
        .iter()
        .zip(weights)
        .map(|(point, weight)| point.altitude_ft * weight)
        .sum::<f64>()
        / weight_sum;
    let denominator = points
        .iter()
        .zip(weights)
        .map(|(point, weight)| weight * (point.time_seconds - mean_time_seconds).powi(2))
        .sum::<f64>();
    if !denominator.is_finite() || denominator <= 0.0 {
        return None;
    }
    let slope_ft_per_second = points
        .iter()
        .zip(weights)
        .map(|(point, weight)| {
            weight
                * (point.time_seconds - mean_time_seconds)
                * (point.altitude_ft - mean_altitude_ft)
        })
        .sum::<f64>()
        / denominator;
    if !slope_ft_per_second.is_finite() {
        return None;
    }
    Some(VerticalSpeedFit {
        intercept_ft: mean_altitude_ft - slope_ft_per_second * mean_time_seconds,
        slope_ft_per_second,
    })
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    let middle = ordered.len() / 2;
    if ordered.len().is_multiple_of(2) {
        Some((ordered[middle - 1] + ordered[middle]) / 2.0)
    } else {
        Some(ordered[middle])
    }
}

fn project_controls(
    policy: &OwnshipPolicy,
    sources: &[OwnshipSourceStatus],
    mode: OwnshipMode,
) -> OwnshipControlModel {
    let mut menu_sources = sources
        .iter()
        .filter(|source| source.selectable)
        .map(|source| project_source_menu_item(source, policy))
        .collect::<Vec<_>>();
    menu_sources.sort_by(|left, right| {
        source_menu_rank(left.source_kind)
            .cmp(&source_menu_rank(right.source_kind))
            .then_with(|| left.label.cmp(&right.label))
            .then_with(|| left.source_id.0.cmp(&right.source_id.0))
    });
    let active = menu_sources.iter().find(|source| source.active);
    let selected_mode = active
        .map(|source| mode_for_kind(source.source_kind))
        .unwrap_or(mode);
    let launcher_label = active
        .map(|source| source.launcher_label.clone())
        .unwrap_or_else(|| "No GPS".to_string());
    let launcher_tone = active
        .map(|source| source.tone)
        .unwrap_or(OwnshipControlTone::Unavailable);
    let launcher_text_tone = launcher_text_tone_for_control_tone(launcher_tone);

    OwnshipControlModel {
        mode,
        selection: match &policy.selection {
            OwnshipSelectionPolicy::Auto => OwnshipSelectionCommand::Auto,
            OwnshipSelectionPolicy::Manual { source_id } => OwnshipSelectionCommand::Source {
                source_id: source_id.clone(),
            },
        },
        launcher_label,
        launcher_tone,
        launcher_text_tone,
        sources: menu_sources,
        situation_controls: situation_control_handler_for_mode(selected_mode).menu_items(),
    }
}

fn project_source_menu_item(
    source: &OwnshipSourceStatus,
    policy: &OwnshipPolicy,
) -> OwnshipSourceMenuItem {
    let launcher_label = source_launcher_label(source);
    let enabled = source.selectable && source.enabled;
    OwnshipSourceMenuItem {
        source_id: source.source_id.clone(),
        source_kind: source.source_kind,
        label: source_menu_label(source),
        launcher_label,
        tone: source_control_tone(source),
        enabled,
        disabled_reason: (!enabled).then(|| source_disabled_reason(source)),
        active: source.active || source_selected_by_policy(source, policy),
        status_label: source.status_label.clone(),
    }
}

fn source_disabled_reason(source: &OwnshipSourceStatus) -> String {
    if !source.selectable {
        return "This ownship source cannot be selected.".to_string();
    }
    source
        .status_label
        .trim()
        .is_empty()
        .then(|| "This ownship source is unavailable.".to_string())
        .unwrap_or_else(|| source.status_label.clone())
}

fn source_selected_by_policy(source: &OwnshipSourceStatus, policy: &OwnshipPolicy) -> bool {
    matches!(
        &policy.selection,
        OwnshipSelectionPolicy::Manual { source_id } if *source_id == source.source_id
    )
}

fn source_menu_label(source: &OwnshipSourceStatus) -> String {
    match source.source_kind {
        OwnshipSourceKind::DeviceGps | OwnshipSourceKind::ExternalGps => "GPS".to_string(),
        OwnshipSourceKind::ExternalAhrs => "AHARS".to_string(),
        OwnshipSourceKind::GpxPlayback
        | OwnshipSourceKind::AdsbTrackPlayback
        | OwnshipSourceKind::LiveNetworkTrack => "Replay".to_string(),
        OwnshipSourceKind::FlightPlanSimulator => "Plan\nPreview".to_string(),
        OwnshipSourceKind::BadAutopilot => "Bad AP".to_string(),
    }
}

fn source_menu_rank(kind: OwnshipSourceKind) -> u8 {
    match kind {
        OwnshipSourceKind::DeviceGps | OwnshipSourceKind::ExternalGps => 0,
        OwnshipSourceKind::FlightPlanSimulator => 1,
        OwnshipSourceKind::GpxPlayback
        | OwnshipSourceKind::AdsbTrackPlayback
        | OwnshipSourceKind::LiveNetworkTrack => 2,
        OwnshipSourceKind::BadAutopilot => 3,
        OwnshipSourceKind::ExternalAhrs => 4,
    }
}

trait SituationControlHandler {
    fn controls_enabled(&self) -> bool;

    fn disabled_reason(&self) -> Option<String> {
        (!self.controls_enabled()).then(|| {
            "Replay and plan preview controls are not available for this ownship source."
                .to_string()
        })
    }

    fn menu_items(&self) -> Vec<SituationControlMenuItem> {
        let enabled = self.controls_enabled();
        let disabled_reason = self.disabled_reason();
        vec![
            SituationControlMenuItem {
                input: SituationControlInput::SkipBackward,
                label: "⏮".to_string(),
                enabled,
                disabled_reason: disabled_reason.clone(),
            },
            SituationControlMenuItem {
                input: SituationControlInput::FastRewind,
                label: "⏪".to_string(),
                enabled,
                disabled_reason: disabled_reason.clone(),
            },
            SituationControlMenuItem {
                input: SituationControlInput::FastForward,
                label: "⏩".to_string(),
                enabled,
                disabled_reason: disabled_reason.clone(),
            },
            SituationControlMenuItem {
                input: SituationControlInput::SkipForward,
                label: "⏭".to_string(),
                enabled,
                disabled_reason,
            },
        ]
    }
}

struct DisabledSituationControlHandler;
struct ReplaySituationControlHandler;
struct PlanPreviewSituationControlHandler;

impl SituationControlHandler for DisabledSituationControlHandler {
    fn controls_enabled(&self) -> bool {
        false
    }
}

impl SituationControlHandler for ReplaySituationControlHandler {
    fn controls_enabled(&self) -> bool {
        true
    }
}

impl SituationControlHandler for PlanPreviewSituationControlHandler {
    fn controls_enabled(&self) -> bool {
        true
    }
}

fn situation_control_handler_for_mode(mode: OwnshipMode) -> Box<dyn SituationControlHandler> {
    match mode {
        OwnshipMode::Replay => Box::new(ReplaySituationControlHandler),
        OwnshipMode::Simulated => Box::new(PlanPreviewSituationControlHandler),
        OwnshipMode::None | OwnshipMode::Live => Box::new(DisabledSituationControlHandler),
    }
}

fn source_launcher_label(source: &OwnshipSourceStatus) -> String {
    match source.source_kind {
        OwnshipSourceKind::DeviceGps | OwnshipSourceKind::ExternalGps => {
            gps_launcher_label(source).to_string()
        }
        OwnshipSourceKind::ExternalAhrs => "AHARS".to_string(),
        OwnshipSourceKind::GpxPlayback => format!("Replay: {}", gps_launcher_label(source)),
        OwnshipSourceKind::AdsbTrackPlayback | OwnshipSourceKind::LiveNetworkTrack => {
            "Replay".to_string()
        }
        OwnshipSourceKind::FlightPlanSimulator => "Plan Preview".to_string(),
        OwnshipSourceKind::BadAutopilot => "Bad AP".to_string(),
    }
}

fn gps_launcher_label(source: &OwnshipSourceStatus) -> &'static str {
    if source.connection_state == SourceConnectionState::Connected
        && source
            .latest_sample
            .as_ref()
            .is_some_and(|sample| sample.position.is_some())
    {
        "GPS"
    } else {
        "No GPS"
    }
}

fn source_control_tone(source: &OwnshipSourceStatus) -> OwnshipControlTone {
    if !source.enabled {
        return OwnshipControlTone::Unavailable;
    }
    match source.source_kind {
        OwnshipSourceKind::DeviceGps | OwnshipSourceKind::ExternalGps => {
            if gps_launcher_label(source) == "GPS" {
                OwnshipControlTone::Ready
            } else {
                OwnshipControlTone::Unavailable
            }
        }
        OwnshipSourceKind::GpxPlayback => {
            if gps_launcher_label(source) == "GPS" {
                OwnshipControlTone::Neutral
            } else {
                OwnshipControlTone::Unavailable
            }
        }
        OwnshipSourceKind::ExternalAhrs
        | OwnshipSourceKind::AdsbTrackPlayback
        | OwnshipSourceKind::LiveNetworkTrack
        | OwnshipSourceKind::FlightPlanSimulator
        | OwnshipSourceKind::BadAutopilot => OwnshipControlTone::Neutral,
    }
}

fn launcher_text_tone_for_control_tone(tone: OwnshipControlTone) -> OwnshipLauncherTextTone {
    match tone {
        OwnshipControlTone::Unavailable => OwnshipLauncherTextTone::Unavailable,
        OwnshipControlTone::Ready | OwnshipControlTone::Neutral => OwnshipLauncherTextTone::Normal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_or_missing_sources_resolve_to_none() {
        let state = push_sample(
            &OwnshipState::default(),
            SituationSample {
                source_id: OwnshipSourceId("gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 10_000,
                received_time_epoch_ms: 10_000,
                position: Some(LatLon {
                    lat: 47.0,
                    lon: -122.0,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(90.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: None,
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        );
        let state = push_sample(
            &state,
            SituationSample {
                source_id: OwnshipSourceId("other".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 16_001,
                received_time_epoch_ms: 16_001,
                position: None,
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: None,
                heading_deg_true: None,
                ground_speed_kt: None,
                altitude_msl_ft: None,
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        );

        assert_eq!(state.resolved.mode, OwnshipMode::None);
        assert_eq!(state.resolved.banner_text, "NO GPS POSITION");
        assert!(!state.render.draw_aircraft);
        assert!(!state.render.draw_cdi);
    }

    #[test]
    fn selected_unavailable_gps_uses_unavailable_launcher_text_tone() {
        let state = register_source(
            &OwnshipState::default(),
            OwnshipSourceRegistration {
                source_id: OwnshipSourceId("gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                display_name: "GPS".to_string(),
                selectable: true,
                auto_eligible: true,
            },
        );
        let state = update_source_status(
            &state,
            OwnshipSourceStatusUpdate {
                source_id: OwnshipSourceId("gps".to_string()),
                connection_state: SourceConnectionState::Searching,
                enabled: true,
                status_label: "Searching".to_string(),
            },
        );
        let state = select_source(
            &state,
            OwnshipSelectionCommand::Source {
                source_id: OwnshipSourceId("gps".to_string()),
            },
        );

        assert_eq!(state.controls.launcher_label, "No GPS");
        assert_eq!(
            state.controls.launcher_text_tone,
            OwnshipLauncherTextTone::Unavailable
        );
    }

    #[test]
    fn connected_gps_uses_normal_launcher_text_tone() {
        let state = push_sample(
            &OwnshipState::default(),
            SituationSample {
                source_id: OwnshipSourceId("gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(LatLon {
                    lat: 47.0,
                    lon: -122.0,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(90.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: None,
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        );

        assert_eq!(state.controls.launcher_label, "GPS");
        assert_eq!(
            state.controls.launcher_text_tone,
            OwnshipLauncherTextTone::Normal
        );
    }

    #[test]
    fn searching_gps_stops_projecting_position_immediately() {
        let state = push_sample(
            &OwnshipState::default(),
            SituationSample {
                source_id: OwnshipSourceId("gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(LatLon {
                    lat: 47.0,
                    lon: -122.0,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(90.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(1_000.0),
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        );
        let state = select_source(
            &state,
            OwnshipSelectionCommand::Source {
                source_id: OwnshipSourceId("gps".to_string()),
            },
        );
        assert_eq!(state.controls.launcher_label, "GPS");
        assert!(state.render.draw_aircraft);

        let state = update_source_status(
            &state,
            OwnshipSourceStatusUpdate {
                source_id: OwnshipSourceId("gps".to_string()),
                connection_state: SourceConnectionState::Searching,
                enabled: true,
                status_label: "Searching".to_string(),
            },
        );

        assert_eq!(state.controls.launcher_label, "No GPS");
        assert_eq!(
            state.controls.launcher_text_tone,
            OwnshipLauncherTextTone::Unavailable
        );
        assert_eq!(state.resolved.mode, OwnshipMode::None);
        assert!(!state.render.draw_aircraft);
        assert!(!state.render.draw_cdi);
    }

    #[test]
    fn manual_selection_does_not_fall_back() {
        let state = push_sample(
            &OwnshipState::default(),
            SituationSample {
                source_id: OwnshipSourceId("gps".to_string()),
                source_kind: OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(LatLon {
                    lat: 47.0,
                    lon: -122.0,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(90.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: None,
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        );
        let state = push_sample(
            &state,
            SituationSample {
                source_id: OwnshipSourceId("sim".to_string()),
                source_kind: OwnshipSourceKind::FlightPlanSimulator,
                event_time_epoch_ms: 2_000,
                received_time_epoch_ms: 2_000,
                position: Some(LatLon {
                    lat: 48.0,
                    lon: -123.0,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(180.0),
                heading_deg_true: Some(180.0),
                ground_speed_kt: Some(100.0),
                altitude_msl_ft: None,
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        );
        let state = set_policy(
            &state,
            OwnshipPolicy {
                selection: OwnshipSelectionPolicy::Manual {
                    source_id: OwnshipSourceId("gps".to_string()),
                },
                source_priority: Vec::new(),
                allow_auto_replay: false,
                allow_auto_simulated: false,
            },
        );
        let state = push_sample(
            &state,
            SituationSample {
                source_id: OwnshipSourceId("clock".to_string()),
                source_kind: OwnshipSourceKind::LiveNetworkTrack,
                event_time_epoch_ms: 7_000,
                received_time_epoch_ms: 7_000,
                position: None,
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: None,
                heading_deg_true: None,
                ground_speed_kt: None,
                altitude_msl_ft: None,
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        );

        assert_eq!(state.resolved.mode, OwnshipMode::None);
    }

    #[test]
    fn manual_selection_can_use_selectable_non_auto_source() {
        let state = register_source(
            &OwnshipState::default(),
            OwnshipSourceRegistration {
                source_id: OwnshipSourceId("sim".to_string()),
                source_kind: OwnshipSourceKind::FlightPlanSimulator,
                display_name: "Sim".to_string(),
                selectable: true,
                auto_eligible: false,
            },
        );
        let state = push_sample(
            &state,
            SituationSample {
                source_id: OwnshipSourceId("sim".to_string()),
                source_kind: OwnshipSourceKind::FlightPlanSimulator,
                event_time_epoch_ms: 2_000,
                received_time_epoch_ms: 2_000,
                position: Some(LatLon {
                    lat: 48.676,
                    lon: -122.86,
                }),
                horizontal_accuracy_m: None,
                vertical_accuracy_m: None,
                track_deg_true: Some(315.0),
                heading_deg_true: Some(315.0),
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: Some(1_000.0),
                pressure_altitude_ft: None,
                vertical_speed_fpm: None,
            },
        );
        let state = select_source(
            &state,
            OwnshipSelectionCommand::Source {
                source_id: OwnshipSourceId("sim".to_string()),
            },
        );

        assert_eq!(
            state.resolved.active_source_id,
            Some(OwnshipSourceId("sim".to_string()))
        );
        assert_eq!(state.resolved.mode, OwnshipMode::Simulated);
        assert_eq!(state.render.terrain_altitude_bucket_ft, Some(1_000.0));
    }

    #[test]
    fn situation_ring_candidates_are_core_owned() {
        let candidates = situation_ring_candidates();

        assert_eq!(candidates.first().unwrap().label, "500ft");
        assert_eq!(candidates.last().unwrap().label, "800nm");
        assert!(candidates
            .iter()
            .any(|candidate| candidate.label == "1.5nm"));
        assert!(candidates
            .windows(2)
            .all(|pair| pair[0].radius_nm < pair[1].radius_nm));
    }

    #[test]
    fn vertical_speed_prefers_native_sample_value() {
        let state = push_sample(
            &OwnshipState::default(),
            sample_with_altitude("gps", 1_000, 3_000.0, Some(450.0)),
        );
        let state = push_sample(
            &state,
            sample_with_altitude("gps", 7_000, 3_600.0, Some(500.0)),
        );

        assert_eq!(
            state
                .resolved
                .kinematics
                .as_ref()
                .and_then(|kinematics| kinematics.vertical_speed_fpm),
            Some(500.0)
        );
    }

    #[test]
    fn vertical_speed_is_derived_from_meter_quantized_altitude_history() {
        let mut state = OwnshipState::default();
        for second in 0..=20 {
            let altitude_ft = 3_000.0 + second as f64 * 500.0 / 60.0;
            let quantized_altitude_ft = (altitude_ft / FEET_PER_METER).round() * FEET_PER_METER;
            state = push_sample(
                &state,
                sample_with_altitude_and_accuracy(
                    "gps",
                    second * 1_000,
                    quantized_altitude_ft,
                    Some(3.0),
                    None,
                ),
            );
        }

        let vertical_speed_fpm = state
            .resolved
            .kinematics
            .as_ref()
            .and_then(|kinematics| kinematics.vertical_speed_fpm)
            .unwrap();
        assert!((vertical_speed_fpm - 500.0).abs() < 15.0);
    }

    #[test]
    fn vertical_speed_rejects_trace_shaped_low_confidence_altitude_excursion() {
        let mut state = OwnshipState::default();
        for second in 0..=20 {
            let is_bad_fix = second == 14;
            state = push_sample(
                &state,
                sample_with_altitude_and_accuracy(
                    "gps",
                    second * 1_000,
                    if is_bad_fix { 111.8 } else { 51.0 } * FEET_PER_METER,
                    Some(if is_bad_fix { 20.0 } else { 1.0 }),
                    None,
                ),
            );
        }

        let vertical_speed_fpm = state
            .resolved
            .kinematics
            .as_ref()
            .and_then(|kinematics| kinematics.vertical_speed_fpm)
            .unwrap();
        assert!(vertical_speed_fpm.abs() < 5.0);
    }

    #[test]
    fn vertical_speed_waits_for_enough_altitude_history() {
        let state = push_sample(
            &OwnshipState::default(),
            sample_with_altitude("gps", 1_000, 3_000.0, None),
        );
        let state = push_sample(&state, sample_with_altitude("gps", 2_000, 3_010.0, None));

        assert_eq!(
            state
                .resolved
                .kinematics
                .as_ref()
                .and_then(|kinematics| kinematics.vertical_speed_fpm),
            None
        );
    }

    fn sample_with_altitude(
        source_id: &str,
        event_time_epoch_ms: i64,
        altitude_msl_ft: f64,
        vertical_speed_fpm: Option<f64>,
    ) -> SituationSample {
        sample_with_altitude_and_accuracy(
            source_id,
            event_time_epoch_ms,
            altitude_msl_ft,
            None,
            vertical_speed_fpm,
        )
    }

    fn sample_with_altitude_and_accuracy(
        source_id: &str,
        event_time_epoch_ms: i64,
        altitude_msl_ft: f64,
        vertical_accuracy_m: Option<f64>,
        vertical_speed_fpm: Option<f64>,
    ) -> SituationSample {
        SituationSample {
            source_id: OwnshipSourceId(source_id.to_string()),
            source_kind: OwnshipSourceKind::DeviceGps,
            event_time_epoch_ms,
            received_time_epoch_ms: event_time_epoch_ms,
            position: Some(LatLon {
                lat: 47.0,
                lon: -122.0,
            }),
            horizontal_accuracy_m: None,
            vertical_accuracy_m,
            track_deg_true: Some(90.0),
            heading_deg_true: None,
            ground_speed_kt: Some(120.0),
            altitude_msl_ft: Some(altitude_msl_ft),
            pressure_altitude_ft: None,
            vertical_speed_fpm,
        }
    }
}
