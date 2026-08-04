// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::{
    AircraftPerformanceProfile, CruisePerformancePoint, PerformanceAirspeedBasis,
    VerticalPerformancePoint,
};

pub const PA46_310P_AIRCRAFT_LABEL: &str = "PA46-310P MALIBU";
pub const PA46_310P_AIRCRAFT_MODEL_ID: &str = "piper-pa46-310p-tsio-520-be";
pub const PA46_310P_PERFORMANCE_SOURCE: &str =
    "N9124Y Power Settings v3; PA46-310P POH power table and digitized cruise-speed chart";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pa46CruiseConfiguration {
    HighSpeed75,
    Economy65,
    LongRange55,
}

impl Pa46CruiseConfiguration {
    pub const fn profile_id(self) -> &'static str {
        match self {
            Self::HighSpeed75 => "high-speed-75",
            Self::Economy65 => "economy-65",
            Self::LongRange55 => "long-range-55",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::HighSpeed75 => "75% HIGH SPEED",
            Self::Economy65 => "65% ECONOMY",
            Self::LongRange55 => "55% LONG RANGE",
        }
    }

    pub const fn cruise_points(self) -> &'static [CruisePerformancePoint] {
        match self {
            Self::HighSpeed75 => &PA46_HIGH_SPEED_75_CRUISE,
            Self::Economy65 => &PA46_ECONOMY_65_CRUISE,
            Self::LongRange55 => &PA46_LONG_RANGE_55_CRUISE,
        }
    }

    pub const fn descent_points(self) -> &'static [VerticalPerformancePoint] {
        match self {
            Self::HighSpeed75 => &PA46_HIGH_SPEED_75_DESCENT,
            Self::Economy65 => &PA46_ECONOMY_65_DESCENT,
            Self::LongRange55 => &PA46_LONG_RANGE_55_DESCENT,
        }
    }
}

// Five samples retain the source model's two-thousand-foot curve to within
// 1 KT over its published 0..24,000-foot range.
const PA46_HIGH_SPEED_75_CRUISE: [CruisePerformancePoint; 5] = [
    cruise_point(0.0, 161.0, 16.0),
    cruise_point(6_000.0, 175.0, 16.0),
    cruise_point(12_000.0, 188.0, 16.0),
    cruise_point(18_000.0, 201.0, 16.0),
    cruise_point(24_000.0, 213.0, 16.0),
];

const PA46_ECONOMY_65_CRUISE: [CruisePerformancePoint; 5] = [
    cruise_point(0.0, 148.0, 14.0),
    cruise_point(6_000.0, 162.0, 14.0),
    cruise_point(12_000.0, 177.0, 14.0),
    cruise_point(18_000.0, 190.0, 14.0),
    cruise_point(24_000.0, 202.0, 14.0),
];

const PA46_LONG_RANGE_55_CRUISE: [CruisePerformancePoint; 5] = [
    cruise_point(0.0, 134.0, 12.0),
    cruise_point(6_000.0, 149.0, 12.0),
    cruise_point(12_000.0, 164.0, 12.0),
    cruise_point(18_000.0, 179.0, 12.0),
    cruise_point(24_000.0, 193.0, 12.0),
];

const PA46_CLIMB: [VerticalPerformancePoint; 2] = [climb_point(0.0), climb_point(24_000.0)];

const PA46_HIGH_SPEED_75_DESCENT: [VerticalPerformancePoint; 5] = [
    descent_point(0.0, 169.0, 16.0),
    descent_point(6_000.0, 183.0, 16.0),
    descent_point(12_000.0, 196.0, 16.0),
    descent_point(18_000.0, 209.0, 16.0),
    descent_point(24_000.0, 221.0, 16.0),
];

const PA46_ECONOMY_65_DESCENT: [VerticalPerformancePoint; 5] = [
    descent_point(0.0, 156.0, 14.0),
    descent_point(6_000.0, 170.0, 14.0),
    descent_point(12_000.0, 185.0, 14.0),
    descent_point(18_000.0, 198.0, 14.0),
    descent_point(24_000.0, 210.0, 14.0),
];

const PA46_LONG_RANGE_55_DESCENT: [VerticalPerformancePoint; 5] = [
    descent_point(0.0, 142.0, 12.0),
    descent_point(6_000.0, 157.0, 12.0),
    descent_point(12_000.0, 172.0, 12.0),
    descent_point(18_000.0, 187.0, 12.0),
    descent_point(24_000.0, 201.0, 12.0),
];

pub const fn pa46_310p_climb_points() -> &'static [VerticalPerformancePoint] {
    &PA46_CLIMB
}

