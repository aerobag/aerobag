// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;

use crate::{flight_plan_controller::GuidanceLegGeometry, FlightPlan, GuidanceState, LatLon};

pub(crate) const AMBIGUOUS: &str = "Active leg ambiguous; activate a leg via its flight-plan row.";
pub(crate) const NO_POSITION: &str = "No current position; activate a leg via its flight-plan row.";
pub(crate) const TOO_FAR: &str = "No nearby forward leg; activate a leg via its flight-plan row.";
pub(crate) const NO_GEOMETRY: &str = "Flight-plan geometry is not ready. Try again shortly.";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NavigationStartInput {
    pub position: Option<LatLon>,
    pub track_deg_true: Option<f64>,
    pub speed_kt: Option<f64>,
}

impl From<&crate::OwnshipRenderState> for NavigationStartInput {
    fn from(ownship: &crate::OwnshipRenderState) -> Self {
        Self {
            position: ownship.position,
            track_deg_true: ownship.track_deg_true,
            speed_kt: ownship.speed_kt,
        }
    }
}

pub(crate) fn select_start_detail(
    plan: &FlightPlan,
    geometries: &HashMap<String, GuidanceLegGeometry>,
    stopped: Option<&GuidanceState>,
    input: NavigationStartInput,
) -> Result<(usize, usize), &'static str> {
    let position = input
        .position
        .filter(|point| valid_position(*point))
        .ok_or(NO_POSITION)?;
    let track = input.track_deg_true.filter(|track| {
        track.is_finite()
            && input
                .speed_kt
                .is_some_and(|speed| speed.is_finite() && speed >= 5.0)
    });
    let mut candidates = Vec::new();
    for detail in crate::planning::guidance_detail_refs(plan) {
        let leg = &plan.resolved_legs[detail.leg_index];
        let id =
            crate::guidance_detail_id_for_leg_element(detail.leg_index, leg, detail.element_index);
        let geometry = geometries.get(&id).ok_or(NO_GEOMETRY)?;
        let endpoints = [geometry.from, geometry.to];
        let path = if geometry.path.len() >= 2 {
            &geometry.path[..]
        } else {
            &endpoints
        };
        if path.iter().any(|point| !valid_position(*point)) {
            return Err(NO_GEOMETRY);
        }
        let Some(score) = path_score(path, position, track) else {
            continue;
        };
        let remembered = stopped.is_some_and(|guidance| {
            guidance.sequencing_mode != crate::SequencingMode::DirectTo
                && guidance.active_leg_index == detail.leg_index
                && guidance.active_detail_index == Some(detail.detail_index)
        });
        // Do not discard restricted candidates before ranking: doing so could
        // silently jump over a nearby hold or missed approach to a worse fit.
        let restricted = leg.procedure_provenance.as_ref().is_some_and(|provenance| {
            // The published model has no approach/missed phase marker yet.
            provenance.kind == crate::ProcedureKind::Approach
                || provenance.discontinuity_after.is_some()
                || matches!(
                    provenance.path_termination,
                    crate::PathTermination::HeadingToManual
                )
        });
        candidates.push((
            score,
            detail.leg_index,
            detail.detail_index,
            remembered,
            restricted,
        ));
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    let best = candidates.first().ok_or(TOO_FAR)?;
    // A half-mile-equivalent margin avoids selecting arbitrarily at crossings.
    // A stop hint breaks only a close tie, never a clearly worse geometric fit.
    let close: Vec<_> = candidates
        .iter()
        .take_while(|candidate| candidate.0 - best.0 <= 0.5)
        .collect();
    let selected = match close.as_slice() {
        [only] => *only,
        _ => close
            .iter()
            .copied()
            .find(|candidate| candidate.3)
            .ok_or(AMBIGUOUS)?,
    };
    if selected.4 && !selected.3 {
        return Err(AMBIGUOUS);
    }
    Ok((selected.1, selected.2))
}

fn valid_position(point: LatLon) -> bool {
    point.lat.is_finite()
        && point.lon.is_finite()
        && point.lat.abs() <= 90.0
        && point.lon.abs() <= 180.0
}

