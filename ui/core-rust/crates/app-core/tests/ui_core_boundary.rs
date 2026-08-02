// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../")
        .canonicalize()
        .expect("repo root")
}

fn read_repo_file(path: &str) -> String {
    std::fs::read_to_string(repo_root().join(path)).expect(path)
}

fn read_repo_path(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn collect_rust_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap_or_else(|err| panic!("{}: {err}", root.display())) {
        let entry = entry.expect("read dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
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

fn balanced_block_after_marker<'a>(source: &'a str, marker: &str) -> &'a str {
    let start = source
        .find(marker)
        .unwrap_or_else(|| panic!("missing marker {marker}"));
    let rest = &source[start..];
    let body_start = rest.find('{').expect("block start");
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
    panic!("unterminated block {marker}");
}

#[test]
fn production_session_snapshot_apis_are_always_paged() {
    let source_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let source = strip_rust_tests(&source_text);
    assert!(
        !source.contains("AppResult<UiSessionSnapshot>"),
        "production session APIs must return HadOperationOutcome; plain snapshots erase HAD page faults"
    );
    assert!(
        !source.contains("snapshot_for_changed_session")
            && !source.contains("fn snapshot_for_session"),
        "runtime snapshot helpers must not convert HAD page faults into ordinary command failures"
    );
}

#[test]
fn platform_session_adapters_have_no_plain_snapshot_escape_hatch() {
    let web = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let android = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt",
    );

    assert!(
        !web.contains("const parseSessionSnapshot ="),
        "web snapshot-producing commands must use the paged session runner"
    );
    assert!(
        web.contains("() => this.module.get_session_snapshot_paged(sessionHandle)"),
        "web paged mutations need the generic snapshot-resume continuation"
    );
    assert!(
        !android.contains("runPlainSnapshot"),
        "Android snapshot-producing commands must use the paged session runner"
    );
    assert!(
        android.contains("resumeSnapshot = { bridge.getSessionSnapshotPagedJson(handle) }"),
        "Android paged mutations need the generic snapshot-resume continuation"
    );
}

#[test]
fn cloud_ui_actions_and_wire_contract_are_core_owned() {
    let app = read_repo_file("ui/web-app/src/App.tsx");
    let adapter = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let wasm = read_repo_file("ui/core-rust/crates/app-wasm/src/lib.rs");

    assert!(
        !app.contains("Unsupported core Cloud action id")
            && !app.contains("case \"begin_setup\"")
            && !app.contains("case \"authorize_provider\""),
        "platform UI must render core Cloud actions and return their typed IDs without translating them"
    );
    assert!(
        !adapter.contains("export type CloudAction =")
            && !adapter.contains("export type UiCloudPanel =")
            && adapter.contains("../generated/cloudWire"),
        "Cloud platform wire types must come from the generated core schema"
    );
    assert!(
        !wasm.contains("pub fn perform_cloud_action_in_session(")
            && !wasm.contains("pub fn report_cloud_authorization_state_in_session("),
        "the old platform-constructed Cloud action and authorization-state APIs must stay deleted"
    );
}

#[test]
fn platform_route_editors_promote_letters_without_filtering_search_syntax() {
    let web = read_repo_file("ui/web-app/src/App.tsx");
    let android_map =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt");
    let android_plan =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/FlightPlanPage.kt");

    assert!(
        web.contains("setRouteEntryText(event.target.value.toUpperCase());"),
        "web must promote append-route letters while preserving coordinate punctuation"
    );
    assert!(
        android_plan.contains("routeEntryText = value.uppercase()"),
        "Android must promote append-route letters while preserving coordinate punctuation"
    );
    assert!(
        android_map.contains("chartSearchText = value"),
        "Android must pass chart-search text to core without a platform character whitelist"
    );
    assert!(
        !web.contains("chartSearch.query.trim().toUpperCase()")
            && !android_map.contains("chartSearchText.trim().uppercase()"),
        "platform chart search must not normalize syntax before core sees it"
    );
}

