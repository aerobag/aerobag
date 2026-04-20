import type {
  ChartPageData,
  AirwayBranch,
  CifpTppMatchRow,
  LatLon,
  MapViewOptionJson,
  NavRef,
  NavSymbolFeature,
  ProcedureDistinctRow,
  ProcedureLegMaterializationRecord,
  ProcedureKind,
} from "./types";
import { loadNavKvJson } from "./navKv";

export type PlateAirportIndexEntry = {
  id: string;
  label: string;
};

export type PlateAirportRecord = ChartPageData["airports"][number];

export type ProcedureListRecord = {
  airport_id: string;
  procedure_id: string;
  kind: ProcedureKind;
};

export type WaypointIdentifierRecord = {
  identifier: string;
  nav_ref: NavRef;
  kind: string;
  city: string;
  state: string;
  facility_name: string;
  position: LatLon;
};

export type AirwaySpatialPoint = {
  airway_name: string;
  branch_key: string;
  sequence: number;
  position: LatLon;
  nav_ref: NavRef;
};

export function hadKeyComponent(value: string): string {
  return encodeURIComponent(value.trim());
}

export function hadUpperKeyComponent(value: string): string {
  return hadKeyComponent(value.toUpperCase());
}

export function chartCatalogKey(): string {
  return "chart/catalog";
}

export function plateAirportIndexKey(): string {
  return "plate/airport-index";
}

export function plateAirportKey(airportId: string): string {
  return `plate/airport/${hadUpperKeyComponent(airportId)}`;
}

export function plateByIdKey(plateId: string): string {
  return `plate/by-id/${hadKeyComponent(plateId)}`;
}

export function plateCifpMatchKey(airportId: string, cifpId: string): string {
  return `plate/cifp/${hadUpperKeyComponent(airportId)}/${hadUpperKeyComponent(cifpId)}`;
}

export function plateProcedureCandidatesKey(plateId: string): string {
  return `plate/procedure-candidates/${hadKeyComponent(plateId)}`;
}

export function procedureListKey(airportId: string, kind: ProcedureKind): string {
  return `procedure/list/${hadUpperKeyComponent(airportId)}/${hadUpperKeyComponent(kind)}`;
}

export function procedureDistinctRowsKey(airportId: string, procedureId: string): string {
  return `procedure/distinct-rows/${hadUpperKeyComponent(airportId)}/${hadUpperKeyComponent(procedureId)}`;
}

export function procedureMaterializationRowsKey(airportId: string, procedureId: string): string {
  return `procedure/materialization-rows/${hadUpperKeyComponent(airportId)}/${hadUpperKeyComponent(procedureId)}`;
}

export function navRefPositionKey(navRef: NavRef, procedureAirportId?: string | null): string {
  if ("Airport" in navRef) {
    return `navref/position/airport/${hadUpperKeyComponent(navRef.Airport)}`;
  }
  if ("Navaid" in navRef) {
    return `navref/position/navaid/${hadUpperKeyComponent(navRef.Navaid)}`;
  }
  if ("Fix" in navRef && procedureAirportId && navRef.Fix.trim().toUpperCase().startsWith("RW")) {
    return `navref/position/runway/${hadUpperKeyComponent(procedureAirportId)}/${hadUpperKeyComponent(navRef.Fix)}`;
  }
  if ("Fix" in navRef) {
    return `navref/position/fix/${hadUpperKeyComponent(navRef.Fix)}`;
  }
  throw new Error("LatLon nav refs do not have HAD position keys");
}

export function navRefSymbolKey(navRef: NavRef): string | null {
  if ("Airport" in navRef) {
    return `navref/symbol/airport/${hadUpperKeyComponent(navRef.Airport)}`;
  }
  if ("Navaid" in navRef) {
    return `navref/symbol/navaid/${hadUpperKeyComponent(navRef.Navaid)}`;
  }
  if ("Fix" in navRef) {
    return `navref/symbol/fix/${hadUpperKeyComponent(navRef.Fix)}`;
  }
  return null;
}

export function airwayBranchesKey(airwayName: string): string {
  return `airway/${hadUpperKeyComponent(airwayName)}`;
}

export function airwaySpatialKey(latTile: number, lonTile: number): string {
  return `airway/spatial/${latTile}/${lonTile}`;
}

