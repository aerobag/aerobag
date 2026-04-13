import { describe, expect, it } from "vitest";
import { loadBestAvailableAdapter } from "./appCoreAdapter";

describe("loadBestAvailableAdapter", () => {
  it("fails loudly when the generated wasm module is missing", async () => {
    await expect(loadBestAvailableAdapter(async () => {
      throw new Error("module not found");
    })).rejects.toThrow("module not found");
  });

  it("uses the wasm adapter when the generated module exports the expected API", async () => {
    const loaded = await loadBestAvailableAdapter(async () => ({
      create_ui_session: async () => "{\"handle\":1,\"chart_catalog\":{\"airports\":[]},\"snapshot\":{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}}",
      remove_leg_in_session: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
      move_waypoint_in_session: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
      set_situation_in_session: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
      replace_flight_plan_in_session: async () => "{\"app_state\":{\"active_plan\":null,\"situation\":{\"position\":{\"kind\":\"unknown\"},\"orientation_deg\":null,\"speed_kt\":null},\"content_policy\":\"PreferLocal\",\"last_content_requirements\":[],\"last_content_report\":null},\"chart_page_state\":{\"ordered_airport_ids\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}}",
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
      delete_component_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      suspend_sequencing_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      unsuspend_sequencing_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      sequence_active_leg_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      prepare_airway_presentation: async () => "{\"airway_name\":\"V2\",\"branch_key\":\"A\",\"points\":[],\"suggested_entry_index\":0,\"suggested_exit_index\":null}",
      sort_airway_suggestions_for_ui: async () => "[]",
      insert_airway_materialized_ui: async () => "{\"mutation\":{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0}},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
      replace_airway_materialized_ui: async () => "{\"mutation\":{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0}},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"guidance\":null}}",
    }));

    expect(loaded.backend).toBe("wasm");
    expect(loaded.detail).toContain("Rust WASM");
  });
});
