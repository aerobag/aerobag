import type {
  AirwayAutoSelection,
  AirwayBranch,
  AirwayEntryCandidate,
  AirwayExitCandidate,
  AirwayFixPoint,
  AirwayPresentationPlan,
  AirwaySegment,
  AirwaySuggestion,
  LatLon,
  NavRef,
  ResolvedLeg,
} from "./types";
import type { AppCoreAdapter } from "./appCoreAdapter";
import { getBrowserNavDb } from "./webNavDb";
import { debugLog } from "./debugLog";

type PositionRow = {
  lat: number;
  lon: number;
};

type AirwayPointRow = {
  name: string;
  branch_key: string;
  sequence_number: number;
  point_name: string;
  Latitude: number;
  Longitude: number;
};

type IdentifierRow = {
  LocationID: string;
};

const identifierCache = new Map<string, NavRef>();

export async function suggestAirwaysNearAnchor(
  adapter: AppCoreAdapter,
  anchor: NavRef,
  limit = 5,
): Promise<AirwaySuggestion[]> {
  const startMs = performance.now();
  debugLog("airway.suggest.start", { anchor, limit });
  const db = await getBrowserNavDb();
  const anchorPos = await resolveNavRefPosition(anchor);
  const seen = new Map<string, AirwaySuggestion>();

  for (const radius of [1.0, 2.0, 4.0, 8.0]) {
    const points = db.queryObjects<AirwayPointRow>(
      `
        SELECT name, branch_key, sequence_number, point_name, Latitude, Longitude
        FROM airways_branch
        WHERE Latitude BETWEEN ?1 AND ?2
          AND Longitude BETWEEN ?3 AND ?4
        ORDER BY ((Latitude - ?5) * (Latitude - ?5)) + ((Longitude - ?6) * (Longitude - ?6))
        LIMIT 400
      `,
      [
        anchorPos.lat - radius,
        anchorPos.lat + radius,
        anchorPos.lon - radius,
        anchorPos.lon + radius,
        anchorPos.lat,
        anchorPos.lon,
      ],
    );

    for (const point of points) {
      const navRef = await resolvePointNavRef(point);
      const distanceNm = distanceNmBetween(anchorPos, { lat: point.Latitude, lon: point.Longitude });
      const existing = seen.get(point.name);
      if (!existing || distanceNm < existing.distance_from_anchor_nm) {
        seen.set(point.name, {
          airway_name: point.name,
          nearest_branch_key: point.branch_key,
          nearest_nav_ref: navRef,
          nearest_sequence: point.sequence_number,
          distance_from_anchor_nm: distanceNm,
        });
      }
    }

    if (seen.size >= limit) {
      break;
    }
  }

  const sorted = await adapter.sortAirwaySuggestionsForUi([...seen.values()]);
  const admittedPointKeys = new Set<string>();
  for (const suggestion of sorted) {
    admittedPointKeys.add(navRefKey(suggestion.nearest_nav_ref));
    if (admittedPointKeys.size >= limit) {
      break;
    }
  }
  const suggestions = sorted.filter((suggestion) => admittedPointKeys.has(navRefKey(suggestion.nearest_nav_ref)));
  debugLog("airway.suggest.done", {
    anchor,
    limit,
    suggestions: suggestions.length,
    admitted_points: admittedPointKeys.size,
    elapsed_ms: Math.round(performance.now() - startMs),
  });
  return suggestions;
}

export async function prepareAirwayPresentationForAnchors(
  adapter: AppCoreAdapter,
  airwayName: string,
  originAnchor: NavRef,
  destinationAnchor: NavRef | null,
): Promise<AirwayPresentationPlan> {
  const branches = await loadAirwayBranches(airwayName);
  const originPosition = await resolveNavRefPosition(originAnchor);
  const destinationPosition = destinationAnchor ? await resolveNavRefPosition(destinationAnchor) : null;
  return adapter.prepareAirwayPresentation(
    airwayName,
    branches,
    originPosition,
    destinationPosition,
  );
}

export function airwayEntryCandidateFromPresentation(
  presentation: AirwayPresentationPlan,
  pointIndex: number,
): AirwayEntryCandidate {
  const point = presentation.points[pointIndex];
  if (!point) {
    throw new Error(`airway entry index out of range: ${pointIndex}`);
  }
  return {
    airway_name: presentation.airway_name,
    branch_key: presentation.branch_key,
    branch_point_index: point.branch_point_index,
    sequence: point.sequence,
    nav_ref: point.nav_ref,
    distance_from_anchor_nm: 0,
    previous_nav_ref: pointIndex > 0 ? presentation.points[pointIndex - 1]?.nav_ref ?? null : null,
    next_nav_ref: pointIndex + 1 < presentation.points.length ? presentation.points[pointIndex + 1]?.nav_ref ?? null : null,
  };
}