fn path_score(path: &[LatLon], position: LatLon, track: Option<f64>) -> Option<f64> {
    const EARTH_RADIUS_NM: f64 = 3440.065;
    const MAX_DISTANCE_NM: f64 = 5.0;
    let lengths: Vec<_> = path
        .windows(2)
        .map(|pair| crate::great_circle_distance_nm(pair[0], pair[1]))
        .collect();
    let mut remaining: f64 = lengths.iter().sum();
    let mut best: Option<(f64, f64, f64)> = None;
    for (pair, length) in path.windows(2).zip(lengths) {
        remaining -= length;
        if length < 0.000_001 {
            continue;
        }
        let course = crate::initial_course_deg(pair[0], pair[1]);
        let distance_angle = crate::great_circle_distance_nm(pair[0], position) / EARTH_RADIUS_NM;
        let bearing_delta = (crate::initial_course_deg(pair[0], position) - course).to_radians();
        let along = (distance_angle.sin() * bearing_delta.cos()).atan2(distance_angle.cos())
            * EARTH_RADIUS_NM;
        let clamped = along.clamp(0.0, length);
        let nearest = crate::great_circle_intermediate(pair[0], pair[1], clamped / length);
        let distance = crate::great_circle_distance_nm(nearest, position);
        let tangent = if length - clamped > 0.000_001 {
            crate::initial_course_deg(nearest, pair[1])
        } else {
            course
        };
        let difference = track
            .map(|track| ((track - tangent + 180.0).rem_euclid(360.0) - 180.0).abs())
            .unwrap_or(0.0);
        // Heading is a tie-breaker, not permission to choose a remote part of a curve.
        if best.is_none_or(|previous| distance < previous.0) {
            best = Some((distance, difference, remaining + length - clamped));
        }
    }
    // Check completion only after finding the closest point on the whole path.
    // Otherwise a completed curve can match an earlier, more distant piece.
    best.filter(|(distance, difference, remaining)| {
        *distance <= MAX_DISTANCE_NM && *difference <= 100.0 && *remaining >= 0.01
    })
    .map(|(distance, difference, _)| distance + 2.0 * difference / 90.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NavRef, ProcedureDiscontinuity, ProcedureKind, SequencingMode};

    fn point(east_nm: f64, north_nm: f64) -> LatLon {
        LatLon {
            lat: north_nm / 60.0,
            lon: east_nm / 60.0,
        }
    }

    fn input(east: f64, north: f64, track: Option<f64>) -> NavigationStartInput {
        NavigationStartInput {
            position: Some(point(east, north)),
            track_deg_true: track,
            speed_kt: Some(120.0),
        }
    }

    fn route(paths: &[Vec<LatLon>]) -> (FlightPlan, HashMap<String, GuidanceLegGeometry>) {
        let mut plan = FlightPlan::empty();
        let mut geometries = HashMap::new();
        for (index, path) in paths.iter().enumerate() {
            let id = format!("leg-{index}");
            let from = path[0];
            let to = *path.last().unwrap();
            plan.resolved_legs.push(crate::ResolvedLeg {
                id: id.clone(),
                from: NavRef::LatLon(from),
                to: NavRef::LatLon(to),
                source: crate::ResolvedLegSource::RouteComponent {
                    component_index: index,
                },
                procedure_provenance: None,
            });
            let detail_id =
                crate::guidance_detail_id_for_leg_element(index, &plan.resolved_legs[index], 0);
            geometries.insert(
                detail_id.clone(),
                GuidanceLegGeometry {
                    leg_id: detail_id,
                    from,
                    to,
                    path: path.clone(),
                },
            );
        }
        (plan, geometries)
    }

    fn stopped(leg: usize, detail: usize) -> GuidanceState {
        GuidanceState {
            active_leg_index: leg,
            active_detail_index: Some(detail),
            sequencing_mode: SequencingMode::FollowPlan,
            direct_to: None,
            suspend_reason: None,
        }
    }

    fn procedure(
        plan: &mut FlightPlan,
        index: usize,
        kind: ProcedureKind,
        discontinuity: Option<ProcedureDiscontinuity>,
    ) {
        plan.resolved_legs[index].procedure_provenance = Some(crate::ProcedureLegProvenance {
            airport_id: "KAAA".into(),
            procedure_id: "TEST".into(),
            kind,
            role: crate::ProcedureSegmentRole::Common,
            path_termination: crate::PathTermination::TrackToFix,
            leg_sequence: 0,
            discontinuity_after: discontinuity,
            display_path: None,
        });
    }

    #[test]
    fn starts_halfway_along_plan_and_does_not_blindly_resume_old_leg() {
        let (plan, geometry) = route(&[
            vec![point(0.0, 0.0), point(20.0, 0.0)],
            vec![point(20.0, 0.0), point(40.0, 0.0)],
            vec![point(40.0, 0.0), point(60.0, 0.0)],
        ]);
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(30.0, 0.1, Some(90.0))),
            Ok((1, 1))
        );
        assert_eq!(
            select_start_detail(
                &plan,
                &geometry,
                Some(&stopped(0, 0)),
                input(50.0, 0.0, Some(90.0))
            ),
            Ok((2, 2))
        );
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(20.0, 0.0, Some(90.0))),
            Ok((1, 1))
        );
    }

    #[test]
    fn crossings_need_track_or_a_valid_stop_hint_and_ignore_low_speed_track() {
        let (plan, geometry) = route(&[
            vec![point(-10.0, 0.0), point(10.0, 0.0)],
            vec![point(0.0, -10.0), point(0.0, 10.0)],
        ]);
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(0.0, 0.0, None)),
            Err(AMBIGUOUS)
        );
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(0.0, 0.0, Some(0.0))),
            Ok((1, 1))
        );
        assert_eq!(
            select_start_detail(
                &plan,
                &geometry,
                Some(&stopped(0, 0)),
                input(0.0, 0.0, Some(0.0))
            ),
            Ok((1, 1))
        );
        assert_eq!(
            select_start_detail(
                &plan,
                &geometry,
                Some(&stopped(1, 1)),
                input(0.0, 0.0, None)
            ),
            Ok((1, 1))
        );
        assert_eq!(
            select_start_detail(
                &plan,
                &geometry,
                None,
                NavigationStartInput {
                    speed_kt: Some(0.5),
                    ..input(0.0, 0.0, Some(0.0))
                }
            ),
            Err(AMBIGUOUS)
        );
    }

    #[test]
    fn reciprocal_legs_use_track_but_coincident_same_direction_legs_are_ambiguous() {
        let (plan, geometry) = route(&[
            vec![point(0.0, 0.0), point(20.0, 0.0)],
            vec![point(20.0, 0.0), point(0.0, 0.0)],
        ]);
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(10.0, 0.0, Some(270.0))),
            Ok((1, 1))
        );
        let (plan, geometry) = route(&vec![vec![point(0.0, 0.0), point(20.0, 0.0)]; 2]);
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(10.0, 0.0, Some(90.0))),
            Err(AMBIGUOUS)
        );
    }

    #[test]
    fn requires_position_near_actual_finite_path_not_its_infinite_extension() {
        let (plan, geometry) = route(&[vec![point(0.0, 0.0), point(20.0, 0.0)]]);
        assert_eq!(
            select_start_detail(
                &plan,
                &geometry,
                None,
                NavigationStartInput {
                    position: None,
                    ..input(0.0, 0.0, None)
                }
            ),
            Err(NO_POSITION)
        );
        for (east, north) in [(30.0, 0.0), (10.0, 20.0), (-20.0, 0.0)] {
            assert_eq!(
                select_start_detail(&plan, &geometry, None, input(east, north, Some(90.0))),
                Err(TOO_FAR)
            );
        }
        assert_eq!(
            select_start_detail(&plan, &HashMap::new(), None, input(10.0, 0.0, None)),
            Err(NO_GEOMETRY)
        );
    }

    #[test]
    fn curved_path_uses_local_tangent_not_endpoint_chord() {
        let (plan, geometry) = route(&[vec![point(0.0, 0.0), point(0.0, 10.0), point(10.0, 10.0)]]);
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(5.0, 10.0, Some(90.0))),
            Ok((0, 0))
        );
    }

    #[test]
    fn earlier_hold_does_not_block_enroute_start_but_nearby_hold_is_not_silently_skipped() {
        let (mut plan, geometry) = route(&[
            vec![point(0.0, 0.0), point(10.0, 0.0)],
            vec![point(30.0, 0.0), point(50.0, 0.0)],
        ]);
        procedure(
            &mut plan,
            0,
            ProcedureKind::Sid,
            Some(ProcedureDiscontinuity::Hold),
        );
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(40.0, 0.0, Some(90.0))),
            Ok((1, 1))
        );
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(5.0, 0.0, Some(90.0))),
            Err(AMBIGUOUS)
        );
    }

    #[test]
    fn unclassified_approach_and_missed_require_explicit_activation_or_exact_resume() {
        let (mut plan, geometry) = route(&[
            vec![point(0.0, 0.0), point(10.0, 0.0)],
            vec![point(10.0, 0.0), point(20.0, 0.0)],
        ]);
        for index in 0..2 {
            procedure(&mut plan, index, ProcedureKind::Approach, None);
        }
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(15.0, 0.0, Some(90.0))),
            Err(AMBIGUOUS)
        );
        assert_eq!(
            select_start_detail(
                &plan,
                &geometry,
                Some(&stopped(0, 0)),
                input(15.0, 0.0, Some(90.0))
            ),
            Err(AMBIGUOUS)
        );
        assert_eq!(
            select_start_detail(
                &plan,
                &geometry,
                Some(&stopped(1, 1)),
                input(15.0, 0.0, Some(90.0))
            ),
            Ok((1, 1))
        );
    }

    #[test]
    fn vectors_are_not_inferred_as_a_fixed_course_to_resume() {
        let (mut plan, geometry) = route(&[vec![point(0.0, 0.0), point(10.0, 0.0)]]);
        procedure(
            &mut plan,
            0,
            ProcedureKind::Star,
            Some(ProcedureDiscontinuity::Vectors),
        );
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(5.0, 0.0, Some(90.0))),
            Err(AMBIGUOUS)
        );
    }

    #[test]
    fn completed_polyline_cannot_match_an_earlier_piece() {
        let (plan, geometry) = route(&[vec![point(0.0, 0.0), point(2.0, 0.0), point(2.0, 2.0)]]);
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(2.0, 3.0, None)),
            Err(TOO_FAR)
        );
    }

    #[test]
    fn missing_geometry_cannot_hide_an_ambiguous_candidate() {
        let (plan, mut geometry) = route(&[
            vec![point(-10.0, 0.0), point(10.0, 0.0)],
            vec![point(0.0, -10.0), point(0.0, 10.0)],
        ]);
        geometry.remove(&crate::guidance_detail_id_for_leg_element(
            1,
            &plan.resolved_legs[1],
            0,
        ));
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(0.0, 0.0, Some(90.0))),
            Err(NO_GEOMETRY)
        );
    }

    #[test]
    fn antimeridian_route_uses_short_geographic_path() {
        let (plan, geometry) = route(&[vec![
            LatLon {
                lat: 0.0,
                lon: 179.0,
            },
            LatLon {
                lat: 0.0,
                lon: -179.0,
            },
        ]]);
        assert_eq!(
            select_start_detail(
                &plan,
                &geometry,
                None,
                NavigationStartInput {
                    position: Some(LatLon {
                        lat: 0.0,
                        lon: -179.5
                    }),
                    ..input(0.0, 0.0, Some(90.0))
                }
            ),
            Ok((0, 0))
        );
    }

    #[test]
    fn chooses_actual_procedure_detail_not_first_element_of_leg() {
        let (mut plan, mut geometry) = route(&[vec![point(0.0, 0.0), point(10.0, 0.0)]]);
        procedure(&mut plan, 0, ProcedureKind::Sid, None);
        plan.resolved_legs[0]
            .procedure_provenance
            .as_mut()
            .unwrap()
            .display_path = Some(crate::LegDisplayPath {
            style: crate::LegDisplayPathStyle::Solid,
            elements: vec![
                crate::LegDisplayElement::Segment {
                    start: point(0.0, 0.0),
                    end: point(10.0, 0.0),
                },
                crate::LegDisplayElement::Segment {
                    start: point(10.0, 0.0),
                    end: point(10.0, 10.0),
                },
            ],
            effective_terminal_course_deg: None,
            debug_element_sources: Vec::new(),
            debug_element_roles: Vec::new(),
        });
        let id = crate::guidance_detail_id_for_leg_element(0, &plan.resolved_legs[0], 1);
        geometry.insert(
            id.clone(),
            GuidanceLegGeometry {
                leg_id: id,
                from: point(10.0, 0.0),
                to: point(10.0, 10.0),
                path: Vec::new(),
            },
        );
        assert_eq!(
            select_start_detail(&plan, &geometry, None, input(10.0, 5.0, Some(0.0))),
            Ok((0, 1))
        );
    }
}
