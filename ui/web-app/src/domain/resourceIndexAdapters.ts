import type { CatalogJson, ChartFamilyId, ChartPageData, FlightPlan, MapViewJson, MapViewOptionJson, RegionId, ResourceIndexJson, TileStorageKind } from "./types";

type MapView = MapViewJson;
type MapViewOption = MapViewOptionJson;
type ChartPage = ChartPageData;
type SupportedChartFamily = Extract<ChartFamilyId, "sec" | "tac" | "enr-l" | "enr-h">;
type ChartAsset = ChartPage["airports"][number]["charts"][number];
type FolderCategory = ChartAsset["folder_category"];

const supportedChartFamilies = new Set<SupportedChartFamily>(["sec", "tac", "enr-l", "enr-h"]);

function isSupportedChartFamily(familyId: ChartFamilyId): familyId is SupportedChartFamily {
  return supportedChartFamilies.has(familyId as SupportedChartFamily);
}

function mapLauncherLabel(familyId: string): string {
  switch (familyId) {
    case "sec":
      return "SEC";
    case "tac":
      return "TAC";
    case "enr-l":
      return "IFR L";
    case "enr-h":
      return "IFR H";
    default:
      return familyId.toUpperCase();
  }
}

function tileStorageKindForCollection(_collectionId: string): TileStorageKind {
  return "sectional_package";
}

function tileUrlRoot(packageName: string): string {
  return `/sectional-packages/${packageName}/tiles`;
}

function tileSizeForFamily(_resourceIndex: ResourceIndexJson, _familyId: SupportedChartFamily): number {
  return 512;
}

function minZoomForLevels(levels: MapView["levels"]): number {
  const minLevel = Math.min(...levels.map((level) => level.zoom));
  return Math.max(1.5, minLevel - 2.8);
}

function maxZoomForLevels(levels: MapView["levels"]): number {
  const maxLevel = Math.max(...levels.map((level) => level.zoom));
  return maxLevel + 0.8;
}

function familyDisplayName(resourceIndex: ResourceIndexJson, familyId: SupportedChartFamily): string {
  return resourceIndex.families.find((entry) => entry.id === familyId)?.display_name ?? familyId;
}

function regionDisplayName(resourceIndex: ResourceIndexJson, regionId: RegionId): string {
  return resourceIndex.regions.find((entry) => entry.id === regionId)?.display_name ?? regionId.toUpperCase();
}

function deriveMapView(
  resourceIndex: ResourceIndexJson,
  collection: ResourceIndexJson["chart_collections"][number] & { family_id: SupportedChartFamily },
): MapView {
  const levels = collection.levels.map((level) => ({
    zoom: level.zoom,
    x_min: level.x_min,
    x_max: level.x_max,
    y_tms_min: level.y_tms_min,
    y_tms_max: level.y_tms_max,
  }));
  return {
    chart_family: collection.family_id,
    chart_name: `${regionDisplayName(resourceIndex, collection.region_id)} ${familyDisplayName(resourceIndex, collection.family_id)}`,
    chart_index: collection.chart_index,
    tile_root: "tiles",
    tile_url_root: tileUrlRoot(collection.package_id),
    tile_size: tileSizeForFamily(resourceIndex, collection.family_id),
    min_zoom: minZoomForLevels(levels),
    max_zoom: maxZoomForLevels(levels),
    storage_kind: tileStorageKindForCollection(collection.id),
    package_name: collection.package_id,
    initial_viewport: {
      lat: collection.default_view.lat,
      lon: collection.default_view.lon,
      zoom: collection.default_view.zoom,
    },
    levels,
  };
}

