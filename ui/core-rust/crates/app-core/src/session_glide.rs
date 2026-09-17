// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::glide::{
    glide_needs_refresh, GlideComputation, GlideOrigin, GlidePerformance, GlideTerrain,
};
use crate::{AtmosphereModel, NoWindIsaAtmosphere};
use app_ui_contracts::session::{UiGlidePoint, UiGlideRing};
use std::cell::RefCell;

pub(crate) struct GlideRingCache {
    origin: GlideOrigin,
    height_agl_ft: f64,
    identity: String,
    view: UiGlideRing,
}

pub(crate) struct GlideRingJob {
    identity: String,
    computation: GlideComputation,
    used_no_wind: bool,
}

fn message(session: &mut UiSession, text: impl Into<String>) -> AppResult<HadOperationOutcome> {
    session.map.runtime_mut().glide_ring = None;
    session.map.runtime_mut().glide_job = None;
    complete(
        session,
        UiGlideRing {
            message: Some(text.into()),
            ..Default::default()
        },
    )
}

fn complete(session: &mut UiSession, mut view: UiGlideRing) -> AppResult<HadOperationOutcome> {
    if view.recheck_after_ms == 0 {
        view.recheck_after_ms = 1_000;
    }
    let changed = match &view.message {
        Some(detail) => upsert_data_status_record(
            session,
            DataStatusRecord::new(
                "glide:unavailable",
                "GLIDE",
                Some("CHECK".to_string()),
                UiStatusSeverity::Unavailable,
                true,
                detail.clone(),
            ),
        ),
        None => clear_data_status_record(session, "glide:unavailable"),
    };
    Ok(HadOperationOutcome::complete_with_invalidations(
        serde_json::to_value(view).map_err(internal_json_error)?,
        if changed {
            vec![UiInvalidation::SessionSnapshot]
        } else {
            Vec::new()
        },
    ))
}

