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
    let _ = (airport_id, procedure_id, route_type, transition_id, sequence);
    turn_to_pi_outbound_deg >= 150.0
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
