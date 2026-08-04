// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{initial_course_deg, ProcedureLegMaterializationRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedTurnDirection {
    Left,
    Right,
}

impl ResolvedTurnDirection {
    pub fn clockwise(self) -> bool {
        matches!(self, Self::Right)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InventedPiEntryCourseReversal {
    pub turn_direction: ResolvedTurnDirection,
    pub outbound_intercept_angle_deg: f64,
}

pub const SIMPLE_PI_ENTRY_MAX_TURN_DEG: f64 = 120.0;

pub const INVENTED_PI_ENTRY_REVERSAL_WARNING: &str =
    "Procedure encoding requires a PI/course reversal from an excessive inbound turn and provides no same-fix hold to define protected-side reversal geometry; invented a conservative intercept to the PI outbound course.";
pub const BORROWED_LATER_HOLD_FOR_PI_WARNING: &str =
    "Procedure encoding requires a PI/course reversal from an excessive inbound turn; borrowed a later same-fix hold to define protected-side reversal geometry.";
pub const BORROWED_SIBLING_TRANSITION_HOLD_WARNING: &str =
    "Procedure encoding resumes the common segment through a sharp course reversal without including the charted hold-in-lieu in this transition; borrowed a sibling same-fix transition hold.";
pub const UNSPECIFIED_MISSED_TURN_FROM_GEOMETRY_WARNING: &str =
    "Missed-approach encoding omits an explicit turn direction; inferred the turn direction from geometry or same-fix hold context.";
pub const UNSPECIFIED_MISSED_TURN_PLATE_EXCEPTION_WARNING: &str =
    "Missed-approach encoding omits an explicit turn direction where geometry is ambiguous; applied a plate-read exception for the published turn direction.";
pub const KNOWN_BAD_COURSE_FIELD_WARNING: &str =
    "Procedure encoding contains a known-bad course field contradicted by adjacent procedure geometry; repaired the course before constructing geometry.";
pub const KNOWN_MISSING_TURN_DIRECTION_WARNING: &str =
    "Procedure encoding omits a turn direction that is explicit on the published plate; applied the published turn direction before constructing geometry.";

pub fn sid_segment_boundary_requires_unspecified_course_reversal(
    airport_id: &str,
    procedure_id: &str,
    boundary_fix: &str,
) -> bool {
    matches!(
        (airport_id.trim(), procedure_id.trim(), boundary_fix.trim()),
        ("KSBA", "FLOUT5", "FLOUT") | ("KSBA", "KWANG6", "KWANG")
    )
}

pub fn invent_pi_entry_course_reversal_when_no_hold_is_available(
    airport_id: &str,
    procedure_id: &str,
    route_type: &str,
    transition_id: &str,
    sequence: i32,
    turn_to_pi_outbound_deg: f64,
    clockwise_short_turn_to_pi_outbound: bool,
) -> Option<InventedPiEntryCourseReversal> {
    // KOMA I32R/OVR is the original motivating case: the A-route arrives at
    // BEEFF and ARINC immediately starts a PI row whose initial outbound course
    // is nearly reciprocal. KCWI I03/CVA shows the same issue at HILLZ with a
    // less-extreme, but still invalid, 147-degree direct turn into the PI
    // outbound. Unlike KILE VOR-A/SLIMM, these procedures do not encode a later
    // same-fix hold that we can borrow as an authoritative course reversal.
    //
    // AIM says the depicted PT is still required without NoPT or straight-in
    // clearance, but it does not specify exactly how to reverse from this awkward
    // arrival geometry. We therefore invent a conservative first reversal: a
    // turn in the short-turn direction until the airplane is on a 30-degree
    // intercept heading for the PI outbound course. Keep this decision here so
    // it stays visible as an ARINC/source-data apology rather than becoming
    // ordinary planner logic.
    let _ = (
        airport_id,
        procedure_id,
        route_type,
        transition_id,
        sequence,
    );
    if turn_to_pi_outbound_deg <= SIMPLE_PI_ENTRY_MAX_TURN_DEG {
        return None;
    }
    Some(InventedPiEntryCourseReversal {
        turn_direction: if clockwise_short_turn_to_pi_outbound {
            ResolvedTurnDirection::Right
        } else {
            ResolvedTurnDirection::Left
        },
        outbound_intercept_angle_deg: 30.0,
    })
}

pub fn borrow_later_same_fix_hold_for_excessive_pi_entry_turn(
    airport_id: &str,
    procedure_id: &str,
    route_type: &str,
    transition_id: &str,
    sequence: i32,
    turn_to_pi_outbound_deg: f64,
) -> bool {
    // KILE VOR-A/SLIMM is the original motivating case: the A-route arrives at
    // GRK almost inbound on the later charted hold, then the next ARINC row is a
    // PI course reversal whose initial outbound course is nearly reciprocal.
    // KSJN VOR-A/PERRL shows the same shape below the old 150-degree threshold:
    // the missed hold at SJN stays on the protected side, while inventing a
    // free-form reversal overshoots the final course to intercept the PI
    // outbound. ARINC does encode these same-fix holds, but only later in the
    // procedure. There is no official guidance here; we are choosing to borrow
    // that same-fix hold entry before inventing fallback PI-entry geometry.
    //
    // DARTE on the same plate needs the PI but only turns about 67 degrees to
    // outbound, so the threshold must not convert ordinary PI entries into
    // borrowed holds. Use SIMPLE_PI_ENTRY_MAX_TURN_DEG as the single boundary:
    // if the turn is small enough to fillet later, keep the simple PI entry;
    // otherwise, borrow a charted same-fix/same-position hold when one exists.
    // KABI I35R/MEDLY is the motivating same-position case, where TOMHI and the
    // later AB hold are co-located but not the same ARINC nav_ref.
    let _ = (
        airport_id,
        procedure_id,
        route_type,
        transition_id,
        sequence,
    );
    turn_to_pi_outbound_deg > SIMPLE_PI_ENTRY_MAX_TURN_DEG
}

pub fn borrow_sibling_transition_hold_for_common_if_course_reversal(
    airport_id: &str,
    procedure_id: &str,
    selected_transition_id: &str,
    common_if_sequence: i32,
    turn_to_common_course_deg: f64,
) -> bool {
    // KGSP I04/SPA and L04/SPA are the motivating cases. The selected SPA
    // transition ends at OXABY on an outbound-ish CF, then the common segment
    // begins inbound from OXABY. ARINC does encode the charted hold-in-lieu at
    // OXABY, but as a different A-route transition ("A OXABY HF"), not inside
    // the selected SPA transition. Borrow that sibling same-fix hold only when
    // the selected transition cannot sensibly continue into the common course.
    //
    // This is the same kind of source-data apology as borrowing a later missed
    // hold for a PI: the authority is the same-fix hold row, while the angle
    // gate keeps us from turning every sibling IAF hold into ordinary handoff
    // logic.
    let _ = (
        airport_id,
        procedure_id,
        selected_transition_id,
        common_if_sequence,
    );
    turn_to_common_course_deg >= 150.0
}

pub fn handle_unspecified_missed_turn_to_same_fix_hold(
    airport_id: &str,
    procedure_id: &str,
    route_type: &str,
    sequence: i32,
) -> Option<ResolvedTurnDirection> {
    // These plates show a left turn in the PDF/charted missed approach, but the
    // ARINC DF row that goes back to the same-fix hold has no L/R turn
    // direction. Keep this list narrow so future source-data fixes are visible.
    match (
        airport_id.trim(),
        procedure_id.trim(),
        route_type.trim(),
        sequence,
    ) {
        ("KGDB", "R33", "R", 50) => Some(ResolvedTurnDirection::Left),
        ("KHOU", "I31L", "I", 50) => Some(ResolvedTurnDirection::Left),
        ("KHOU", "L31L", "L", 50) => Some(ResolvedTurnDirection::Left),
        ("KIEN", "R30", "R", 50) => Some(ResolvedTurnDirection::Left),
        _ => None,
    }
}

pub fn repair_known_bad_procedure_fields(
    airport_id: &str,
    procedure_id: &str,
    records: &mut [ProcedureLegMaterializationRecord],
) -> Vec<String> {
    let mut warnings = Vec::new();
    if repair_kjst_rnav_rwy_33_ca_true_course_encoded_as_magnetic(airport_id, procedure_id, records)
    {
        warnings.push(KNOWN_BAD_COURSE_FIELD_WARNING.to_string());
    }
    if repair_khio_scapo7_rw20_missing_left_turn(airport_id, procedure_id, records) {
        warnings.push(KNOWN_MISSING_TURN_DIRECTION_WARNING.to_string());
    }
    if repair_ktrm_mecca1_missing_left_turn_at_mecca(airport_id, procedure_id, records) {
        warnings.push(KNOWN_MISSING_TURN_DIRECTION_WARNING.to_string());
    }
    warnings
}

fn repair_ktrm_mecca1_missing_left_turn_at_mecca(
    airport_id: &str,
    procedure_id: &str,
    records: &mut [ProcedureLegMaterializationRecord],
) -> bool {
    if airport_id.trim() != "KTRM" || procedure_id.trim() != "MECCA1" {
        return false;
    }
    let mut repaired = false;
    for record in records.iter_mut().filter(|record| {
        record.key.route_type.trim() == "1"
            && record.sequence == 40
            && record.path_termination.trim() == "DF"
            && record_anchor_name(record) == Some("TRM")
            && record
                .turn_direction
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
    }) {
        record.turn_direction = Some("L".to_string());
        repaired = true;
    }
    repaired
}

fn repair_khio_scapo7_rw20_missing_left_turn(
    airport_id: &str,
    procedure_id: &str,
    records: &mut [ProcedureLegMaterializationRecord],
) -> bool {
    if airport_id.trim() != "KHIO" || procedure_id.trim() != "SCAPO7" {
        return false;
    }
    let Some(turn) = records.iter_mut().find(|record| {
        record.key.route_type.trim() == "1"
            && record.key.transition_id.trim() == "RW20"
            && record.sequence == 20
            && record.path_termination.trim() == "VI"
            && record
                .turn_direction
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            && record
                .magnetic_course_deg
                .is_some_and(|course| (course - 270.0).abs() <= 0.1)
    }) else {
        return false;
    };
    turn.turn_direction = Some("L".to_string());
    true
}

fn repair_kjst_rnav_rwy_33_ca_true_course_encoded_as_magnetic(
    airport_id: &str,
    procedure_id: &str,
    records: &mut [ProcedureLegMaterializationRecord],
) -> bool {
    // KJST RNAV RWY 33 is encoded with a CA "magnetic_course" of 324.2,
    // matching the runway true heading instead of the charted APP CRS 334
    // magnetic. Keep the repair narrow: only rewrite the CA row when the
    // preceding WASDO->RW33 TF leg is still present and independently confirms
    // the expected magnetic course.
    if airport_id.trim() != "KJST" || procedure_id.trim() != "R33" {
        return false;
    }

    let Some(wasdo) = records.iter().find(|record| {
        record.key.route_type.trim() == "R"
            && record.key.transition_id.trim().is_empty()
            && record.sequence == 21
            && record.path_termination.trim() == "TF"
            && record_anchor_name(record) == Some("WASDO")
    }) else {
        return false;
    };
    let Some(rw33) = records.iter().find(|record| {
        record.key.route_type.trim() == "R"
            && record.key.transition_id.trim().is_empty()
            && record.sequence == 30
            && record.path_termination.trim() == "TF"
            && record_anchor_name(record) == Some("RW33")
    }) else {
        return false;
    };
    let (Some(wasdo_position), Some(rw33_position), Some(variation_deg)) = (
        wasdo.nav_position,
        rw33.nav_position,
        rw33.airport_magnetic_variation_deg,
    ) else {
        return false;
    };

    let inbound_true_course_deg = initial_course_deg(wasdo_position, rw33_position);
    let inbound_magnetic_course_deg =
        normalize_bearing_degrees(inbound_true_course_deg - variation_deg);
    if angular_difference_degrees(inbound_magnetic_course_deg, 334.0) > 1.0 {
        return false;
    }

    let Some(ca) = records.iter_mut().find(|record| {
        record.key.route_type.trim() == "R"
            && record.key.transition_id.trim().is_empty()
            && record.sequence == 40
            && record.path_termination.trim() == "CA"
    }) else {
        return false;
    };
    if ca.magnetic_course_deg != Some(324.2) {
        return false;
    }

    ca.magnetic_course_deg = Some(inbound_magnetic_course_deg);
    true
}

pub fn acute_turn_ksan_09_family_at_pgy(
    previous_airport_id: &str,
    current_airport_id: &str,
    previous_procedure_id: &str,
    current_procedure_id: &str,
    previous_end_label: &str,
    current_start_label: &str,
) -> bool {
    previous_airport_id.trim() == "KSAN"
        && current_airport_id.trim() == "KSAN"
        && previous_end_label == "PGY"
        && current_start_label == "PGY"
        && matches!(previous_procedure_id, "I09-Y" | "I09-Z" | "L09-Y" | "L09-Z")
        && previous_procedure_id == current_procedure_id
}

pub fn acute_turn_kykm_vora_missed_at_ykm(
    previous_airport_id: &str,
    current_airport_id: &str,
    previous_procedure_id: &str,
    current_procedure_id: &str,
    previous_end_label: &str,
    current_start_label: &str,
    inbound_magnetic_heading_deg: f64,
    outbound_magnetic_heading_deg: f64,
) -> bool {
    previous_airport_id.trim() == "KYKM"
        && current_airport_id.trim() == "KYKM"
        && previous_procedure_id == "VOR-A"
        && current_procedure_id == "VOR-A"
        && previous_end_label == "YKM"
        && current_start_label == "YKM"
        && angular_difference_degrees(inbound_magnetic_heading_deg, 274.0) <= 10.0
        && angular_difference_degrees(outbound_magnetic_heading_deg, 94.0) <= 10.0
}

fn record_anchor_name(record: &ProcedureLegMaterializationRecord) -> Option<&str> {
    match record.nav_ref.as_ref()? {
        crate::NavRef::Airport(code) | crate::NavRef::Navaid(code) | crate::NavRef::Fix(code) => {
            Some(code.as_str())
        }
        crate::NavRef::ArincNavaid { identifier, .. }
        | crate::NavRef::TerminalNavaid { identifier, .. } => Some(identifier.as_str()),
        crate::NavRef::LatLon(_) | crate::NavRef::Spot(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repairs_scapo7_rw20_missing_published_left_turn() {
        let mut records =
            serde_json::from_value::<Vec<ProcedureLegMaterializationRecord>>(serde_json::json!([{
                "key": {
                    "airport_id": "KHIO",
                    "procedure_id": "SCAPO7",
                    "route_type": "1",
                    "transition_id": "RW20",
                },
                "sequence": 20,
                "nav_ref": null,
                "path_termination": "VI",
                "magnetic_course_deg": 270.0,
            }]))
            .expect("test VI row should decode");

        let warnings = repair_known_bad_procedure_fields("KHIO", "SCAPO7", &mut records);

        assert_eq!(records[0].turn_direction.as_deref(), Some("L"));
        assert!(!warnings.is_empty());
    }

    #[test]
    fn repairs_mecca1_missing_published_left_turn_at_mecca() {
        let mut records =
            serde_json::from_value::<Vec<ProcedureLegMaterializationRecord>>(serde_json::json!([{
                "key": {
                    "airport_id": "KTRM",
                    "procedure_id": "MECCA1",
                    "route_type": "1",
                    "transition_id": "RW12",
                },
                "sequence": 40,
                "nav_ref": { "Navaid": "TRM" },
                "path_termination": "DF",
            }]))
            .expect("test DF row should decode");

        let warnings = repair_known_bad_procedure_fields("KTRM", "MECCA1", &mut records);

        assert_eq!(records[0].turn_direction.as_deref(), Some("L"));
        assert_eq!(warnings, [KNOWN_MISSING_TURN_DIRECTION_WARNING]);
    }
}

fn angular_difference_degrees(left: f64, right: f64) -> f64 {
    let mut delta = (normalize_bearing_degrees(left) - normalize_bearing_degrees(right)).abs();
    if delta > 180.0 {
        delta = 360.0 - delta;
    }
    delta
}

fn normalize_bearing_degrees(deg: f64) -> f64 {
    let mut normalized = deg % 360.0;
    if normalized < 0.0 {
        normalized += 360.0;
    }
    normalized
}
