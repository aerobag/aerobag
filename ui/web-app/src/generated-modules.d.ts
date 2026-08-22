// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

declare module "@shared-ui-theme" {
  const value: unknown;
  export default value;
}

declare module "@shared/*.html?raw" {
  const value: string;
  export default value;
}

declare module "@generated/app_wasm.js" {
  const init: (...args: unknown[]) => Promise<unknown>;
  export default init;
  export const perform_flight_plan_command_in_session: (...args: unknown[]) => string;
  export const perform_time_display_action_in_session: (...args: unknown[]) => string;
  export const perform_flight_plan_column_action_in_session: (...args: unknown[]) => string;
  export const query_flight_plan_in_session: (...args: unknown[]) => string;
  export const perform_status_action_in_session: (...args: unknown[]) => string;
  export const status_action_decision_in_session: (...args: unknown[]) => string;
  export const perform_ownship_text_action_in_session: (...args: unknown[]) => string;
  export const map_selection_action_decision_in_session: (...args: unknown[]) => string;
  export const perform_map_selection_ui_action_in_session: (...args: unknown[]) => string;
  export const flight_plan_row_action_decision_in_session: (...args: unknown[]) => string;
  export const create_ui_session: (...args: unknown[]) => string;
  export const session_diagnostics: (...args: unknown[]) => string;
  export const sync_guidance_geometry_in_session: (...args: unknown[]) => string;
  export const project_flight_plan_route_in_session: (...args: unknown[]) => string;
  export const set_situation_in_session: (...args: unknown[]) => string;
  export const engage_map_follow_in_session: (...args: unknown[]) => string;
  export const disengage_map_follow_in_session: (...args: unknown[]) => string;
  export const set_map_follow_offset_in_session: (...args: unknown[]) => string;
  export const sync_map_follow_in_session: (...args: unknown[]) => string;
  export const load_playback_trace_in_session_paged: (...args: unknown[]) => string;
  export const play_playback_in_session_paged: (...args: unknown[]) => string;
  export const pause_playback_in_session_paged: (...args: unknown[]) => string;
  export const seek_playback_in_session_paged: (...args: unknown[]) => string;
  export const set_playback_rate_in_session_paged: (...args: unknown[]) => string;
  export const tick_playback_in_session_paged: (...args: unknown[]) => string;
  export const register_ownship_source_in_session_paged: (...args: unknown[]) => string;
  export const update_ownship_source_status_in_session_paged: (...args: unknown[]) => string;
  export const push_situation_sample_in_session_paged: (...args: unknown[]) => string;
  export const select_ownship_source_in_session_paged: (...args: unknown[]) => string;
  export const apply_situation_control_input_in_session: (...args: unknown[]) => string;
  export const set_map_layer_visibility_in_session_paged: (...args: unknown[]) => string;
  export const set_map_layer_enabled_in_session_paged: (...args: unknown[]) => string;
  export const set_resource_policy_in_session: (...args: unknown[]) => string;
  export const accept_disclaimer_in_session: (...args: unknown[]) => string;
  export const perform_settings_action_in_session: (...args: unknown[]) => string;
  export const perform_aircraft_library_action_in_session: (...args: unknown[]) => string;
  export const select_airport_in_session: (...args: unknown[]) => string;
  export const select_chart_in_session: (...args: unknown[]) => string;
  export const ingest_point_tiles_in_session: (...args: unknown[]) => void;
  export const ingest_airspace_ref_tiles_in_session: (...args: unknown[]) => void;
  export const ingest_airspace_features_in_session: (...args: unknown[]) => void;
  export const ingest_airspace_label_tiles_in_session: (...args: unknown[]) => void;
  export const ingest_prepared_live_feed_resource_in_session: (...args: unknown[]) => void;
  export const ingest_resource_in_session: (...args: unknown[]) => void;
  export const prepare_live_feed_resource: (...args: unknown[]) => Uint8Array;
  export const reset_live_feed_preparer: (...args: unknown[]) => void;
  export const report_session_resource_failure_in_session: (...args: unknown[]) => string;
  export const report_session_resource_failure_in_session_at_epoch_ms: (...args: unknown[]) => string;
  export const report_live_feed_connection_event_in_session: (...args: unknown[]) => string;
  export const get_map_overlay_in_session: (...args: unknown[]) => string;
  export const get_map_selection_in_session: (...args: unknown[]) => string;
  export const get_map_selection_distance_in_session: (...args: unknown[]) => string;
  export const get_terrain_overlay_in_session: (...args: unknown[]) => string;
  export const get_scheduled_terrain_overlay_in_session: (...args: unknown[]) => string;
  export const render_terrain_overlay_tile_by_key_in_session: (...args: unknown[]) => Uint8Array;
  export const render_terrain_overlay_tile_in_session: (...args: unknown[]) => Uint8Array;
  export const render_terrain_overlay_tiles_in_session: (...args: unknown[]) => Uint8Array;
  export const render_terrain_warning_raw_rgba: (...args: unknown[]) => Uint8Array;
  export const render_terrain_warning_raw_rgba_from_packed_tiles: (...args: unknown[]) => Uint8Array;
  export const get_session_snapshot_paged: (...args: unknown[]) => string;
  export const get_session_snapshot_at_epoch_ms_paged: (...args: unknown[]) => string;
  export const create_session_snapshot_refresh_scheduler: (...args: unknown[]) => number;
  export const destroy_session_snapshot_refresh_scheduler: (...args: unknown[]) => void;
  export const session_snapshot_refresh_scheduler_request: (...args: unknown[]) => string;
  export const session_snapshot_refresh_scheduler_viewport_gesture_active_changed: (...args: unknown[]) => string;
  export const session_snapshot_refresh_scheduler_viewport_activity: (...args: unknown[]) => string;
  export const session_snapshot_refresh_scheduler_refresh_completed: (...args: unknown[]) => string;
  export const session_snapshot_refresh_scheduler_poll: (...args: unknown[]) => string;
  export const create_ui_session_work_scheduler: (...args: unknown[]) => number;
  export const destroy_ui_session_work_scheduler: (...args: unknown[]) => void;
  export const ui_session_work_scheduler_request: (...args: unknown[]) => string;
  export const ui_session_work_scheduler_complete: (...args: unknown[]) => string;
  export const restore_chart_page_state_in_session: (...args: unknown[]) => string;
  export const destroy_session: (...args: unknown[]) => void;
  export const install_rust_debug_logger: (...args: unknown[]) => void;
  export const nav_db_open_controller_create: (...args: unknown[]) => number;
  export const nav_db_open_controller_destroy: (...args: unknown[]) => void;
  export const nav_db_open_controller_finish: (...args: unknown[]) => string;
  export const nav_db_open_controller_ingest_resource: (...args: unknown[]) => void;
  export const nav_db_open_controller_step: (...args: unknown[]) => string;
  export const nav_kv_insert_resource: (...args: unknown[]) => void;
  export const nav_kv_prefetch_pages: (...args: unknown[]) => string;
  export const nav_kv_destroy: (...args: unknown[]) => void;
  export const attach_nav_kv_store_to_session: (...args: unknown[]) => void;
  export const advance_nav_kv_store_in_session: (...args: unknown[]) => string;
  export const maintain_nav_db_in_session_at_epoch_ms: (...args: unknown[]) => string;
  export const core_had_operation: (...args: unknown[]) => string;
  export const resolve_metar_manifest_in_session: (...args: unknown[]) => string;
  export const resolve_nav_db_artifact_candidates_in_session: (...args: unknown[]) => string;
  export const resolve_chart_asset_resource_in_session: (...args: unknown[]) => string;
}

declare module "*.svg" {
  const value: string;
  export default value;
}

declare module "*.html?raw" {
  const value: string;
  export default value;
}