export function airwayExitCandidatesFromPresentation(
  presentation: AirwayPresentationPlan,
  entryIndex: number,
): AirwayExitCandidate[] {
  return presentation.points.map((point, pointIndex) => ({
    airway_name: presentation.airway_name,
    branch_key: presentation.branch_key,
    branch_point_index: point.branch_point_index,
    sequence: point.sequence,
    nav_ref: point.nav_ref,
    leg_offset_from_entry: pointIndex - entryIndex,
    is_entry: pointIndex === entryIndex,
    distance_from_target_nm: null,
  }));
}

export async function materializeAirwaySelection(
  startComponentIndex: number,
  entry: AirwayEntryCandidate,
  exit: AirwayExitCandidate,
  originAnchor: NavRef,
  destinationAnchor: NavRef | null,
): Promise<{
  selection: AirwayAutoSelection;
  airway: AirwaySegment;
  resolvedLegs: ResolvedLeg[];
}> {
  const startMs = performance.now();
  debugLog("airway.materialize.start", {
    airway_name: entry.airway_name,
    branch_key: entry.branch_key,
    entry: entry.nav_ref,
    exit: exit.nav_ref,
  });
  const db = await getBrowserNavDb();
  const points = db.queryObjects<AirwayPointRow>(
    `
      SELECT name, branch_key, sequence_number, point_name, Latitude, Longitude
      FROM airways_branch
      WHERE name = ?1 AND branch_key = ?2
      ORDER BY sequence_number
    `,
    [entry.airway_name, entry.branch_key],
  );

  const firstIndex = entry.branch_point_index;
  const lastIndex = exit.branch_point_index;
  if (firstIndex === lastIndex) {
    throw new Error("airway entry and exit must differ");
  }

  const slice = firstIndex < lastIndex
    ? points.slice(firstIndex, lastIndex + 1)
    : points.slice(lastIndex, firstIndex + 1).reverse();

  const navRefs = await Promise.all(slice.map((point) => resolvePointNavRef(point)));
  const resolvedLegs: ResolvedLeg[] = [];
  for (let index = 0; index + 1 < navRefs.length; index += 1) {
    resolvedLegs.push({
      id: `airway:${entry.airway_name}:${entry.branch_key}:${index}`,
      from: navRefs[index],
      to: navRefs[index + 1],
      source: {
        kind: "route_component",
        component_index: startComponentIndex + 1,
      },
    });
  }

  const originPos = await resolveNavRefPosition(originAnchor);
  const entryPos = await resolveNavRefPosition(entry.nav_ref);
  const exitPos = await resolveNavRefPosition(exit.nav_ref);
  const originDistanceNm = distanceNmBetween(originPos, entryPos);
  const destinationDistanceNm = destinationAnchor
    ? distanceNmBetween(await resolveNavRefPosition(destinationAnchor), exitPos)
    : 0;

  const materialized = {
    selection: {
      airway_name: entry.airway_name,
      branch_key: entry.branch_key,
      entry,
      exit,
      origin_distance_nm: originDistanceNm,
      destination_distance_nm: destinationDistanceNm,
      total_anchor_distance_nm: originDistanceNm + destinationDistanceNm,
    },
    airway: {
      name: entry.airway_name,
      branch_key: entry.branch_key,
      entry: entry.nav_ref,
      exit: exit.nav_ref,
    },
    resolvedLegs,
  };
  debugLog("airway.materialize.done", {
    airway_name: entry.airway_name,
    branch_key: entry.branch_key,
    resolved_legs: resolvedLegs.length,
    elapsed_ms: Math.round(performance.now() - startMs),
  });
  return materialized;
}

async function resolveRunwayFixPosition(runwayFix: string, airportId?: string | null): Promise<LatLon | null> {
  const runwayIdent = runwayFix.trim().replace(/^RW/i, "");
  if (!runwayIdent) {
    return null;
  }

  const db = await getBrowserNavDb();
  const params = airportId ? [airportId, runwayIdent, runwayIdent] : [runwayIdent, runwayIdent];
  const sql = airportId
    ? `
      SELECT
        CASE
          WHEN trim(LEIdent) = trim(?2) THEN CAST(LELatitude AS REAL)
          ELSE CAST(HELatitude AS REAL)
        END AS lat,
        CASE
          WHEN trim(LEIdent) = trim(?2) THEN CAST(LELongitude AS REAL)
          ELSE CAST(HELongitude AS REAL)
        END AS lon
      FROM airportrunways
      WHERE trim(LocationID) = trim(?1)
        AND (trim(LEIdent) = trim(?2) OR trim(HEIdent) = trim(?3))
      LIMIT 1
    `
    : `
      SELECT
        CASE
          WHEN trim(LEIdent) = trim(?1) THEN CAST(LELatitude AS REAL)
          ELSE CAST(HELatitude AS REAL)
        END AS lat,
        CASE
          WHEN trim(LEIdent) = trim(?1) THEN CAST(LELongitude AS REAL)
          ELSE CAST(HELongitude AS REAL)
        END AS lon
      FROM airportrunways
      WHERE trim(LEIdent) = trim(?1) OR trim(HEIdent) = trim(?2)
      LIMIT 1
    `;
  return db.queryObjects<PositionRow>(sql, params)[0] ?? null;
}

