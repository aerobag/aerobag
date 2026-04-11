import fixture from "@generated/contentFixture.json";
import resourceIndexFixture from "@generated/resourceIndex.json";
import { deriveChartPage, deriveMapViews } from "./resourceIndexAdapters";
import type { AppState, ContentFixtureBundle, ResourceIndexJson } from "./types";

const contentFixture = fixture as ContentFixtureBundle;
export const resourceIndex = resourceIndexFixture as ResourceIndexJson;

export const sampleCatalog = contentFixture.catalog;
export const sampleGeometry = contentFixture.geometry;
export const initialProbe = contentFixture.initial_probe;
export const mapViews = deriveMapViews(
  resourceIndex,
  contentFixture.map_views?.map((view) => view.id) ?? [],
);
export const mapView = mapViews[0].map_view;
export const mapTileView = contentFixture.map_tile_view;
export const samplePlan = contentFixture.flight_plan;
export const chartPage = deriveChartPage(resourceIndex, samplePlan);
export const remoteOnlyInventory = contentFixture.remote_only_inventory;
export const installedInventory = contentFixture.installed_inventory;

export const emptyState: AppState = {
  active_plan: null,
  situation: {
    position: { kind: "unknown" },
    orientation_deg: null,
    speed_kt: null,
  },
  content_policy: "PreferLocal",
  last_content_requirements: [],
  last_content_report: null,
};
