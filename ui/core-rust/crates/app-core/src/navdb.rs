use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppErrorKind, AppResult};
use crate::geometry::LatLon;
use crate::planning::{
    interpret_path_termination, AirwaySegment, NavRef, PathTermination, ProcedureKind,
    ResolvedLeg, ResolvedLegSource,
};

const MAX_AIRWAY_BRANCH_HOP_NM: f64 = 500.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayPoint {
    pub airway_name: String,
    pub sequence: i32,
    pub position: LatLon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayFixPoint {
    pub airway_name: String,
    pub sequence: i32,
    pub position: LatLon,
    pub nav_ref: NavRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirwayBranch {
    pub display_name: String,
    pub branch_key: String,
    pub points: Vec<AirwayFixPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureVariantKey {
    pub airport_id: String,
    pub procedure_id: String,
    pub route_type: String,
    pub transition_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcedureLegRecord {
    pub key: ProcedureVariantKey,
    pub sequence: i32,
    pub fix_identifier: String,
    pub path_termination: String,
    pub path_termination_kind: PathTermination,
    pub inferred_kind: ProcedureKind,
}

pub fn load_airway_points(db_path: &Path, airway_name: &str) -> AppResult<Vec<AirwayPoint>> {
    with_connection(db_path, |connection| {
        let mut stmt = connection.prepare(
            "SELECT trim(name), CAST(sequence AS INTEGER), Latitude, Longitude
             FROM airways
             WHERE trim(name) = trim(?1)
             ORDER BY CAST(sequence AS INTEGER)",
        )?;

        let rows = stmt.query_map(params![airway_name], |row| {
            Ok(AirwayPoint {
                airway_name: row.get::<_, String>(0)?,
                sequence: row.get::<_, i32>(1)?,
                position: LatLon {
                    lat: row.get::<_, f64>(2)?,
                    lon: row.get::<_, f64>(3)?,
                },
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })
}

pub fn load_airway_branches(db_path: &Path, airway_name: &str) -> AppResult<Vec<AirwayBranch>> {
    if let Some(branches) =
        with_connection(db_path, |connection| load_airway_branches_from_branch_table(connection, airway_name))?
    {
        return Ok(branches);
    }

    let points = load_airway_points(db_path, airway_name)?;
    with_connection(db_path, |connection| {
        let named_points = points
            .into_iter()
            .map(|point| resolve_airway_fix_point(connection, point))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(split_airway_branches(named_points))
    })
}

pub fn select_airway_branch(
    db_path: &Path,
    airway_name: &str,
    entry: &NavRef,
    exit: &NavRef,
) -> AppResult<AirwayBranch> {
    let branches = load_airway_branches(db_path, airway_name)?;
    let matching = branches
        .into_iter()
        .filter(|branch| {
            branch.points.iter().any(|point| &point.nav_ref == entry)
                && branch.points.iter().any(|point| &point.nav_ref == exit)
        })
        .collect::<Vec<_>>();

    match matching.as_slice() {
        [branch] => Ok(branch.clone()),
        [] => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: format!(
                "no airway branch for {} contains both entry and exit",
                airway_name.trim()
            ),
        }),
        _ => Err(AppError {
            kind: AppErrorKind::UnsupportedOperation,
            message: format!(
                "multiple airway branches for {} contain the selected entry and exit",
                airway_name.trim()
            ),
        }),
    }
}

pub fn resolve_airway_segment(
    db_path: &Path,
    airway_name: &str,
    entry: &NavRef,
    exit: &NavRef,
    component_index: usize,
) -> AppResult<(AirwaySegment, Vec<ResolvedLeg>)> {
    let branch = select_airway_branch(db_path, airway_name, entry, exit)?;
    let entry_index = branch
        .points
        .iter()
        .position(|point| &point.nav_ref == entry)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "selected airway entry point is not on branch".to_string(),
        })?;
    let exit_index = branch
        .points
        .iter()
        .position(|point| &point.nav_ref == exit)
        .ok_or_else(|| AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "selected airway exit point is not on branch".to_string(),
        })?;

    if entry_index == exit_index {
        return Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message: "airway entry and exit cannot be the same point".to_string(),
        });
    }

    let slice = if entry_index < exit_index {
        &branch.points[entry_index..=exit_index]
    } else {
        &branch.points[exit_index..=entry_index]
    };

    let traversed = if entry_index < exit_index {
        slice.to_vec()
    } else {
        slice.iter().rev().cloned().collect::<Vec<_>>()
    };

    let legs = traversed
        .windows(2)
        .enumerate()
        .map(|(index, pair)| ResolvedLeg {
            id: format!("airway-{}-{index}", branch.branch_key),
            from: pair[0].nav_ref.clone(),
            to: pair[1].nav_ref.clone(),
            source: ResolvedLegSource::RouteComponent { component_index },
        })
        .collect::<Vec<_>>();

    Ok((
        AirwaySegment {
            name: branch.display_name,
            branch_key: Some(branch.branch_key),
            entry: entry.clone(),
            exit: exit.clone(),
        },
        legs,
    ))
}

pub fn load_procedure_legs(
    db_path: &Path,
    key: &ProcedureVariantKey,
) -> AppResult<Vec<ProcedureLegRecord>> {
    with_connection(db_path, |connection| {
        let mut stmt = connection.prepare(
            "SELECT
                trim(airport_identifier),
                trim(sid_star_approach_identifier),
                trim(route_type),
                trim(transition_identifier),
                CAST(sequence_number AS INTEGER),
                trim(fix_identifier),
                trim(path_and_termination)
             FROM cifp_sid_star_app
             WHERE trim(airport_identifier) = trim(?1)
               AND trim(sid_star_approach_identifier) = trim(?2)
               AND trim(route_type) = trim(?3)
               AND trim(transition_identifier) = trim(?4)
             ORDER BY CAST(sequence_number AS INTEGER)",
        )?;

        let rows = stmt.query_map(
            params![
                key.airport_id,
                key.procedure_id,
                key.route_type,
                key.transition_id
            ],
            |row| {
                let route_type = row.get::<_, String>(2)?;
                let path_termination = row.get::<_, String>(6)?;
                Ok(ProcedureLegRecord {
                    key: ProcedureVariantKey {
                        airport_id: row.get::<_, String>(0)?,
                        procedure_id: row.get::<_, String>(1)?,
                        route_type: route_type.clone(),
                        transition_id: row.get::<_, String>(3)?,
                    },
                    sequence: row.get::<_, i32>(4)?,
                    fix_identifier: row.get::<_, String>(5)?,
                    path_termination_kind: interpret_path_termination(&path_termination),
                    path_termination,
                    inferred_kind: infer_procedure_kind(&route_type),
                })
            },
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
    })
}

pub fn load_resolved_procedure_legs(
    db_path: &Path,
    key: &ProcedureVariantKey,
    component_index: usize,
) -> AppResult<Vec<ResolvedLeg>> {
    let fixes: Vec<ProcedureLegRecord> = load_procedure_legs(db_path, key)?
        .into_iter()
        .filter(|leg| !leg.fix_identifier.is_empty())
        .collect();

    Ok(fixes
        .windows(2)
        .enumerate()
        .map(|(index, pair)| ResolvedLeg {
            id: format!(
                "procedure-{}-{}-{}",
                key.procedure_id.trim(),
                key.route_type.trim(),
                index
            ),
            from: NavRef::Fix(pair[0].fix_identifier.clone()),
            to: NavRef::Fix(pair[1].fix_identifier.clone()),
            source: ResolvedLegSource::RouteComponent { component_index },
        })
        .collect())
}

fn resolve_airway_fix_point(
    connection: &Connection,
    point: AirwayPoint,
) -> rusqlite::Result<AirwayFixPoint> {
    let nav_ref = resolve_named_nav_ref(connection, point.position)?
        .unwrap_or(NavRef::LatLon(point.position));

    Ok(AirwayFixPoint {
        airway_name: point.airway_name,
        sequence: point.sequence,
        position: point.position,
        nav_ref,
    })
}

fn load_airway_branches_from_branch_table(
    connection: &Connection,
    airway_name: &str,
) -> rusqlite::Result<Option<Vec<AirwayBranch>>> {
    if !table_exists(connection, "airways_branch")? {
        return Ok(None);
    }

    let mut stmt = connection.prepare(
        "SELECT
            trim(name),
            trim(branch_key),
            sequence_number,
            trim(point_name),
            Latitude,
            Longitude
         FROM airways_branch
         WHERE trim(name) = trim(?1)
         ORDER BY trim(branch_key), sequence_number",
    )?;

    let rows = stmt
        .query_map(params![airway_name], |row| {
            let position = LatLon {
                lat: row.get::<_, f64>(4)?,
                lon: row.get::<_, f64>(5)?,
            };
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, String>(3)?,
                position,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if rows.is_empty() {
        return Ok(Some(Vec::new()));
    }

    let mut branches = Vec::new();
    let mut current_key = None::<String>;
    let mut current_points = Vec::<AirwayFixPoint>::new();
    let mut display_name = String::new();

    for (name, branch_key, sequence_number, point_name, position) in rows {
        if current_key.as_deref() != Some(branch_key.as_str()) && !current_points.is_empty() {
            branches.push(AirwayBranch {
                display_name: display_name.clone(),
                branch_key: current_key.take().unwrap_or_default(),
                points: std::mem::take(&mut current_points),
            });
        }

        if current_key.is_none() || current_key.as_deref() != Some(branch_key.as_str()) {
            display_name = name.clone();
            current_key = Some(branch_key.clone());
        }

        let nav_ref = resolve_named_nav_ref(connection, position)?
            .unwrap_or_else(|| fallback_nav_ref(&point_name, position));
        current_points.push(AirwayFixPoint {
            airway_name: name,
            sequence: sequence_number,
            position,
            nav_ref,
        });
    }

    if !current_points.is_empty() {
        branches.push(AirwayBranch {
            display_name,
            branch_key: current_key.unwrap_or_default(),
            points: current_points,
        });
    }

    Ok(Some(branches))
}

fn fallback_nav_ref(point_name: &str, position: LatLon) -> NavRef {
    let trimmed = point_name.trim();
    if trimmed.is_empty() {
        NavRef::LatLon(position)
    } else {
        NavRef::Fix(trimmed.to_string())
    }
}

fn table_exists(connection: &Connection, table_name: &str) -> rusqlite::Result<bool> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1 LIMIT 1",
            params![table_name],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
}

fn resolve_named_nav_ref(
    connection: &Connection,
    position: LatLon,
) -> rusqlite::Result<Option<NavRef>> {
    for (table, variant) in [
        ("fix", 0usize),
        ("nav", 1usize),
        ("airports", 2usize),
    ] {
        let mut stmt = connection.prepare(&format!(
            "SELECT trim(LocationID) FROM {table}
             WHERE abs(ARPLatitude - ?1) < 1e-6
               AND abs(ARPLongitude - ?2) < 1e-6
             LIMIT 1"
        ))?;

        if let Some(id) = stmt
            .query_row(params![position.lat, position.lon], |row| row.get::<_, String>(0))
            .optional()?
        {
            return Ok(Some(match variant {
                0 => NavRef::Fix(id),
                1 => NavRef::Navaid(id),
                _ => NavRef::Airport(id),
            }));
        }
    }

    Ok(None)
}

fn split_airway_branches(points: Vec<AirwayFixPoint>) -> Vec<AirwayBranch> {
    let mut branches: Vec<Vec<AirwayFixPoint>> = Vec::new();

    for point in points {
        let mut assigned_branch = None;
        let mut best_distance = f64::MAX;

        for (branch_index, branch) in branches.iter().enumerate() {
            let Some(last) = branch.last() else {
                continue;
            };

            if point.sequence < last.sequence {
                continue;
            }

            let distance = distance_nm(last.position, point.position);
            if distance <= MAX_AIRWAY_BRANCH_HOP_NM && distance < best_distance {
                assigned_branch = Some(branch_index);
                best_distance = distance;
            }
        }

        if let Some(branch_index) = assigned_branch {
            branches[branch_index].push(point);
        } else {
            branches.push(vec![point]);
        }
    }

    branches
        .into_iter()
        .enumerate()
        .map(|(index, points)| AirwayBranch {
            display_name: points
                .first()
                .map(|point| point.airway_name.clone())
                .unwrap_or_default(),
            branch_key: format!(
                "{}-{}",
                points
                    .first()
                    .map(|point| point.airway_name.as_str())
                    .unwrap_or("AWY"),
                branch_suffix(index)
            ),
            points,
        })
        .collect()
}

fn distance_nm(a: LatLon, b: LatLon) -> f64 {
    let earth_radius_nm = 3440.065_f64;
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * earth_radius_nm * h.sqrt().asin()
}

fn branch_suffix(index: usize) -> char {
    (b'A' + (index as u8)) as char
}

fn with_connection<T>(
    db_path: &Path,
    f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
) -> AppResult<T> {
    let connection = Connection::open(db_path).map_err(sqlite_error)?;
    f(&connection).map_err(sqlite_error)
}

fn sqlite_error(err: rusqlite::Error) -> AppError {
    AppError {
        kind: AppErrorKind::InvalidFlightPlan,
        message: format!("nav database query failed: {err}"),
    }
}

fn infer_procedure_kind(route_type: &str) -> ProcedureKind {
    match route_type.trim() {
        "1" | "2" | "3" => ProcedureKind::Star,
        "4" | "5" | "6" => ProcedureKind::Sid,
        _ => ProcedureKind::Approach,
    }
}

trait OptionalRow<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalRow<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_db_path() -> &'static Path {
        Path::new("/root/aerobag-three/aerobag/ui/android-app/app/src/main/assets/nav-db/main.db")
    }

    #[test]
    fn loads_airway_points_from_real_nav_db() {
        let points = load_airway_points(fixture_db_path(), "V16").unwrap();

        assert!(points.len() > 50);
        assert_eq!(points[0].airway_name, "V16");
        assert!(points.windows(2).all(|pair| pair[0].sequence <= pair[1].sequence));
        assert!(points.windows(2).any(|pair| pair[0].sequence == pair[1].sequence));
    }

    #[test]
    fn splits_v16_into_multiple_internal_branches() {
        let branches = load_airway_branches(fixture_db_path(), "V16").unwrap();

        assert!(branches.len() >= 2);
        assert_eq!(branches[0].display_name, "V16");
        assert_eq!(branches[0].branch_key, "V16-A");
        assert!(branches.iter().all(|branch| branch.points.len() > 10));
    }

    #[test]
    fn resolves_named_airway_branch_from_entry_and_exit() {
        let (segment, legs) = resolve_airway_segment(
            fixture_db_path(),
            "V16",
            &NavRef::Navaid("LAX".to_string()),
            &NavRef::Navaid("PDZ".to_string()),
            1,
        )
        .unwrap();

        assert_eq!(segment.name, "V16");
        assert_eq!(segment.branch_key.as_deref(), Some("V16-A"));
        assert!(!legs.is_empty());
        assert_eq!(legs.first().unwrap().from, NavRef::Navaid("LAX".to_string()));
        assert_eq!(legs.last().unwrap().to, NavRef::Navaid("PDZ".to_string()));
    }

    #[test]
    fn loads_sid_like_procedure_variant_from_real_nav_db() {
        let legs = load_procedure_legs(
            fixture_db_path(),
            &ProcedureVariantKey {
                airport_id: "KBOS".to_string(),
                procedure_id: "BLZZR6".to_string(),
                route_type: "4".to_string(),
                transition_id: "RW04R".to_string(),
            },
        )
        .unwrap();

        assert!(!legs.is_empty());
        assert_eq!(legs[0].inferred_kind, ProcedureKind::Sid);
        assert_eq!(legs[0].path_termination_kind, PathTermination::HeadingToAltitude);
        assert_eq!(legs[1].fix_identifier, "NHANT");
    }

    #[test]
    fn loads_approach_like_procedure_variant_from_real_nav_db() {
        let legs = load_procedure_legs(
            fixture_db_path(),
            &ProcedureVariantKey {
                airport_id: "KBOS".to_string(),
                procedure_id: "I04R".to_string(),
                route_type: "I".to_string(),
                transition_id: "".to_string(),
            },
        )
        .unwrap();

        assert!(!legs.is_empty());
        assert_eq!(legs[0].inferred_kind, ProcedureKind::Approach);
        assert_eq!(legs[0].fix_identifier, "WINNI");
        assert_eq!(legs[0].path_termination_kind, PathTermination::InitialFix);
    }

    #[test]
    fn resolves_procedure_rows_into_fix_to_fix_legs() {
        let legs = load_resolved_procedure_legs(
            fixture_db_path(),
            &ProcedureVariantKey {
                airport_id: "KBOS".to_string(),
                procedure_id: "I04R".to_string(),
                route_type: "I".to_string(),
                transition_id: "".to_string(),
            },
            3,
        )
        .unwrap();

        assert!(!legs.is_empty());
        assert_eq!(legs[0].from, NavRef::Fix("WINNI".to_string()));
        assert_eq!(legs[0].to, NavRef::Fix("NABBO".to_string()));
        assert_eq!(
            legs[0].source,
            ResolvedLegSource::RouteComponent { component_index: 3 }
        );
    }
}
