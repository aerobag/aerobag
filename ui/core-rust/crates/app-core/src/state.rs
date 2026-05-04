use serde::{Deserialize, Serialize};

use crate::content::{ContentInventory, ContentPolicy, ContentReport};
use crate::errors::AppResult;
use crate::ownship::{
    push_sample, register_source, select_source, set_policy, update_source_status, OwnshipPolicy,
    OwnshipSelectionCommand, OwnshipSourceRegistration, OwnshipSourceStatusUpdate, OwnshipState,
    OwnshipUiState, SituationSample,
};
use crate::planning::{project_ui_state, FlightPlan, FlightPlanUiState};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    pub active_plan: Option<FlightPlan>,
    pub ownship: OwnshipState,
    pub content_policy: ContentPolicy,
    pub last_content_report: Option<ContentReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppUiState {
    pub active_plan: Option<FlightPlanUiState>,
    pub ownship: OwnshipUiState,
    pub content_policy: ContentPolicy,
    pub last_content_report: Option<ContentReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UiSnapshotAppState {
    pub active_plan: Option<FlightPlan>,
    pub content_policy: ContentPolicy,
    pub last_content_report: Option<ContentReport>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_plan: None,
            ownship: OwnshipState::default(),
            content_policy: ContentPolicy::PreferLocal,
            last_content_report: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AppEvent {
    SetContentPolicy(ContentPolicy),
    RegisterOwnshipSource(OwnshipSourceRegistration),
    UpdateOwnshipSourceStatus(OwnshipSourceStatusUpdate),
    PushSituationSample(SituationSample),
    SetOwnshipPolicy(OwnshipPolicy),
    SelectOwnshipSource(OwnshipSelectionCommand),
    ReplaceFlightPlan(FlightPlan),
    RefreshContent { inventory: ContentInventory },
    ClearFlightPlan,
}

pub fn reduce(state: &AppState, event: AppEvent) -> AppResult<AppState> {
    let mut next = state.clone();

    match event {
        AppEvent::SetContentPolicy(policy) => {
            next.content_policy = policy;
        }
        AppEvent::RegisterOwnshipSource(registration) => {
            next.ownship = register_source(&next.ownship, registration);
        }
        AppEvent::UpdateOwnshipSourceStatus(update) => {
            next.ownship = update_source_status(&next.ownship, update);
        }
        AppEvent::PushSituationSample(sample) => {
            next.ownship = push_sample(&next.ownship, sample);
        }
        AppEvent::SetOwnshipPolicy(policy) => {
            next.ownship = set_policy(&next.ownship, policy);
        }
        AppEvent::SelectOwnshipSource(selection) => {
            next.ownship = select_source(&next.ownship, selection);
        }
        AppEvent::ReplaceFlightPlan(plan) => {
            let plan = crate::build_flight_plan(plan)?;
            next.active_plan = Some(plan);
            next.last_content_report = None;
        }
        AppEvent::RefreshContent { inventory: _ } => {
            // Content reporting now requires HAD-backed facts. The reducer no
            // longer carries precomputed requirements; session operations should
            // resolve content status through the shared HAD client.
            next.last_content_report = None;
        }
        AppEvent::ClearFlightPlan => {
            next.active_plan = None;
            next.last_content_report = None;
        }
    }

    Ok(next)
}

pub fn project_app_ui_state(state: &AppState) -> AppUiState {
    AppUiState {
        active_plan: state.active_plan.as_ref().map(project_ui_state),
        ownship: OwnshipUiState {
            render: state.ownship.render.clone(),
            controls: state.ownship.controls.clone(),
        },
        content_policy: state.content_policy,
        last_content_report: state.last_content_report.clone(),
    }
}

pub fn project_ui_snapshot_app_state(state: &AppState) -> UiSnapshotAppState {
    UiSnapshotAppState {
        active_plan: state.active_plan.clone(),
        content_policy: state.content_policy,
        last_content_report: state.last_content_report.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AirportId, AppErrorKind, NavRef, RouteComponent};

    fn sample_plan() -> FlightPlan {
        FlightPlan {
            id: "plan-1".to_string(),
            name: "KBOS to KJFK".to_string(),
            legs: Vec::new(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBOS".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KJFK".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KBOS".to_string())),
            destination: Some(AirportId("KJFK".to_string())),
            alternate: None,
            cruise_altitude_ft: Some(3000),
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        }
    }

    #[test]
    fn replace_plan_sets_active_plan_and_clears_stale_report() {
        let initial = AppState {
            last_content_report: Some(ContentReport {
                fully_satisfied: true,
                items: Vec::new(),
            }),
            ..AppState::default()
        };

        let next = reduce(&initial, AppEvent::ReplaceFlightPlan(sample_plan())).unwrap();

        assert!(next.active_plan.is_some());
        assert!(next.last_content_report.is_none());
    }

    #[test]
    fn push_situation_sample_updates_ownship() {
        let next = reduce(
            &AppState::default(),
            AppEvent::PushSituationSample(crate::SituationSample {
                source_id: crate::OwnshipSourceId("gps".to_string()),
                source_kind: crate::OwnshipSourceKind::DeviceGps,
                event_time_epoch_ms: 1_000,
                received_time_epoch_ms: 1_000,
                position: Some(crate::LatLon {
                    lat: 47.5,
                    lon: -122.3,
                }),
                track_deg_true: Some(90.0),
                heading_deg_true: None,
                ground_speed_kt: Some(120.0),
                altitude_msl_ft: None,
                pressure_altitude_ft: None,
            }),
        )
        .unwrap();

        assert_eq!(next.ownship.resolved.mode, crate::OwnshipMode::Live);
        assert_eq!(
            next.ownship.render.position,
            Some(crate::LatLon {
                lat: 47.5,
                lon: -122.3
            })
        );
        assert_eq!(next.ownship.render.orientation_deg, Some(90.0));
        assert_eq!(next.ownship.render.speed_kt, Some(120.0));
    }

    #[test]
    fn clear_flight_plan_drops_report() {
        let with_plan = reduce(
            &AppState::default(),
            AppEvent::ReplaceFlightPlan(sample_plan()),
        )
        .unwrap();
        let with_report = AppState {
            last_content_report: Some(ContentReport {
                fully_satisfied: true,
                items: Vec::new(),
            }),
            ..with_plan
        };
        let cleared = reduce(&with_report, AppEvent::ClearFlightPlan).unwrap();

        assert!(cleared.active_plan.is_none());
        assert!(cleared.last_content_report.is_none());
    }

    #[test]
    fn reducer_reuses_plan_validation() {
        let result = reduce(
            &AppState::default(),
            AppEvent::ReplaceFlightPlan(FlightPlan {
                id: "bad".to_string(),
                name: "bad".to_string(),
                legs: Vec::new(),
                route_components: Vec::new(),
                route_component_uids: Vec::new(),
                route_component_uid_counter: 0,
                resolved_legs: Vec::new(),
                guidance: None,
                departure: None,
                destination: None,
                alternate: None,
                cruise_altitude_ft: None,
                notes: None,
                updated_at_epoch_ms: 0,
                version: 1,
            }),
        );

        assert_eq!(result.unwrap_err().kind, AppErrorKind::InvalidFlightPlan);
    }

    #[test]
    fn project_app_ui_state_projects_active_plan() {
        let with_plan = reduce(
            &AppState::default(),
            AppEvent::ReplaceFlightPlan(sample_plan()),
        )
        .unwrap();

        let ui = project_app_ui_state(&with_plan);

        assert!(ui.active_plan.is_some());
        assert_eq!(ui.ownship.render.mode, crate::OwnshipMode::None);
        assert_eq!(ui.content_policy, with_plan.content_policy);
        assert_eq!(
            ui.active_plan.as_ref().unwrap().components.len(),
            with_plan
                .active_plan
                .as_ref()
                .unwrap()
                .clone()
                .normalized()
                .route_components
                .len()
        );
    }
}