/// Called by the shared low-priority work runner (web core worker / Android
/// background dispatcher). It never participates in the high-rate projection.
pub fn query_glide_ring_in_session(handle: u32, epoch_ms: i64) -> AppResult<HadOperationOutcome> {
    let slot = session_slot(handle)?;
    let mut guard = slot.lock_running()?;
    let session = &mut *guard;
    advance_session_wall_clock(session, epoch_ms);
    if !session.map.layer_state().glide_ring.visible {
        session.map.runtime_mut().glide_ring = None;
        session.map.runtime_mut().glide_job = None;
        return complete(session, UiGlideRing::default());
    }
    let Some(kinematics) = session.situation.ownship().resolved.kinematics.as_ref() else {
        return message(session, "Glide reach needs an aircraft position");
    };
    let Some(altitude_msl_ft) = kinematics.altitude_msl_ft else {
        return message(session, "Glide reach needs an MSL altitude");
    };
    let origin = GlideOrigin {
        position: kinematics.position,
        altitude_msl_ft,
        pressure_altitude_ft: kinematics.pressure_altitude_ft.unwrap_or(altitude_msl_ft),
        epoch_ms: session.coordinator.wall_clock_epoch_ms,
    };
    let track = kinematics.track_deg_true;
    let aircraft = match crate::had_ops::planner_aircraft(
        session_nav_kv_store(session)?,
        session_plan(session)?.aircraft.as_ref(),
        &session_aircraft_definitions(session)?,
    ) {
        Ok(aircraft) => aircraft,
        Err(HadReadError::NeedPages(pages)) => {
            return Ok(HadOperationOutcome::NeedResources {
                resources: nav_kv_page_resources(pages),
            })
        }
        Err(HadReadError::Fatal(reason)) => return message(session, reason),
    };
    if let Some(advisory) = aircraft.advisory {
        return message(session, format!("Glide reach unavailable: {advisory}"));
    }
    let Some(glide) = aircraft.definition.glide else {
        return message(
            session,
            format!(
                "{} has no glide performance model",
                aircraft.definition.label
            ),
        );
    };
    let performance = GlidePerformance {
        best_glide_ias_kt: glide.best_glide_ias_kt,
        glide_ratio: glide.glide_ratio,
    };
    let forecast_selected =
        session.coordinator.altitude_planner_wind_selection == AltitudePlannerWindSelection::Gfs;
    let forecast = session.weather.runtime().forecast_atmosphere.as_ref();
    let identity = format!(
        "{}:{forecast_selected}:{}:{}:{}",
        aircraft.selection.definition_hash,
        forecast
            .map(|forecast| forecast.version_label())
            .unwrap_or("none"),
        session.packages.revision(),
        session.nav_data.generation()
    );
    let mut job = session
        .map
        .runtime_mut()
        .glide_job
        .take()
        .filter(|job| job.identity == identity);
    // Keep the origin fixed through terrain/forecast fetches and CPU batches.
    // A new model, forecast, package generation or mode discards the old job.
    if job.is_none() {
        if let Some(cache) = &session.map.runtime().glide_ring {
            if cache.identity == identity
                && !glide_needs_refresh(&cache.origin, &origin, cache.height_agl_ft, performance)
            {
                return complete(session, cache.view.clone());
            }
        }
        let computation = match GlideComputation::new(origin, performance) {
            Ok(computation) => computation,
            Err(reason) => return message(session, reason),
        };
        job = Some(GlideRingJob {
            identity: identity.clone(),
            computation,
            used_no_wind: !forecast_selected
                || session.weather.runtime().forecast_atmosphere.is_none(),
        });
    }
    let mut job = job.unwrap();
    let origin = job.computation.origin.clone();
    let terrain = SessionGlideTerrain {
        session,
        requests: RefCell::new(HashMap::new()),
        decoded: RefCell::new(HashMap::new()),
        missing: RefCell::new(BTreeMap::new()),
    };
    let forecast = session.weather.runtime().forecast_atmosphere.as_ref();
    if let Some(forecast) = forecast {
        forecast.clear_observed_missing_pages();
    }
    let atmosphere: &dyn AtmosphereModel = if job.used_no_wind {
        &NoWindIsaAtmosphere
    } else {
        forecast.expect("forecast identity checked")
    };
    let before_batch = job.computation.clone();
    let boundary = job.computation.advance(atmosphere, &terrain, 8);
    let mut resources = if forecast_selected && !job.used_no_wind {
        observed_forecast_page_resources(session)?
    } else {
        Vec::new()
    };
    // Missing pages are fetched first. If the selected forecast cannot cover
    // the descent, start the entire job again in no wind, never mix assumptions.
    if boundary.is_err() && !job.used_no_wind && resources.is_empty() {
        job.computation =
            GlideComputation::new(origin.clone(), performance).map_err(|message| AppError {
                kind: AppErrorKind::Internal,
                message,
            })?;
        job.used_no_wind = true;
        session.map.runtime_mut().glide_job = Some(job);
        return pending(session, &identity);
    }
    let height_agl_ft = terrain
        .elevation_ft(origin.position)
        .ok()
        .flatten()
        .map(|ground| (origin.altitude_msl_ft - ground).max(0.0))
        .unwrap_or(0.0);
    let missing = terrain.missing.into_inner();
    for (_, request) in missing {
        for source in request.source_tiles {
            resources.extend(
                session
                    .packages
                    .package_resource_requests(
                        &format!(
                            "terrain/source/{}",
                            terrain_source_tile_cache_key(&source.product_id, &source.path)
                        ),
                        &source.product_id,
                        &source.path,
                        true,
                    )
                    .map_err(|message| AppError {
                        kind: AppErrorKind::InvalidManifest,
                        message,
                    })?,
            );
        }
    }
    if !resources.is_empty() {
        job.computation = before_batch;
        session.map.runtime_mut().glide_job = Some(job);
        return Ok(HadOperationOutcome::NeedResources {
            resources: dedupe_resource_requests(resources),
        });
    }
    let boundary = match boundary {
        Ok(Some(boundary)) => boundary,
        Ok(None) => {
            session.map.runtime_mut().glide_job = Some(job);
            return pending(session, &identity);
        }
        Err(reason) => return message(session, reason),
    };
    let used_no_wind = job.used_no_wind;
    let wind = if used_no_wind {
        NoWindIsaAtmosphere.sample(
            origin.position,
            origin.pressure_altitude_ft,
            origin.epoch_ms,
        )
    } else {
        atmosphere.sample(
            origin.position,
            origin.pressure_altitude_ft,
            origin.epoch_ms,
        )
    };
    let (wind_label, wind_direction_deg_true) = if used_no_wind {
        ("NO WIND".to_string(), None)
    } else if let Ok(wind) = wind {
        glide_wind_label(wind.wind_east_kt, wind.wind_north_kt, track)
    } else {
        ("Wind unavailable".to_string(), None)
    };
    let paths = boundary_paths(&boundary.endpoints);
    let label_position = paths
        .iter()
        .flatten()
        .copied()
        .max_by(|a, b| a.lat.total_cmp(&b.lat));
    let view = UiGlideRing {
        paths,
        label_position,
        wind_label,
        wind_direction_deg_true,
        recheck_after_ms: 1_000,
        speed_label: format!("{:.0} kt IAS", performance.best_glide_ias_kt),
        message: if !boundary.complete {
            Some(
                "Glide reach is incomplete where terrain is unknown or calculation limits are reached"
                    .to_string(),
            )
        } else if forecast_selected && used_no_wind {
            Some("Selected wind forecast unavailable here; glide reach uses no wind".to_string())
        } else {
            None
        },
    };
    session.map.runtime_mut().glide_ring = Some(GlideRingCache {
        origin,
        height_agl_ft,
        identity,
        view: view.clone(),
    });
    complete(session, view)
}

