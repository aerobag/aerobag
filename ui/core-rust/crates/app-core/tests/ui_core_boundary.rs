use std::{collections::BTreeSet, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../")
        .canonicalize()
        .expect("repo root")
}

fn read_repo_file(path: &str) -> String {
    std::fs::read_to_string(repo_root().join(path)).expect(path)
}

fn strip_rust_tests(source: &str) -> &str {
    source.split("\n#[cfg(test)]").next().expect("source split")
}

fn function_body<'a>(source: &'a str, name: &str) -> &'a str {
    let marker = format!("fn {name}");
    let start = source
        .find(&marker)
        .or_else(|| source.find(&format!("pub fn {name}")))
        .unwrap_or_else(|| panic!("missing function {name}"));
    let rest = &source[start..];
    let body_start = rest.find('{').expect("function body start");
    let mut depth = 0_i32;
    for (offset, ch) in rest[body_start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[..body_start + offset + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated function {name}");
}

#[test]
fn exported_plain_snapshot_session_apis_are_allowlisted() {
    let source_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let source = strip_rust_tests(&source_text);
    let allowed: BTreeSet<&str> = [
        "set_map_layer_visibility_in_session",
        "set_raster_resource_mode_in_session",
        "select_raster_map_in_session",
        "set_map_layer_enabled_in_session",
        "project_flight_plan_route_in_session",
        "select_airport_in_session",
        "select_chart_in_session",
        "register_ownship_source_in_session",
        "update_ownship_source_status_in_session",
        "push_situation_sample_in_session",
        "set_ownship_policy_in_session",
        "select_ownship_source_in_session",
        "apply_situation_control_input_in_session",
        "load_playback_trace_in_session",
        "play_playback_in_session",
        "pause_playback_in_session",
        "seek_playback_in_session",
        "set_playback_rate_in_session",
        "tick_playback_in_session",
        "set_situation_in_session",
        "tick_debug_ownship_driver_in_session",
        "activate_next_leg_in_session",
        "suspend_sequencing_in_session",
        "unsuspend_sequencing_in_session",
        "sequence_active_leg_in_session",
        "activate_direct_to_leg_in_session",
        "restore_direct_to_in_session",
        "engage_map_follow_in_session",
        "disengage_map_follow_in_session",
        "set_map_follow_offset_in_session",
        "sync_map_follow_in_session",
        "restore_chart_page_state_in_session",
        "set_debug_flag_in_session",
        "get_session_snapshot",
    ]
    .into_iter()
    .collect();

    let mut violations = Vec::new();
    for line in source.lines() {
        let Some(rest) = line.trim_start().strip_prefix("pub fn ") else {
            continue;
        };
        if !line.contains("AppResult<UiSessionSnapshot>") {
            continue;
        }
        let name = rest.split('(').next().unwrap_or(rest);
        if !allowed.contains(name) {
            violations.push(name.to_string());
        }
    }

    assert!(
        violations.is_empty(),
        "new plain UiSessionSnapshot exports must be explicitly classified or converted to HadOperationOutcome: {violations:?}"
    );
}

#[test]
fn paged_flight_plan_mutations_commit_only_after_snapshot_projection() {
    let source_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let source = strip_rust_tests(&source_text);
    let functions = [
        "insert_waypoint_at_flight_plan_row_in_session",
        "insert_airway_at_flight_plan_row_in_session",
        "select_procedure_at_flight_plan_row_in_session",
        "load_plate_procedure_in_session",
        "append_flight_plan_entry_in_session",
        "perform_flight_plan_row_action_in_session",
    ];
    let mut violations = Vec::new();
    for function in functions {
        let body = function_body(source, function);
        if !body.contains("commit_session_flight_plan_with_snapshot_outcome") {
            violations.push(format!("{function}: missing staged paged commit helper"));
        }
        if body.contains("replace_session_flight_plan(session") {
            violations.push(format!(
                "{function}: commits before paged snapshot projection"
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "paged flight-plan mutations must be side-effect-free on NeedResources:\n{}",
        violations.join("\n")
    );
}

#[test]
fn platform_adapters_use_paged_loops_for_paged_session_exports() {
    let web = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let android = read_repo_file(
        "ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/NativeAppCoreAdapter.kt",
    );
    let paged_web_exports = [
        "load_raster_map_catalog_in_session",
        "select_map_family_in_session",
        "perform_map_selection_action_in_session",
        "insert_waypoint_at_flight_plan_row_in_session",
        "suggest_waypoint_identifiers_at_flight_plan_row_in_session",
        "insert_airway_at_flight_plan_row_in_session",
        "select_procedure_at_flight_plan_row_in_session",
        "load_plate_procedure_in_session",
        "perform_flight_plan_row_action_in_session",
        "sync_guidance_geometry_in_session",
        "project_flight_plan_route_in_session",
        "set_situation_in_session_paged",
        "tick_debug_ownship_driver_in_session_paged",
        "register_ownship_source_in_session_paged",
        "update_ownship_source_status_in_session_paged",
        "push_situation_sample_in_session_paged",
        "select_ownship_source_in_session_paged",
    ];
    let mut violations = Vec::new();
    for export in paged_web_exports {
        let needle = format!("this.module.{export}");
        let Some(index) = web.find(&needle) else {
            violations.push(format!("web missing {export}"));
            continue;
        };
        let window = &web[index.saturating_sub(220)..web.len().min(index + 420)];
        if !window.contains("runCoreHadSessionOperation") {
            violations.push(format!(
                "web calls {export} without runCoreHadSessionOperation"
            ));
        }
    }

    let paged_android_exports = [
        "loadRasterMapCatalogInSessionJson",
        "selectMapFamilyInSessionJson",
        "performMapSelectionActionInSessionJson",
        "insertWaypointAtFlightPlanRowInSessionJson",
        "suggestWaypointIdentifiersAtFlightPlanRowInSessionJson",
        "insertAirwayAtFlightPlanRowInSessionJson",
        "selectProcedureAtFlightPlanRowInSessionJson",
        "loadPlateProcedureInSessionJson",
        "performFlightPlanRowActionInSessionJson",
        "syncGuidanceGeometryInSessionJson",
        "projectFlightPlanRouteInSessionJson",
    ];
    for export in paged_android_exports {
        let needle = format!("bridge.{export}");
        let Some(index) = android.find(&needle) else {
            violations.push(format!("android missing {export}"));
            continue;
        };
        let window = &android[index.saturating_sub(220)..android.len().min(index + 420)];
        if !window.contains("runPagedSessionOperationElement") {
            violations.push(format!(
                "android calls {export} without runPagedSessionOperationElement"
            ));
        }
    }

    let paged_android_snapshot_helper_exports = [
        "registerOwnshipSourceInSessionPagedJson",
        "updateOwnshipSourceStatusInSessionPagedJson",
        "pushSituationSampleInSessionPagedJson",
        "selectOwnshipSourceInSessionPagedJson",
    ];
    for export in paged_android_snapshot_helper_exports {
        let needle = format!("bridge.{export}");
        let Some(index) = android.find(&needle) else {
            violations.push(format!("android missing {export}"));
            continue;
        };
        let window = &android[index.saturating_sub(220)..android.len().min(index + 420)];
        if !window.contains("runPagedSnapshot") {
            violations.push(format!("android calls {export} without runPagedSnapshot"));
        }
    }

    assert!(
        violations.is_empty(),
        "platform adapters must drive paged session exports through resource loops:\n{}",
        violations.join("\n")
    );
}

#[test]
fn platform_adapters_do_not_call_plain_had_sensitive_snapshot_exports() {
    let web = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let android = read_repo_file(
        "ui/android-app/app/src/main/java/net/jonh/aerobag/prototype/domain/NativeAppCoreAdapter.kt",
    );
    let mut violations = Vec::new();

    for export in [
        "set_situation_in_session",
        "tick_debug_ownship_driver_in_session",
        "register_ownship_source_in_session",
        "update_ownship_source_status_in_session",
        "push_situation_sample_in_session",
        "select_ownship_source_in_session",
    ] {
        if web.contains(&format!("this.module.{export}(")) {
            violations.push(format!("web calls plain HAD-sensitive export {export}"));
        }
    }

    for export in [
        "registerOwnshipSourceInSessionJson",
        "updateOwnshipSourceStatusInSessionJson",
        "pushSituationSampleInSessionJson",
        "selectOwnshipSourceInSessionJson",
    ] {
        if android.contains(&format!("bridge.{export}(")) {
            violations.push(format!("android calls plain HAD-sensitive export {export}"));
        }
    }

    assert!(
        violations.is_empty(),
        "full session snapshots can need HAD resources during projection; platform adapters must call paged variants:\n{}",
        violations.join("\n")
    );
}

#[test]
fn exported_boundary_modules_do_not_panic_on_poisoned_stores() {
    let files = [
        "ui/core-rust/crates/app-core/src/session.rs",
        "ui/core-rust/crates/app-wasm/src/lib.rs",
        "ui/core-rust/crates/app-ffi/src/lib.rs",
    ];
    let mut violations = Vec::new();
    for file in files {
        let source_text = read_repo_file(file);
        let source = strip_rust_tests(&source_text);
        for (line_index, line) in source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.contains("expect(\"")
                || trimmed.contains(".unwrap()")
                || trimmed.contains("panic!(")
                || trimmed.contains("unreachable!(")
                || trimmed.contains("todo!(")
            {
                violations.push(format!("{file}:{}: {trimmed}", line_index + 1));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "boundary modules must return errors instead of panicking:\n{}",
        violations.join("\n")
    );
}
