import bootstrapJson from "@shared-bootstrap";
import catalogJson from "@product-catalog";
import { resourceIndex } from "./productResourceIndex";
import { deriveChartPage, deriveMapViews } from "./resourceIndexAdapters";
import type {
  AppState,
  CatalogJson,
  ContentInventory,
  DevBootstrapJson,
  GeometryJson,
  MapTileViewJson,
} from "./types";

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
  ownship: {
    policy: {
      selection: { kind: "auto" },
      source_priority: [],
      allow_auto_replay: false,
      allow_auto_simulated: false,
    },
    resolved: {
      mode: "none",
      active_source_id: null,
      active_source_kind: null,
      banner_text: "NO GPS POSITION",
      banner_severity: "warning",
      guidance_enabled: false,
      sequencing_enabled: false,
    },
    render: {
      mode: "none",
      banner_text: "NO GPS POSITION",
      banner_severity: "warning",
      draw_aircraft: false,
      draw_predictor: false,
      draw_cdi: false,
      position: null,
      orientation_deg: null,
      speed_kt: null,
      altitude_msl_ft: null,
      pressure_altitude_ft: null,
    },
    controls: {
      mode: "none",
      selection: { kind: "auto" },
      sources: [],
    },
    sources: [],
  },
  content_policy: bootstrap.content_policy,
  last_content_report: null,
};