fn glide_wind_label(east_kt: f64, north_kt: f64, track: Option<f64>) -> (String, Option<f64>) {
    let speed = east_kt.hypot(north_kt);
    if speed < 0.5 {
        return ("·".to_string(), None);
    }
    let label = match track {
        Some(track) => crate::altitude_planner::format_signed_wind_component(
            east_kt * track.to_radians().sin() + north_kt * track.to_radians().cos(),
        ),
        None => format!("{speed:.0} kt"),
    };
    (
        label,
        Some(east_kt.atan2(north_kt).to_degrees().rem_euclid(360.0)),
    )
}

fn pending(session: &mut UiSession, identity: &str) -> AppResult<HadOperationOutcome> {
    let mut view = session
        .map
        .runtime()
        .glide_ring
        .as_ref()
        .filter(|cache| cache.identity == identity)
        .map(|cache| cache.view.clone())
        .unwrap_or_default();
    view.recheck_after_ms = 20;
    complete(session, view)
}

fn boundary_paths(endpoints: &[Option<LatLon>]) -> Vec<Vec<UiGlidePoint>> {
    if endpoints.is_empty() {
        return Vec::new();
    }
    let first_gap = endpoints.iter().position(Option::is_none);
    let start = first_gap
        .map(|index| (index + 1) % endpoints.len())
        .unwrap_or(0);
    let count = endpoints.len() + usize::from(first_gap.is_none());
    let mut paths = Vec::new();
    let mut path = Vec::new();
    for index in 0..count {
        match endpoints[(start + index) % endpoints.len()] {
            Some(point) => path.push(UiGlidePoint {
                lat: point.lat,
                lon: point.lon,
            }),
            None => {
                if path.len() >= 2 {
                    paths.push(std::mem::take(&mut path));
                } else {
                    path.clear();
                }
            }
        }
    }
    if path.len() >= 2 {
        paths.push(path);
    }
    paths
}

struct SessionGlideTerrain<'a> {
    session: &'a UiSession,
    requests: RefCell<HashMap<(u32, u32), Option<crate::terrain::TerrainElevationRequest>>>,
    decoded: RefCell<HashMap<String, (crate::TerrainTileInfo, Vec<i16>)>>,
    missing: RefCell<BTreeMap<String, crate::terrain::TerrainElevationRequest>>,
}

