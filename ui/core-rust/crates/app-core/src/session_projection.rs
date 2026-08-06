// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use app_ui_contracts::session::{ClientBuildInfo, UiChartPageState, UiDebugState};

use crate::{
    session::AltitudePlannerWindSelection, AltitudePlannerDepartureTimeBasis, ContentPolicy,
    ContentReport,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SessionProjectionVersions {
    pub envelope: u64,
    pub nav_data: u64,
    pub application: u64,
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
pub(crate) struct FlightDataBannerProjectionDependencies {
    pub flight_plan_revision: u64,
    pub flight_plan_route_revision: u64,
    pub situation_revision: u64,
    pub weather_revision: u64,
    pub map_revision: u64,
    pub nav_data_generation: u64,
    pub cloud_revision: u64,
    pub wall_clock_epoch_ms: i64,
    pub local_time_zone: Option<String>,
    pub wind_selection: AltitudePlannerWindSelection,
    pub departure_time_basis: AltitudePlannerDepartureTimeBasis,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ApplicationProjectionDependencies {
    pub flight_data_banner: FlightDataBannerProjectionDependencies,
    pub settings_revision: u64,
    pub content_policy: ContentPolicy,
    pub last_content_report: Option<ContentReport>,
    pub bad_autopilot_enabled: bool,
    pub internet_adsb_enabled: bool,
    pub internet_adsb_registration: Option<String>,
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
    pub nav_data_revision: u64,
    pub package_revision: u64,
    pub cloud_revision: u64,
    pub weather_revision: u64,
    pub wall_clock_epoch_ms: i64,
    pub client_build: Option<ClientBuildInfo>,
    pub cloud_available: bool,
    pub next_freshness_check_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SettingsProjectionDependencies {
    pub settings_revision: u64,
    pub display_policy_available: bool,
    pub flight_data_banner: FlightDataBannerProjectionDependencies,
    pub debug_state: UiDebugState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CloudProjectionDependencies {
    pub cloud_revision: u64,
    pub wall_clock_epoch_ms: i64,
    pub qr_scanner_available: bool,
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
    pub application: ApplicationProjectionDependencies,
    pub situation: u64,
    pub charts: UiChartPageState,
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
            &mut self.versions.application,
            &previous.application,
            &dependencies.application,
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

    fn flight_data_banner_dependencies() -> FlightDataBannerProjectionDependencies {
        FlightDataBannerProjectionDependencies {
            flight_plan_revision: 0,
            flight_plan_route_revision: 0,
            situation_revision: 0,
            weather_revision: 0,
            map_revision: 0,
            nav_data_generation: 0,
            cloud_revision: 0,
            wall_clock_epoch_ms: 0,
            local_time_zone: None,
            wind_selection: AltitudePlannerWindSelection::NoWind,
            departure_time_basis: AltitudePlannerDepartureTimeBasis::Local,
        }
    }

    fn application_dependencies() -> ApplicationProjectionDependencies {
        ApplicationProjectionDependencies {
            flight_data_banner: flight_data_banner_dependencies(),
            settings_revision: 0,
            content_policy: ContentPolicy::PreferLocal,
            last_content_report: None,
            bad_autopilot_enabled: false,
            internet_adsb_enabled: false,
            internet_adsb_registration: None,
        }
    }

    fn dependencies() -> SessionProjectionDependencies {
        let application = application_dependencies();
        SessionProjectionDependencies {
            envelope: 0,
            nav_data: NavDataProjectionDependencies {
                nav_data_revision: 0,
                package_revision: 0,
                next_maintenance_epoch_ms: None,
            },
            application,
            situation: 0,
            charts: chart_state(),
            map: MapProjectionDependencies {
                map_revision: 0,
                internet_adsb_enabled: false,
            },
            status: StatusProjectionDependencies {
                data_status_revision: 0,
                nav_data_revision: 0,
                package_revision: 0,
                cloud_revision: 0,
                weather_revision: 0,
                wall_clock_epoch_ms: 0,
                client_build: None,
                cloud_available: false,
                next_freshness_check_epoch_ms: None,
            },
            settings: SettingsProjectionDependencies {
                settings_revision: 0,
                display_policy_available: false,
                flight_data_banner: flight_data_banner_dependencies(),
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
                cloud_revision: 0,
                wall_clock_epoch_ms: 0,
                qr_scanner_available: false,
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
        assert_group_change!(application, |value: &mut SessionProjectionDependencies| {
            value.application.settings_revision += 1;
        });
        assert_group_change!(situation, |value: &mut SessionProjectionDependencies| {
            value.situation += 1;
        });
        assert_group_change!(charts, |value: &mut SessionProjectionDependencies| {
            value.charts.selected_airport_id = "KSEA".to_string();
        });
        assert_group_change!(map, |value: &mut SessionProjectionDependencies| {
            value.map.map_revision += 1;
        });
        assert_group_change!(status, |value: &mut SessionProjectionDependencies| {
            value.status.data_status_revision += 1;
        });
        assert_group_change!(settings, |value: &mut SessionProjectionDependencies| {
            value.settings.settings_revision += 1;
        });
        assert_group_change!(cloud, |value: &mut SessionProjectionDependencies| {
            value.cloud.cloud_revision += 1;
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
