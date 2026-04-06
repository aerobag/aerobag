import fixture from "./generated/contentFixture.json";
import type { AppState, ContentFixtureBundle } from "./types";

const contentFixture = fixture as ContentFixtureBundle;

export const sampleCatalog = contentFixture.catalog;
export const sampleGeometry = contentFixture.geometry;
export const initialProbe = contentFixture.initial_probe;
export const mapTileView = contentFixture.map_tile_view;
export const samplePlan = contentFixture.flight_plan;
export const remoteOnlyInventory = contentFixture.remote_only_inventory;
export const installedInventory = contentFixture.installed_inventory;

export const emptyState: AppState = {
  active_plan: null,
  content_policy: "PreferLocal",
  last_content_requirements: [],
  last_content_report: null,
};
