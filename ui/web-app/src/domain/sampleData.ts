import bootstrapJson from "@shared-bootstrap";
import catalogJson from "@product-catalog";
import resourceIndexJson from "@product-resource-index";
import { deriveChartPage, deriveMapViews } from "./resourceIndexAdapters";
import type {
  AppState,
  CatalogJson,
  ContentInventory,
  DevBootstrapJson,
  GeometryJson,
  MapTileViewJson,
  ResourceIndexJson,
} from "./types";

export const resourceIndex = resourceIndexJson as ResourceIndexJson;
export const bootstrap = bootstrapJson as DevBootstrapJson;

export const sampleCatalog = catalogJson as CatalogJson;
export const sampleGeometry: GeometryJson = {
  schema_version: 1,
  polygons: [],
};

export const mapViews = deriveMapViews(resourceIndex, []);
export const mapView = mapViews[0].map_view;
const defaultLevel = mapView.levels.reduce((best, current) => (current.zoom > best.zoom ? current : best));
export const mapTileView: MapTileViewJson = {
  chart_family: mapView.chart_family,
  chart_name: mapView.chart_name,
  chart_index: mapView.chart_index,
  tile_root: mapView.tile_root,
  zoom: defaultLevel.zoom,
  tile_size: mapView.tile_size,
  radius: 0,
  center_x: Math.floor((defaultLevel.x_min + defaultLevel.x_max) / 2),
  center_y_tms: Math.floor((defaultLevel.y_tms_min + defaultLevel.y_tms_max) / 2),
  probe_offset_x: 0,
  probe_offset_y: 0,
};

export const samplePlan = bootstrap.flight_plan;
export const chartPage = deriveChartPage(resourceIndex, samplePlan);

export const remoteOnlyInventory: ContentInventory = {
  installed_packages: [],
  cached_tilesets: [],
  cached_plates: [],
};

export const installedInventory: ContentInventory = {
  installed_packages: [],
  cached_tilesets: [],
  cached_plates: [],
};

export const emptyState: AppState = {
  active_plan: null,
  situation: {
    position: { kind: "unknown" },
    orientation_deg: null,
    speed_kt: null,
  },
  content_policy: bootstrap.content_policy,
  last_content_requirements: [],
  last_content_report: null,
};