#[test]
fn platform_live_feed_adapters_do_not_own_nexrad_policy() {
    let android_cache =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/LiveFeedCache.kt");
    let android_fetch =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/RuntimeFetch.kt");
    let android_session = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt",
    );
    let web_session = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");

    assert!(
        !android_cache.to_ascii_lowercase().contains("nexrad")
            && !android_fetch.to_ascii_lowercase().contains("nexrad"),
        "Android live-feed persistence and resource effects must stay product-generic"
    );
    assert!(
        android_session.contains("\"durable_complete_states\"")
            && !android_session.contains("\"jit_public_resources\""),
        "Android must only declare the durable complete-state policy owned by core"
    );
    assert!(
        web_session.contains("acquisition_policy: \"jit_public_resources\"")
            && !web_session.contains("\"durable_complete_states\""),
        "web must only declare the JIT public-resource policy owned by core"
    );
}

#[test]
fn cloud_provider_adapters_do_not_own_storage_or_application_policy() {
    let web_provider =
        read_repo_file("ui/web-app/src/domain/googleDriveCloudProvider.ts").to_ascii_lowercase();
    let web_runtime =
        read_repo_file("ui/web-app/src/domain/cloudProviderRuntime.ts").to_ascii_lowercase();
    let android_provider =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/CloudProviderRuntime.kt")
            .to_ascii_lowercase();
    let forbidden = [
        "flight_plan",
        "flightplan",
        "merkle",
        "successor",
        "merge_policy",
        "mergepolicy",
        "files/generateids",
        "upload/drive",
        "appdatafolder",
    ];
    let violations = forbidden
        .into_iter()
        .filter(|needle| {
            web_provider.contains(needle)
                || web_runtime.contains(needle)
                || android_provider.contains(needle)
        })
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "cloud provider adapters are platform effects only; core owns storage and application policy: {}",
        violations.join(", ")
    );
}

#[test]
fn bulk_notam_state_cannot_cross_into_the_ui_session() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let overlay = read_repo_file("ui/core-rust/crates/app-core/src/map_overlay.rs");
    let airport_index = balanced_block_after_marker(&overlay, "pub struct AirportNotamIndex");
    let android_main =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt");
    let promote = balanced_block_after_marker(&android_main, "suspend fun promoteLiveFeed");
    let restore = balanced_block_after_marker(
        &android_main,
        "LaunchedEffect(uiSession, liveFeedCache, context, prefs)",
    );

    assert!(
        !session.contains("NotamState::from_checkpoint")
            && !session.contains("install_notam_resource_chain"),
        "UiSession must consume only prepared airport-NOTAM projections"
    );
    assert!(
        !airport_index.contains("NotamState") && !airport_index.contains("NotamCheckpoint"),
        "the UI-facing airport NOTAM index must not retain canonical bulk NOTAM state"
    );
    assert!(
        promote.contains("withContext(Dispatchers.IO)")
            && promote.contains("preparedInstallCandidate")
            && promote.contains("installPreparedLiveFeedCacheProduct"),
        "Android must prepare durable NOTAM state off main before installing its projection"
    );
    assert!(
        restore.contains("withContext(Dispatchers.IO)")
            && restore.contains("LiveFeedCacheStore.restore"),
        "Android must rebuild durable NOTAM state off main during startup"
    );
}

#[test]
fn paged_flight_plan_mutations_commit_only_after_guidance_projection() {
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
        if !body.contains("commit_session_flight_plan_with_invalidations_outcome") {
            violations.push(format!("{function}: missing staged paged commit helper"));
        }
        if body.contains("replace_session_flight_plan(session") {
            violations.push(format!(
                "{function}: commits before paged snapshot projection"
            ));
        }
    }
    let mutation_helper = function_body(source, "mutate_session_flight_plan");
    if !mutation_helper.contains("commit_session_flight_plan_with_invalidations_outcome") {
        violations
            .push("mutate_session_flight_plan: missing staged paged commit helper".to_string());
    }
    for function in [
        "activate_next_leg_in_session",
        "stop_navigation_in_session",
        "suspend_sequencing_in_session",
        "unsuspend_sequencing_in_session",
        "sequence_active_leg_in_session",
    ] {
        let body = function_body(source, function);
        if !body.contains("mutate_session_flight_plan") {
            violations.push(format!(
                "{function}: bypasses the common flight-plan mutation boundary"
            ));
        }
    }
    assert!(
        violations.is_empty(),
        "paged flight-plan mutations must be side-effect-free on NeedResources and return invalidations instead of snapshots:\n{}",
        violations.join("\n")
    );
}

