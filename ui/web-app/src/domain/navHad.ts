import type {
  ChartPageData,
  CifpTppMatchRow,
  MapViewOptionJson,
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
