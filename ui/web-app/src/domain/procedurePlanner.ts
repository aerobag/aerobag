import type {
  CifpTppMatchRow,
  NavRef,
  ProcedureDistinctRow,
  ProcedureKind,
  ProcedureLegMaterializationRecord,
  ProcedureSummary,
} from "./types";
import { getBrowserNavDb } from "./webNavDb";

type IdentifierRow = {
  LocationID: string;
};

function inferProcedureKind(routeType: string): ProcedureKind {
  switch (routeType.trim()) {
    case "1":
    case "2":
    case "3":
      return "star";
    case "4":
    case "5":
    case "6":
      return "sid";
    default:
      return "approach";
  }
}

async function navRefForProcedureIdentifier(identifier: string): Promise<NavRef | null> {
  const trimmed = identifier.trim();
  if (trimmed === "") {
    return null;
  }
  if (/^RW/i.test(trimmed)) {
    return { Fix: trimmed };
  }

  const db = await getBrowserNavDb();
  if (db.queryObjects<IdentifierRow>("SELECT LocationID FROM airports WHERE trim(LocationID) = trim(?1) LIMIT 1", [trimmed]).length > 0) {
    return { Airport: trimmed };
  }
  if (db.queryObjects<IdentifierRow>("SELECT LocationID FROM nav WHERE trim(LocationID) = trim(?1) LIMIT 1", [trimmed]).length > 0) {
    return { Navaid: trimmed };
  }
  if (db.queryObjects<IdentifierRow>("SELECT LocationID FROM fix WHERE trim(LocationID) = trim(?1) LIMIT 1", [trimmed]).length > 0) {
    return { Fix: trimmed };
  }
  return null;
}

export async function listProceduresForAirport(
  airportId: string,
  kind: ProcedureKind,
): Promise<ProcedureSummary[]> {
  const db = await getBrowserNavDb();
  const rows = db.queryObjects<{ airport_id: string; procedure_id: string; route_type: string }>(
    `
      SELECT DISTINCT
        trim(airport_identifier) AS airport_id,
        trim(sid_star_approach_identifier) AS procedure_id,
        trim(route_type) AS route_type
      FROM cifp_sid_star_app
      WHERE trim(airport_identifier) = trim(?1)
      ORDER BY trim(sid_star_approach_identifier), trim(route_type)
    `,
    [airportId],
  );

  const deduped = new Map<string, ProcedureSummary>();
  for (const row of rows) {
    const inferredKind = inferProcedureKind(row.route_type);
    if (inferredKind !== kind) {
      continue;
    }
    deduped.set(row.procedure_id, {
      airport_id: row.airport_id,
      procedure_id: row.procedure_id,
      kind: inferredKind,
    });
  }
  return [...deduped.values()].sort((left, right) => left.procedure_id.localeCompare(right.procedure_id));
}

export async function loadProcedureDistinctRows(
  airportId: string,
  procedureId: string,
): Promise<ProcedureDistinctRow[]> {
  const db = await getBrowserNavDb();
  return db.queryObjects<ProcedureDistinctRow>(
    `
      SELECT DISTINCT
        trim(route_type) AS route_type,
        trim(transition_identifier) AS transition_id
      FROM cifp_sid_star_app
      WHERE trim(airport_identifier) = trim(?1)
        AND trim(sid_star_approach_identifier) = trim(?2)
      ORDER BY trim(route_type), trim(transition_identifier)
    `,
    [airportId, procedureId],
  );
}

export async function loadProcedureMaterializationRecords(
  airportId: string,
  procedureId: string,
): Promise<ProcedureLegMaterializationRecord[]> {
  const db = await getBrowserNavDb();
  const rows = db.queryObjects<{
    airport_id: string;
    procedure_id: string;
    route_type: string;
    transition_id: string;
    sequence: number;
    fix_identifier: string;
    path_termination: string;
  }>(
    `
      SELECT
        trim(airport_identifier) AS airport_id,
        trim(sid_star_approach_identifier) AS procedure_id,
        trim(route_type) AS route_type,
        trim(transition_identifier) AS transition_id,
        CAST(sequence_number AS INTEGER) AS sequence,
        trim(fix_identifier) AS fix_identifier,
        trim(path_and_termination) AS path_termination
      FROM cifp_sid_star_app
      WHERE trim(airport_identifier) = trim(?1)
        AND trim(sid_star_approach_identifier) = trim(?2)
      ORDER BY trim(route_type), trim(transition_identifier), CAST(sequence_number AS INTEGER)
    `,
    [airportId, procedureId],
  );

  return Promise.all(
    rows.map(async (row) => ({
      key: {
        airport_id: row.airport_id,
        procedure_id: row.procedure_id,
        route_type: row.route_type,
        transition_id: row.transition_id,
      },
      sequence: row.sequence,
      nav_ref: await navRefForProcedureIdentifier(row.fix_identifier),
      path_termination: row.path_termination,
    })),
  );
}

export async function loadCifpTppMatchesForProcedure(
  airportId: string,
  cifpId: string,
): Promise<CifpTppMatchRow[]> {
  const db = await getBrowserNavDb();
  return db.queryObjects<CifpTppMatchRow>(
    `
      SELECT
        trim(airport_id) AS airport_id,
        trim(cifp_id) AS cifp_id,
        trim(plate_id) AS plate_id,
        trim(plate_label) AS plate_label,
        trim(package_id) AS package_id,
        CAST(public AS INTEGER) AS public,
        CAST(priority AS INTEGER) AS priority,
        trim(match_kind) AS match_kind,
        CAST(is_primary AS INTEGER) AS is_primary
      FROM cifp_tpp_matches
      WHERE trim(airport_id) = trim(?1)
        AND trim(cifp_id) = trim(?2)
      ORDER BY CAST(is_primary AS INTEGER) DESC, CAST(priority AS INTEGER), trim(plate_label)
    `,
    [airportId, cifpId],
  );
}

export async function loadCifpTppMatchesForPlate(
  plateId: string,
): Promise<CifpTppMatchRow[]> {
  const db = await getBrowserNavDb();
  return db.queryObjects<CifpTppMatchRow>(
    `
      SELECT
        trim(airport_id) AS airport_id,
        trim(cifp_id) AS cifp_id,
        trim(plate_id) AS plate_id,
        trim(plate_label) AS plate_label,
        trim(package_id) AS package_id,
        CAST(public AS INTEGER) AS public,
        CAST(priority AS INTEGER) AS priority,
        trim(match_kind) AS match_kind,
        CAST(is_primary AS INTEGER) AS is_primary
      FROM cifp_tpp_matches
      WHERE trim(plate_id) = trim(?1)
      ORDER BY trim(cifp_id), CAST(is_primary AS INTEGER) DESC, CAST(priority AS INTEGER), trim(plate_label)
    `,
    [plateId],
  );
}
