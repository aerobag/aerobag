import { describe, expect, it } from "vitest";
import { loadBestAvailableAdapter } from "./appCoreAdapter";

const snapshotJson = JSON.stringify({
  app_state: {
    active_plan: null,
    content_policy: "PreferLocal",
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
        selection: { kind: "auto" },
        sources: [],
      },
    },
    content_policy: "PreferLocal",
    last_content_report: null,
  },
  chart_page_state: {
    ordered_airport_ids: [],
    recent_airport_ids: [],
    selected_airport_id: "",
    selected_chart_id: "",
  },
  map_layer_state: {
    vectors: { visible: true, enabled: true },
    nexrad: { visible: false, enabled: true },
    terrain_warning: { visible: true, enabled: true },
  },
  caution_state: {
    obstacle_display_limited: false,
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
      situation_ring_candidates_json: () => "[]",
      create_ui_session: async () => JSON.stringify({ handle: 1, snapshot: JSON.parse(snapshotJson) }),
      remove_leg_in_session: async () => snapshotJson,
      move_waypoint_in_session: async () => snapshotJson,
      remove_top_level_waypoint_by_nav_ref_in_session: async () => snapshotJson,
      insert_waypoint_best_position_in_session: async () => JSON.stringify({ state: "complete", result: JSON.parse(snapshotJson) }),
      set_situation_in_session: async () => snapshotJson,
      engage_map_follow_in_session: async () => snapshotJson,
      disengage_map_follow_in_session: async () => snapshotJson,
      set_map_follow_offset_in_session: async () => snapshotJson,
      sync_map_follow_in_session: async () => snapshotJson,
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
      set_map_layer_visibility_in_session: async () => snapshotJson,
      set_map_layer_enabled_in_session: async () => snapshotJson,
      set_raster_map_catalog_in_session: async () => snapshotJson,
      select_map_in_session: async () => snapshotJson,
      replace_flight_plan_in_session: async () => snapshotJson,
      set_guidance_leg_geometry_in_session: async () => snapshotJson,
      select_airport_in_session: async () => snapshotJson,
      select_chart_in_session: async () => snapshotJson,
      ingest_point_tiles_in_session: async () => {},
      ingest_metar_tiles_in_session: async () => {},
      ingest_airspace_ref_tiles_in_session: async () => {},
      ingest_airspace_features_in_session: async () => {},
      ingest_airspace_label_tiles_in_session: async () => {},
      ingest_tfrs_in_session: async () => {},
      ingest_metars_in_session: async () => {},
      get_map_overlay_in_session: async () => "{\"state\":\"complete\",\"result\":{\"visible_features\":[],\"visible_metars\":[],\"needed_point_tiles\":[],\"needed_metar_tiles\":[],\"needed_metars\":false,\"needed_airspace_ref_tiles\":[],\"needed_airspace_features\":[],\"needed_airspace_label_tiles\":[],\"needed_tfrs\":false,\"airspace_paths\":[],\"tfr_paths\":[],\"airspace_labels\":[],\"warnings\":[]}}",
      get_map_selection_in_session: async () => "{\"state\":\"complete\",\"result\":{\"click_lat\":0,\"click_lon\":0,\"categories\":[]}}",
      get_terrain_overlay_in_session: async () => "{\"needed_terrain_tiles\":[],\"status\":\"hidden\"}",
      get_raster_tile_plan_in_session: async () => "{\"background_color\":\"#000000\",\"layers\":[]}",
      render_terrain_overlay_tile_in_session: async () => new Uint8Array(),
      render_terrain_overlay_tiles_in_session: async () => new Uint8Array(),
      get_session_snapshot: async () => snapshotJson,
      restore_chart_page_state_in_session: async () => snapshotJson,
      destroy_session: () => {},
      nav_kv_open: async () => 1,
      nav_kv_insert_page: async () => {},
      nav_kv_destroy: async () => {},
      attach_nav_kv_store_to_session: async () => {},
      core_had_operation: async () => JSON.stringify({ state: "complete", result: null }),
      insert_airway_at_flight_plan_row_in_session: async () => JSON.stringify({ state: "complete", result: JSON.parse(snapshotJson) }),
      select_procedure_at_flight_plan_row_in_session: async () => JSON.stringify({ state: "complete", result: JSON.parse(snapshotJson) }),
      load_plate_procedure_in_session: async () => JSON.stringify({ state: "complete", result: JSON.parse(snapshotJson) }),
      activate_next_leg_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      suspend_sequencing_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      unsuspend_sequencing_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
      sequence_active_leg_ui: async () => "{\"plan\":{\"id\":\"p\",\"name\":\"Plan\",\"legs\":[],\"route_components\":[],\"resolved_legs\":[],\"guidance\":null,\"departure\":null,\"destination\":null,\"alternate\":null,\"cruise_altitude_ft\":null,\"notes\":null,\"updated_at_epoch_ms\":0,\"version\":0},\"ui_state\":{\"components\":[],\"resolved_legs\":[],\"display_rows\":[],\"guidance\":null}}",
    }));

    expect(loaded.backend).toBe("wasm");
    expect(loaded.detail).toContain("Rust WASM");
  });
});
