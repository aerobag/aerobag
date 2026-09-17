// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use app_ui_contracts::session::UiPlaybackPanelState;

use crate::{
    map_follow::{MapFollowSessionState, MapFollowUiState},
    ownship::{
        push_sample, refresh_at, register_source, select_source, set_policy,
        set_source_power_paused, set_source_power_sleeping, update_source_status, OwnshipPolicy,
        OwnshipSelectionCommand, OwnshipSelectionPolicy, OwnshipSourceId, OwnshipSourceKind,
        OwnshipSourceRegistration, OwnshipSourceStatusUpdate, OwnshipState, OwnshipUiState,
        SituationSample,
    },
    playback::PlaybackSessionState,
    LatLon, MapViewport, PlaybackUiState,
};

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct BadAutopilotState {
    pub running: bool,
    pub active_detail_id: Option<String>,
    pub offset_nm: f64,
    pub wander_phase_rad: f64,
    pub last_tick_epoch_ms: Option<f64>,
    pub last_position: Option<LatLon>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct PlanPreviewState {
    pub pointer: Option<PlanPreviewPointer>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlanPreviewPointer {
    pub row_uid: String,
    pub offset_nm: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct SituationModel {
    barometer: crate::barometer::Barometer,
    altitude_target: crate::altitude_target::AltitudeTarget,
    ownship: OwnshipState,
    playback: PlaybackSessionState,
    plan_preview: PlanPreviewState,
    bad_autopilot: BadAutopilotState,
    map_follow: MapFollowSessionState,
    revision: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SituationProjection {
    pub ownship: OwnshipUiState,
    pub playback_ui_state: PlaybackUiState,
    pub playback_panel_state: UiPlaybackPanelState,
    pub map_follow_ui_state: MapFollowUiState,
    pub map_follow_target_viewport: Option<MapViewport>,
}

pub(crate) struct SituationProjectionResult {
    pub projection: SituationProjection,
    pub rebuilt: bool,
}

#[derive(Clone)]
struct SituationProjectionCache {
    revision: u64,
    projection: SituationProjection,
}

#[derive(Clone)]
pub(crate) struct SituationModelCheckpoint {
    model: SituationModel,
}

#[derive(Default)]
pub(crate) struct SituationController {
    model: SituationModel,
    projection_cache: Option<SituationProjectionCache>,
}

impl SituationController {
    pub fn altitude_target(&self) -> &crate::altitude_target::AltitudeTarget {
        &self.model.altitude_target
    }

    pub fn altitude_target_mut(&mut self) -> &mut crate::altitude_target::AltitudeTarget {
        self.note_change();
        &mut self.model.altitude_target
    }

    pub fn refresh_altitude_target(&mut self, now: i64) {
        if self
            .model
            .altitude_target
            .refresh(&self.model.ownship, &self.model.barometer, now)
        {
            self.note_change();
        }
    }

    pub fn barometer(&self) -> &crate::barometer::Barometer {
        &self.model.barometer
    }

    pub fn barometer_mut(&mut self) -> &mut crate::barometer::Barometer {
        self.note_change();
        &mut self.model.barometer
    }

    pub fn new(ownship: OwnshipState) -> Self {
        Self {
            model: SituationModel {
                ownship,
                ..SituationModel::default()
            },
            projection_cache: None,
        }
    }

    pub fn revision(&self) -> u64 {
        self.model.revision
    }

    pub fn checkpoint_model(&self) -> SituationModelCheckpoint {
        SituationModelCheckpoint {
            model: self.model.clone(),
        }
    }

    pub fn rollback_model(&mut self, checkpoint: SituationModelCheckpoint) {
        self.model = checkpoint.model;
        self.projection_cache = None;
    }

    /// A tour rewinds preview/playback and selection, while live receiver data
    /// continues to arrive. Keep those new samples and receiver capabilities.
    pub fn restore_user_state(&mut self, saved: SituationModelCheckpoint) {
        let revision_before_restore = self.model.revision;
        let live_sources = self
            .model
            .ownship
            .sources
            .iter()
            .filter(|source| {
                matches!(
                    source.source_kind,
                    OwnshipSourceKind::DeviceGps
                        | OwnshipSourceKind::ExternalGps
                        | OwnshipSourceKind::ExternalAhrs
                        | OwnshipSourceKind::LiveNetworkTrack
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        self.rollback_model(saved);
        // Restoring user choices is a new published mutation, not a transaction
        // rollback. Rewinding this dependency counter can collide with the tour
        // revision and omit the restored situation from the session delta.
        // select_source below advances the live revision and clears the cache.
        self.model.revision = revision_before_restore;
        for mut source in live_sources {
            if let Some(previous) = self
                .model
                .ownship
                .sources
                .iter_mut()
                .find(|old| old.source_id == source.source_id)
            {
                source.power_state = previous.power_state;
                *previous = source;
            } else {
                self.model.ownship.sources.push(source);
            }
        }
        let selection = self.model.ownship.controls.selection.clone();
        self.select_source(selection);
    }

    pub fn ownship(&self) -> &OwnshipState {
        &self.model.ownship
    }

    pub fn register_source(&mut self, registration: OwnshipSourceRegistration) {
        self.model.ownship = register_source(&self.model.ownship, registration);
        self.note_change();
    }

    pub fn update_source_status(&mut self, update: OwnshipSourceStatusUpdate) {
        self.model.ownship = update_source_status(&self.model.ownship, update);
        self.note_change();
    }

    pub fn set_policy(&mut self, policy: OwnshipPolicy) {
        self.model.ownship = set_policy(&self.model.ownship, policy);
        self.note_change();
    }

    pub fn select_source(&mut self, selection: OwnshipSelectionCommand) {
        self.model.ownship = select_source(&self.model.ownship, selection);
        self.note_change();
    }

    pub fn push_sample(&mut self, sample: SituationSample) {
        self.model.ownship = push_sample(&self.model.ownship, sample);
        self.note_change();
    }

    pub fn set_source_power_paused(&mut self, source_id: &OwnshipSourceId, paused: bool) {
        let next = set_source_power_paused(&self.model.ownship, source_id, paused);
        if next != self.model.ownship {
            self.model.ownship = next;
            self.note_change();
        }
    }

    pub fn set_source_power_sleeping(&mut self, source_id: &OwnshipSourceId, sleeping: bool) {
        let next = set_source_power_sleeping(&self.model.ownship, source_id, sleeping);
        if next != self.model.ownship {
            self.model.ownship = next;
            self.note_change();
        }
    }

    pub fn refresh_ownship_at(&mut self, now_epoch_ms: i64) {
        let next = refresh_at(&self.model.ownship, now_epoch_ms);
        if next != self.model.ownship {
            self.model.ownship = next;
            self.note_change();
        }
    }

    pub fn playback(&self) -> &PlaybackSessionState {
        &self.model.playback
    }

    pub fn playback_mut(&mut self) -> &mut PlaybackSessionState {
        self.note_change();
        &mut self.model.playback
    }

    pub fn plan_preview(&self) -> &PlanPreviewState {
        &self.model.plan_preview
    }

    pub fn plan_preview_mut(&mut self) -> &mut PlanPreviewState {
        self.note_change();
        &mut self.model.plan_preview
    }

    pub fn bad_autopilot(&self) -> &BadAutopilotState {
        &self.model.bad_autopilot
    }

    pub fn bad_autopilot_mut(&mut self) -> &mut BadAutopilotState {
        self.note_change();
        &mut self.model.bad_autopilot
    }

    pub fn reset_bad_autopilot(&mut self) {
        if self.model.bad_autopilot != BadAutopilotState::default() {
            self.model.bad_autopilot = BadAutopilotState::default();
            self.note_change();
        }
    }

    pub fn engage_map_follow(&mut self, viewport: MapViewport) {
        self.model.map_follow.engage(viewport);
        self.note_change();
    }

    pub fn disengage_map_follow(&mut self, viewport: MapViewport) {
        self.model.map_follow.disengage(viewport);
        self.note_change();
    }

    pub fn set_map_follow_anchor(
        &mut self,
        viewport: MapViewport,
        offset_x_px: f64,
        offset_y_px: f64,
    ) {
        self.model
            .map_follow
            .set_anchor_offset(viewport, offset_x_px, offset_y_px);
        self.note_change();
    }

    pub fn sync_map_follow_for_viewport(
        &mut self,
        viewport: MapViewport,
        width_px: f64,
        height_px: f64,
    ) {
        self.model.map_follow.sync_for_viewport(
            &self.model.ownship.render,
            viewport,
            width_px,
            height_px,
        );
        self.note_change();
    }

    pub fn project(&mut self) -> SituationProjectionResult {
        if let Some(cache) = self.projection_cache.as_ref() {
            if cache.revision == self.model.revision {
                return SituationProjectionResult {
                    projection: cache.projection.clone(),
                    rebuilt: false,
                };
            }
        }

        let map_follow_before = self.model.map_follow.clone();
        let (map_follow_ui_state, map_follow_target_viewport) = self
            .model
            .map_follow
            .snapshot_projection(&self.model.ownship.render);
        if self.model.map_follow != map_follow_before {
            self.model.revision = self.model.revision.saturating_add(1);
        }
        let projection = SituationProjection {
            ownship: OwnshipUiState {
                render: crate::OwnshipRenderState {
                    altitude_intercept: self.model.altitude_target.annotation(),
                    ..self.model.ownship.render.clone()
                },
                controls: self.model.ownship.controls.clone(),
            },
            playback_ui_state: self.model.playback.ui_state(),
            playback_panel_state: UiPlaybackPanelState {
                visible: selected_ownship_source_kind(&self.model.ownship)
                    .is_some_and(is_replay_source_kind),
            },
            map_follow_ui_state,
            map_follow_target_viewport,
        };
        self.projection_cache = Some(SituationProjectionCache {
            revision: self.model.revision,
            projection: projection.clone(),
        });
        SituationProjectionResult {
            projection,
            rebuilt: true,
        }
    }

    fn note_change(&mut self) {
        self.model.revision = self.model.revision.saturating_add(1);
        self.projection_cache = None;
    }
}

pub(crate) fn selected_ownship_source_kind(ownship: &OwnshipState) -> Option<OwnshipSourceKind> {
    match &ownship.policy.selection {
        OwnshipSelectionPolicy::Manual { source_id } => ownship
            .sources
            .iter()
            .find(|source| source.source_id == *source_id)
            .map(|source| source.source_kind),
        OwnshipSelectionPolicy::Auto => ownship.resolved.active_source_kind,
    }
}

fn is_replay_source_kind(kind: OwnshipSourceKind) -> bool {
    matches!(
        kind,
        OwnshipSourceKind::GpxPlayback | OwnshipSourceKind::AdsbTrackPlayback
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OwnshipSourceId, SourceConnectionState};

    fn replay_controller() -> SituationController {
        let mut controller = SituationController::default();
        let source_id = OwnshipSourceId("replay".to_string());
        controller.register_source(OwnshipSourceRegistration {
            source_id: source_id.clone(),
            source_kind: OwnshipSourceKind::AdsbTrackPlayback,
            display_name: "Replay".to_string(),
            selectable: true,
            auto_eligible: false,
            stale_after_ms: None,
            power_state: None,
        });
        controller.update_source_status(OwnshipSourceStatusUpdate {
            source_id: source_id.clone(),
            connection_state: SourceConnectionState::Connected,
            enabled: true,
            status_label: "Ready".to_string(),
        });
        controller.select_source(OwnshipSelectionCommand::Source { source_id });
        controller
    }

    #[test]
    fn guided_tour_restore_preserves_live_receivers_and_restores_selection_and_power() {
        let mut controller = replay_controller();
        let gps_id = OwnshipSourceId("gps".into());
        controller.register_source(OwnshipSourceRegistration {
            source_id: gps_id.clone(),
            source_kind: OwnshipSourceKind::DeviceGps,
            display_name: "GPS".into(),
            selectable: true,
            auto_eligible: true,
            stale_after_ms: None,
            power_state: Some(crate::OwnshipSourcePowerState::Paused),
        });
        let selection = controller.ownship().controls.selection.clone();
        let saved = controller.checkpoint_model();
        controller.select_source(OwnshipSelectionCommand::Source {
            source_id: gps_id.clone(),
        });
        controller.set_source_power_paused(&gps_id, false);
        controller.update_source_status(OwnshipSourceStatusUpdate {
            source_id: gps_id.clone(),
            connection_state: SourceConnectionState::Connected,
            enabled: true,
            status_label: "New receiver status".into(),
        });
        controller.restore_user_state(saved);
        assert_eq!(controller.ownship().controls.selection, selection);
        let gps = controller
            .ownship()
            .sources
            .iter()
            .find(|source| source.source_id == gps_id)
            .unwrap();
        assert_eq!(
            gps.power_state,
            Some(crate::OwnshipSourcePowerState::Paused)
        );
        assert_eq!(gps.status_label, "New receiver status");
    }

    #[test]
    fn restoring_user_state_publishes_a_new_revision_even_after_one_tour_change() {
        let mut controller = SituationController::default();
        let saved = controller.checkpoint_model();
        let viewport = MapViewport {
            center: crate::LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            zoom: 10.0,
            rotation_deg: 0.0,
            pitch_deg: 0.0,
        };
        controller.disengage_map_follow(viewport);
        let tour_revision = controller.revision();
        assert!(
            !controller
                .project()
                .projection
                .map_follow_ui_state
                .following
        );

        controller.restore_user_state(saved);

        assert!(
            controller
                .project()
                .projection
                .map_follow_ui_state
                .following
        );
        assert!(
            controller.revision() > tour_revision,
            "restoring the saved model must invalidate the visible tour state, not reuse its revision"
        );
    }

    #[test]
    fn projection_is_cached_and_owns_replay_panel_policy() {
        let mut controller = replay_controller();

        let first = controller.project();
        assert!(first.rebuilt);
        assert!(first.projection.playback_panel_state.visible);
        assert!(!controller.project().rebuilt);

        controller.select_source(OwnshipSelectionCommand::Auto);
        let changed = controller.project();
        assert!(changed.rebuilt);
        assert!(!changed.projection.playback_panel_state.visible);
    }

    #[test]
    fn checkpoint_rolls_back_ownship_and_preview_model() {
        let mut controller = replay_controller();
        let checkpoint = controller.checkpoint_model();
        controller.select_source(OwnshipSelectionCommand::Auto);
        controller.plan_preview_mut().pointer = Some(PlanPreviewPointer {
            row_uid: "row-1".to_string(),
            offset_nm: 12.0,
        });

        controller.rollback_model(checkpoint);

        assert_eq!(
            selected_ownship_source_kind(controller.ownship()),
            Some(OwnshipSourceKind::AdsbTrackPlayback)
        );
        assert!(controller.plan_preview().pointer.is_none());
    }
}