export function deriveMapViews(
  resourceIndex: ResourceIndexJson,
  preferredIds: string[],
): MapViewOption[] {
  const collections = resourceIndex.chart_collections.flatMap((collection) =>
    isSupportedChartFamily(collection.family_id) ? [{ ...collection, family_id: collection.family_id }] : [],
  );
  const selectedCollections =
    preferredIds.length > 0
      ? preferredIds
          .map((id) => collections.find((collection) => collection.id === id))
          .filter((entry): entry is (typeof collections)[number] => entry !== undefined)
      : collections;
  return selectedCollections.map((collection) => ({
    id: collection.id,
    label: `${regionDisplayName(resourceIndex, collection.region_id)} ${familyDisplayName(resourceIndex, collection.family_id)}`,
    region_id: collection.region_id,
    map_view: deriveMapView(resourceIndex, collection),
  }));
}

export function deriveCatalog(resourceIndex: ResourceIndexJson): CatalogJson {
  const cycle = resourceIndex.cycle ?? "unknown";
  const families = resourceIndex.families.flatMap((family) =>
    isSupportedChartFamily(family.id as ChartFamilyId)
      ? [{
          id: family.id as Extract<ChartFamilyId, "sec" | "tac" | "enr-l" | "enr-h">,
          display_name: family.display_name,
          kind: family.kind,
          max_zoom: null,
          tile_size: tileSizeForFamily(resourceIndex, family.id as Extract<ChartFamilyId, "sec" | "tac" | "enr-l" | "enr-h">),
        }]
      : [],
  );
  const packages = resourceIndex.packages.flatMap((pkg) =>
    isSupportedChartFamily(pkg.family_id as ChartFamilyId)
      ? [{
          id: {
            region: pkg.region_id,
            family: pkg.family_id as Extract<ChartFamilyId, "sec" | "tac" | "enr-l" | "enr-h">,
            cycle,
          },
          package_name: pkg.id,
          family_id: pkg.family_id as Extract<ChartFamilyId, "sec" | "tac" | "enr-l" | "enr-h">,
          region_id: pkg.region_id,
          cycle,
          artifact_kind: "zip",
          relative_url: `/${cycle}/${pkg.id}.zip`,
          manifest_name: pkg.id,
          size_bytes: pkg.size_bytes,
          checksum_sha256: pkg.checksum_sha256,
        }]
      : [],
  );
  const charts = resourceIndex.chart_collections.flatMap((collection) =>
    isSupportedChartFamily(collection.family_id)
      ? [{
          id: {
            family: collection.family_id,
            name: collection.id,
            cycle,
          },
          family_id: collection.family_id,
          name: collection.id,
          display_name: `${regionDisplayName(resourceIndex, collection.region_id)} ${familyDisplayName(resourceIndex, collection.family_id)}`,
          cycle,
          region_ids: [collection.region_id],
          max_zoom: Math.max(...collection.levels.map((level) => level.zoom)),
          tile_path_template: `${collection.chart_index}/{z}/{x}/{y}.webp`,
          coverage: {
            kind: "b_box",
            value: {
              south: collection.coverage_bounds.lat_min,
              north: collection.coverage_bounds.lat_max,
              west: collection.coverage_bounds.lon_min,
              east: collection.coverage_bounds.lon_max,
            },
          },
        }]
      : [],
  );
  const plates = resourceIndex.plates.map((plate) => ({
    id: {
      airport_id: plate.airport_id,
      procedure_code: plate.label,
      page: 1,
      cycle,
    },
    airport_id: plate.airport_id,
    region_id: plate.region_id,
    cycle,
    procedure_code: plate.label,
    display_name: plate.label,
    kind: plate.asset_kind,
    georeferenced: true,
    page_count: 1,
    asset_base_path: plate.asset_path.replace(/\.[^.]+$/, ""),
  }));
  return {
    schema_version: 1,
    cycle,
    catalog_revision: resourceIndex.generated_at_utc,
    families,
    regions: resourceIndex.regions,
    packages,
    charts,
    plates,
    supplements: [],
  };
}

function airportIdsFromPlan(plan: FlightPlan): string[] {
  const airportIds = new Set<string>();
  if (plan.departure) airportIds.add(plan.departure);
  if (plan.destination) airportIds.add(plan.destination);
  if (plan.alternate) airportIds.add(plan.alternate);
  for (const leg of plan.legs) {
    if ("Airport" in leg.from) airportIds.add(leg.from.Airport);
    if ("Airport" in leg.to) airportIds.add(leg.to.Airport);
  }
  return [...airportIds];
}

