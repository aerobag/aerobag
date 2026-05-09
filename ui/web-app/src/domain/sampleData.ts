import bootstrapJson from "@shared-bootstrap";
import { sampleCatalogFixture } from "./sampleFixtures";
import type {
  AppState,
  CatalogJson,
  ChartPageData,
  ContentInventory,
  DevBootstrapJson,
  GeometryJson,
  MapViewJson,
  MapViewOptionJson,
  MapTileViewJson,
} from "./types";

export const bootstrap = bootstrapJson as DevBootstrapJson;

export const sampleCatalog = sampleCatalogFixture as CatalogJson;
export const sampleGeometry: GeometryJson = {
  schema_version: 1,
  polygons: [],
  polygon_sets: [],
};

export const mapViews: MapViewOptionJson[] = [
  {
    id: "sec:nw",
    label: "Northwest Sectional",
    region_id: "nw",
    map_view: {
      chart_family: "sec",
      chart_name: "Northwest Sectional",
      chart_index: 0,
      tile_root: "tiles",
      tile_url_root: "/packages/published_unpacked/sec_nw_2604_01_sample/tiles",
      tile_path_template: "{z}/{x}/{y}.webp",
      tile_size: 512,
      min_zoom: 5.2,
      max_zoom: 12.5,
      max_source_zoom: 12,
      max_display_zoom: 12.5,
      storage_kind: "sectional_package",
      package_name: "SEC_NW_2604_01",
      full_coverage_zoom: 7,
      initial_viewport: {
        lat: 46.0,
        lon: -122.0,
        zoom: 8.2,
      },
      levels: [
        { zoom: 8, x_min: 0, x_max: 255, y_tms_min: 0, y_tms_max: 255 },
        { zoom: 10, x_min: 0, x_max: 1023, y_tms_min: 0, y_tms_max: 1023 },
      ],
    },
  },
];
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

export const chartPage: ChartPageData = {
  airports: [],
};

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
      launcher_label: "No GPS",
      launcher_tone: "unavailable",
      sources: [],
      situation_controls: [],
    },
    sources: [],
  },
  content_policy: bootstrap.content_policy,
  last_content_report: null,
};