pub fn pa46_310p_profile(configuration: Pa46CruiseConfiguration) -> AircraftPerformanceProfile {
    AircraftPerformanceProfile {
        schema_version: 1,
        aircraft_model_id: PA46_310P_AIRCRAFT_MODEL_ID.to_string(),
        profile_id: configuration.profile_id().to_string(),
        profile_version: "n9124y-v3-rough-vertical-v1".to_string(),
        aircraft_label: PA46_310P_AIRCRAFT_LABEL.to_string(),
        profile_label: configuration.label().to_string(),
        source: format!(
            "{PA46_310P_PERFORMANCE_SOURCE}; rough climb assumption: 130 KIAS, 36 GPH, 1100 FPM; rough descent assumption: selected cruise TAS + 8 KT, selected cruise GPH, 500 FPM"
        ),
        reference_weight_lb: Some(3_740.0),
        cruise: configuration.cruise_points().to_vec(),
        climb: PA46_CLIMB.to_vec(),
        descent: configuration.descent_points().to_vec(),
    }
}

const fn cruise_point(
    pressure_altitude_ft: f64,
    true_airspeed_kt: f64,
    fuel_flow_gph: f64,
) -> CruisePerformancePoint {
    CruisePerformancePoint {
        pressure_altitude_ft,
        true_airspeed_kt,
        fuel_flow_gph,
    }
}

const fn climb_point(pressure_altitude_ft: f64) -> VerticalPerformancePoint {
    VerticalPerformancePoint {
        pressure_altitude_ft,
        airspeed_basis: PerformanceAirspeedBasis::Indicated,
        airspeed_kt: 130.0,
        fuel_flow_gph: 36.0,
        vertical_speed_fpm: 1_100.0,
    }
}

const fn descent_point(
    pressure_altitude_ft: f64,
    true_airspeed_kt: f64,
    fuel_flow_gph: f64,
) -> VerticalPerformancePoint {
    VerticalPerformancePoint {
        pressure_altitude_ft,
        airspeed_basis: PerformanceAirspeedBasis::True,
        airspeed_kt: true_airspeed_kt,
        fuel_flow_gph,
        vertical_speed_fpm: -500.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_cruise_samples_preserve_source_anchors() {
        let expected = [
            (Pa46CruiseConfiguration::HighSpeed75, 16.0, 161.0, 213.0),
            (Pa46CruiseConfiguration::Economy65, 14.0, 148.0, 202.0),
            (Pa46CruiseConfiguration::LongRange55, 12.0, 134.0, 193.0),
        ];

        for (configuration, fuel_flow_gph, sea_level_tas, top_tas) in expected {
            let points = configuration.cruise_points();
            assert_eq!(points.len(), 5);
            assert_eq!(points[0].pressure_altitude_ft, 0.0);
            assert_eq!(points[0].true_airspeed_kt, sea_level_tas);
            assert_eq!(points[4].pressure_altitude_ft, 24_000.0);
            assert_eq!(points[4].true_airspeed_kt, top_tas);
            assert!(points
                .iter()
                .all(|point| point.fuel_flow_gph == fuel_flow_gph));
        }
    }

    #[test]
    fn climb_schedule_uses_the_explicit_rough_model() {
        let points = pa46_310p_climb_points();
        assert_eq!(points.len(), 2);
        assert!(points.iter().all(|point| {
            point.airspeed_basis == PerformanceAirspeedBasis::Indicated
                && point.airspeed_kt == 130.0
                && point.fuel_flow_gph == 36.0
                && point.vertical_speed_fpm == 1_100.0
        }));
    }

    #[test]
    fn descent_schedule_is_cruise_plus_eight_kt_at_minus_five_hundred_fpm() {
        for configuration in [
            Pa46CruiseConfiguration::HighSpeed75,
            Pa46CruiseConfiguration::Economy65,
            Pa46CruiseConfiguration::LongRange55,
        ] {
            for (cruise, descent) in configuration
                .cruise_points()
                .iter()
                .zip(configuration.descent_points())
            {
                assert_eq!(descent.airspeed_basis, PerformanceAirspeedBasis::True);
                assert_eq!(descent.pressure_altitude_ft, cruise.pressure_altitude_ft);
                assert_eq!(descent.airspeed_kt, cruise.true_airspeed_kt + 8.0);
                assert_eq!(descent.fuel_flow_gph, cruise.fuel_flow_gph);
                assert_eq!(descent.vertical_speed_fpm, -500.0);
            }
        }
    }

    #[test]
    fn complete_profile_records_sourced_and_assumed_components() {
        let profile = pa46_310p_profile(Pa46CruiseConfiguration::Economy65);

        assert_eq!(profile.aircraft_model_id, PA46_310P_AIRCRAFT_MODEL_ID);
        assert_eq!(profile.reference_weight_lb, Some(3_740.0));
        assert_eq!(profile.cruise.len(), 5);
        assert_eq!(profile.climb.len(), 2);
        assert_eq!(profile.descent.len(), 5);
        assert!(profile.source.contains("rough climb assumption"));
        assert!(profile.source.contains("rough descent assumption"));
    }
}
