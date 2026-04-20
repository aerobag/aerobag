declare module "@product-resource-index" {
  const value: unknown;
  export default value;
}

declare module "@product-catalog" {
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
  export const activate_leg_ui: (...args: unknown[]) => string;
  export const activate_next_leg_ui: (...args: unknown[]) => string;
  export const delete_component_ui: (...args: unknown[]) => string;
  export const move_component_ui: (...args: unknown[]) => string;
  export const insert_waypoint_ui: (...args: unknown[]) => string;
  export const suspend_sequencing_ui: (...args: unknown[]) => string;
  export const unsuspend_sequencing_ui: (...args: unknown[]) => string;
  export const sequence_active_leg_ui: (...args: unknown[]) => string;
  export const insert_airway_materialized_ui: (...args: unknown[]) => string;
  export const replace_airway_materialized_ui: (...args: unknown[]) => string;
  export const insert_procedure_materialized_ui: (...args: unknown[]) => string;
  export const replace_procedure_materialized_ui: (...args: unknown[]) => string;
  export const describe_procedure_options_from_rows: (...args: unknown[]) => string;
  export const list_approach_procedures_from_match_rows: (...args: unknown[]) => string;
  export const materialize_procedure_from_records: (...args: unknown[]) => string;
  export const select_preferred_cifp_tpp_match: (...args: unknown[]) => string;
  export const describe_show_plate_for_procedure: (...args: unknown[]) => string;
  export const describe_load_procedure_from_plate: (...args: unknown[]) => string;
  export const describe_plate_procedure_load_options: (...args: unknown[]) => string;
  export const airway_spatial_tile_keys: (...args: unknown[]) => string;
  export const flight_plan_insert_anchor: (...args: unknown[]) => string;
  export const materialize_airway_selection_from_branches: (...args: unknown[]) => string;
  export const prepare_airway_presentation: (...args: unknown[]) => string;
  export const sort_airway_suggestions_for_ui: (...args: unknown[]) => string;
  export const project_flight_plan_route_from_positions: (...args: unknown[]) => string;
  export const suggest_airways_near_from_points: (...args: unknown[]) => string;
  export const suggest_waypoint_identifiers_from_candidates: (...args: unknown[]) => string;
  export const create_ui_session: (...args: unknown[]) => string;
  export const remove_leg_in_session: (...args: unknown[]) => string;
  export const set_guidance_leg_geometry_in_session: (...args: unknown[]) => string;
  export const move_waypoint_in_session: (...args: unknown[]) => string;
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
  export const replace_flight_plan_in_session: (...args: unknown[]) => string;
  export const select_airport_in_session: (...args: unknown[]) => string;
  export const select_chart_in_session: (...args: unknown[]) => string;
  export const ingest_point_tiles_in_session: (...args: unknown[]) => void;
  export const get_map_overlay_in_session: (...args: unknown[]) => string;
  export const get_session_snapshot: (...args: unknown[]) => string;
  export const restore_chart_page_state_in_session: (...args: unknown[]) => string;
  export const destroy_session: (...args: unknown[]) => void;
  export const derive_chart_catalog: (...args: unknown[]) => string;
  export const derive_chart_page: (...args: unknown[]) => string;
  export const derive_chart_page_state: (...args: unknown[]) => string;
  export const replace_flight_plan_state: (...args: unknown[]) => string;
  export const set_content_policy_state: (...args: unknown[]) => string;
  export const refresh_content_state: (...args: unknown[]) => string;
}

declare module "*.svg" {
  const value: string;
  export default value;
}
