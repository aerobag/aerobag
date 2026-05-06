use crate::{PathTermination, ProcedureDiscontinuity, ProcedureLegMaterializationRecord};

pub fn interpret_path_termination(code: &str) -> PathTermination {
    match code.trim() {
        "IF" => PathTermination::InitialFix,
        "TF" => PathTermination::TrackToFix,
        "CF" => PathTermination::CourseToFix,
        "DF" => PathTermination::DirectToFix,
        "FM" | "HF" | "HM" => PathTermination::HeadingToManual,
        "VA" | "VI" => PathTermination::HeadingToAltitude,
        other => PathTermination::Other(other.to_string()),
    }
}

pub fn parse_cifp_tenths_value(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = trimmed.parse::<f64>().ok()?;
    Some(parsed / 10.0)
}

pub fn parse_cifp_hundredths_value(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = trimmed.parse::<f64>().ok()?;
    Some(parsed / 100.0)
}

pub fn parse_cifp_thousandths_value(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = trimmed.parse::<f64>().ok()?;
    Some(parsed / 1000.0)
}

pub fn parse_cifp_altitude_ft(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<f64>().ok()
}

pub fn parse_airport_magnetic_variation(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (magnitude_text, suffix) = trimmed.split_at(trimmed.len().saturating_sub(1));
    match suffix {
        "E" => magnitude_text.parse::<f64>().ok(),
        "W" => magnitude_text.parse::<f64>().ok().map(|degrees| -degrees),
        _ => trimmed.parse::<f64>().ok(),
    }
}

pub fn terminal_procedure_discontinuity(
    leg: &ProcedureLegMaterializationRecord,
) -> Option<ProcedureDiscontinuity> {
    match leg.path_termination.trim() {
        "FM" => Some(ProcedureDiscontinuity::Vectors),
        "HF" | "HM" => Some(ProcedureDiscontinuity::Hold),
        "VA" | "VI" if leg.nav_ref.is_none() => Some(ProcedureDiscontinuity::Vectors),
        _ => None,
    }
}

pub fn leading_procedure_discontinuity(
    leg: &ProcedureLegMaterializationRecord,
) -> Option<ProcedureDiscontinuity> {
    match leg.path_termination.trim() {
        "FM" => Some(ProcedureDiscontinuity::Vectors),
        "HF" | "HM" => Some(ProcedureDiscontinuity::Hold),
        "VA" | "VI" if leg.nav_ref.is_none() => Some(ProcedureDiscontinuity::Vectors),
        _ => None,
    }
}
