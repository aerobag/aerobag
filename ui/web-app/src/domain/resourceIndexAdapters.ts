import type { ChartFamilyId, ContentFixtureBundle, FlightPlan, RegionId, ResourceIndexJson, TileStorageKind } from "./types";

type MapView = ContentFixtureBundle["map_view"];
type MapViewOption = NonNullable<ContentFixtureBundle["map_views"]>[number];
type ChartPage = NonNullable<ContentFixtureBundle["chart_page"]>;
type SupportedChartFamily = Extract<ChartFamilyId, "sectional" | "tac" | "ifr_low" | "ifr_high">;
type ChartAsset = ChartPage["airports"][number]["charts"][number];

const supportedChartFamilies = new Set<SupportedChartFamily>(["sectional", "tac", "ifr_low", "ifr_high"]);

function isSupportedChartFamily(familyId: ChartFamilyId): familyId is SupportedChartFamily {
  return supportedChartFamilies.has(familyId as SupportedChartFamily);
}

function mapLauncherLabel(familyId: string): string {
  switch (familyId) {
    case "sectional":
      return "SEC";
    case "tac":
      return "TAC";
    case "ifr_low":
      return "IFR L";
    case "ifr_high":
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

function chartAssetForRecord(
  airportId: string,
  kind: "plate" | "csup",
  record: ResourceIndexJson["plates"][number] | ResourceIndexJson["csups"][number],
): ChartAsset {
  const filename = record.asset_path.split("/").pop() ?? record.asset_path;
  return {
    id: `${kind}:${airportId}:${filename}`,
    airport_id: airportId,
    label: kind === "csup" ? "CSup" : record.label,
    kind,
    asset_path: `chart-assets/${airportId}/${filename}`,
    asset_url: `/chart-assets/${airportId}/${filename}`,
  };
}

export function deriveChartPage(
  resourceIndex: ResourceIndexJson,
  fixtureChartPage: ContentFixtureBundle["chart_page"] | undefined,
  samplePlan: FlightPlan,
): ChartPage {
  const plateById = new Map(resourceIndex.plates.map((record) => [record.id, record]));
  const csupById = new Map(resourceIndex.csups.map((record) => [record.id, record]));
  const airportResourcesByAirportId = new Map(
    resourceIndex.airport_resources.map((entry) => [entry.airport_id, entry]),
  );
  const hintedAirportIds = new Set<string>(fixtureChartPage?.recent_airport_ids ?? []);
  for (const airportId of airportIdsFromPlan(samplePlan)) {
    hintedAirportIds.add(airportId);
  }
  const airports = [...hintedAirportIds]
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

  const initialAirportId =
    fixtureChartPage?.initial_airport_id && airports.some((airport) => airport.id === fixtureChartPage.initial_airport_id)
      ? fixtureChartPage.initial_airport_id
      : airports[0]?.id ?? "";
  const initialChartId =
    fixtureChartPage?.initial_chart_id &&
    airports.some((airport) => airport.charts.some((chart) => chart.id === fixtureChartPage.initial_chart_id))
      ? fixtureChartPage.initial_chart_id
      : airports.find((airport) => airport.id === initialAirportId)?.charts[0]?.id ?? "";

  return {
    recent_airport_ids: airports.map((airport) => airport.id),
    initial_airport_id: initialAirportId,
    initial_chart_id: initialChartId,
    airports,
  };
}

export function launcherLabelForFamily(familyId: string): string {
  return mapLauncherLabel(familyId);
}
