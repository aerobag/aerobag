import type {
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

function pathTerminationKind(code: string): ProcedureLegMaterializationRecord["path_termination_kind"] {
  switch (code.trim()) {
    case "IF":
      return "initial_fix";
    case "TF":
      return "track_to_fix";
    case "CF":
      return "course_to_fix";
    case "DF":
      return "direct_to_fix";
    case "FM":
    case "HM":
      return "heading_to_manual";
    case "VA":
    case "VI":
      return "heading_to_altitude";
    default:
      return { other: code.trim() };
  }
}

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
    path_termination_kind: string;
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
      path_termination_kind: pathTerminationKind(row.path_termination),
    })),
  );
}
