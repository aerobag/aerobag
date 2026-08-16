// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use app_ui_contracts::session::UiPlaybackPanelState;

use crate::{
    map_follow::{MapFollowSessionState, MapFollowUiState},
    ownship::{
        push_sample, refresh_at, register_source, select_source, set_policy,
        set_source_power_paused, update_source_status, OwnshipPolicy, OwnshipSelectionCommand,
        OwnshipSelectionPolicy, OwnshipSourceId, OwnshipSourceKind, OwnshipSourceRegistration,
        OwnshipSourceStatusUpdate, OwnshipState, OwnshipUiState, SituationSample,
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

#[derive(Debug, Clone, PartialEq)]
struct SituationModel {
    ownship: OwnshipState,
    playback: PlaybackSessionState,
    plan_preview: PlanPreviewState,
    bad_autopilot: BadAutopilotState,
    map_follow: MapFollowSessionState,
    revision: u64,
}

impl Default for SituationModel {
    fn default() -> Self {
        Self {
            ownship: OwnshipState::default(),
            playback: PlaybackSessionState::default(),
            plan_preview: PlanPreviewState::default(),
            bad_autopilot: BadAutopilotState::default(),
            map_follow: MapFollowSessionState::default(),
            revision: 0,
        }
    }
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
                render: self.model.ownship.render.clone(),
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
