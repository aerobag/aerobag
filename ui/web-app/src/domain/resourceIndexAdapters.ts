import type { ChartFamilyId, ChartPageData, CurrentArtifactsJson, FlightPlan, MapViewJson, MapViewOptionJson, RegionId, ResourceIndexJson, TileStorageKind } from "./types";

type MapView = MapViewJson;
type MapViewOption = MapViewOptionJson;
type ChartPage = ChartPageData;
type SupportedChartFamily = Extract<ChartFamilyId, "sec" | "tac" | "enr-l" | "enr-h">;
type ChartAsset = ChartPage["airports"][number]["charts"][number];
type FolderCategory = ChartAsset["folder_category"];
type ShadedReliefProduct = NonNullable<CurrentArtifactsJson["static_products"]>[number] & { id: string };

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
    case "shaded-relief":
      return "RELIEF";
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

function tilePathTemplateForCollection(collection: ResourceIndexJson["chart_collections"][number]): string {
  return collection.tile_path_template.replace(/^tiles\//, "");
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
    tile_path_template: tilePathTemplateForCollection(collection),
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

function fullPyramidLevels(minZoom: number, maxZoom: number): MapView["levels"] {
  const levels: MapView["levels"] = [];
  for (let zoom = minZoom; zoom <= maxZoom; zoom += 1) {
    const maxTile = (2 ** zoom) - 1;
    levels.push({ zoom, x_min: 0, x_max: maxTile, y_tms_min: 0, y_tms_max: maxTile });
  }
  return levels;
}

function shadedReliefRegionId(productId: string): RegionId | null {
  const regionId = productId.replace(/^shaded-relief-/, "") as RegionId;
  return ["ne", "nc", "nw", "se", "sc", "sw", "ec", "ak", "pac"].includes(regionId) ? regionId : null;
}

function defaultViewportForRegion(resourceIndex: ResourceIndexJson, regionId: RegionId): MapView["initial_viewport"] {
  const referenceCollection =
    resourceIndex.chart_collections.find((collection) => collection.region_id === regionId && collection.family_id === "sec")
    ?? resourceIndex.chart_collections.find((collection) => collection.region_id === regionId);
  return referenceCollection?.default_view ?? { lat: 39, lon: -98, zoom: 4 };
}

function deriveShadedReliefMapView(resourceIndex: ResourceIndexJson, product: ShadedReliefProduct, regionId: RegionId): MapView {
  const levels = fullPyramidLevels(0, 10);
  return {
    chart_family: "shaded-relief",
    chart_name: `${regionDisplayName(resourceIndex, regionId)} Shaded Relief`,
    chart_index: 0,
    tile_root: "tiles",
    tile_url_root: `/shaded-relief-products/${product.id}/tiles`,
    tile_path_template: "{z}/{x}/{y}.webp",
    tile_size: 512,
    min_zoom: 0,
    max_zoom: 10.8,
    storage_kind: "static_product",
    package_name: product.id,
    initial_viewport: defaultViewportForRegion(resourceIndex, regionId),
    levels,
  };
}

function deriveShadedReliefMapViews(resourceIndex: ResourceIndexJson, currentArtifacts?: CurrentArtifactsJson): MapViewOption[] {
  return (currentArtifacts?.static_products ?? [])
    .flatMap((product): Array<{ product: ShadedReliefProduct; regionId: RegionId }> => {
      if (!product.id?.startsWith("shaded-relief-")) {
        return [];
      }
      const regionId = shadedReliefRegionId(product.id);
      return regionId ? [{ product: product as ShadedReliefProduct, regionId }] : [];
    })
    .map(({ product, regionId }) => ({
      id: product.id,
      label: `${regionDisplayName(resourceIndex, regionId)} Shaded Relief`,
      region_id: regionId,
      map_view: deriveShadedReliefMapView(resourceIndex, product, regionId),
    }));
}

export function deriveMapViews(
  resourceIndex: ResourceIndexJson,
  preferredIds: string[],
  currentArtifacts?: CurrentArtifactsJson,
): MapViewOption[] {
  const collections = resourceIndex.chart_collections.flatMap((collection) =>
    isSupportedChartFamily(collection.family_id) ? [{ ...collection, family_id: collection.family_id }] : [],
  );
  const chartMapViews = collections.map((collection) => ({
    id: collection.id,
    label: `${regionDisplayName(resourceIndex, collection.region_id)} ${familyDisplayName(resourceIndex, collection.family_id)}`,
    region_id: collection.region_id,
    map_view: deriveMapView(resourceIndex, collection),
  }));
  const allMapViews = [...chartMapViews, ...deriveShadedReliefMapViews(resourceIndex, currentArtifacts)];
  const selectedCollections =
    preferredIds.length > 0
      ? preferredIds
          .map((id) => allMapViews.find((entry) => entry.id === id))
          .filter((entry): entry is MapViewOption => entry !== undefined)
      : allMapViews;
  return selectedCollections;
}

function airportIdsFromPlan(plan: FlightPlan): string[] {
  const airportIds = new Set<string>();
  if (plan.departure) airportIds.add(plan.departure);
  if (plan.destination) airportIds.add(plan.destination);
  if (plan.alternate) airportIds.add(plan.alternate);
  for (const component of plan.route_components ?? []) {
    if (component.kind === "waypoint" && "Airport" in component.waypoint) {
      airportIds.add(component.waypoint.Airport);
    } else if (component.kind === "procedure") {
      airportIds.add(component.procedure.airport_id);
    }
  }
  return [...airportIds];
}

function folderCategoryForRecord(
  kind: "plate" | "csup",
  record: ResourceIndexJson["plates"][number] | ResourceIndexJson["csups"][number],
): FolderCategory {
  const documentType = kind === "csup" ? "csup" : record.document_type;
  switch (documentType) {
    case "airport_diagram":
      return "airport-diagram";
    case "takeoff_minimums":
    case "alternate_minimums":
    case "minimums":
      return "takeoff-mins";
    case "departure":
      return "departure";
    case "star":
      return "star";
    case "csup":
      return "csup";
    case "approach":
    case "other":
    default:
      return "approach";
  }
}

function chartAssetForRecord(
  airportId: string,
  kind: "plate" | "csup",
  record: ResourceIndexJson["plates"][number] | ResourceIndexJson["csups"][number],
): ChartAsset {
  return {
    id: `${kind}:${airportId}:${record.asset_path.split("/").pop() ?? record.asset_path}`,
    airport_id: airportId,
    package_id: record.package_id,
    label: record.label,
    kind,
    folder_category: folderCategoryForRecord(kind, record),
    source_asset_path: record.asset_path,
    asset_path: record.asset_path,
    asset_url: `/${record.asset_path}`,
    thumbnail_source_path: record.thumbnail_path ?? null,
    thumbnail_path: record.thumbnail_path ?? null,
    thumbnail_url: record.thumbnail_path ? `/${record.thumbnail_path}` : null,
    georef: "georef" in record ? record.georef ?? null : null,
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
      const charts = [...plates, ...csups];
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
