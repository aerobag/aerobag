declare module "@current-artifacts" {
  const value: unknown;
  export default value;
}

declare module "@shared-ui-theme" {
  const value: unknown;
  export default value;
}

declare module "@shared-bootstrap" {
  const value: unknown;
  export default value;
}

declare module "@generated/app_wasm.js" {
  const init: (...args: unknown[]) => Promise<unknown>;
  export default init;
  export const activate_next_leg_in_session: (...args: unknown[]) => string;
  export const suspend_sequencing_in_session: (...args: unknown[]) => string;
  export const unsuspend_sequencing_in_session: (...args: unknown[]) => string;
  export const sequence_active_leg_in_session: (...args: unknown[]) => string;
  export const perform_flight_plan_row_action_in_session: (...args: unknown[]) => string;
  export const perform_map_selection_action_in_session: (...args: unknown[]) => string;
  export const empty_flight_plan_json: (...args: unknown[]) => string;
  export const create_ui_session: (...args: unknown[]) => string;
  export const set_guidance_leg_geometry_in_session: (...args: unknown[]) => string;
  export const set_situation_in_session: (...args: unknown[]) => string;
  export const engage_map_follow_in_session: (...args: unknown[]) => string;
  export const disengage_map_follow_in_session: (...args: unknown[]) => string;
  export const set_map_follow_offset_in_session: (...args: unknown[]) => string;
  export const sync_map_follow_in_session: (...args: unknown[]) => string;
  export const load_playback_trace_in_session: (...args: unknown[]) => string;
  export const play_playback_in_session: (...args: unknown[]) => string;
  export const pause_playback_in_session: (...args: unknown[]) => string;
  export const seek_playback_in_session: (...args: unknown[]) => string;
  export const set_playback_rate_in_session: (...args: unknown[]) => string;
  export const tick_playback_in_session: (...args: unknown[]) => string;
  export const register_ownship_source_in_session: (...args: unknown[]) => string;
  export const update_ownship_source_status_in_session: (...args: unknown[]) => string;
  export const push_situation_sample_in_session: (...args: unknown[]) => string;
  export const select_ownship_source_in_session: (...args: unknown[]) => string;
  export const apply_situation_control_input_in_session: (...args: unknown[]) => string;
  export const set_map_layer_visibility_in_session: (...args: unknown[]) => string;
  export const set_map_layer_enabled_in_session: (...args: unknown[]) => string;
  export const set_raster_resource_mode_in_session: (...args: unknown[]) => string;
  export const replace_flight_plan_in_session: (...args: unknown[]) => string;
  export const insert_waypoint_at_flight_plan_row_in_session: (...args: unknown[]) => string;
  export const suggest_waypoint_identifiers_at_flight_plan_row_in_session: (...args: unknown[]) => string;
  export const insert_airway_at_flight_plan_row_in_session: (...args: unknown[]) => string;
  export const select_procedure_at_flight_plan_row_in_session: (...args: unknown[]) => string;
  export const load_plate_procedure_in_session: (...args: unknown[]) => string;
  export const restore_direct_to_in_session: (...args: unknown[]) => string;
  export const select_airport_in_session: (...args: unknown[]) => string;
  export const select_chart_in_session: (...args: unknown[]) => string;
  export const ingest_point_tiles_in_session: (...args: unknown[]) => void;
  export const ingest_airspace_ref_tiles_in_session: (...args: unknown[]) => void;
  export const ingest_airspace_features_in_session: (...args: unknown[]) => void;
  export const ingest_airspace_label_tiles_in_session: (...args: unknown[]) => void;
  export const ingest_resource_in_session: (...args: unknown[]) => void;
  export const get_map_overlay_in_session: (...args: unknown[]) => string;
  export const get_map_selection_in_session: (...args: unknown[]) => string;
  export const get_terrain_overlay_in_session: (...args: unknown[]) => string;
  export const render_terrain_overlay_tile_in_session: (...args: unknown[]) => Uint8Array;
  export const render_terrain_overlay_tiles_in_session: (...args: unknown[]) => Uint8Array;
  export const render_terrain_warning_raw_rgba: (...args: unknown[]) => Uint8Array;
  export const render_terrain_warning_raw_rgba_from_packed_tiles: (...args: unknown[]) => Uint8Array;
  export const get_session_snapshot: (...args: unknown[]) => string;
  export const restore_chart_page_state_in_session: (...args: unknown[]) => string;
  export const destroy_session: (...args: unknown[]) => void;
  export const nav_kv_open: (...args: unknown[]) => number;
  export const nav_kv_insert_resource: (...args: unknown[]) => void;
  export const nav_kv_destroy: (...args: unknown[]) => void;
  export const attach_nav_kv_store_to_session: (...args: unknown[]) => void;
  export const core_had_operation: (...args: unknown[]) => string;
}

declare module "*.svg" {
  const value: string;
  export default value;
}
