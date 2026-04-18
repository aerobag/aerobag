import { describe, expect, it } from "vitest";
import type { FlightPlan, ResourceIndexJson } from "./types";
import { deriveChartPage, deriveMapViews } from "./resourceIndexAdapters";

const resourceIndex: ResourceIndexJson = {
  schema_version: 1,
  cycle: "2604",
  generated_at_utc: "2026-04-07T00:00:00Z",
  families: [
    { id: "sec", display_name: "Sectional", kind: "tiled_raster" },
    { id: "tac", display_name: "TAC", kind: "tiled_raster" },
    { id: "tpp", display_name: "TPP", kind: "flat_image" },
    { id: "csup", display_name: "CSUP", kind: "flat_image" },
  ],
  regions: [
    { id: "nw", display_name: "Northwest", sort_order: 0 },
    { id: "ne", display_name: "Northeast", sort_order: 1 },
  ],
  packages: [],
  airport_resources: [
    {
      airport_id: "BOS",
      plate_ids: ["plate:BOS:IAP-MA-ILS OR LOC RWY 04R.png"],
      csup_ids: ["csup:BOS:CSUP-NE_0-0.png"],
      package_ids: ["NE_CSUP", "NE_TPP"],
    },
  ],
  chart_collections: [
    {
      id: "sec:nw",
      family_id: "sec",
      region_id: "nw",
      package_id: "NW_SEC",
      chart_index: 0,
      tile_path_template: "tiles/0/{z}/{x}/{y}.webp",
      levels: [{ zoom: 10, x_min: 1, x_max: 2, y_tms_min: 3, y_tms_max: 4 }],
      coverage_bounds: { lat_min: 1, lat_max: 2, lon_min: 3, lon_max: 4 },
      default_view: { lat: 45, lon: -122, zoom: 8 },
    },
    {
      id: "tac:nw",
      family_id: "tac",
      region_id: "nw",
      package_id: "NW_TAC",
      chart_index: 1,
      tile_path_template: "tiles/1/{z}/{x}/{y}.webp",
      levels: [{ zoom: 11, x_min: 5, x_max: 6, y_tms_min: 7, y_tms_max: 8 }],
      coverage_bounds: { lat_min: 1, lat_max: 2, lon_min: 3, lon_max: 4 },
      default_view: { lat: 46, lon: -123, zoom: 9 },
    },
  ],
  airports: [],
  plates: [
    {
      id: "plate:BOS:IAP-MA-ILS OR LOC RWY 04R.png",
      airport_id: "BOS",
      region_id: "ne",
      package_id: "NE_TPP",
      asset_path: "plates/BOS/IAP-MA-ILS OR LOC RWY 04R.png",
      label: "ILS or LOC 04R",
      asset_kind: "png",
      document_type: "approach",
    },
  ],
  csups: [
    {
      id: "csup:BOS:CSUP-NE_0-0.png",
      airport_id: "BOS",
      region_id: "ne",
      package_id: "NE_CSUP",
      asset_path: "afd/BOS/CSUP-NE_0-0.png",
      label: "Chart Supplement",
      asset_kind: "png",
      document_type: "csup",
    },
  ],
};

const samplePlan: FlightPlan = {
  id: "plan-1",
  name: "test",
  legs: [],
  route_components: [
    { kind: "waypoint", waypoint: { Airport: "BOS" } },
    { kind: "waypoint", waypoint: { Airport: "BOS" } },
  ],
  resolved_legs: [
    {
      id: "component-0-1",
      from: { Airport: "BOS" },
      to: { Airport: "BOS" },
      source: { kind: "route_component", component_index: 0 },
      procedure_airport_id: null,
    },
  ],
  guidance: null,
  departure: "BOS",
  destination: "BOS",
  alternate: null,
  cruise_altitude_ft: null,
  notes: null,
  updated_at_epoch_ms: 0,
  version: 1,
};

describe("resourceIndexAdapters", () => {
  it("derives map views from preferred chart collection ids", () => {
    const mapViews = deriveMapViews(resourceIndex, ["sec:nw", "tac:nw"]);
    expect(mapViews.map((entry) => entry.id)).toEqual(["sec:nw", "tac:nw"]);
    expect(mapViews[0].map_view.package_name).toBe("NW_SEC");
    expect(mapViews[1].map_view.chart_family).toBe("tac");
    expect(mapViews[0].map_view.tile_size).toBe(512);
    expect(mapViews[1].map_view.tile_size).toBe(512);
  });

  it("derives chart page assets from plates and csups", () => {
    const chartPage = deriveChartPage(resourceIndex, samplePlan);
    expect(chartPage.airports).toHaveLength(1);
    expect(chartPage.airports[0].id).toBe("BOS");
    expect(chartPage.airports[0].charts.map((chart) => chart.kind)).toEqual(["plate", "csup"]);
    expect(chartPage.airports[0].charts.map((chart) => chart.folder_category)).toEqual(["approach", "csup"]);
  });
});