impl GlideTerrain for SessionGlideTerrain<'_> {
    fn elevation_ft(&self, position: LatLon) -> Result<Option<f64>, String> {
        let (tile_x, tile_y, _, _) = crate::terrain::terrain_tile_position(
            position,
            product_contracts::TERRAIN_TER2_MAX_ZOOM,
        );
        let mut requests = self.requests.borrow_mut();
        let request = requests.entry((tile_x, tile_y)).or_insert_with(|| {
            match self.session.packages.resource_policy() {
                CoreResourcePolicy::InstalledPackage => {
                    crate::terrain::terrain_elevation_request_with_available_packages(
                        position,
                        self.session.packages.installed_package_ids(),
                    )
                }
                CoreResourcePolicy::PublicUnpacked => {
                    crate::terrain::terrain_elevation_request(position)
                }
            }
        });
        let Some(request) = request else {
            return Ok(None);
        };
        let (_, _, fx, fy) = crate::terrain::terrain_tile_position(position, request.z);
        let mut elevation: Option<f64> = None;
        let mut center_known = false;
        let mut decoded = self.decoded.borrow_mut();
        for source in &request.source_tiles {
            let key = terrain_source_tile_cache_key(&source.product_id, &source.path);
            let Some(bytes) = self
                .session
                .map
                .runtime()
                .terrain_source_tile_cache
                .get(&key)
            else {
                self.missing
                    .borrow_mut()
                    .insert(request.key(), request.clone());
                return Ok(None);
            };
            if bytes.is_empty() {
                continue;
            }
            if !decoded.contains_key(&key) {
                decoded.insert(key.clone(), crate::terrain::parse_abt2_tile(bytes)?);
            }
            let (info, samples) = &decoded[&key];
            let x = (fx * f64::from(info.width)).floor() as i32;
            let y = (fy * f64::from(info.height)).floor() as i32;
            let center_x = x.clamp(0, i32::from(info.width) - 1) as usize;
            let center_y = y.clamp(0, i32::from(info.height) - 1) as usize;
            center_known |= samples[center_y * info.width as usize + center_x] != info.nodata;
            for row in (y - 1).max(0)..=(y + 1).min(i32::from(info.height) - 1) {
                for col in (x - 1).max(0)..=(x + 1).min(i32::from(info.width) - 1) {
                    let raw = samples[row as usize * info.width as usize + col as usize];
                    if raw == info.nodata {
                        continue;
                    }
                    let value = f64::from(raw) * f64::from(info.scale) + f64::from(info.offset);
                    elevation = Some(elevation.map(|current| current.max(value)).unwrap_or(value));
                }
            }
        }
        // A neighboring known cell must not fill a hole in the terrain coverage.
        Ok(if center_known { elevation } else { None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wind_flow_bearing_is_true_and_independent_of_aircraft_track() {
        assert_eq!(
            glide_wind_label(20.0, 0.0, Some(90.0)),
            ("+20".into(), Some(90.0))
        );
        assert_eq!(
            glide_wind_label(20.0, 0.0, Some(270.0)),
            ("−20".into(), Some(90.0))
        );
        assert_eq!(
            glide_wind_label(20.0, 0.0, Some(0.0)),
            ("0".into(), Some(90.0))
        );
        assert_eq!(
            glide_wind_label(0.0, -20.0, None),
            ("20 kt".into(), Some(180.0))
        );
        assert_eq!(
            glide_wind_label(-10.0, 10.0, None),
            ("14 kt".into(), Some(315.0))
        );
        assert_eq!(glide_wind_label(0.1, 0.1, Some(90.0)), ("·".into(), None));
    }

    #[test]
    fn gaps_are_not_bridged_even_across_north() {
        let point = |lat| Some(LatLon { lat, lon: 0.0 });
        let paths = boundary_paths(&[point(1.0), point(2.0), None, point(3.0), point(4.0)]);
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].iter().map(|point| point.lat).collect::<Vec<_>>(),
            vec![3.0, 4.0, 1.0, 2.0]
        );
        let closed = boundary_paths(&[point(1.0), point(2.0), point(3.0)]);
        assert_eq!(closed[0].first(), closed[0].last());
        assert!(boundary_paths(&[None, point(1.0), None]).is_empty());
    }
}