function folderCategoryForRecord(
  kind: "plate" | "csup",
  record: ResourceIndexJson["plates"][number] | ResourceIndexJson["csups"][number],
): FolderCategory {
  if (kind === "csup") {
    return "csup";
  }
  const label = record.label.toUpperCase();
  if (label.includes("AIRPORT DIAGRAM")) {
    return "airport-diagram";
  }
  if (label.startsWith("MIN-") || label.includes("TAKEOFF MINIMUMS") || label.includes("ALTERNATE MINIMUMS")) {
    return "takeoff-mins";
  }
  if (label.startsWith("DP-") || label.startsWith("ODP-") || label.includes("DEPARTURE")) {
    return "departure";
  }
  if (label.startsWith("STAR-") || label.includes(" ARRIVAL")) {
    return "star";
  }
  return "approach";
}

const folderCategoryRank: Record<FolderCategory, number> = {
  "airport-diagram": 0,
  csup: 1,
  "takeoff-mins": 2,
  approach: 3,
  departure: 4,
  star: 5,
};

function chartAssetForRecord(
  airportId: string,
  kind: "plate" | "csup",
  record: ResourceIndexJson["plates"][number] | ResourceIndexJson["csups"][number],
): ChartAsset {
  const filename = record.asset_path.split("/").pop() ?? record.asset_path;
  return {
    id: `${kind}:${airportId}:${filename}`,
    airport_id: airportId,
    package_id: record.package_id,
    label: kind === "csup" ? "CSup" : record.label,
    kind,
    folder_category: folderCategoryForRecord(kind, record),
    source_asset_path: record.asset_path,
    asset_path: `chart-assets/${airportId}/${filename}`,
    asset_url: `/chart-assets/${airportId}/${filename}`,
    thumbnail_source_path: record.thumbnail_path ?? null,
    thumbnail_path: record.thumbnail_path ? `chart-thumbnails/${airportId}/${record.thumbnail_path.split("/").pop() ?? filename}` : null,
    thumbnail_url: record.thumbnail_path ? `/chart-thumbnails/${airportId}/${record.thumbnail_path.split("/").pop() ?? filename}` : null,
  };
}

export function deriveChartPage(
  resourceIndex: ResourceIndexJson,
  samplePlan: FlightPlan,
): ChartPage {
  const plateById = new Map(resourceIndex.plates.map((record) => [record.id, record]));
  const csupById = new Map(resourceIndex.csups.map((record) => [record.id, record]));
  const airportResourcesByAirportId = new Map(
    resourceIndex.airport_resources.map((entry) => [entry.airport_id, entry]),
  );
  const orderedAirportIds = airportIdsFromPlan(samplePlan);
  const airports = [...orderedAirportIds]
    .map((airportId) => {
      const airportResources = airportResourcesByAirportId.get(airportId);
      if (!airportResources) {
        return null;
      }
      const plates = airportResources.plate_ids
        .map((id) => plateById.get(id))
        .filter((record): record is ResourceIndexJson["plates"][number] => record !== undefined)
        .map((record) => chartAssetForRecord(airportId, "plate", record));
      const csups = airportResources.csup_ids
        .map((id) => csupById.get(id))
        .filter((record): record is ResourceIndexJson["csups"][number] => record !== undefined)
        .map((record) => chartAssetForRecord(airportId, "csup", record));
      const charts = [...plates, ...csups].sort((left, right) => {
        const rank = folderCategoryRank[left.folder_category] - folderCategoryRank[right.folder_category];
        return rank !== 0 ? rank : left.label.localeCompare(right.label);
      });
      if (charts.length === 0) {
        return null;
      }
      return {
        id: airportId,
        label: airportId,
        charts,
      };
    })
    .filter((airport): airport is ChartPage["airports"][number] => airport !== null);

  return {
    airports,
  };
}

export function launcherLabelForFamily(familyId: string): string {
  return mapLauncherLabel(familyId);
}
