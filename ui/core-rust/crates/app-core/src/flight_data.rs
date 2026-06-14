use serde::{Deserialize, Serialize};

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightDataCell {
    pub id: String,
    pub label: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightDataColumn {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlightDataBannerModel {
    pub cells: Vec<FlightDataCell>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FlightDataComputer {
    ground_speed_kt: Option<f64>,
    fuel_flow_gph: Option<f64>,
    now_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FlightDataBannerInput {
    pub altitude_ft: Option<f64>,
    pub vertical_speed_fpm: Option<f64>,
    pub track_magnetic_deg: Option<f64>,
    pub desired_track_magnetic_deg: Option<f64>,
    pub waypoint_distance_nm: Option<f64>,
    pub final_distance_nm: Option<f64>,
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
        Self {
            ground_speed_kt: ground_speed_kt.filter(|speed| *speed > 1.0),
            fuel_flow_gph: fuel_flow_gph.filter(|fuel_flow| *fuel_flow > 0.0),
            now_epoch_ms,
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

        FlightDataBannerModel {
            cells: vec![
                cell("altitude", "ALT ft", input.altitude_ft.map(format_feet)),
                cell(
                    "ground_speed",
                    "GS kt",
                    self.ground_speed_kt.map(format_knots),
                ),
                cell(
                    "vertical_speed",
                    "VS fpm",
                    input.vertical_speed_fpm.map(format_feet_per_minute),
                ),
                cell(
                    "track",
                    "TRK °M",
                    input.track_magnetic_deg.map(format_course_degrees),
                ),
                cell(
                    "desired_track",
                    "DTK °M",
                    input.desired_track_magnetic_deg.map(format_course_degrees),
                ),
                cell(
                    "waypoint_distance",
                    "WPT nm",
                    input.waypoint_distance_nm.map(format_nm),
                ),
                cell("waypoint_ete", "ETE", waypoint_ete),
                cell(
                    "final_distance",
                    "FINAL nm",
                    input.final_distance_nm.map(format_nm),
                ),
                cell("final_ete", "F-ETE", final_ete),
                cell("final_fuel", "F-FUEL gal", final_fuel),
                cell("final_eta", "ETA", final_eta),
            ],
        }
    }

    pub fn flight_plan_row_cells(
        &self,
        row_has_data: bool,
        distance_nm: Option<f64>,
        eta_distance_nm: Option<f64>,
        course_magnetic_deg: Option<f64>,
    ) -> Vec<FlightDataCell> {
        if !row_has_data {
            return flight_plan_columns()
                .into_iter()
                .map(|column| FlightDataCell {
                    id: column.id,
                    label: column.label,
                    value: Some(String::new()),
                })
                .collect();
        }

        let ete = distance_nm.and_then(|distance| self.format_ete(distance));
        let eta = eta_distance_nm.and_then(|distance| self.format_eta(distance));
        let fuel = distance_nm.and_then(|distance| self.format_fuel(distance));

        vec![
            cell("waypoint_distance", "WPT nm", distance_nm.map(format_nm)),
            cell("final_eta", "ETA", eta),
            cell("waypoint_ete", "ETE", ete),
            cell("fuel", "FUEL gal", fuel),
            cell(
                "desired_track",
                "DTK °M",
                course_magnetic_deg.map(format_course_degrees),
            ),
        ]
    }

    fn format_ete(&self, distance_nm: f64) -> Option<String> {
        self.ete_seconds(distance_nm).map(format_ete_seconds)
    }

    fn format_eta(&self, distance_nm: f64) -> Option<String> {
        let now_epoch_ms = self.now_epoch_ms?;
        let ete_seconds = self.ete_seconds(distance_nm)?;
        Some(format_eta(now_epoch_ms, ete_seconds))
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
}

pub fn flight_plan_columns() -> Vec<FlightDataColumn> {
    vec![
        column("waypoint_distance", "WPT nm"),
        column("final_eta", "ETA"),
        column("waypoint_ete", "ETE"),
        column("fuel", "FUEL gal"),
        column("desired_track", "DTK °M"),
    ]
}

pub fn possible_columns() -> Vec<FlightDataColumn> {
    vec![
        column("altitude", "ALT ft"),
        column("ground_speed", "GS kt"),
        column("vertical_speed", "VS fpm"),
        column("track", "TRK °M"),
        column("desired_track", "DTK °M"),
        column("waypoint_distance", "WPT nm"),
        column("waypoint_ete", "ETE"),
        column("final_distance", "FINAL nm"),
        column("final_ete", "F-ETE"),
        column("final_fuel", "F-FUEL gal"),
        column("final_eta", "ETA"),
        column("fuel", "FUEL gal"),
    ]
}

pub fn cell(id: &str, label: &str, value: Option<String>) -> FlightDataCell {
    FlightDataCell {
        id: id.to_string(),
        label: label.to_string(),
        value,
    }
}

fn column(id: &str, label: &str) -> FlightDataColumn {
    FlightDataColumn {
        id: id.to_string(),
        label: label.to_string(),
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
        format!("{hours:02}:{minutes:02}")
    } else {
        let minutes = total_seconds / 60;
        let seconds = total_seconds % 60;
        format!("{minutes:02}:{seconds:02}")
    }
}

fn format_eta(now_epoch_ms: i64, ete_seconds: i64) -> String {
    let eta_epoch_ms = now_epoch_ms
        .saturating_add(ete_seconds.saturating_mul(1000))
        .saturating_add(30_000);
    DateTime::<Utc>::from_timestamp_millis(eta_epoch_ms)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
        .format("%H:%M")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let row_cells = computer.flight_plan_row_cells(true, Some(30.0), None, None);
        let row_ete = row_cells
            .iter()
            .find(|cell| cell.id == "waypoint_ete")
            .and_then(|cell| cell.value.as_deref());

        assert_eq!(final_ete, Some("15:00"));
        assert_eq!(row_ete, final_ete);
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
        let row_cells = computer.flight_plan_row_cells(true, Some(30.0), None, None);
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
        let row_cells = computer.flight_plan_row_cells(true, Some(30.0), Some(30.0), None);
        let row_eta = row_cells
            .iter()
            .find(|cell| cell.id == "final_eta")
            .and_then(|cell| cell.value.as_deref());

        assert_eq!(final_eta, Some("12:15"));
        assert_eq!(row_eta, final_eta);
    }
}
