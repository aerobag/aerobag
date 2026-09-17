// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Straight ground-track glides, terminated at the first terrain intersection.
//! The caller supplies one immutable aircraft/weather/terrain snapshot. Neither
//! map scale nor viewport enters the calculation.

use crate::{AtmosphereModel, LatLon};
use serde::{Deserialize, Serialize};

const FEET_PER_NM: f64 = 6_076.115_485_56;
const EARTH_RADIUS_NM: f64 = 3_440.065;
const STEP_NM: f64 = 0.025;
const MAX_DISTANCE_NM: f64 = 200.0;
const BEARING_COUNT: usize = 180;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlidePerformance {
    pub best_glide_ias_kt: f64,
    pub glide_ratio: f64,
}

impl GlidePerformance {
    pub fn validate(self) -> Result<(), String> {
        if !self.best_glide_ias_kt.is_finite()
            || !(10.0..=300.0).contains(&self.best_glide_ias_kt)
            || !self.glide_ratio.is_finite()
            || !(1.0..=100.0).contains(&self.glide_ratio)
        {
            return Err("Invalid best-glide speed or glide ratio".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlideOrigin {
    pub position: LatLon,
    pub altitude_msl_ft: f64,
    pub pressure_altitude_ft: f64,
    pub epoch_ms: i64,
}

/// `None` means unknown terrain, never sea level. Terrain providers should use
/// conservative cell maxima, rather than interpolate through a narrow ridge.
pub trait GlideTerrain {
    fn elevation_ft(&self, position: LatLon) -> Result<Option<f64>, String>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlideBoundary {
    /// Unknown or unfinished directions are gaps; renderers must not close them.
    pub endpoints: Vec<Option<LatLon>>,
    pub complete: bool,
}

pub fn compute_glide_boundary(
    origin: &GlideOrigin,
    performance: GlidePerformance,
    atmosphere: &dyn AtmosphereModel,
    terrain: &dyn GlideTerrain,
) -> Result<GlideBoundary, String> {
    let mut computation = GlideComputation::new(origin.clone(), performance)?;
    loop {
        if let Some(boundary) = computation.advance(atmosphere, terrain, BEARING_COUNT)? {
            return Ok(boundary);
        }
    }
}

/// Resumable CPU work. The session yields between batches, allowing higher
/// priority input work to run without moving an unfinished footprint's origin.
#[derive(Clone)]
pub(crate) struct GlideComputation {
    pub origin: GlideOrigin,
    performance: GlidePerformance,
    endpoints: Vec<Option<LatLon>>,
}

impl GlideComputation {
    pub fn new(origin: GlideOrigin, performance: GlidePerformance) -> Result<Self, String> {
        performance.validate()?;
        if !origin.position.lat.is_finite()
            || !origin.position.lon.is_finite()
            || origin.position.lat.abs() > 85.0
            || origin.position.lon.abs() > 180.0
            || !origin.altitude_msl_ft.is_finite()
            || !(-1_500.0..=50_000.0).contains(&origin.altitude_msl_ft)
            || !origin.pressure_altitude_ft.is_finite()
        {
            return Err("Glide reach requires a valid position and altitude".to_string());
        }
        Ok(Self {
            origin,
            performance,
            endpoints: Vec::with_capacity(BEARING_COUNT),
        })
    }

    pub fn advance(
        &mut self,
        atmosphere: &dyn AtmosphereModel,
        terrain: &dyn GlideTerrain,
        bearings: usize,
    ) -> Result<Option<GlideBoundary>, String> {
        let origin = &self.origin;
        let Some(ground_ft) = terrain.elevation_ft(origin.position)? else {
            return Ok(Some(GlideBoundary {
                endpoints: vec![None; BEARING_COUNT],
                complete: false,
            }));
        };
        if !ground_ft.is_finite() {
            return Err("Glide terrain sample is not finite".to_string());
        }
        if ground_ft >= origin.altitude_msl_ft {
            return Ok(Some(GlideBoundary {
                endpoints: Vec::new(),
                complete: true,
            }));
        }
        for index in self.endpoints.len()..(self.endpoints.len() + bearings).min(BEARING_COUNT) {
            self.endpoints.push(glide_ray(
                origin,
                self.performance,
                atmosphere,
                terrain,
                index as f64 * 360.0 / BEARING_COUNT as f64,
            )?);
        }
        Ok(
            (self.endpoints.len() == BEARING_COUNT).then(|| GlideBoundary {
                complete: self.endpoints.iter().all(Option::is_some),
                endpoints: self.endpoints.clone(),
            }),
        )
    }
}

fn glide_ray(
    origin: &GlideOrigin,
    performance: GlidePerformance,
    atmosphere: &dyn AtmosphereModel,
    terrain: &dyn GlideTerrain,
    bearing_deg: f64,
) -> Result<Option<LatLon>, String> {
    let mut position = origin.position;
    let mut altitude_ft = origin.altitude_msl_ft;
    let mut elapsed_seconds = 0.0;
    let pressure_offset_ft = origin.pressure_altitude_ft - origin.altitude_msl_ft;
    for step in 1..=(MAX_DISTANCE_NM / STEP_NM) as usize {
        let next = glide_destination(origin.position, bearing_deg, step as f64 * STEP_NM);
        let track = crate::geodesy::initial_course_deg(position, next).to_radians();
        let sample = atmosphere.sample(
            position,
            altitude_ft + pressure_offset_ft,
            origin.epoch_ms + (elapsed_seconds * 1_000.0_f64).round() as i64,
        )?;
        if !sample.wind_east_kt.is_finite() || !sample.wind_north_kt.is_finite() {
            return Err("Glide wind sample is not finite".to_string());
        }
        let tas = crate::altitude_planner::indicated_to_true_airspeed(
            performance.best_glide_ias_kt,
            altitude_ft + pressure_offset_ft,
            sample.temperature_c,
        )
        .map_err(|error| error.to_string())?;
        // TAS follows the descending flight path; resolve its horizontal and
        // vertical components before solving the crosswind triangle.
        let sink_kt = tas / (performance.glide_ratio.powi(2) + 1.0).sqrt();
        let horizontal_tas = sink_kt * performance.glide_ratio;
        let along = sample.wind_east_kt * track.sin() + sample.wind_north_kt * track.cos();
        let across = sample.wind_east_kt * track.cos() - sample.wind_north_kt * track.sin();
        if across.abs() >= horizontal_tas {
            return Ok(Some(position));
        }
        let ground_speed = (horizontal_tas.powi(2) - across.powi(2)).sqrt() + along;
        if ground_speed <= 0.0 {
            return Ok(Some(position));
        }
        let seconds = STEP_NM / ground_speed * 3_600.0;
        let next_altitude = altitude_ft - sink_kt * FEET_PER_NM * seconds / 3_600.0;
        let Some(ground_ft) = terrain.elevation_ft(next)? else {
            return Ok(None);
        };
        if !ground_ft.is_finite() {
            return Err("Glide terrain sample is not finite".to_string());
        }
        if next_altitude <= ground_ft {
            // Stop at the last verified point. Never resume a ray in a valley
            // beyond a ridge that the glide could not cross.
            return Ok(Some(position));
        }
        position = next;
        altitude_ft = next_altitude;
        elapsed_seconds += seconds;
        if elapsed_seconds > 7_200.0 || altitude_ft < -1_500.0 {
            return Ok(None);
        }
    }
    // A computational bound is not a terrain-intersection estimate.
    Ok(None)
}

pub fn glide_destination(origin: LatLon, bearing_deg: f64, distance_nm: f64) -> LatLon {
    let lat = origin.lat.to_radians();
    let lon = origin.lon.to_radians();
    let bearing = bearing_deg.to_radians();
    let angular = distance_nm / EARTH_RADIUS_NM;
    let next_lat = (lat.sin() * angular.cos() + lat.cos() * angular.sin() * bearing.cos()).asin();
    let next_lon = lon
        + (bearing.sin() * angular.sin() * lat.cos())
            .atan2(angular.cos() - lat.sin() * next_lat.sin());
    LatLon {
        lat: next_lat.to_degrees(),
        lon: (next_lon.to_degrees() + 180.0).rem_euclid(360.0) - 180.0,
    }
}

/// Motion is measured from the last actual calculation, avoiding cell-boundary
/// jitter and latitude-dependent errors. Altitude can invalidate a stationary
/// result. A model/forecast/terrain revision change bypasses these tolerances.
pub fn glide_needs_refresh(
    previous: &GlideOrigin,
    current: &GlideOrigin,
    height_agl_ft: f64,
    performance: GlidePerformance,
) -> bool {
    let height = height_agl_ft.max(0.0);
    let movement_nm = (height * performance.glide_ratio / FEET_PER_NM * 0.1).clamp(0.05, 0.5);
    let altitude_ft = (height * 0.05).clamp(25.0, 100.0);
    current.epoch_ms < previous.epoch_ms
        || current.epoch_ms - previous.epoch_ms >= 30_000
        || crate::geodesy::great_circle_distance_nm(previous.position, current.position)
            >= movement_nm
        || (current.altitude_msl_ft - previous.altitude_msl_ft).abs() >= altitude_ft
        || (current.pressure_altitude_ft - previous.pressure_altitude_ft).abs() >= altitude_ft
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AtmosphereSample, NoWindIsaAtmosphere};

    struct Flat(f64);
    impl GlideTerrain for Flat {
        fn elevation_ft(&self, _: LatLon) -> Result<Option<f64>, String> {
            Ok(Some(self.0))
        }
    }
    fn origin() -> GlideOrigin {
        GlideOrigin {
            position: LatLon {
                lat: 47.0,
                lon: -122.0,
            },
            altitude_msl_ft: 6_000.0,
            pressure_altitude_ft: 6_000.0,
            epoch_ms: 0,
        }
    }
    fn performance() -> GlidePerformance {
        GlidePerformance {
            best_glide_ias_kt: 62.0,
            glide_ratio: 10.0,
        }
    }
    fn range(origin: &GlideOrigin, boundary: &GlideBoundary, bearing: usize) -> f64 {
        crate::geodesy::great_circle_distance_nm(
            origin.position,
            boundary.endpoints[bearing].unwrap(),
        )
    }
    #[test]
    fn still_air_range_is_height_times_glide_ratio() {
        let origin = origin();
        let ring =
            compute_glide_boundary(&origin, performance(), &NoWindIsaAtmosphere, &Flat(1_000.0))
                .unwrap();
        let expected = 5_000.0 * 10.0 / FEET_PER_NM;
        assert!(ring.complete);
        for point in ring.endpoints {
            let distance =
                crate::geodesy::great_circle_distance_nm(origin.position, point.unwrap());
            assert!(distance <= expected + 1e-8);
            assert!(expected - distance <= STEP_NM + 1e-8);
        }
    }
    struct Wind(f64);
    impl AtmosphereModel for Wind {
        fn sample(
            &self,
            position: LatLon,
            altitude: f64,
            epoch: i64,
        ) -> Result<AtmosphereSample, String> {
            Ok(AtmosphereSample {
                wind_east_kt: self.0,
                ..NoWindIsaAtmosphere.sample(position, altitude, epoch)?
            })
        }
    }
    #[test]
    fn headwind_tailwind_and_crosswind_change_ground_reach() {
        let origin = origin();
        let calm = compute_glide_boundary(&origin, performance(), &Wind(0.0), &Flat(0.0)).unwrap();
        let windy =
            compute_glide_boundary(&origin, performance(), &Wind(20.0), &Flat(0.0)).unwrap();
        assert!(range(&origin, &windy, 45) > range(&origin, &calm, 45));
        assert!(range(&origin, &windy, 135) < range(&origin, &calm, 135));
        assert!(range(&origin, &windy, 0) < range(&origin, &calm, 0));
    }
    #[test]
    fn unreachable_upwind_tracks_never_turn_into_negative_range() {
        let origin = origin();
        let ring =
            compute_glide_boundary(&origin, performance(), &Wind(150.0), &Flat(0.0)).unwrap();
        assert_eq!(range(&origin, &ring, 135), 0.0);
        assert_eq!(range(&origin, &ring, 0), 0.0);
        assert!(range(&origin, &ring, 45) > 10.0);
    }
    struct Ridge(LatLon);
    impl GlideTerrain for Ridge {
        fn elevation_ft(&self, position: LatLon) -> Result<Option<f64>, String> {
            let distance = crate::geodesy::great_circle_distance_nm(self.0, position);
            Ok(Some(if (2.0..2.2).contains(&distance) {
                5_500.0
            } else {
                0.0
            }))
        }
    }
    #[test]
    fn first_ridge_blocks_the_valley_beyond_it() {
        let origin = origin();
        let ring =
            compute_glide_boundary(&origin, performance(), &Wind(0.0), &Ridge(origin.position))
                .unwrap();
        assert!(ring.complete);
        for index in 0..BEARING_COUNT {
            assert!(range(&origin, &ring, index) < 2.0);
        }
    }
    struct Missing;
    impl GlideTerrain for Missing {
        fn elevation_ft(&self, _: LatLon) -> Result<Option<f64>, String> {
            Ok(None)
        }
    }
    #[test]
    fn missing_terrain_does_not_become_sea_level() {
        let ring = compute_glide_boundary(&origin(), performance(), &Wind(0.0), &Missing).unwrap();
        assert!(!ring.complete);
        assert!(ring.endpoints.iter().all(Option::is_none));
    }
    #[test]
    fn refresh_tracks_motion_altitude_and_age_independently() {
        let previous = origin();
        let mut current = previous.clone();
        current.position = glide_destination(previous.position, 90.0, 0.49);
        assert!(!glide_needs_refresh(
            &previous,
            &current,
            6_000.0,
            performance()
        ));
        current.position = glide_destination(previous.position, 90.0, 0.51);
        assert!(glide_needs_refresh(
            &previous,
            &current,
            6_000.0,
            performance()
        ));
        current = previous.clone();
        current.altitude_msl_ft -= 100.0;
        assert!(glide_needs_refresh(
            &previous,
            &current,
            6_000.0,
            performance()
        ));
        current = previous.clone();
        current.epoch_ms = 30_000;
        assert!(glide_needs_refresh(
            &previous,
            &current,
            6_000.0,
            performance()
        ));
        current = previous.clone();
        current.position = glide_destination(previous.position, 90.0, 0.1);
        assert!(glide_needs_refresh(
            &previous,
            &current,
            100.0,
            performance()
        ));
    }

    #[test]
    fn incremental_work_matches_whole_ring_and_samples_the_descent() {
        use std::cell::Cell;
        struct ChangingWind {
            lowest: Cell<f64>,
            farthest: Cell<f64>,
            latest: Cell<i64>,
        }
        impl AtmosphereModel for ChangingWind {
            fn sample(
                &self,
                p: LatLon,
                altitude: f64,
                epoch: i64,
            ) -> Result<AtmosphereSample, String> {
                self.lowest.set(self.lowest.get().min(altitude));
                self.farthest.set(self.farthest.get().max(
                    crate::geodesy::great_circle_distance_nm(origin().position, p),
                ));
                self.latest.set(self.latest.get().max(epoch));
                Ok(AtmosphereSample {
                    wind_east_kt: altitude / 300.0,
                    ..NoWindIsaAtmosphere.sample(p, altitude, epoch)?
                })
            }
        }
        let atmosphere = ChangingWind {
            lowest: Cell::new(6000.0),
            farthest: Cell::new(0.0),
            latest: Cell::new(0),
        };
        let full =
            compute_glide_boundary(&origin(), performance(), &atmosphere, &Flat(0.0)).unwrap();
        let mut job = GlideComputation::new(origin(), performance()).unwrap();
        let mut batches = 0;
        let result = loop {
            batches += 1;
            if let Some(result) = job.advance(&atmosphere, &Flat(0.0), 8).unwrap() {
                break result;
            }
            assert!(job.endpoints.len() <= batches * 8);
        };
        assert_eq!(batches, 23);
        assert_eq!(full, result);
        assert!(atmosphere.lowest.get() < 50.0);
        assert!(atmosphere.farthest.get() > 5.0);
        assert!(atmosphere.latest.get() > 60_000);
        let constant =
            compute_glide_boundary(&origin(), performance(), &Wind(20.0), &Flat(0.0)).unwrap();
        assert!(range(&origin(), &result, 45) < range(&origin(), &constant, 45));
    }

    #[test]
    fn invalid_inputs_and_unknown_ground_are_not_reachable_land() {
        assert!(
            compute_glide_boundary(&origin(), performance(), &Wind(0.0), &Flat(f64::NAN)).is_err()
        );
        let mut invalid = origin();
        invalid.position.lon = 181.0;
        assert!(GlideComputation::new(invalid, performance()).is_err());
        let grounded =
            compute_glide_boundary(&origin(), performance(), &Wind(0.0), &Flat(6000.0)).unwrap();
        assert!(grounded.endpoints.is_empty());
    }
}
