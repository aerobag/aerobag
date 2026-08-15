// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub use app_ui_contracts::session::{
    FlightDataBannerModel, FlightDataCell, FlightDataCellTone, FlightDataColumn, FlightEstimateKind,
};
use chrono_tz::Tz;

#[derive(Debug, Clone, Copy, Default)]
pub struct FlightDataComputer {
    ground_speed_kt: Option<f64>,
    fuel_flow_gph: Option<f64>,
    now_epoch_ms: Option<i64>,
    time_display_mode: crate::TimeDisplayMode,
    local_time_zone: Tz,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlightTimeFuelEstimate {
    pub cumulative_ete_seconds: Option<f64>,
    pub cumulative_fuel_gal: Option<f64>,
    pub estimate_kind: FlightEstimateKind,
}

#[derive(Debug, Clone, Default)]
pub struct FlightDataBannerInput {
    pub altitude_ft: Option<f64>,
    pub agl_ft: Option<f64>,
    pub vertical_speed_fpm: Option<f64>,
    pub track_magnetic_deg: Option<f64>,
    pub desired_track_magnetic_deg: Option<f64>,
    pub waypoint_distance_nm: Option<f64>,
    pub final_distance_nm: Option<f64>,
    pub nexrad_age: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlightDataBannerField {
    Altitude,
    AboveGroundLevel,
    GroundSpeed,
    VerticalSpeed,
    Track,
    DesiredTrack,
    WaypointDistance,
    WaypointEte,
    FinalDistance,
    FinalEte,
    FinalFuel,
    FinalEta,
    Clock,
    NexradAge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlightDataBannerCellDefinition {
    field: FlightDataBannerField,
    id: &'static str,
    label: &'static str,
}

pub(crate) const FLIGHT_DATA_AGL_CELL_ID: &str = "agl";

const FLIGHT_DATA_BANNER_CELLS: [FlightDataBannerCellDefinition; 14] = [
    banner_cell(FlightDataBannerField::Altitude, "altitude", "MSL ft"),
    banner_cell(
        FlightDataBannerField::AboveGroundLevel,
        FLIGHT_DATA_AGL_CELL_ID,
        "AGL ft",
    ),
    banner_cell(FlightDataBannerField::GroundSpeed, "ground_speed", "GS kt"),
    banner_cell(
        FlightDataBannerField::VerticalSpeed,
        "vertical_speed",
        "VS fpm",
    ),
    banner_cell(FlightDataBannerField::Track, "track", "TRK °M"),
    banner_cell(FlightDataBannerField::DesiredTrack, "desired_track", "DTK"),
    banner_cell(
        FlightDataBannerField::WaypointDistance,
        "waypoint_distance",
        "WPT nm",
    ),
    banner_cell(FlightDataBannerField::WaypointEte, "waypoint_ete", "ETE"),
    banner_cell(
        FlightDataBannerField::FinalDistance,
        "final_distance",
        "FINAL nm",
    ),
    banner_cell(FlightDataBannerField::FinalEte, "final_ete", "F-ETE"),
    banner_cell(FlightDataBannerField::FinalFuel, "final_fuel", "F-FUEL gal"),
    banner_cell(FlightDataBannerField::FinalEta, "final_eta", "ETA"),
    banner_cell(FlightDataBannerField::Clock, "clock", "TIME"),
    banner_cell(FlightDataBannerField::NexradAge, "nexrad_age", "NEXRAD"),
];

const fn banner_cell(
    field: FlightDataBannerField,
    id: &'static str,
    label: &'static str,
) -> FlightDataBannerCellDefinition {
    FlightDataBannerCellDefinition { field, id, label }
}

impl FlightDataComputer {
    pub fn new(ground_speed_kt: Option<f64>) -> Self {
        Self::with_fuel_flow(ground_speed_kt, None)
    }

    pub fn with_fuel_flow(ground_speed_kt: Option<f64>, fuel_flow_gph: Option<f64>) -> Self {
        Self::with_fuel_flow_and_clock(ground_speed_kt, fuel_flow_gph, None)
    }

    pub fn with_clock(ground_speed_kt: Option<f64>, now_epoch_ms: Option<i64>) -> Self {
        Self::with_fuel_flow_and_clock(ground_speed_kt, None, now_epoch_ms)
    }

    pub fn with_fuel_flow_and_clock(
        ground_speed_kt: Option<f64>,
        fuel_flow_gph: Option<f64>,
        now_epoch_ms: Option<i64>,
    ) -> Self {
        Self::with_fuel_flow_clock_and_time_display(
            ground_speed_kt,
            fuel_flow_gph,
            now_epoch_ms,
            crate::TimeDisplayMode::Utc,
            chrono_tz::UTC,
        )
    }

    pub fn with_fuel_flow_clock_and_time_display(
        ground_speed_kt: Option<f64>,
        fuel_flow_gph: Option<f64>,
        now_epoch_ms: Option<i64>,
        time_display_mode: crate::TimeDisplayMode,
        local_time_zone: Tz,
    ) -> Self {
        Self {
            ground_speed_kt: ground_speed_kt.filter(|speed| *speed > 1.0),
            fuel_flow_gph: fuel_flow_gph.filter(|fuel_flow| *fuel_flow > 0.0),
            now_epoch_ms,
            time_display_mode,
            local_time_zone,
        }
    }

    pub fn banner(&self, input: FlightDataBannerInput) -> FlightDataBannerModel {
        let waypoint_ete = input
            .waypoint_distance_nm
            .and_then(|distance| self.format_ete(distance));
        let final_ete = input
            .final_distance_nm
            .and_then(|distance| self.format_ete(distance));
        let final_fuel = input
            .final_distance_nm
            .and_then(|distance| self.format_fuel(distance));
        let final_eta = input
            .final_distance_nm
            .and_then(|distance| self.format_eta(distance));
        let clock = self.now_epoch_ms.map(|epoch_ms| {
            crate::format_time_of_day(
                epoch_ms,
                self.time_display_mode,
                self.local_time_zone,
                crate::TimeOfDayStyle::Colon,
            )
        });

        FlightDataBannerModel {
            cells: FLIGHT_DATA_BANNER_CELLS
                .iter()
                .map(|definition| {
                    let value = match definition.field {
                        FlightDataBannerField::Altitude => input.altitude_ft.map(format_feet),
                        FlightDataBannerField::AboveGroundLevel => input.agl_ft.map(format_feet),
                        FlightDataBannerField::GroundSpeed => {
                            self.ground_speed_kt.map(format_knots)
                        }
                        FlightDataBannerField::VerticalSpeed => {
                            input.vertical_speed_fpm.map(format_feet_per_minute)
                        }
                        FlightDataBannerField::Track => {
                            input.track_magnetic_deg.map(format_course_degrees)
                        }
                        FlightDataBannerField::DesiredTrack => {
                            input.desired_track_magnetic_deg.map(format_course_degrees)
                        }
                        FlightDataBannerField::WaypointDistance => {
                            input.waypoint_distance_nm.map(format_nm)
                        }
                        FlightDataBannerField::WaypointEte => waypoint_ete.clone(),
                        FlightDataBannerField::FinalDistance => {
                            input.final_distance_nm.map(format_nm)
                        }
                        FlightDataBannerField::FinalEte => final_ete.clone(),
                        FlightDataBannerField::FinalFuel => final_fuel.clone(),
                        FlightDataBannerField::FinalEta => final_eta.clone(),
                        FlightDataBannerField::Clock => {
                            clock.as_ref().map(|display| display.value.clone())
                        }
                        FlightDataBannerField::NexradAge => input.nexrad_age.clone(),
                    };
                    let label = match definition.field {
                        FlightDataBannerField::FinalEta => Some(self.eta_label()),
                        FlightDataBannerField::Clock => Some(self.clock_label()),
                        _ => None,
                    };
                    let mut cell = cell(
                        definition.id,
                        label.as_deref().unwrap_or(definition.label),
                        value,
                    );
                    if matches!(
                        definition.field,
                        FlightDataBannerField::FinalEta | FlightDataBannerField::Clock
                    ) {
                        cell.action_id =
                            Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string());
                    }
                    cell
                })
                .collect(),
        }
    }

    pub fn flight_plan_row_cells(
        &self,
        row_has_data: bool,
        segment_distance_nm: Option<f64>,
        cumulative_distance_nm: Option<f64>,
        eta: Option<String>,
        course_magnetic_deg: Option<f64>,
        distance_tone: FlightDataCellTone,
    ) -> Vec<FlightDataCell> {
        let estimate = FlightTimeFuelEstimate {
            cumulative_ete_seconds: cumulative_distance_nm
                .and_then(|distance| self.ete_seconds(distance))
                .map(|seconds| seconds as f64),
            cumulative_fuel_gal: cumulative_distance_nm.and_then(|distance| {
                self.ground_speed_kt
                    .zip(self.fuel_flow_gph)
                    .map(|(speed_kt, fuel_flow_gph)| distance / speed_kt * fuel_flow_gph)
            }),
            estimate_kind: FlightEstimateKind::Basic,
        };
        self.flight_plan_row_cells_with_estimate(
            row_has_data,
            segment_distance_nm,
            eta,
            course_magnetic_deg,
            distance_tone,
            estimate,
        )
    }

    pub fn flight_plan_row_cells_with_estimate(
        &self,
        row_has_data: bool,
        segment_distance_nm: Option<f64>,
        eta: Option<String>,
        course_magnetic_deg: Option<f64>,
        distance_tone: FlightDataCellTone,
        estimate: FlightTimeFuelEstimate,
    ) -> Vec<FlightDataCell> {
        if !row_has_data {
            return self
                .flight_plan_columns()
                .into_iter()
                .map(|column| {
                    let actionable = column.id == "final_eta";
                    FlightDataCell {
                        id: column.id,
                        label: column.label,
                        value: Some(String::new()),
                        action_id: actionable
                            .then(|| crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string()),
                        tone: FlightDataCellTone::Planned,
                        estimate_kind: FlightEstimateKind::Basic,
                    }
                })
                .collect();
        }

        let ete_seconds = estimate
            .cumulative_ete_seconds
            .map(|seconds| seconds.round().max(0.0) as i64);
        let ete = ete_seconds.map(format_ete_seconds);
        let eta = eta.or_else(|| {
            self.now_epoch_ms
                .zip(ete_seconds)
                .map(|(now, ete)| self.format_eta_epoch(now, ete))
        });
        let fuel = estimate.cumulative_fuel_gal.map(format_fuel_gal);

        vec![
            cell_with_tone(
                "waypoint_distance",
                "DIST nm",
                segment_distance_nm.map(format_nm),
                distance_tone,
            ),
            actionable_cell_with_estimate(
                "final_eta",
                &self.eta_label(),
                eta,
                estimate.estimate_kind,
                crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID,
            ),
            cell_with_estimate("waypoint_ete", "ETE", ete, estimate.estimate_kind),
            cell_with_estimate("fuel", "FUEL gal", fuel, estimate.estimate_kind),
            cell(
                "desired_track",
                "DTK",
                course_magnetic_deg.map(format_course_degrees),
            ),
        ]
    }

    pub fn flight_plan_summary_cells(
        &self,
        total_distance_nm: Option<f64>,
        tone: FlightDataCellTone,
    ) -> Vec<FlightDataCell> {
        let estimate = FlightTimeFuelEstimate {
            cumulative_ete_seconds: total_distance_nm
                .and_then(|distance| self.ete_seconds(distance))
                .map(|seconds| seconds as f64),
            cumulative_fuel_gal: total_distance_nm.and_then(|distance| {
                self.ground_speed_kt
                    .zip(self.fuel_flow_gph)
                    .map(|(speed_kt, fuel_flow_gph)| distance / speed_kt * fuel_flow_gph)
            }),
            estimate_kind: FlightEstimateKind::Basic,
        };
        self.flight_plan_summary_cells_with_estimate(total_distance_nm, tone, estimate)
    }

    pub fn flight_plan_summary_cells_with_estimate(
        &self,
        total_distance_nm: Option<f64>,
        tone: FlightDataCellTone,
        estimate: FlightTimeFuelEstimate,
    ) -> Vec<FlightDataCell> {
        self.flight_plan_row_cells_with_estimate(
            true,
            total_distance_nm,
            None,
            None,
            tone,
            estimate,
        )
    }

    pub fn format_eta_at(&self, distance_nm: f64, now_epoch_ms: i64) -> Option<String> {
        let ete_seconds = self.ete_seconds(distance_nm)?;
        Some(self.format_eta_epoch(now_epoch_ms, ete_seconds))
    }

    fn format_ete(&self, distance_nm: f64) -> Option<String> {
        self.ete_seconds(distance_nm).map(format_ete_seconds)
    }

    fn format_eta(&self, distance_nm: f64) -> Option<String> {
        let now_epoch_ms = self.now_epoch_ms?;
        let ete_seconds = self.ete_seconds(distance_nm)?;
        Some(self.format_eta_epoch(now_epoch_ms, ete_seconds))
    }

    fn ete_seconds(&self, distance_nm: f64) -> Option<i64> {
        self.ground_speed_kt
            .map(|speed_kt| (distance_nm / speed_kt * 3600.0).round().max(0.0) as i64)
    }

    fn format_fuel(&self, distance_nm: f64) -> Option<String> {
        self.ground_speed_kt
            .zip(self.fuel_flow_gph)
            .map(|(speed_kt, fuel_flow_gph)| {
                format_fuel_gal(distance_nm / speed_kt * fuel_flow_gph)
            })
    }

    fn format_eta_epoch(&self, now_epoch_ms: i64, ete_seconds: i64) -> String {
        format_eta(
            now_epoch_ms,
            ete_seconds,
            self.time_display_mode,
            self.local_time_zone,
        )
    }

    pub fn eta_label(&self) -> String {
        format!("ETA {}", self.time_basis_label())
    }

    fn clock_label(&self) -> String {
        format!("TIME {}", self.time_basis_label())
    }

    fn time_basis_label(&self) -> String {
        match self.time_display_mode {
            crate::TimeDisplayMode::Local => {
                crate::time_zone_label(self.now_epoch_ms.unwrap_or_default(), self.local_time_zone)
            }
            crate::TimeDisplayMode::Utc => "Z".to_string(),
        }
    }

    pub fn flight_plan_columns(&self) -> Vec<FlightDataColumn> {
        flight_plan_columns_with_eta_label(&self.eta_label())
    }
}

pub fn flight_plan_columns() -> Vec<FlightDataColumn> {
    flight_plan_columns_with_eta_label("ETA")
}

fn flight_plan_columns_with_eta_label(eta_label: &str) -> Vec<FlightDataColumn> {
    let mut columns = vec![
        column("waypoint_distance", "DIST nm"),
        column("final_eta", eta_label),
        column("waypoint_ete", "ETE"),
        column("fuel", "FUEL gal"),
        column("desired_track", "DTK"),
    ];
    columns[1].action_id = Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string());
    columns
}

pub fn possible_columns() -> Vec<FlightDataColumn> {
    let mut columns = FLIGHT_DATA_BANNER_CELLS
        .iter()
        .map(|definition| column(definition.id, definition.label))
        .collect::<Vec<_>>();
    columns.push(column("fuel", "FUEL gal"));
    columns
}

pub fn altitude_comparison_cells(
    altitude_ft: i32,
    estimate: Option<FlightTimeFuelEstimate>,
    wind: Option<String>,
) -> Vec<FlightDataCell> {
    let estimate_kind = estimate
        .map(|estimate| estimate.estimate_kind)
        .unwrap_or(FlightEstimateKind::Basic);
    vec![
        cell(
            "cruise_altitude",
            "ALT ft",
            Some(format_feet(altitude_ft as f64)),
        ),
        cell_with_estimate(
            "waypoint_ete",
            "ETE",
            estimate
                .and_then(|estimate| estimate.cumulative_ete_seconds)
                .map(|seconds| format_ete_seconds(seconds.round().max(0.0) as i64)),
            estimate_kind,
        ),
        cell_with_estimate(
            "fuel",
            "FUEL gal",
            estimate
                .and_then(|estimate| estimate.cumulative_fuel_gal)
                .map(format_fuel_gal),
            estimate_kind,
        ),
        cell_with_estimate("wind", "WIND kt", wind, estimate_kind),
    ]
}

pub fn altitude_comparison_columns() -> Vec<FlightDataColumn> {
    vec![
        column("cruise_altitude", "ALT ft"),
        column("waypoint_ete", "ETE"),
        column("fuel", "FUEL gal"),
        column("wind", "WIND kt"),
    ]
}

pub fn is_flight_data_banner_cell_id(id: &str) -> bool {
    FLIGHT_DATA_BANNER_CELLS
        .iter()
        .any(|definition| definition.id == id)
}

pub fn cell(id: &str, label: &str, value: Option<String>) -> FlightDataCell {
    cell_with_tone(id, label, value, FlightDataCellTone::Planned)
}

pub fn cell_with_tone(
    id: &str,
    label: &str,
    value: Option<String>,
    tone: FlightDataCellTone,
) -> FlightDataCell {
    FlightDataCell {
        id: id.to_string(),
        label: label.to_string(),
        value,
        action_id: None,
        tone,
        estimate_kind: FlightEstimateKind::Basic,
    }
}

pub fn cell_with_estimate(
    id: &str,
    label: &str,
    value: Option<String>,
    estimate_kind: FlightEstimateKind,
) -> FlightDataCell {
    FlightDataCell {
        id: id.to_string(),
        label: label.to_string(),
        value,
        action_id: None,
        tone: FlightDataCellTone::Planned,
        estimate_kind,
    }
}

fn actionable_cell_with_estimate(
    id: &str,
    label: &str,
    value: Option<String>,
    estimate_kind: FlightEstimateKind,
    action_id: &str,
) -> FlightDataCell {
    FlightDataCell {
        id: id.to_string(),
        label: label.to_string(),
        value,
        action_id: Some(action_id.to_string()),
        tone: FlightDataCellTone::Planned,
        estimate_kind,
    }
}

fn column(id: &str, label: &str) -> FlightDataColumn {
    FlightDataColumn {
        id: id.to_string(),
        label: label.to_string(),
        action_id: None,
    }
}

pub fn format_feet(value: f64) -> String {
    format!("{value:.0}")
}

pub fn format_nm(value: f64) -> String {
    if value < 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    }
}

