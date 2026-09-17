// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::planning::{
    self as p,
    airway_tests::{append_airway, waypoints},
};

fn round_trip(plan: &FlightPlan) -> (serde_json::Value, FlightPlan) {
    let record = CloudRecord::encode::<FlightPlanRecord>(
        &wire::flight_plan::StoredFlightPlan::from(plan.clone()),
        Some(123),
    )
    .unwrap();
    let json = serde_json::to_value(&record).unwrap();
    let decoded: CloudRecord = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(decoded.modified_at_epoch_ms(), Some(123));
    let restored = FlightPlan::try_from(decoded.decode::<FlightPlanRecord>().unwrap()).unwrap();
    (json, restored)
}

#[test]
fn two_airways_keep_distinct_occurrences_and_resolved_offline_geometry() {
    let fix = |s: &str| NavRef::Fix(s.into());
    let initial = waypoints(&[fix("BANDR")]);
    let first = append_airway(&initial, "V2", &[fix("BANDR"), fix("MID"), fix("ELN")]);
    let mut plan = append_airway(&first, "V187", &[fix("ELN"), fix("MID"), fix("BANDR")]);
    plan.name = "Repeated fixes do not identify occurrences".into();
    plan.alternate = Some(crate::AirportId("KPAE".into()));
    plan.notes = Some("Offline planning".into());
    plan.planned_departure_time_epoch_ms = Some(123_456);
    plan.guidance = Some(p::GuidanceState {
        active_leg_index: 1,
        active_detail_index: None,
        sequencing_mode: p::SequencingMode::FollowPlan,
        direct_to: None,
        suspend_reason: None,
    });
    let (json, restored) = round_trip(&plan);
    assert!(json["value"].get("guidance").is_none());
    plan.guidance = None;
    assert_eq!(restored, plan);
    assert_eq!(
        p::project_identity_rows(&restored),
        p::project_identity_rows(&plan)
    );
}

#[test]
fn nav_identity_domains_round_trip_without_identifier_guessing() {
    let point = crate::LatLon {
        lat: 47.5,
        lon: -122.3,
    };
    let refs = vec![
        NavRef::Airport("KRNT".into()),
        NavRef::Navaid("RBG".into()),
        NavRef::Fix("ZGOOD".into()),
        NavRef::LatLon(point),
        NavRef::Spot(point),
        NavRef::ArincNavaid {
            identifier: "RBG".into(),
            icao_code: "K1".into(),
            section_code: "D".into(),
            subsection_code: " ".into(),
        },
        NavRef::TerminalNavaid {
            airport_id: "KRNT".into(),
            identifier: "LOC".into(),
            icao_code: "K1".into(),
            section_code: "P".into(),
            subsection_code: "I".into(),
        },
    ];
    let plan = waypoints(&refs);
    assert_eq!(round_trip(&plan).1, plan);
}

#[test]
fn procedure_selection_and_paths_round_trip_but_debug_state_does_not() {
    let point = crate::LatLon {
        lat: 47.5,
        lon: -122.3,
    };
    let end = crate::LatLon {
        lat: 47.6,
        lon: -122.3,
    };
    for kind in [
        p::ProcedureKind::Sid,
        p::ProcedureKind::Star,
        p::ProcedureKind::Approach,
    ] {
        for role in [
            p::ProcedureSegmentRole::EnrouteTransition,
            p::ProcedureSegmentRole::Common,
            p::ProcedureSegmentRole::RunwayTransition,
        ] {
            let mut plan = waypoints(&[
                NavRef::Airport("KRNT".into()),
                NavRef::Airport("KPAE".into()),
            ]);
            let airport = if kind == p::ProcedureKind::Sid {
                "KRNT"
            } else {
                "KPAE"
            };
            plan.route_components.insert(
                1,
                p::RouteComponent::Procedure {
                    procedure: p::ProcedureSegment {
                        airport_id: crate::AirportId(airport.into()),
                        procedure_id: "TEST".into(),
                        display_label: Some("Test procedure".into()),
                        kind: kind.clone(),
                        runway_transition: Some("RW16".into()),
                        enroute_transition: Some("TRANS".into()),
                        terminal_discontinuity: Some(p::ProcedureDiscontinuity::Vectors),
                        data_quality: vec!["test diagnostic".into()],
                    },
                },
            );
            plan.route_component_uids.insert(1, "procedure-row".into());
            plan.resolved_legs = vec![p::ResolvedLeg {
                id: "procedure-leg".into(),
                from: NavRef::Spot(point),
                to: NavRef::Spot(end),
                source: p::ResolvedLegSource::RouteComponent { component_index: 1 },
                procedure_provenance: Some(p::ProcedureLegProvenance {
                    airport_id: airport.into(),
                    procedure_id: "TEST".into(),
                    kind: kind.clone(),
                    role,
                    path_termination: p::PathTermination::Other("RF".into()),
                    leg_sequence: 10,
                    discontinuity_after: Some(p::ProcedureDiscontinuity::Hold),
                    display_path: Some(p::LegDisplayPath {
                        style: p::LegDisplayPathStyle::Dashed,
                        effective_terminal_course_deg: Some(90.0),
                        elements: vec![
                            p::LegDisplayElement::Segment { start: point, end },
                            p::LegDisplayElement::Arc {
                                center: point,
                                radius_nm: 1.0,
                                start: end,
                                end: point,
                                clockwise: true,
                                sweep_degrees: 90.0,
                            },
                        ],
                        debug_element_sources: vec!["runtime-only".into()],
                        debug_element_roles: vec!["runtime-only".into()],
                    }),
                }),
            }];
            let plan = crate::build_flight_plan(plan).unwrap();
            let (json, restored) = round_trip(&plan);
            assert!(!json.to_string().contains("runtime-only"));
            let mut expected = plan;
            let path = expected.resolved_legs[0]
                .procedure_provenance
                .as_mut()
                .unwrap()
                .display_path
                .as_mut()
                .unwrap();
            path.debug_element_sources.clear();
            path.debug_element_roles.clear();
            assert_eq!(restored, expected);
        }
    }
}

#[test]
fn legacy_membership_variants_migrate_explicitly_preserving_the_choice_and_stamp() {
    for included in [true, false] {
        for deleted in [None, Some(true), Some(false)] {
            let mut value = serde_json::json!({"included": included});
            if let Some(deleted) = deleted {
                value["deleted"] = deleted.into();
            }
            let key = format!("{AIRCRAFT_LIBRARY_RECORD_PREFIX}{}", "a".repeat(64));
            let mut records =
                BTreeMap::from([(key.clone(), CloudRecord::fixture(1, Some(42), value))]);
            account_format::CURRENT.migrate(1, &mut records).unwrap();
            let record = &records[&key];
            assert_eq!(record.schema_version(), 2);
            assert_eq!(record.modified_at_epoch_ms(), Some(42));
            assert_eq!(
                record
                    .decode::<AircraftMembershipRecord>()
                    .unwrap()
                    .included,
                included
            );
            assert_eq!(record.value(), &serde_json::json!({"included": included}));
        }
    }
}