export async function resolveNavRefPosition(navRef: NavRef, procedureAirportId?: string | null): Promise<LatLon> {
  if ("LatLon" in navRef) {
    return navRef.LatLon;
  }

  const db = await getBrowserNavDb();
  if ("Airport" in navRef) {
    const row = db.queryObjects<PositionRow>(
      "SELECT ARPLatitude AS lat, ARPLongitude AS lon FROM airports WHERE LocationID = ?1 LIMIT 1",
      [navRef.Airport],
    )[0];
    if (!row) {
      throw new Error(`unknown airport ${navRef.Airport}`);
    }
    return row;
  }
  if ("Navaid" in navRef) {
    const row = db.queryObjects<PositionRow>(
      "SELECT ARPLatitude AS lat, ARPLongitude AS lon FROM nav WHERE LocationID = ?1 LIMIT 1",
      [navRef.Navaid],
    )[0];
    if (!row) {
      throw new Error(`unknown navaid ${navRef.Navaid}`);
    }
    return row;
  }

  const row = db.queryObjects<PositionRow>(
    "SELECT ARPLatitude AS lat, ARPLongitude AS lon FROM fix WHERE LocationID = ?1 LIMIT 1",
    [navRef.Fix],
  )[0];
  if (!row) {
    const runwayPosition = await resolveRunwayFixPosition(navRef.Fix, procedureAirportId);
    if (runwayPosition) {
      return runwayPosition;
    }
    throw new Error(`unknown fix ${navRef.Fix}`);
  }
  return row;
}

async function loadAirwayBranches(airwayName: string): Promise<AirwayBranch[]> {
  const db = await getBrowserNavDb();
  const points = db.queryObjects<AirwayPointRow>(
    `
      SELECT name, branch_key, sequence_number, point_name, Latitude, Longitude
      FROM airways_branch
      WHERE name = ?1
      ORDER BY branch_key, sequence_number
    `,
    [airwayName],
  );

  const byBranch = new Map<string, AirwayFixPoint[]>();
  for (const point of points) {
    const navRef = await resolvePointNavRef(point);
    const branchPoints = byBranch.get(point.branch_key);
    const fixPoint: AirwayFixPoint = {
      airway_name: point.name,
      sequence: point.sequence_number,
      position: { lat: point.Latitude, lon: point.Longitude },
      nav_ref: navRef,
    };
    if (branchPoints) {
      branchPoints.push(fixPoint);
    } else {
      byBranch.set(point.branch_key, [fixPoint]);
    }
  }

  return [...byBranch.entries()].map(([branchKey, branchPoints]) => ({
    display_name: airwayName,
    branch_key: branchKey,
    points: branchPoints,
  }));
}

async function resolvePointNavRef(point: AirwayPointRow): Promise<NavRef> {
  const db = await getBrowserNavDb();
  for (const [table, kind] of [
    ["fix", "fix"],
    ["nav", "navaid"],
    ["airports", "airport"],
  ] as const) {
    const row = db.queryObjects<IdentifierRow>(
      `
        SELECT trim(LocationID) AS LocationID
        FROM ${table}
        WHERE abs(ARPLatitude - ?1) < 1e-6
          AND abs(ARPLongitude - ?2) < 1e-6
        LIMIT 1
      `,
      [point.Latitude, point.Longitude],
    )[0];
    if (!row) {
      continue;
    }
    const navRef =
      kind === "fix"
        ? ({ Fix: row.LocationID } satisfies NavRef)
        : kind === "navaid"
          ? ({ Navaid: row.LocationID } satisfies NavRef)
          : ({ Airport: row.LocationID } satisfies NavRef);
    identifierCache.set(row.LocationID, navRef);
    return navRef;
  }
  return { LatLon: { lat: point.Latitude, lon: point.Longitude } };
}

function distanceNmBetween(a: LatLon, b: LatLon) {
  const latNm = (b.lat - a.lat) * 60;
  const lonNm = (b.lon - a.lon) * 60 * Math.cos(((a.lat + b.lat) * Math.PI) / 360);
  return Math.hypot(latNm, lonNm);
}

function navRefKey(navRef: NavRef) {
  if ("Airport" in navRef) return `A:${navRef.Airport}`;
  if ("Navaid" in navRef) return `N:${navRef.Navaid}`;
  if ("Fix" in navRef) return `F:${navRef.Fix}`;
  return `L:${navRef.LatLon.lat},${navRef.LatLon.lon}`;
}
