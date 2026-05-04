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
    if turn_to_pi_outbound_deg <= 120.0 {
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
    // KILE VOR-A/SLIMM is the motivating case: the A-route arrives at GRK
    // almost inbound on the later charted hold, then the next ARINC row is a PI
    // course reversal whose initial outbound course is nearly reciprocal. ARINC
    // does encode the GRK hold, but only later in the missed route. There is no
    // official guidance here; we are choosing to borrow that same-fix hold entry
    // only when the direct arrival-to-PI-outbound turn would be absurdly sharp.
    //
    // DARTE on the same plate needs the PI but only turns about 67 degrees to
    // outbound, so the threshold must not convert ordinary PI entries into
    // borrowed holds. We chose 150 degrees as the line between "just turn
    // outbound" and "use the charted same-fix hold to reverse first."
    let _ = (
        airport_id,
        procedure_id,
        route_type,
        transition_id,
        sequence,
    );
    turn_to_pi_outbound_deg >= 150.0
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
