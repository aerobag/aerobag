// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    AircraftPerformanceProfile, CruisePerformancePoint, PerformanceAirspeedBasis,
    VerticalPerformancePoint,
};
use product_contracts::{
    AircraftDefinition, AircraftPerformanceAirspeedBasis, ClimbPerformanceModel,
    CruisePerformanceModel, DescentPerformanceModel,
};

pub fn performance_profile_from_definition(
    definition_hash: &str,
    definition: &AircraftDefinition,
    profile_id: &str,
) -> Result<AircraftPerformanceProfile, String> {
    definition.validate()?;
    let actual_hash = definition.content_hash()?;
    if actual_hash != definition_hash {
        return Err(format!(
            "aircraft definition hash mismatch: expected {definition_hash}, got {actual_hash}"
        ));
    }
    let source = definition.profile(profile_id).ok_or_else(|| {
        format!(
            "aircraft definition {} has no profile {profile_id}",
            definition.lineage_id
        )
    })?;
    let cruise = match &source.cruise {
        CruisePerformanceModel::PressureAltitudeTable { points } => points
            .iter()
            .map(|point| CruisePerformancePoint {
                pressure_altitude_ft: point.pressure_altitude_ft,
                true_airspeed_kt: point.true_airspeed_kt,
                fuel_flow_gph: point.fuel_flow_gph,
            })
            .collect::<Vec<_>>(),
    };
    let altitude_bounds = cruise
        .first()
        .zip(cruise.last())
        .map(|(first, last)| (first.pressure_altitude_ft, last.pressure_altitude_ft))
        .ok_or_else(|| format!("aircraft profile {profile_id} has no cruise performance"))?;
    let climb = match &source.climb {
        ClimbPerformanceModel::ConstantIasRate {
            ias_kt,
            fuel_flow_gph,
            vertical_speed_fpm,
        } => [altitude_bounds.0, altitude_bounds.1]
            .into_iter()
            .map(|pressure_altitude_ft| VerticalPerformancePoint {
                pressure_altitude_ft,
                airspeed_basis: PerformanceAirspeedBasis::Indicated,
                airspeed_kt: *ias_kt,
                fuel_flow_gph: *fuel_flow_gph,
                vertical_speed_fpm: *vertical_speed_fpm,
            })
            .collect(),
        ClimbPerformanceModel::PressureAltitudeTable { points } => {
            points.iter().map(convert_vertical_point).collect()
        }
    };
    let descent = match &source.descent {
        DescentPerformanceModel::CruiseOffset {
            tas_offset_kt,
            vertical_speed_fpm,
        } => cruise
            .iter()
            .map(|point| VerticalPerformancePoint {
                pressure_altitude_ft: point.pressure_altitude_ft,
                airspeed_basis: PerformanceAirspeedBasis::True,
                airspeed_kt: point.true_airspeed_kt + tas_offset_kt,
                fuel_flow_gph: point.fuel_flow_gph,
                vertical_speed_fpm: *vertical_speed_fpm,
            })
            .collect(),
        DescentPerformanceModel::PressureAltitudeTable { points } => {
            points.iter().map(convert_vertical_point).collect()
        }
    };

    Ok(AircraftPerformanceProfile {
        schema_version: definition.schema_version,
        aircraft_model_id: definition.lineage_id.clone(),
        profile_id: source.id.clone(),
        profile_version: definition_hash.to_string(),
        aircraft_label: definition.label.clone(),
        profile_label: source.label.clone(),
        source: source.source.clone(),
        reference_weight_lb: source.reference_weight_lb,
        cruise,
        climb,
        descent,
    })
}

fn convert_vertical_point(
    point: &product_contracts::AircraftVerticalPerformancePoint,
) -> VerticalPerformancePoint {
    VerticalPerformancePoint {
        pressure_altitude_ft: point.pressure_altitude_ft,
        airspeed_basis: match point.airspeed_basis {
            AircraftPerformanceAirspeedBasis::Indicated => PerformanceAirspeedBasis::Indicated,
            AircraftPerformanceAirspeedBasis::True => PerformanceAirspeedBasis::True,
        },
        airspeed_kt: point.airspeed_kt,
        fuel_flow_gph: point.fuel_flow_gph,
        vertical_speed_fpm: point.vertical_speed_fpm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tagged_models_convert_to_runtime_profile() {
        let definition: AircraftDefinition = serde_json::from_str(include_str!(
            "../../../../../product/preprocessor/preprocessor-cli/resources/aircraft/piper-pa46-310p.json"
        ))
        .expect("bundled PA46 definition");
        let hash = definition.content_hash().expect("definition hash");
        let profile = performance_profile_from_definition(&hash, &definition, "economy-65")
            .expect("runtime profile");

        assert_eq!(profile.aircraft_model_id, "piper-pa46-310p-tsio-520-be");
        assert_eq!(profile.reference_weight_lb, Some(3_740.0));
        assert_eq!(profile.cruise.len(), 5);
        assert_eq!(profile.climb.len(), 2);
        assert_eq!(profile.descent.len(), 5);
        assert!(profile.climb.iter().all(|point| {
            point.airspeed_basis == PerformanceAirspeedBasis::Indicated
                && point.airspeed_kt == 130.0
                && point.fuel_flow_gph == 36.0
                && point.vertical_speed_fpm == 1_100.0
        }));
        for (cruise, descent) in profile.cruise.iter().zip(&profile.descent) {
            assert_eq!(descent.airspeed_kt, cruise.true_airspeed_kt + 8.0);
            assert_eq!(descent.fuel_flow_gph, cruise.fuel_flow_gph);
            assert_eq!(descent.vertical_speed_fpm, -500.0);
        }
    }
}
