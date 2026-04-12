import { describe, expect, it } from "vitest";
import { loadBestAvailableAdapter, MockAppCoreAdapter } from "./appCoreAdapter";
import type { CatalogJson } from "./types";

const chartCatalog: CatalogJson = {
  schema_version: 1,
  cycle: "2026-04-16",
  catalog_revision: "test",
  families: [],
  regions: [],
  packages: [],
  charts: [
    {
      id: { family: "tac", name: "Boston TAC", cycle: "2026-04-16" },
      family_id: "tac",
      name: "Boston TAC",
      display_name: "Boston TAC",
      cycle: "2026-04-16",
      region_ids: ["ne"],
      max_zoom: 11,
      tile_path_template: "tiles/1/{z}/{x}/{y}.webp",
      coverage: {
        kind: "polygon_ref",
        value: { polygon_id: "tac:boston" },
      },
    },
  ],
  plates: [],
  supplements: [],
};

const geometry = {
  polygons: [
    {
      id: "tac:boston",
      points: [
        [-71.2, 42.2],
        [-70.8, 42.2],
        [-70.8, 42.5],
        [-71.2, 42.5],
        [-71.2, 42.2],
      ],
    },
  ],
};

describe("loadBestAvailableAdapter", () => {
  it("falls back to the mock adapter when the generated wasm module is missing", async () => {
    const loaded = await loadBestAvailableAdapter(async () => {
      throw new Error("module not found");
    });

    expect(loaded.backend).toBe("mock");
    expect(loaded.detail).toContain("module not found");
  });

  it("uses the wasm adapter when the generated module exports the expected API", async () => {
    const loaded = await loadBestAvailableAdapter(async () => ({
      create_ui_session: async () => "{\"handle\":1,\"chart_catalog\":{\"airports\":[]},\"snapshot\":{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}}",
      remove_leg_in_session: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
      move_waypoint_in_session: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
      set_situation_in_session: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
      select_airport_in_session: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
      select_chart_in_session: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
      ingest_point_tiles_in_session: async () => {},
      get_map_overlay_in_session: async () => "{\"visible_features\":[],\"needed_point_tiles\":[],\"warnings\":[]}",
      get_session_snapshot: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
      restore_chart_page_state_in_session: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
      destroy_session: () => {},
      remove_flight_plan_leg: async () => "{}",
      derive_chart_page: async () => "{\"airports\":[]}",
      derive_chart_page_state: async () => "{\"airports\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}",
      replace_flight_plan_state: async (stateJson: string) => stateJson,
      set_content_policy_state: async (stateJson: string) => stateJson,
      refresh_content_state: async (stateJson: string) => stateJson,
      build_flight_plan_ui: async () => "{\"components\":[],\"resolved_legs\":[],\"guidance\":null}",
      activate_leg_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      activate_next_leg_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      suspend_sequencing_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      unsuspend_sequencing_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      sequence_active_leg_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      prepare_airway_presentation: async () => "{\"airway_name\":\"V2\",\"branch_key\":\"A\",\"points\":[],\"suggested_entry_index\":0,\"suggested_exit_index\":null}",
      sort_airway_suggestions_for_ui: async () => "[]",
      insert_airway_materialized_ui: async () => "{\"mutation\":{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0}},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      chart_for_position: async () => "null",
    }));

    expect(loaded.backend).toBe("wasm");
    expect(loaded.detail).toContain("Rust WASM");
  });

  it("mock chart lookup finds the configured TAC polygon", async () => {
    const adapter = new MockAppCoreAdapter();
    const chart = await adapter.chartForPosition(
      chartCatalog,
      geometry,
      "tac",
      42.35,
      -71.0,
    );

    expect(chart?.display_name).toBe("Boston TAC");
  });
});
