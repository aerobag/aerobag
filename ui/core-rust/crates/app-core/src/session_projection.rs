// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use app_ui_contracts::session::{UiChartPageState, UiDebugState, UiSettingsGridItem};

use crate::{
    ContentPolicy, ContentReport, FlightDataBannerModel, FlightPlanUiState, OwnshipUiState,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SessionProjectionVersions {
    pub envelope: u64,
    pub nav_data: u64,
    pub application_shell: u64,
    pub flight_plan: u64,
    pub ownship: u64,
    pub flight_data: u64,
    pub situation: u64,
    pub charts: u64,
    pub map: u64,
    pub status: u64,
    pub settings: u64,
    pub cloud: u64,
    pub packages: u64,
    pub home: u64,
    pub debug: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ApplicationShellProjectionDependencies {
    pub content_policy: ContentPolicy,
    pub last_content_report: Option<ContentReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FlightPlanProjectionDependencies {
    pub route_revision: u64,
    pub active_plan: Option<FlightPlanUiState>,
    pub aircraft_plan_view_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FlightDataProjectionDependencies {
    pub banner: FlightDataBannerModel,
    pub settings_items: Vec<UiSettingsGridItem>,
    pub next_refresh_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChartProjectionDependencies {
    pub state: UiChartPageState,
    pub notam_display_state_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NavDataProjectionDependencies {
    pub nav_data_revision: u64,
    pub package_revision: u64,
    pub next_maintenance_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MapProjectionDependencies {
    pub map_revision: u64,
    pub internet_adsb_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StatusProjectionDependencies {
    pub data_status_revision: u64,
    pub page_projection_revision: u64,
    pub next_freshness_check_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SettingsProjectionDependencies {
    pub static_revision: u64,
    pub display_policy_available: bool,
    pub debug_state: UiDebugState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CloudProjectionDependencies {
    pub projection_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HomeProjectionDependencies {
    pub offline_packages_available: bool,
    pub cloud_available: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SessionProjectionDependencies {
    pub envelope: u64,
    pub nav_data: NavDataProjectionDependencies,
    pub application_shell: ApplicationShellProjectionDependencies,
    pub flight_plan: FlightPlanProjectionDependencies,
    pub ownship: OwnshipUiState,
    pub flight_data: FlightDataProjectionDependencies,
    pub situation: u64,
    pub charts: ChartProjectionDependencies,
    pub map: MapProjectionDependencies,
    pub status: StatusProjectionDependencies,
    pub settings: SettingsProjectionDependencies,
    pub cloud: CloudProjectionDependencies,
    pub packages: u64,
    pub home: HomeProjectionDependencies,
    pub debug: UiDebugState,
}

#[derive(Clone, Default)]
pub(crate) struct SessionProjectionVersionState {
    versions: SessionProjectionVersions,
    dependencies: Option<SessionProjectionDependencies>,
}

impl SessionProjectionVersionState {
    pub fn versions(&self) -> SessionProjectionVersions {
        self.versions
    }

    pub fn observe(&mut self, dependencies: SessionProjectionDependencies) {
        let Some(previous) = self.dependencies.take() else {
            self.dependencies = Some(dependencies);
            return;
        };
        advance_if_changed(
            &mut self.versions.envelope,
            &previous.envelope,
            &dependencies.envelope,
        );
        advance_if_changed(
            &mut self.versions.nav_data,
            &previous.nav_data,
            &dependencies.nav_data,
        );
        advance_if_changed(
            &mut self.versions.application_shell,
            &previous.application_shell,
            &dependencies.application_shell,
        );
        advance_if_changed(
            &mut self.versions.flight_plan,
            &previous.flight_plan,
            &dependencies.flight_plan,
        );
        advance_if_changed(
            &mut self.versions.ownship,
            &previous.ownship,
            &dependencies.ownship,
        );
        advance_if_changed(
            &mut self.versions.flight_data,
            &previous.flight_data,
            &dependencies.flight_data,
        );
        advance_if_changed(
            &mut self.versions.situation,
            &previous.situation,
            &dependencies.situation,
        );
        advance_if_changed(
            &mut self.versions.charts,
            &previous.charts,
            &dependencies.charts,
        );
        advance_if_changed(&mut self.versions.map, &previous.map, &dependencies.map);
        advance_if_changed(
            &mut self.versions.status,
            &previous.status,
            &dependencies.status,
        );
        advance_if_changed(
            &mut self.versions.settings,
            &previous.settings,
            &dependencies.settings,
        );
        advance_if_changed(
            &mut self.versions.cloud,
            &previous.cloud,
            &dependencies.cloud,
        );
        advance_if_changed(
            &mut self.versions.packages,
            &previous.packages,
            &dependencies.packages,
        );
        advance_if_changed(&mut self.versions.home, &previous.home, &dependencies.home);
        advance_if_changed(
            &mut self.versions.debug,
            &previous.debug,
            &dependencies.debug,
        );
        self.dependencies = Some(dependencies);
    }
}

fn advance_if_changed<T: PartialEq>(version: &mut u64, previous: &T, current: &T) {
    if previous != current {
        *version = version.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart_state() -> UiChartPageState {
        UiChartPageState {
            ordered_airport_ids: Vec::new(),
            recent_airport_ids: Vec::new(),
            plate_target_airport_id: None,
            selected_airport_id: String::new(),
            selected_reference_family_id: None,
            selected_chart_id: String::new(),
            suggested_chart_ids: Vec::new(),
        }
    }

    fn application_shell_dependencies() -> ApplicationShellProjectionDependencies {
        ApplicationShellProjectionDependencies {
            content_policy: ContentPolicy::PreferLocal,
            last_content_report: None,
        }
    }

    fn dependencies() -> SessionProjectionDependencies {
        SessionProjectionDependencies {
            envelope: 0,
            nav_data: NavDataProjectionDependencies {
                nav_data_revision: 0,
                package_revision: 0,
                next_maintenance_epoch_ms: None,
            },
            application_shell: application_shell_dependencies(),
            flight_plan: FlightPlanProjectionDependencies {
                route_revision: 0,
                active_plan: None,
                aircraft_plan_view_path: String::new(),
            },
            ownship: {
                let ownship = crate::OwnshipState::default();
                OwnshipUiState {
                    render: ownship.render,
                    controls: ownship.controls,
                }
            },
            flight_data: FlightDataProjectionDependencies {
                banner: FlightDataBannerModel::default(),
                settings_items: Vec::new(),
                next_refresh_epoch_ms: 60_000,
            },
            situation: 0,
            charts: ChartProjectionDependencies {
                state: chart_state(),
                notam_display_state_id: None,
            },
            map: MapProjectionDependencies {
                map_revision: 0,
                internet_adsb_enabled: false,
            },
            status: StatusProjectionDependencies {
                data_status_revision: 0,
                page_projection_revision: 0,
                next_freshness_check_epoch_ms: None,
            },
            settings: SettingsProjectionDependencies {
                static_revision: 0,
                display_policy_available: false,
                debug_state: UiDebugState {
                    tile_labels: false,
                    nexrad_tile_labels: false,
                    fast_tiles: false,
                    offline_simulated_clock_buttons: false,
                    sequencing_finish_lines: false,
                    plate_flight_plan: false,
                    bad_autopilot: false,
                    internet_adsb: false,
                    gps_capture: false,
                    debug_log_to_developer_server: false,
                },
            },
            cloud: CloudProjectionDependencies {
                projection_revision: 0,
            },
            packages: 0,
            home: HomeProjectionDependencies {
                offline_packages_available: false,
                cloud_available: false,
            },
            debug: UiDebugState {
                tile_labels: false,
                nexrad_tile_labels: false,
                fast_tiles: false,
                offline_simulated_clock_buttons: false,
                sequencing_finish_lines: false,
                plate_flight_plan: false,
                bad_autopilot: false,
                internet_adsb: false,
                gps_capture: false,
                debug_log_to_developer_server: false,
            },
        }
    }

    #[test]
    fn every_token_advances_only_for_its_dependency_group() {
        let mut state = SessionProjectionVersionState::default();
        let mut current = dependencies();
        let mut expected = SessionProjectionVersions::default();
        state.observe(current.clone());
        assert_eq!(state.versions(), SessionProjectionVersions::default());

        macro_rules! assert_group_change {
            ($version:ident, $mutate:expr) => {{
                let mut changed = current.clone();
                ($mutate)(&mut changed);
                expected.$version = expected.$version.saturating_add(1);
                state.observe(changed.clone());
                assert_eq!(state.versions(), expected);
                current = changed;
            }};
        }

        assert_group_change!(envelope, |value: &mut SessionProjectionDependencies| {
            value.envelope += 1;
        });
        assert_group_change!(nav_data, |value: &mut SessionProjectionDependencies| {
            value.nav_data.nav_data_revision += 1;
        });
        assert_group_change!(
            application_shell,
            |value: &mut SessionProjectionDependencies| {
                value.application_shell.content_policy = ContentPolicy::StreamAllowed;
            }
        );
        assert_group_change!(flight_plan, |value: &mut SessionProjectionDependencies| {
            value.flight_plan.route_revision += 1;
        });
        assert_group_change!(ownship, |value: &mut SessionProjectionDependencies| {
            value.ownship.render.draw_aircraft = true;
        });
        assert_group_change!(flight_data, |value: &mut SessionProjectionDependencies| {
            value
                .flight_data
                .banner
                .cells
                .push(app_ui_contracts::session::FlightDataCell {
                    id: "ground_speed".to_string(),
                    label: "GS".to_string(),
                    value: Some("120".to_string()),
                    action_id: None,
                    tone: Default::default(),
                    estimate_kind: Default::default(),
                });
        });
        assert_group_change!(situation, |value: &mut SessionProjectionDependencies| {
            value.situation += 1;
        });
        assert_group_change!(charts, |value: &mut SessionProjectionDependencies| {
            value.charts.state.selected_airport_id = "KSEA".to_string();
        });

        let mut versions = SessionProjectionVersionState::default();
        versions.observe(dependencies());
        let mut changed = dependencies();
        changed.charts.notam_display_state_id = Some("notam-state-2".to_string());
        versions.observe(changed);
        assert_eq!(versions.versions().charts, 1);
        assert_group_change!(map, |value: &mut SessionProjectionDependencies| {
            value.map.map_revision += 1;
        });
        assert_group_change!(status, |value: &mut SessionProjectionDependencies| {
            value.status.data_status_revision += 1;
        });
        assert_group_change!(settings, |value: &mut SessionProjectionDependencies| {
            value.settings.static_revision += 1;
        });
        assert_group_change!(cloud, |value: &mut SessionProjectionDependencies| {
            value.cloud.projection_revision += 1;
        });
        assert_group_change!(packages, |value: &mut SessionProjectionDependencies| {
            value.packages += 1;
        });
        assert_group_change!(home, |value: &mut SessionProjectionDependencies| {
            value.home.offline_packages_available = true;
        });
        assert_group_change!(debug, |value: &mut SessionProjectionDependencies| {
            value.debug.tile_labels = true;
        });

        state.observe(current);
        assert_eq!(state.versions(), expected);
    }
}
