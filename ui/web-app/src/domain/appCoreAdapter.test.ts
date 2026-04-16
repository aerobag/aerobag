import { describe, expect, it } from "vitest";
import { loadBestAvailableAdapter } from "./appCoreAdapter";

const snapshotJson = JSON.stringify({
  app_state: {
    active_plan: null,
    content_policy: "PreferLocal",
    last_content_requirements: [],
    last_content_report: null,
  },
  app_ui_state: {
    active_plan: null,
    ownship: {
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
      },
      controls: {
        mode: "none",
        policy: { kind: "auto" },
        sources: [],
      },
    },
    content_policy: "PreferLocal",
    last_content_requirements: [],
    last_content_report: null,
  },
  chart_page_state: {
    ordered_airport_ids: [],
    recent_airport_ids: [],
    selected_airport_id: "",
    selected_chart_id: "",
  },
});

describe("loadBestAvailableAdapter", () => {
  it("fails loudly when the generated wasm module is missing", async () => {
    await expect(loadBestAvailableAdapter(async () => {
      throw new Error("module not found");
    })).rejects.toThrow("module not found");
  });

  it("uses the wasm adapter when the generated module exports the expected API", async () => {
    const loaded = await loadBestAvailableAdapter(async () => ({
      create_ui_session: async () => JSON.stringify({ handle: 1, chart_catalog: { airports: [] }, snapshot: JSON.parse(snapshotJson) }),
      remove_leg_in_session: async () => snapshotJson,
      move_waypoint_in_session: async () => snapshotJson,
      set_situation_in_session: async () => snapshotJson,
      engage_map_follow_in_session: async () => snapshotJson,
      disengage_map_follow_in_session: async () => snapshotJson,
      set_map_follow_offset_in_session: async () => snapshotJson,
      load_playback_trace_in_session: async () => snapshotJson,
      play_playback_in_session: async () => snapshotJson,
      pause_playback_in_session: async () => snapshotJson,
      seek_playback_in_session: async () => snapshotJson,
      set_playback_rate_in_session: async () => snapshotJson,
      tick_playback_in_session: async () => snapshotJson,
      register_ownship_source_in_session: async () => snapshotJson,
      update_ownship_source_status_in_session: async () => snapshotJson,
      push_situation_sample_in_session: async () => snapshotJson,
      select_ownship_source_in_session: async () => snapshotJson,
      replace_flight_plan_in_session: async () => snapshotJson,
      select_airport_in_session: async () => snapshotJson,
      select_chart_in_session: async () => snapshotJson,
      ingest_point_tiles_in_session: async () => {},
      get_map_overlay_in_session: async () => "{\"visible_features\":[],\"needed_point_tiles\":[],\"warnings\":[]}",
      get_session_snapshot: async () => snapshotJson,
      restore_chart_page_state_in_session: async () => snapshotJson,
      destroy_session: () => {},
      remove_flight_plan_leg: async () => "{}",
      derive_chart_page: async () => "{\"airports\":[]}",
      derive_chart_page_state: async () => "{\"airports\":[],\"recent_airport_ids\":[],\"selected_airport_id\":\"\",\"selected_chart_id\":\"\"}",
      replace_flight_plan_state: async (stateJson: string) => stateJson,
      set_content_policy_state: async (stateJson: string) => stateJson,
      refresh_content_state: async (stateJson: string) => stateJson,
      build_flight_plan_ui: async () => "{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}",
      activate_leg_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      activate_next_leg_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      delete_component_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      move_component_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      suspend_sequencing_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      unsuspend_sequencing_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      sequence_active_leg_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      prepare_airway_presentation: async () => "{\"airway_name\":\"V2\",\"branch_key\":\"A\",\"points\":[],\"suggested_entry_index\":0,\"suggested_exit_index\":null}",
      sort_airway_suggestions_for_ui: async () => "[]",
      insert_airway_materialized_ui: async () => "{\"mutation\":{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0}},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      replace_airway_materialized_ui: async () => "{\"mutation\":{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0}},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      insert_procedure_materialized_ui: async () => "{\"mutation\":{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0}},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      replace_procedure_materialized_ui: async () => "{\"mutation\":{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0}},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      describe_procedure_options_from_rows: async () => "{\"airport_id\":\"KAAA\",\"procedure_id\":\"PROC\",\"kind\":\"approach\",\"runway_transitions\":[],\"enroute_transitions\":[],\"has_common_segment\":false,\"valid_choices\":[]}",
      list_approach_procedures_from_match_rows: async () => "[]",
      materialize_procedure_from_records: async () => "{\"procedure\":{\"airport_id\":\"KAAA\",\"procedure_id\":\"PROC\",\"kind\":\"approach\",\"runway_transition\":null,\"enroute_transition\":null,\"terminal_discontinuity\":null},\"concretized_items\":[],\"resolved_legs\":[]}",
      select_preferred_cifp_tpp_match: async () => "null",
      describe_show_plate_for_procedure: async () => "null",
      describe_load_procedure_from_plate: async () => "null",
      describe_plate_procedure_load_options: async () => "[]",
    }));

    expect(loaded.backend).toBe("wasm");
    expect(loaded.detail).toContain("Rust WASM");
  });
});
