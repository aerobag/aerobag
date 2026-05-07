use serde::{Deserialize, Serialize};

use crate::geometry::LatLon;

const DEFAULT_STALE_AFTER_MS: i64 = 5_000;
const FEET_PER_NAUTICAL_MILE: f64 = 6076.12;

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
    DebugOwnshipDriver,
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
    pub speed_kt: Option<f64>,
    pub altitude_msl_ft: Option<f64>,
    pub pressure_altitude_ft: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipSourceMenuItem {
    pub source_id: OwnshipSourceId,
    pub source_kind: OwnshipSourceKind,
    pub label: String,
    pub launcher_label: String,
    pub tone: OwnshipControlTone,
    pub enabled: bool,
    pub active: bool,
    pub status_label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SituationControlMenuItem {
    pub input: SituationControlInput,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OwnshipControlModel {
    pub mode: OwnshipMode,
    pub selection: OwnshipSelectionCommand,
    pub launcher_label: String,
    pub launcher_tone: OwnshipControlTone,
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
                    | OwnshipSourceKind::DebugOwnshipDriver
            ),
            provides_track: !matches!(registration.source_kind, OwnshipSourceKind::ExternalAhrs),
            provides_speed: true,
            provides_altitude: matches!(
                registration.source_kind,
                OwnshipSourceKind::ExternalAhrs
                    | OwnshipSourceKind::FlightPlanSimulator
                    | OwnshipSourceKind::DebugOwnshipDriver
            ),
            status_label: "Unavailable".to_string(),
            latest_sample: None,
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
    source.latest_sample = Some(sample);
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
            source.source_id == *source_id && is_candidate(source, now_epoch_ms, policy)
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

fn is_candidate(source: &OwnshipSourceStatus, now_epoch_ms: i64, policy: &OwnshipPolicy) -> bool {
    source.enabled
        && source.auto_eligible
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
        OwnshipSourceKind::FlightPlanSimulator | OwnshipSourceKind::DebugOwnshipDriver => {
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
        speed_kt,
        altitude_msl_ft,
        pressure_altitude_ft,
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
        sources: menu_sources,
        situation_controls: situation_control_handler_for_mode(selected_mode).menu_items(),
    }
}

fn project_source_menu_item(
    source: &OwnshipSourceStatus,
    policy: &OwnshipPolicy,
) -> OwnshipSourceMenuItem {
    let launcher_label = source_launcher_label(source);
    OwnshipSourceMenuItem {
        source_id: source.source_id.clone(),
        source_kind: source.source_kind,
        label: source_menu_label(source),
        launcher_label,
        tone: source_control_tone(source),
        enabled: source.selectable,
        active: source.active || source_selected_by_policy(source, policy),
        status_label: source.status_label.clone(),
    }
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
        OwnshipSourceKind::DebugOwnshipDriver => "Bad Autopilot".to_string(),
    }
}

fn source_menu_rank(kind: OwnshipSourceKind) -> u8 {
    match kind {
        OwnshipSourceKind::DeviceGps | OwnshipSourceKind::ExternalGps => 0,
        OwnshipSourceKind::FlightPlanSimulator => 1,
        OwnshipSourceKind::GpxPlayback
        | OwnshipSourceKind::AdsbTrackPlayback
        | OwnshipSourceKind::LiveNetworkTrack => 2,
        OwnshipSourceKind::DebugOwnshipDriver => 3,
        OwnshipSourceKind::ExternalAhrs => 4,
    }
}

trait SituationControlHandler {
    fn controls_enabled(&self) -> bool;

    fn menu_items(&self) -> Vec<SituationControlMenuItem> {
        let enabled = self.controls_enabled();
        vec![
            SituationControlMenuItem {
                input: SituationControlInput::SkipBackward,
                label: "⏮".to_string(),
                enabled,
            },
            SituationControlMenuItem {
                input: SituationControlInput::FastRewind,
                label: "⏪".to_string(),
                enabled,
            },
            SituationControlMenuItem {
                input: SituationControlInput::FastForward,
                label: "⏩".to_string(),
                enabled,
            },
            SituationControlMenuItem {
                input: SituationControlInput::SkipForward,
                label: "⏭".to_string(),
                enabled,
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
            if matches!(source.connection_state, SourceConnectionState::Connected) {
                "GPS".to_string()
            } else {
                "No GPS".to_string()
            }
        }
        OwnshipSourceKind::ExternalAhrs => "AHARS".to_string(),
        OwnshipSourceKind::GpxPlayback
        | OwnshipSourceKind::AdsbTrackPlayback
        | OwnshipSourceKind::LiveNetworkTrack => "Replay".to_string(),
        OwnshipSourceKind::FlightPlanSimulator => "Plan Preview".to_string(),
        OwnshipSourceKind::DebugOwnshipDriver => "Bad AP".to_string(),
    }
}

fn source_control_tone(source: &OwnshipSourceStatus) -> OwnshipControlTone {
    match source.source_kind {
        OwnshipSourceKind::DeviceGps | OwnshipSourceKind::ExternalGps => {
            if matches!(source.connection_state, SourceConnectionState::Connected) {
                OwnshipControlTone::Ready
            } else {
                OwnshipControlTone::Unavailable
            }
        }
        OwnshipSourceKind::ExternalAhrs
        | OwnshipSourceKind::GpxPlayback
        | OwnshipSourceKind::AdsbTrackPlayback
        | OwnshipSourceKind::LiveNetworkTrack
        | OwnshipSourceKind::FlightPlanSimulator
        | OwnshipSourceKind::DebugOwnshipDriver => OwnshipControlTone::Neutral,
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
            },
        );

        assert_eq!(state.resolved.mode, OwnshipMode::None);
        assert_eq!(state.resolved.banner_text, "NO GPS POSITION");
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
            },
        );

        assert_eq!(state.resolved.mode, OwnshipMode::None);
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
}