pub fn format_knots(value: f64) -> String {
    format!("{value:.0}")
}

pub fn format_feet_per_minute(value: f64) -> String {
    format!("{value:.0}")
}

pub fn format_fuel_gal(value: f64) -> String {
    format!("{value:.1}")
}

pub fn format_course_degrees(course_deg: f64) -> String {
    let rounded = course_deg.round().rem_euclid(360.0) as u16;
    if rounded == 0 {
        "360".to_string()
    } else {
        format!("{rounded:03}")
    }
}

fn format_ete_seconds(total_seconds: i64) -> String {
    let total_seconds = total_seconds.max(0) as u32;
    if total_seconds >= 3600 {
        let total_minutes = (total_seconds + 30) / 60;
        let hours = total_minutes / 60;
        let minutes = total_minutes % 60;
        format!("{hours:02}:{minutes:02}⌛")
    } else {
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{minutes:02}:{seconds:02}⏱️")
    }
}

fn format_eta(
    now_epoch_ms: i64,
    ete_seconds: i64,
    mode: crate::TimeDisplayMode,
    local_time_zone: Tz,
) -> String {
    let eta_epoch_ms = now_epoch_ms
        .saturating_add(ete_seconds.saturating_mul(1000))
        .saturating_add(30_000);
    crate::format_time_of_day(
        eta_epoch_ms,
        mode,
        local_time_zone,
        crate::TimeOfDayStyle::Colon,
    )
    .value
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    #[test]
    fn banner_and_flight_plan_rows_share_ete_formatting() {
        let computer = FlightDataComputer::new(Some(120.0));
        let banner = computer.banner(FlightDataBannerInput {
            final_distance_nm: Some(30.0),
            ..FlightDataBannerInput::default()
        });
        let final_ete = banner
            .cells
            .iter()
            .find(|cell| cell.id == "final_ete")
            .and_then(|cell| cell.value.as_deref());

        let row_cells = computer.flight_plan_row_cells(
            true,
            Some(12.0),
            Some(30.0),
            None,
            None,
            FlightDataCellTone::Planned,
        );
        let row_ete = row_cells
            .iter()
            .find(|cell| cell.id == "waypoint_ete")
            .and_then(|cell| cell.value.as_deref());

        assert_eq!(final_ete, Some("15:00⏱️"));
        assert_eq!(row_ete, final_ete);
    }

    #[test]
    fn ete_formatting_suffix_disambiguates_duration_mode() {
        assert_eq!(format_ete_seconds(12 * 60 + 34), "12:34⏱️");
        assert_eq!(format_ete_seconds(12 * 3600 + 34 * 60), "12:34⌛");
    }

    #[test]
    fn flight_plan_distance_column_is_named_dist() {
        assert_eq!(flight_plan_columns()[0].label, "DIST nm");
    }

    #[test]
    fn desired_track_is_available_as_dtk_grid_cell() {
        let desired_track_column = possible_columns()
            .into_iter()
            .find(|column| column.id == "desired_track")
            .expect("desired track column");
        assert_eq!(desired_track_column.label, "DTK");

        let banner = FlightDataComputer::default().banner(FlightDataBannerInput {
            desired_track_magnetic_deg: Some(271.4),
            ..FlightDataBannerInput::default()
        });
        let desired_track_cell = banner
            .cells
            .iter()
            .find(|cell| cell.id == "desired_track")
            .expect("desired track cell");

        assert_eq!(desired_track_cell.label, "DTK");
        assert_eq!(desired_track_cell.value.as_deref(), Some("271"));
    }

    #[test]
    fn altitude_comparison_includes_core_formatted_wind_column() {
        assert_eq!(
            altitude_comparison_columns()
                .iter()
                .map(|column| (column.id.as_str(), column.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("cruise_altitude", "ALT ft"),
                ("waypoint_ete", "ETE"),
                ("fuel", "FUEL gal"),
                ("wind", "WIND kt"),
            ]
        );
        let cells = altitude_comparison_cells(12_000, None, Some("→ +8".to_string()));
        assert_eq!(
            cells
                .iter()
                .find(|cell| cell.id == "wind")
                .and_then(|cell| cell.value.as_deref()),
            Some("→ +8")
        );
    }

    #[test]
    fn whole_nm_distances_round_to_nearest_mile() {
        assert_eq!(format_nm(180.8), "181");
    }

    #[test]
    fn flight_plan_columns_are_banner_cell_subset() {
        let possible_columns = possible_columns();
        for column in flight_plan_columns() {
            assert!(
                possible_columns
                    .iter()
                    .any(|candidate| candidate.id == column.id),
                "flight-plan column {} is not a flight-data cell",
                column.id
            );
        }
    }

    #[test]
    fn computes_fuel_when_fuel_flow_is_available() {
        let computer = FlightDataComputer::with_fuel_flow(Some(120.0), Some(10.0));
        let row_cells = computer.flight_plan_row_cells(
            true,
            Some(12.0),
            Some(30.0),
            None,
            None,
            FlightDataCellTone::Planned,
        );
        let row_fuel = row_cells
            .iter()
            .find(|cell| cell.id == "fuel")
            .and_then(|cell| cell.value.as_deref());
        let banner = computer.banner(FlightDataBannerInput {
            final_distance_nm: Some(30.0),
            ..FlightDataBannerInput::default()
        });
        let final_fuel = banner
            .cells
            .iter()
            .find(|cell| cell.id == "final_fuel")
            .and_then(|cell| cell.value.as_deref());

        assert_eq!(row_fuel, Some("2.5"));
        assert_eq!(final_fuel, Some("2.5"));
    }

    #[test]
    fn row_eta_uses_cumulative_distance_from_now() {
        let computer = FlightDataComputer::new(Some(120.0));
        let row_cells = computer.flight_plan_row_cells(
            true,
            Some(12.0),
            Some(30.0),
            computer.format_eta_at(30.0, 12 * 60 * 60 * 1000),
            None,
            FlightDataCellTone::Planned,
        );

        assert_eq!(
            row_cells
                .iter()
                .find(|cell| cell.id == "final_eta")
                .and_then(|cell| cell.value.as_deref()),
            Some("12:15")
        );
    }

    #[test]
    fn eta_uses_device_zone_and_exposes_the_shared_time_action() {
        let now = DateTime::parse_from_rfc3339("2026-08-13T02:00:00Z")
            .expect("instant")
            .timestamp_millis();
        let computer = FlightDataComputer::with_fuel_flow_clock_and_time_display(
            Some(120.0),
            None,
            Some(now),
            crate::TimeDisplayMode::Local,
            chrono_tz::America::Los_Angeles,
        );
        let row_cells = computer.flight_plan_row_cells(
            true,
            Some(12.0),
            Some(30.0),
            None,
            None,
            FlightDataCellTone::Planned,
        );
        let eta = row_cells
            .iter()
            .find(|cell| cell.id == "final_eta")
            .expect("ETA cell");

        assert_eq!(eta.label, "ETA PDT");
        assert_eq!(eta.value.as_deref(), Some("19:15"));
        assert_eq!(
            eta.action_id.as_deref(),
            Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID),
        );
        assert_eq!(
            computer.flight_plan_columns()[1].action_id.as_deref(),
            Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID),
        );
    }

    #[test]
    fn clock_uses_the_shared_time_mode_and_action() {
        let now = DateTime::parse_from_rfc3339("2026-08-13T02:34:00Z")
            .expect("instant")
            .timestamp_millis();
        let local = FlightDataComputer::with_fuel_flow_clock_and_time_display(
            None,
            None,
            Some(now),
            crate::TimeDisplayMode::Local,
            chrono_tz::America::Los_Angeles,
        )
        .banner(FlightDataBannerInput::default());
        let local_clock = local
            .cells
            .iter()
            .find(|cell| cell.id == "clock")
            .expect("clock cell");

        assert_eq!(local_clock.label, "TIME PDT");
        assert_eq!(local_clock.value.as_deref(), Some("19:34"));
        assert_eq!(
            local_clock.action_id.as_deref(),
            Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID),
        );

        let zulu = FlightDataComputer::with_fuel_flow_clock_and_time_display(
            None,
            None,
            Some(now),
            crate::TimeDisplayMode::Utc,
            chrono_tz::America::Los_Angeles,
        )
        .banner(FlightDataBannerInput::default());
        let zulu_clock = zulu
            .cells
            .iter()
            .find(|cell| cell.id == "clock")
            .expect("clock cell");

        assert_eq!(zulu_clock.label, "TIME Z");
        assert_eq!(zulu_clock.value.as_deref(), Some("02:34"));
    }

    #[test]
    fn banner_reports_vertical_speed_when_supplied() {
        let computer = FlightDataComputer::default();
        let banner = computer.banner(FlightDataBannerInput {
            vertical_speed_fpm: Some(450.0),
            ..FlightDataBannerInput::default()
        });

        assert_eq!(
            banner
                .cells
                .iter()
                .find(|cell| cell.id == "vertical_speed")
                .and_then(|cell| cell.value.as_deref()),
            Some("450")
        );
    }

    #[test]
    fn banner_uses_one_ordered_definition_set_for_all_fourteen_cells() {
        let banner = FlightDataComputer::default().banner(FlightDataBannerInput {
            nexrad_age: Some("4m".to_string()),
            ..FlightDataBannerInput::default()
        });

        assert_eq!(banner.cells.len(), 14);
        assert_eq!(
            banner.cells.last().map(|cell| cell.id.as_str()),
            Some("nexrad_age")
        );
        assert_eq!(
            banner.cells.last().and_then(|cell| cell.value.as_deref()),
            Some("4m")
        );
        assert!(banner
            .cells
            .iter()
            .all(|cell| is_flight_data_banner_cell_id(&cell.id)));
    }

    #[test]
    fn banner_reports_above_ground_level_when_supplied() {
        let banner = FlightDataComputer::default().banner(FlightDataBannerInput {
            altitude_ft: Some(1_512.0),
            agl_ft: Some(1_000.0),
            ..FlightDataBannerInput::default()
        });

        assert_eq!(
            banner
                .cells
                .iter()
                .find(|cell| cell.id == FLIGHT_DATA_AGL_CELL_ID)
                .and_then(|cell| cell.value.as_deref()),
            Some("1000")
        );
    }

    #[test]
    fn computes_eta_when_clock_and_ground_speed_are_available() {
        let noon_utc = 1_781_438_400_000;
        let computer = FlightDataComputer::with_clock(Some(120.0), Some(noon_utc));
        let banner = computer.banner(FlightDataBannerInput {
            final_distance_nm: Some(30.0),
            ..FlightDataBannerInput::default()
        });
        let final_eta = banner
            .cells
            .iter()
            .find(|cell| cell.id == "final_eta")
            .and_then(|cell| cell.value.as_deref());
        let row_cells = computer.flight_plan_row_cells(
            true,
            Some(30.0),
            Some(30.0),
            None,
            None,
            FlightDataCellTone::Planned,
        );
        let row_eta = row_cells
            .iter()
            .find(|cell| cell.id == "final_eta")
            .and_then(|cell| cell.value.as_deref());
        let summary_cells =
            computer.flight_plan_summary_cells(Some(30.0), FlightDataCellTone::Planned);
        let summary_eta = summary_cells
            .iter()
            .find(|cell| cell.id == "final_eta")
            .and_then(|cell| cell.value.as_deref());

        assert_eq!(final_eta, Some("12:15"));
        assert_eq!(row_eta, final_eta);
        assert_eq!(summary_eta, final_eta);
    }

    #[test]
    fn modeled_prediction_uses_shared_time_fuel_formatting_and_provenance() {
        let computer = FlightDataComputer::with_clock(None, Some(12 * 60 * 60 * 1000));
        let cells = computer.flight_plan_row_cells_with_estimate(
            true,
            Some(12.0),
            None,
            Some(270.0),
            FlightDataCellTone::Active,
            FlightTimeFuelEstimate {
                cumulative_ete_seconds: Some(12.0 * 60.0 + 34.0),
                cumulative_fuel_gal: Some(2.45),
                estimate_kind: FlightEstimateKind::Modeled,
            },
        );

        let ete = cells.iter().find(|cell| cell.id == "waypoint_ete").unwrap();
        let fuel = cells.iter().find(|cell| cell.id == "fuel").unwrap();
        let distance = cells
            .iter()
            .find(|cell| cell.id == "waypoint_distance")
            .unwrap();
        assert_eq!(ete.value.as_deref(), Some("12:34⏱️"));
        assert_eq!(fuel.value.as_deref(), Some("2.5"));
        assert_eq!(ete.estimate_kind, FlightEstimateKind::Modeled);
        assert_eq!(fuel.estimate_kind, FlightEstimateKind::Modeled);
        assert_eq!(distance.estimate_kind, FlightEstimateKind::Basic);
        assert_eq!(distance.tone, FlightDataCellTone::Active);
    }
}