#[test]
fn platform_flight_plan_mutations_do_not_resync_guidance_after_core_mutation() {
    let web = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let android = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt",
    );
    let mut violations = Vec::new();

    for method in [
        "performMapSelectionAction",
        "insertWaypointAtFlightPlanRow",
        "appendFlightPlanEntry",
        "insertAirwayAtFlightPlanRow",
        "selectProcedureAtFlightPlanRow",
        "loadPlateProcedure",
        "performFlightPlanRowAction",
        "activateNextLeg",
        "stopNavigation",
        "suspendSequencing",
        "unsuspendSequencing",
        "sequenceActiveLeg",
    ] {
        let body = balanced_block_after_marker(&web, &format!("{method}: async"));
        if body.contains("syncGuidanceGeometry") {
            violations.push(format!(
                "web {method} resyncs guidance outside core mutation"
            ));
        }
    }

    for method in [
        "performMapSelectionAction",
        "insertWaypointAtFlightPlanRow",
        "appendFlightPlanEntry",
        "insertAirwayAtFlightPlanRow",
        "selectProcedureAtFlightPlanRow",
        "loadPlateProcedure",
        "performFlightPlanRowAction",
        "activateNextLeg",
        "stopNavigation",
        "suspendSequencing",
        "unsuspendSequencing",
        "sequenceActiveLeg",
    ] {
        let body = balanced_block_after_marker(&android, &format!("fun {method}"));
        if body.contains("syncGuidanceGeometry") {
            violations.push(format!(
                "android {method} resyncs guidance outside core mutation"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "flight-plan mutations must let core update guidance and surface one invalidation-driven refresh:\n{}",
        violations.join("\n")
    );
}

#[test]
fn platform_adapters_use_paged_loops_for_paged_session_exports() {
    let web = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let android = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt",
    );
    let paged_web_exports = [
        "load_raster_map_catalog_in_session",
        "select_map_family_in_session",
        "perform_map_selection_action_in_session",
        "perform_flight_plan_command_in_session",
        "query_flight_plan_in_session",
        "sync_guidance_geometry_in_session",
        "project_flight_plan_route_in_session",
        "set_situation_in_session_paged",
        "tick_bad_autopilot_in_session_paged",
        "register_ownship_source_in_session_paged",
        "update_ownship_source_status_in_session_paged",
        "push_situation_sample_in_session_paged",
        "select_ownship_source_in_session_paged",
        "load_playback_trace_in_session_paged",
        "play_playback_in_session_paged",
        "pause_playback_in_session_paged",
        "seek_playback_in_session_paged",
        "set_playback_rate_in_session_paged",
        "tick_playback_in_session_paged",
        "set_map_layer_visibility_in_session_paged",
        "set_map_layer_enabled_in_session_paged",
    ];
    let mut violations = Vec::new();
    for export in paged_web_exports {
        let needle = format!("this.module.{export}");
        let Some(index) = web.find(&needle) else {
            violations.push(format!("web missing {export}"));
            continue;
        };
        let window = &web[index.saturating_sub(220)..web.len().min(index + 420)];
        if !window.contains("runCoreHadSessionOperation")
            && !window.contains("runSessionOperation")
            && !window.contains("runFlightPlanMutation")
        {
            violations.push(format!(
                "web calls {export} without runCoreHadSessionOperation"
            ));
        }
    }

    let paged_android_exports = [
        "loadRasterMapCatalogInSessionJson",
        "selectMapFamilyInSessionJson",
        "performMapSelectionActionInSessionJson",
        "performFlightPlanCommandInSessionJson",
        "queryFlightPlanInSessionJson",
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
        if !window.contains("runPagedSessionOperation") && !window.contains("runPagedSnapshot") {
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
        "loadPlaybackTraceInSessionPagedJson",
        "playPlaybackInSessionPagedJson",
        "pausePlaybackInSessionPagedJson",
        "seekPlaybackInSessionPagedJson",
        "setPlaybackRateInSessionPagedJson",
        "tickPlaybackInSessionPagedJson",
        "tickBadAutopilotInSessionPagedJson",
        "setMapLayerVisibilityInSessionPagedJson",
        "setMapLayerEnabledInSessionPagedJson",
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
fn flight_plan_platform_boundary_is_uid_based_and_singular() {
    let session = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let wasm = read_repo_file("ui/core-rust/crates/app-wasm/src/lib.rs");
    let ffi = read_repo_file("ui/core-rust/crates/app-ffi/src/lib.rs");
    let web = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let android = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt",
    );
    let android_plan =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/FlightPlanPage.kt");
    let web_types = read_repo_file("ui/web-app/src/domain/types.ts");
    let android_models =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/Models.kt");
    let android_wire =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/WireModels.kt");

    for (platform, source, command_name, query_name) in [
        (
            "WASM",
            wasm.as_str(),
            "perform_flight_plan_command_in_session",
            "query_flight_plan_in_session",
        ),
        (
            "JNI",
            ffi.as_str(),
            "performFlightPlanCommandInSession",
            "queryFlightPlanInSession",
        ),
    ] {
        assert!(
            source.contains(command_name) && source.contains(query_name),
            "{platform} must expose the shared flight-plan command/query dispatchers"
        );
        for legacy in [
            "insertWaypointAtFlightPlanRowInSession",
            "suggestWaypointIdentifiersAtFlightPlanRowInSession",
            "insertAirwayAtFlightPlanRowInSession",
            "selectProcedureAtFlightPlanRowInSession",
            "loadPlateProcedureInSession",
            "restoreDirectToInSession",
            "performFlightPlanRowActionInSession",
        ] {
            assert!(
                !source.contains(legacy),
                "{platform} retains legacy per-operation flight-plan export {legacy}"
            );
        }
        assert!(
            !source.contains("empty_flight_plan") && !source.contains("emptyFlightPlan"),
            "{platform} must not round-trip a core-created FlightPlan through platform code"
        );
    }

    for (platform, source) in [("web", web.as_str()), ("Android", android.as_str())] {
        assert!(
            source.contains("row_uid") && source.contains("action_uid"),
            "{platform} flight-plan commands must identify rows and actions by opaque UID"
        );
        assert!(
            !source.contains("\"component_index\"") && !source.contains("\"leg_index\""),
            "{platform} must not construct index-addressed flight-plan commands"
        );
    }

    for kind in [
        "insert_waypoint_at_row",
        "append_entry",
        "insert_airway_at_row",
        "select_procedure_at_row",
        "load_plate_procedure",
        "restore_direct_to",
        "perform_row_action",
        "activate_next_leg",
        "stop_navigation",
        "suspend_sequencing",
        "unsuspend_sequencing",
        "sequence_active_leg",
        "chart_page_state",
        "suggest_waypoint_identifiers_at_row",
        "preview_entry",
        "prepare_airway_presentation_at_row",
        "describe_plate_procedure_loads",
    ] {
        assert!(
            web.contains(&format!("kind: \"{kind}\"")),
            "web is missing shared flight-plan command/query {kind}"
        );
        assert!(
            android.contains(&format!("put(\"kind\", \"{kind}\")")),
            "Android is missing shared flight-plan command/query {kind}"
        );
    }

    assert!(
        android_plan.contains("selectedWaypointUid")
            && !android_plan.contains("selectedWaypointIndex"),
        "Android tray selection must follow the selected row UID across reorders"
    );
    assert!(
        !web.contains("runSessionCall"),
        "web must not retain the old mirrored-session retry wrapper"
    );
    assert!(
        !web_types.contains("export type FlightPlan = {")
            && !android_models.contains("data class FlightPlan(")
            && !android_wire.contains("data class WireFlightPlan("),
        "platform models must not mirror core's authoritative FlightPlan"
    );
    assert!(
        session.contains("#[cfg_attr(not(test), serde(skip))]\n    pub app_state: AppState"),
        "authoritative AppState may be retained for core unit diagnostics only when production serialization skips it"
    );
    for (platform, source) in [
        ("web", web_types.as_str()),
        ("Android domain", android_models.as_str()),
        ("Android wire", android_wire.as_str()),
    ] {
        assert!(
            !source.contains("active_leg_index")
                && !source.contains("activeLegIndex")
                && !source.contains("component_index")
                && !source.contains("componentIndex"),
            "{platform} must not expose core-internal flight-plan indices"
        );
    }
}

#[test]
fn production_session_snapshot_wire_omits_authoritative_flight_plan_state() {
    let init = app_core::create_ui_session(app_core::FlightPlan::empty(), &[], None, None)
        .expect("create core session");
    let wire = serde_json::to_value(&init.snapshot).expect("serialize session snapshot");
    app_core::destroy_session(init.handle);

    assert!(
        wire.get("app_state").is_none(),
        "production session snapshots must not serialize authoritative AppState"
    );
    let plan_ui = &wire["app_ui_state"]["active_plan"];
    assert!(plan_ui.get("plan_id").is_some());
    assert!(plan_ui.get("plan_version").is_some());
    assert!(plan_ui.get("route_components").is_none());
    assert!(plan_ui.get("resolved_legs").is_none());
    assert!(plan_ui.get("guidance").is_some());
}

#[test]
fn platform_adapters_do_not_call_plain_had_sensitive_snapshot_exports() {
    let web = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let android = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt",
    );
    let mut violations = Vec::new();

    for export in [
        "set_situation_in_session",
        "tick_bad_autopilot_in_session",
        "register_ownship_source_in_session",
        "update_ownship_source_status_in_session",
        "push_situation_sample_in_session",
        "select_ownship_source_in_session",
        "set_map_layer_visibility_in_session",
        "set_map_layer_enabled_in_session",
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
        "tickBadAutopilotInSessionJson",
        "setMapLayerVisibilityInSessionJson",
        "setMapLayerEnabledInSessionJson",
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

#[test]
fn app_core_runtime_code_does_not_use_host_clock_apis() {
    let root = repo_root().join("ui/core-rust/crates/app-core/src");
    let mut files = Vec::new();
    collect_rust_files(&root, &mut files);

    let forbidden = [
        "Instant::now(",
        "SystemTime::now(",
        "use std::time::Instant",
        "std::time::Instant",
        "time::Instant",
    ];
    let mut violations = Vec::new();
    for file in files {
        let source_text = read_repo_path(&file);
        let source = strip_rust_tests(&source_text);
        for (line_index, line) in source.lines().enumerate() {
            for needle in forbidden {
                if line.contains(needle) {
                    let relative = file.strip_prefix(repo_root()).unwrap_or(&file);
                    violations.push(format!(
                        "{}:{}: {}",
                        relative.display(),
                        line_index + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "app-core must not call host clock APIs directly; inject platform time through app_core::set_core_clock_ms or explicit inputs:\n{}",
        violations.join("\n")
    );
}

#[test]
fn platform_sse_transports_do_not_define_timing_policy() {
    let android =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/LiveFeedCache.kt");
    let web = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let runtime = read_repo_file("ui/core-rust/crates/app-core/src/live_feed_runtime.rs");
    let daemon = read_repo_file("product/preprocessor/live-feeds-daemon/src/main.rs");

    for forbidden in [
        "LiveFeedSseConnectTimeoutMs",
        "LiveFeedSseIdleTimeoutMs",
        ".connectTimeout(5_000",
        ".readTimeout(65_000",
    ] {
        assert!(
            !android.contains(forbidden),
            "Android defines SSE timing policy instead of executing core's policy: {forbidden}"
        );
    }
    assert!(
        android.contains("transportPolicy.connectTimeoutMs")
            && android.contains("transportPolicy.idleTimeoutMs"),
        "Android must construct the SSE transport from core's serialized policy"
    );
    assert!(
        !web.contains("SSE_CONNECT_TIMEOUT") && !web.contains("SSE_IDLE_TIMEOUT"),
        "web must not define independent SSE timing constants"
    );
    assert!(
        runtime.contains("AEROBAG_SSE_TRANSPORT_POLICY"),
        "app-core reconnect decisions must consume the shared SSE policy"
    );
    assert!(
        daemon.contains("AEROBAG_SSE_TRANSPORT_POLICY.heartbeat_interval_ms")
            && !daemon.contains("recv_timeout(Duration::from_secs(30))"),
        "the live-feed daemon heartbeat must consume the shared SSE policy"
    );
}