export function waypointIdentifierKey(identifier: string): string {
  return `waypoint/identifier/${hadUpperKeyComponent(identifier)}`;
}

export function waypointPrefixKey(prefix: string): string {
  const normalized = prefix.trim().toUpperCase();
  const shard = normalized.length <= 2 ? normalized : normalized.slice(0, 2);
  return `waypoint/prefix/${hadUpperKeyComponent(shard)}`;
}

export async function loadRequiredHadJson<T>(key: string, family: string): Promise<T> {
  const loaded = await loadNavKvJson<T>(key);
  if (loaded === null) {
    throw new Error(`HAD missing required ${family} key: ${key}`);
  }
  return loaded;
}

export async function loadHadChartCatalog(): Promise<MapViewOptionJson[] | null> {
  return loadNavKvJson<MapViewOptionJson[]>(chartCatalogKey());
}

export async function loadHadPlateAirportIndex(): Promise<PlateAirportIndexEntry[] | null> {
  return loadNavKvJson<PlateAirportIndexEntry[]>(plateAirportIndexKey());
}

export async function loadHadPlateAirport(airportId: string): Promise<PlateAirportRecord | null> {
  return loadNavKvJson<PlateAirportRecord>(plateAirportKey(airportId));
}

export async function loadHadPlateById(plateId: string): Promise<PlateAirportRecord["charts"][number] | null> {
  return loadNavKvJson<PlateAirportRecord["charts"][number]>(plateByIdKey(plateId));
}

export async function loadHadCifpPlateMatches(airportId: string, cifpId: string): Promise<CifpTppMatchRow[] | null> {
  return loadNavKvJson<CifpTppMatchRow[]>(plateCifpMatchKey(airportId, cifpId));
}

export async function loadHadPlateProcedureCandidates(plateId: string): Promise<CifpTppMatchRow[] | null> {
  return loadNavKvJson<CifpTppMatchRow[]>(plateProcedureCandidatesKey(plateId));
}

export async function loadHadProcedureList(airportId: string, kind: ProcedureKind): Promise<ProcedureListRecord[] | null> {
  return loadNavKvJson<ProcedureListRecord[]>(procedureListKey(airportId, kind));
}

export async function loadHadProcedureDistinctRows(
  airportId: string,
  procedureId: string,
): Promise<ProcedureDistinctRow[] | null> {
  return loadNavKvJson<ProcedureDistinctRow[]>(procedureDistinctRowsKey(airportId, procedureId));
}

export async function loadHadProcedureMaterializationRows(
  airportId: string,
  procedureId: string,
): Promise<ProcedureLegMaterializationRecord[] | null> {
  return loadNavKvJson<ProcedureLegMaterializationRecord[]>(procedureMaterializationRowsKey(airportId, procedureId));
}

export async function loadHadNavRefPosition(navRef: NavRef, procedureAirportId?: string | null): Promise<LatLon> {
  if ("LatLon" in navRef) {
    return navRef.LatLon;
  }
  return loadRequiredHadJson<LatLon>(navRefPositionKey(navRef, procedureAirportId), "navref position");
}

export async function loadHadNavSymbolFeature(navRef: NavRef): Promise<NavSymbolFeature | null> {
  const key = navRefSymbolKey(navRef);
  if (!key) {
    return null;
  }
  return loadRequiredHadJson<NavSymbolFeature | null>(key, "navref symbol");
}

export async function loadHadAirwayBranches(airwayName: string): Promise<AirwayBranch[]> {
  return loadRequiredHadJson<AirwayBranch[]>(airwayBranchesKey(airwayName), "airway branches");
}

export async function loadHadAirwaySpatialTile(key: string): Promise<AirwaySpatialPoint[]> {
  return (await loadNavKvJson<AirwaySpatialPoint[]>(key)) ?? [];
}

export async function loadHadWaypointIdentifier(identifier: string): Promise<NavRef | null> {
  return loadNavKvJson<NavRef>(waypointIdentifierKey(identifier));
}

export async function loadHadWaypointPrefix(prefix: string): Promise<WaypointIdentifierRecord[]> {
  return loadRequiredHadJson<WaypointIdentifierRecord[]>(waypointPrefixKey(prefix), "waypoint prefix");
}
