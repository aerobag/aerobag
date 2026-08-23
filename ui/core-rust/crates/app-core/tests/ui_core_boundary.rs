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
fn ui_session_exposes_coordinator_state_explicitly() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let coordinator = balanced_block_after_marker(session, "struct SessionCoordinatorModel");

    assert!(
        ui_session.contains("coordinator: SessionCoordinatorModel"),
        "UiSession must name its residual cross-domain coordinator state"
    );
    assert!(
        !session.contains("impl Deref for UiSession")
            && !session.contains("impl DerefMut for UiSession"),
        "UiSession must not make coordinator fields look like directly owned domain state"
    );
    for field in [
        "session_revision",
        "content_policy",
        "last_content_report",
        "chart_page_state",
        "platform_capabilities",
        "persistence_storage",
        "debug_state",
        "cycle_product_freshness",
        "wall_clock_epoch_ms",
        "altitude_planner_wind_selection",
        "time_display_mode",
    ] {
        assert!(
            coordinator.contains(field),
            "coordinator field {field} must remain explicit and reviewable"
        );
    }
}

#[test]
fn session_projection_versions_are_core_owned_and_transactional() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let projection = read_repo_file("ui/core-rust/crates/app-core/src/session_projection.rs");
    let versions = balanced_block_after_marker(
        strip_rust_tests(&projection),
        "struct SessionProjectionVersions",
    );
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let checkpoint =
        balanced_block_after_marker(session, "struct SessionModelTransactionCheckpoint");
    let snapshot = balanced_block_after_marker(session, "pub struct UiSessionSnapshot");

    for group in [
        "envelope",
        "nav_data",
        "application_shell",
        "flight_plan",
        "ownship",
        "flight_data",
        "situation",
        "charts",
        "map",
        "status",
        "settings",
        "cloud",
        "packages",
        "home",
        "debug",
    ] {
        assert!(
            versions.contains(group),
            "projection version group {group} must remain explicit"
        );
    }
    assert!(
        ui_session.contains("projection_versions: SessionProjectionVersionState")
            && checkpoint.contains("projection_versions: SessionProjectionVersionState"),
        "projection versions must be session-owned and roll back with failed transactions"
    );
    assert!(
        !snapshot.contains("projection_versions"),
        "the full startup/resynchronization snapshot must not expose partial-update bookkeeping"
    );
    assert!(
        projection.contains("SessionProjectionDependencies")
            && !projection.contains("serde_json::to_")
            && !projection.contains("serde_json::from_"),
        "core must derive versions from typed dependencies, not serialized snapshot comparison"
    );
}

#[test]
fn session_updates_are_generated_and_assembled_from_core_projection_versions() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let contracts = read_repo_file("ui/core-rust/crates/app-ui-contracts/src/session.rs");
    let update = balanced_block_after_marker(&contracts, "pub struct UiSessionUpdate");
    let assembler = function_body(session, "assemble_session_update");
    let transaction = function_body(session, "run_session_model_transaction_projecting");

    for group in [
        "nav_data",
        "application_shell",
        "flight_plan",
        "ownship",
        "flight_data",
        "situation",
        "charts",
        "map",
        "status",
        "settings",
        "cloud",
        "packages",
        "home",
        "debug",
    ] {
        assert!(
            update.contains(&format!("pub {group}: Option<UiSessionProjectionPatch>")),
            "generated session update group {group} must remain explicit"
        );
        assert!(
            assembler.contains(&format!("previous.{group}"))
                && assembler.contains(&format!("current.{group}")),
            "core must decide {group} inclusion from its projection versions"
        );
    }
    assert!(
        contracts.contains("pub struct UiSessionProjectionPatch")
            && update.contains("pub session_revision: u64")
            && session.contains("try_project_session_update")
            && session.contains("serde_json::to_value(update)"),
        "ordinary mutation results must carry the canonical generated update envelope"
    );
    assert!(
        !session.contains("session_mutation_snapshot_value")
            && !session.contains("ProjectedSessionMutation"),
        "ordinary mutation results must not serialize a transitional full snapshot"
    );
    assert!(
        transaction.contains("previous_versions")
            && transaction.contains("checkpoint.rollback(session)"),
        "partial-update decisions must share aggregate transaction rollback semantics"
    );
    assert!(
        read_repo_file("ui/core-rust/schemas/session-update-wire.schema.json")
            .contains("org.aerobag.ui-wire.session-update")
            && read_repo_file("ui/web-app/src/generated/sessionUpdateWire.ts")
                .contains("@generated by tools/generate-ui-wire-types.mjs"),
        "session update platform types must be generated from the core-owned schema"
    );
    let generator = read_repo_file("tools/generate-ui-wire-types.mjs");
    let web_adapter = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let android_adapter = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt",
    );
    let web_accumulator = read_repo_file("ui/web-app/src/domain/sessionUpdateAccumulator.ts");
    let android_accumulator = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/SessionUpdateAccumulator.kt",
    );
    assert!(
        generator.contains("sessionUpdateGroupProperties")
            && web_accumulator.contains("UI_SESSION_UPDATE_GROUPS")
            && android_accumulator.contains("projectionPatches()"),
        "platform update accumulators must consume the schema-generated group inventory"
    );
    assert!(
        web_adapter.contains("snapshotAccumulator.applyOrResync")
            && android_adapter.contains("snapshotAccumulator.applyOrResync")
            && web_adapter.contains("replaceFullSnapshot")
            && android_adapter.contains("replaceFullSnapshot")
            && web_adapter.contains("get_session_snapshot_paged(handle)")
            && android_adapter.contains("getSessionSnapshotPagedJson(handle)"),
        "both adapters must distinguish revisioned mutation updates from explicit full-snapshot recovery"
    );
    assert!(
        !session.contains("fn run_session_model_update"),
        "model mutations must not retain an update-free result path"
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
fn map_layer_choices_and_labels_are_core_owned() {
    let web = read_repo_file("ui/web-app/src/App.tsx");
    let android =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt");

    assert!(
        web.contains("mapLayerState.options.map((option)")
            && android.contains("mapLayerState.options.map { option ->"),
        "platform layer trays must render the ordered choices and labels exported by core"
    );
    for label in [
        "Observations",
        "Vectors",
        "NEXRAD",
        "ADS-B Traffic",
        "Terrain Warning",
        "World Map",
        "Offline Regions",
    ] {
        assert!(
            !web.contains(&format!("label: \"{label}\""))
                && !android.contains(&format!("label = \"{label}\"")),
            "platform layer tray redeclares core label {label}"
        );
    }
}

#[test]
fn android_public_resource_adapter_does_not_classify_application_resources() {
    let android =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/RuntimeFetch.kt");
    assert!(
        !android.contains("resource.id.startsWith(")
            && !android.contains("resourceId.startsWith(")
            && !android.contains("requireAndroidPublicUrlAllowed"),
        "Android must execute core's typed resource source without knowing application resource families"
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
        .filter(|needle| web_runtime.contains(needle) || android_provider.contains(needle))
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
    let airport_index = balanced_block_after_marker(&overlay, "pub struct NotamDisplayIndex");
    let android_runtime = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/RetainedLiveFeedRuntime.kt",
    );
    let promote = balanced_block_after_marker(&android_runtime, "private suspend fun promote(");
    let restore = balanced_block_after_marker(&android_runtime, "fun start()");
    let restore_products = balanced_block_after_marker(
        &android_runtime,
        "private suspend fun restoreInstalledProducts(",
    );
    let android_cache =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/LiveFeedCache.kt");

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
        restore.contains("restoreInstalledProducts(installed, onRestored)")
            && restore_products.contains("LiveFeedCacheStore.restore")
            && android_cache.contains("launch(Dispatchers.IO)"),
        "Android must rebuild durable NOTAM state off main during startup"
    );
}

#[test]
fn android_cache_promotion_does_not_revalidate_already_validated_packages() {
    let ffi = read_repo_file("ui/core-rust/crates/app-ffi/src/lib.rs");
    let install = balanced_block_after_marker(
        &ffi,
        "pub fn live_feed_cache_install_product_in_session_json(",
    );
    let session = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");

    assert!(
        install.contains("install_validated_live_feed_installed_state_in_session"),
        "the cache-to-session path must not repeat package and canonical-state validation"
    );
    assert!(
        session.contains("prepare_durable_live_feed_install_inner(installed, false)"),
        "validated cache promotion must select the no-repeat-validation preparation path"
    );
}

#[test]
fn paged_flight_plan_mutations_commit_only_after_guidance_projection() {
    let source_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let source = strip_rust_tests(&source_text);
    let definition_functions = [
        "insert_waypoint_at_flight_plan_row_in_session",
        "insert_airway_at_flight_plan_row_in_session",
        "select_procedure_at_flight_plan_row_in_session",
        "load_plate_procedure_in_session",
        "append_flight_plan_entry_in_session",
    ];
    let mut violations = Vec::new();
    for function in definition_functions {
        let body = function_body(source, function);
        if !body.contains("commit_session_flight_plan_edit_with_invalidations_outcome") {
            violations.push(format!(
                "{function}: missing staged definition-edit commit helper"
            ));
        }
        if body.contains("replace_session_flight_plan(session") {
            violations.push(format!(
                "{function}: commits before paged snapshot projection"
            ));
        }
    }

    let row_action = function_body(source, "perform_flight_plan_row_action_in_session");
    for helper in [
        "commit_session_flight_plan_edit_with_invalidations_outcome",
        "commit_session_navigation_update_with_invalidations_outcome",
    ] {
        if !row_action.contains(helper) {
            violations.push(format!(
                "perform_flight_plan_row_action_in_session: missing domain commit helper {helper}"
            ));
        }
    }

    let definition_helper =
        function_body(source, "mutate_session_flight_plan_definition_controller");
    if !definition_helper.contains("commit_session_flight_plan_edit_with_invalidations_outcome") {
        violations.push(
            "mutate_session_flight_plan_definition_controller: missing staged definition-edit commit helper"
                .to_string(),
        );
    }
    let navigation_helper = function_body(source, "mutate_session_navigation_controller");
    if !navigation_helper.contains("commit_session_navigation_update_with_invalidations_outcome") {
        violations.push(
            "mutate_session_navigation_controller: missing staged navigation commit helper"
                .to_string(),
        );
    }
    for function in [
        "activate_next_leg_in_session",
        "stop_navigation_in_session",
        "suspend_sequencing_in_session",
        "unsuspend_sequencing_in_session",
        "sequence_active_leg_in_session",
    ] {
        let body = function_body(source, function);
        if !body.contains("mutate_session_navigation_controller") {
            violations.push(format!(
                "{function}: bypasses the navigation-only mutation boundary"
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
fn durable_session_writes_are_owned_by_transaction_helpers() {
    let source_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let source = strip_rust_tests(&source_text);
    let persistence_write_occurrences = source
        .match_indices("write_session_persistence_to_storage(")
        .count();
    assert_eq!(
        persistence_write_occurrences, 3,
        "persistence must have one definition and calls only in the two transaction executors"
    );
    for executor in [
        "run_durable_session_model_value_transaction",
        "run_session_model_transaction_projecting",
    ] {
        assert!(
            function_body(source, executor).contains("write_session_persistence_to_storage"),
            "{executor} must own its durable persistence write"
        );
    }

    for function in [
        "perform_settings_action_in_session",
        "perform_aircraft_library_action_in_session",
        "accept_disclaimer_in_session",
    ] {
        assert!(
            function_body(source, function).contains("run_session_model_transaction"),
            "{function} must roll back its live model when projection or persistence fails"
        );
    }
    assert!(
        function_body(source, "configure_platform_capabilities_in_session")
            .contains("run_session_model_transaction_without_persistence"),
        "platform configuration must roll back when settings restore fails"
    );
    assert!(
        function_body(source, "take_cloud_provider_request_in_session")
            .contains("run_durable_session_model_value_transaction"),
        "taking durable provider work must restore the request when persistence fails"
    );
}

#[test]
fn debug_flags_use_only_the_core_settings_action_boundary() {
    for path in [
        "ui/web-app/src/domain/appCoreAdapter.ts",
        "ui/web-app/src/domain/workerAppCoreAdapter.ts",
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt",
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeBindings.kt",
        "ui/core-rust/crates/app-wasm/src/lib.rs",
        "ui/core-rust/crates/app-ffi/src/lib.rs",
    ] {
        let source = read_repo_file(path);
        assert!(
            !source.contains("setDebugFlag") && !source.contains("set_debug_flag_in_session"),
            "{path} must not expose a second, unsynchronized debug-setting mutation path"
        );
    }

    let session = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let settings_action = function_body(&session, "perform_settings_action_in_session");
    assert!(settings_action.contains("record_local_debug_flag"));
}

#[test]
fn settings_state_and_projection_are_owned_by_settings_controller() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let controller = read_repo_file("ui/core-rust/crates/app-core/src/settings_controller.rs");
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let model = balanced_block_after_marker(session, "struct SessionCoordinatorModel");
    assert!(
        ui_session.contains("settings: SettingsController"),
        "UiSession must compose settings through its top-level controller"
    );
    assert!(
        !model.contains("settings: SettingsController")
            && !model.contains("settings_preferences")
            && !model.contains("settings_storage"),
        "settings controller or raw settings state must not return to SessionCoordinatorModel"
    );
    assert!(
        controller.contains("struct SettingsModelCheckpoint")
            && controller.contains("pub fn checkpoint_model")
            && controller.contains("pub fn rollback_model"),
        "settings must participate in session transactions through its controller checkpoint"
    );
    assert!(
        !session.contains("settings_preferences")
            && !session.contains("fn project_settings_page_state")
            && !session.contains("fn project_display_policy")
            && !session.contains("fn project_disclaimer_state"),
        "settings state and projection policy must remain inside SettingsController"
    );
    assert_eq!(
        session_text
            .match_indices(".persistent_preferences()")
            .count(),
        1,
        "only aggregate persistence may read the controller's persistent preferences"
    );
    let action = function_body(session, "perform_settings_action_in_session");
    assert!(
        action.contains(".settings") && action.contains(".perform_action"),
        "session settings actions must delegate to SettingsController"
    );
    assert!(
        function_body(session, "accept_disclaimer_in_session")
            .contains("session.settings.accept_disclaimer"),
        "disclaimer acceptance must delegate to SettingsController"
    );
    let snapshot = function_body(session, "try_snapshot_for_session");
    assert!(
        snapshot.contains(".settings") && snapshot.contains(".project"),
        "session snapshots must consume the controller projection"
    );
}

#[test]
fn weather_state_runtime_and_projection_are_owned_by_weather_controller() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let controller = read_repo_file("ui/core-rust/crates/app-core/src/weather_controller.rs");
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let model = balanced_block_after_marker(session, "struct SessionCoordinatorModel");
    let runtime = balanced_block_after_marker(session, "struct SessionRuntime");

    assert!(
        ui_session.contains("weather: WeatherController"),
        "UiSession must compose weather through one controller"
    );
    for field in [
        "live_feeds",
        "live_feed_connection",
        "metar_payload",
        "taf_payload",
        "notam_display_index",
        "tfr_payload",
        "nexrad_installed",
        "obstacle_had",
        "forecast_atmosphere_state",
        "forecast_atmosphere",
    ] {
        assert!(
            !model.contains(field) && !runtime.contains(field),
            "weather field {field} must not return to SessionCoordinatorModel or SessionRuntime"
        );
    }
    assert!(
        controller.contains("struct WeatherModel")
            && controller.contains("struct WeatherRuntime")
            && controller.contains("pub(crate) struct WeatherController"),
        "WeatherController must own its lightweight model and heavy runtime"
    );
    assert!(
        !session.contains("struct NexradFrameCandidate")
            && !session.contains("fn nexrad_animation_for_frames")
            && !session.contains("fn nexrad_frame_age_values"),
        "NEXRAD timeline and projection policy must remain in WeatherController"
    );
    assert!(
        !session.contains("session.live_feeds") && !session.contains(".weather.runtime."),
        "session code must use WeatherController APIs rather than raw weather storage"
    );
    let checkpoint =
        balanced_block_after_marker(session, "struct SessionModelTransactionCheckpoint");
    assert!(
        checkpoint.contains("weather: WeatherModelCheckpoint"),
        "session transactions must checkpoint the lightweight weather model"
    );
    let rollback = function_body(session, "rollback");
    assert!(
        rollback.contains("session.weather.rollback_model"),
        "session transaction rollback must restore the weather model"
    );
    assert!(
        function_body(session, "try_snapshot_for_session").contains("project_weather_for_session"),
        "full snapshots must consume the cached weather projection"
    );
}

#[test]
fn map_state_runtime_and_projection_are_owned_by_map_controller() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let controller = read_repo_file("ui/core-rust/crates/app-core/src/map_controller.rs");
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let model = balanced_block_after_marker(session, "struct SessionCoordinatorModel");
    let runtime = balanced_block_after_marker(session, "struct SessionRuntime");

    assert!(
        ui_session.contains("map: MapController"),
        "UiSession must compose map behavior through one controller"
    );
    for field in [
        "map_layer_state",
        "map_overlay_config",
        "vector_manifest_loaded",
        "raster_map_catalog",
        "vector_tile_cache",
        "airspace_feature_cache",
        "terrain_source_tile_cache",
        "agl_terrain_resource_ids_in_flight",
    ] {
        assert!(
            !model.contains(field) && !runtime.contains(field),
            "map field {field} must not return to SessionCoordinatorModel or SessionRuntime"
        );
    }
    assert!(
        controller.contains("struct MapModel")
            && controller.contains("struct MapRuntime")
            && controller.contains("pub(crate) struct MapController"),
        "MapController must own its lightweight model and heavy runtime"
    );
    assert!(
        controller.contains("fn map_layer_disabled_reason")
            && !session.contains("fn map_layer_disabled_reason")
            && !session.contains("fn map_layer_toggle_mut"),
        "map-layer policy must remain in MapController"
    );
    assert!(
        !session.contains("session.map_layer_state")
            && !session.contains("session.map_overlay_config")
            && !session.contains("session.raster_map_catalog")
            && !session.contains(".map.runtime."),
        "session code must use MapController APIs rather than raw map storage"
    );
    let checkpoint =
        balanced_block_after_marker(session, "struct SessionModelTransactionCheckpoint");
    assert!(
        checkpoint.contains("map: MapModelCheckpoint"),
        "session transactions must checkpoint the lightweight map model"
    );
    let rollback = function_body(session, "rollback");
    assert!(
        rollback.contains("session.map.rollback_model"),
        "session transaction rollback must restore the map model"
    );
    assert!(
        function_body(session, "try_snapshot_for_session").contains("session.map.project"),
        "full snapshots must consume the cached map projection"
    );
}

#[test]
fn situation_state_and_projection_are_owned_by_situation_controller() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let controller = read_repo_file("ui/core-rust/crates/app-core/src/situation_controller.rs");
    let state = read_repo_file("ui/core-rust/crates/app-core/src/state.rs");
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let model = balanced_block_after_marker(session, "struct SessionCoordinatorModel");

    assert!(
        ui_session.contains("situation: SituationController"),
        "UiSession must compose situation behavior through one controller"
    );
    for field in [
        "app_state",
        "ownship",
        "playback",
        "plan_preview",
        "bad_autopilot",
        "map_follow",
    ] {
        assert!(
            !model.contains(field),
            "situation field {field} must not return to SessionCoordinatorModel"
        );
    }
    assert!(
        controller.contains("struct SituationModel")
            && controller.contains("pub(crate) struct SituationController")
            && controller.contains("pub(crate) struct SituationProjection"),
        "SituationController must own its model and cached UI projection"
    );
    assert!(
        !session.contains("session.app_state")
            && !session.contains("session.plan_preview")
            && !session.contains("session.bad_autopilot")
            && !session.contains("session.map_follow"),
        "session code must use SituationController APIs rather than raw situation storage"
    );
    let checkpoint =
        balanced_block_after_marker(session, "struct SessionModelTransactionCheckpoint");
    assert!(
        checkpoint.contains("situation: SituationModelCheckpoint"),
        "session transactions must checkpoint the situation model"
    );
    assert!(
        function_body(session, "rollback").contains("session.situation.rollback_model"),
        "session transaction rollback must restore the situation model"
    );
    assert!(
        function_body(session, "try_snapshot_for_session")
            .contains("project_situation_for_session"),
        "full snapshots must consume the cached situation projection"
    );
    assert!(
        function_body(&session_text, "project_session_app_ui_state")
            .contains("project_app_ui_state_from_ui_parts")
            && function_body(&session_text, "project_session_app_ui_state")
                .contains("situation_projection.ownship"),
        "aggregate UI projection must consume SituationController's ownship projection"
    );
    assert!(
        state.contains("pub struct AppState") && state.contains("pub ownship: OwnshipState"),
        "AppState remains a public compatibility DTO while session storage is decomposed"
    );
}

#[test]
fn flight_plan_state_and_projection_are_owned_by_flight_plan_controller() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let controller = read_repo_file("ui/core-rust/crates/app-core/src/flight_plan_controller.rs");
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let model = balanced_block_after_marker(session, "struct SessionCoordinatorModel");

    assert!(
        ui_session.contains("flight_plan: FlightPlanController"),
        "UiSession must compose flight-plan behavior through one controller"
    );
    for field in [
        "active_plan",
        "guidance_leg_geometry",
        "flight_plan_route_revision",
    ] {
        assert!(
            !model.contains(field),
            "flight-plan field {field} must not return to SessionCoordinatorModel"
        );
    }
    assert!(
        controller.contains("struct FlightPlanModel")
            && controller.contains("pub(crate) struct FlightPlanController")
            && controller.contains("pub(crate) struct FlightPlanProjection"),
        "FlightPlanController must own its model and cached UI projection"
    );
    assert!(
        !session.contains("session.active_plan")
            && !session.contains("session.guidance_leg_geometry")
            && !session.contains("session.flight_plan_route_revision"),
        "session code must use FlightPlanController APIs rather than raw flight-plan storage"
    );
    let checkpoint =
        balanced_block_after_marker(session, "struct SessionModelTransactionCheckpoint");
    assert!(
        checkpoint.contains("flight_plan: FlightPlanModelCheckpoint"),
        "session transactions must checkpoint the flight-plan model"
    );
    assert!(
        function_body(session, "rollback").contains("session.flight_plan.rollback_model"),
        "session transaction rollback must restore the flight-plan model"
    );
    let aggregate_projection = function_body(&session_text, "project_session_app_ui_state");
    assert!(
        aggregate_projection.contains("session.flight_plan.project")
            && aggregate_projection.contains("flight_plan_projection.projection.ui_state"),
        "aggregate UI projection must consume FlightPlanController's cached projection"
    );
    assert!(
        function_body(session, "project_flight_plan_route_in_session").contains(".flight_plan")
            && function_body(session, "project_flight_plan_route_in_session")
                .contains(".project_route"),
        "route projection must be delegated to FlightPlanController"
    );
    assert!(
        function_body(session, "perform_flight_plan_row_action_in_session")
            .contains("flight_plan.plan_after_row_action"),
        "core-owned row actions must be interpreted by FlightPlanController"
    );
}

#[test]
fn nav_data_state_runtime_and_maintenance_are_owned_by_nav_data_controller() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let controller = read_repo_file("ui/core-rust/crates/app-core/src/nav_data_controller.rs");
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let model = balanced_block_after_marker(session, "struct SessionCoordinatorModel");
    let runtime = balanced_block_after_marker(session, "struct SessionRuntime");

    assert!(
        ui_session.contains("nav_data: NavDataController"),
        "UiSession must compose NAVDB behavior through one controller"
    );
    for field in [
        "nav_data_epoch",
        "nav_db_advance_blocked",
        "nav_kv_store_id",
        "nav_kv_store",
        "nav_db_artifact",
        "nav_data_generation",
    ] {
        assert!(
            !model.contains(field) && !runtime.contains(field),
            "NAVDB field {field} must not return to SessionCoordinatorModel or SessionRuntime"
        );
    }
    assert!(
        controller.contains("struct NavDataModel")
            && controller.contains("struct NavDataRuntime")
            && controller.contains("pub(crate) struct NavDataController")
            && controller.contains("enum NavDataMaintenanceDecision"),
        "NavDataController must own its model, heavy runtime, and maintenance policy"
    );
    for access in [
        "session.nav_kv_store",
        "session.nav_db_artifact",
        "session.nav_data_epoch",
        "session.nav_kv_store_id",
        "session.nav_db_advance_blocked",
        "runtime.nav_data_generation",
    ] {
        assert!(
            !session.contains(access),
            "session code must not regain raw NAVDB access through {access}"
        );
    }
    let checkpoint =
        balanced_block_after_marker(session, "struct SessionModelTransactionCheckpoint");
    assert!(
        checkpoint.contains("nav_data: NavDataModelCheckpoint"),
        "session transactions must checkpoint the lightweight NAVDB model"
    );
    assert!(
        function_body(session, "rollback").contains("session.nav_data.rollback_model"),
        "session transaction rollback must restore the NAVDB model"
    );
    assert!(
        function_body(session, "attach_nav_kv_store_to_session_with_open_result")
            .contains("session.nav_data.attach"),
        "NAVDB attachment must delegate to NavDataController"
    );
    let page_delivery = function_body(session, "insert_nav_kv_page_for_attached_sessions");
    assert!(
        page_delivery.contains(".nav_data") && page_delivery.contains(".insert_page_if_attached"),
        "NAVDB page delivery must delegate to NavDataController"
    );
    let maintenance = function_body(session, "maintain_nav_db_in_session_at_epoch_ms");
    assert!(
        maintenance.contains(".nav_data") && maintenance.contains(".maintenance_decision"),
        "NAVDB maintenance decisions must be owned by NavDataController"
    );
    let advance = function_body(session, "advance_nav_kv_store_in_session_with_open_result");
    assert!(
        advance.contains("nav_data: live.nav_data.candidate")
            && advance.contains("*live = candidate"),
        "NAVDB rollover must stage a controller candidate and publish it through the final session swap"
    );
}

#[test]
fn package_state_resolution_and_projection_are_owned_by_package_controller() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let controller = read_repo_file("ui/core-rust/crates/app-core/src/package_controller.rs");
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let model = balanced_block_after_marker(session, "struct SessionCoordinatorModel");

    assert!(
        ui_session.contains("packages: PackageController"),
        "UiSession must compose package behavior through one controller"
    );
    for field in [
        "resource_policy",
        "installed_package_ids",
        "publication_resolver",
        "offline_package_preferences",
    ] {
        assert!(
            !model.contains(field),
            "package field {field} must not return to SessionCoordinatorModel"
        );
    }
    assert!(
        controller.contains("struct PackageModel")
            && controller.contains("pub(crate) struct PackageController")
            && controller.contains("struct PackageProjectionCache"),
        "PackageController must own package policy, resolution, and cached projection"
    );
    for access in [
        "session.resource_policy",
        "session.installed_package_ids",
        "session.publication_resolver",
    ] {
        assert!(
            !session.contains(access),
            "session code must not regain raw package access through {access}"
        );
    }
    let checkpoint =
        balanced_block_after_marker(session, "struct SessionModelTransactionCheckpoint");
    assert!(
        checkpoint.contains("packages: PackageModelCheckpoint"),
        "session transactions must checkpoint package state and publication resolution"
    );
    assert!(
        function_body(session, "rollback").contains("session.packages.rollback_model"),
        "session transaction rollback must restore package state"
    );
    let snapshot = function_body(session, "try_snapshot_for_session");
    assert!(
        snapshot.contains(".packages") && snapshot.contains(".project()"),
        "full snapshots must consume PackageController's cached projection"
    );
    let preference_update = function_body(session, "record_offline_package_preferences_in_session");
    assert!(
        preference_update.contains("session.packages.replace_preferences")
            && preference_update.contains("record_local_offline_package_preferences"),
        "package preferences must update controller state and cloud persistence transactionally"
    );
    assert!(
        controller.contains("fn nav_db_artifact_candidates")
            && !read_repo_file("ui/core-rust/crates/app-core/src/nav_data_controller.rs")
                .contains("installed_package_ids.contains"),
        "package availability filtering must have one owner"
    );
}

#[test]
fn cloud_state_runtime_and_projection_are_owned_by_cloud_controller() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let controller = read_repo_file("ui/core-rust/crates/app-core/src/cloud_controller.rs");
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let model = balanced_block_after_marker(session, "struct SessionCoordinatorModel");

    assert!(
        ui_session.contains("cloud: CloudController"),
        "UiSession must compose cloud behavior through one controller"
    );
    assert!(
        !model.contains("cloud:") && !model.contains("CloudEngine"),
        "raw cloud state must not return to SessionCoordinatorModel"
    );
    assert!(
        controller.contains("struct CloudModel")
            && controller.contains("engine: Arc<CloudEngine>")
            && controller.contains("struct CloudProjectionCache")
            && controller.contains("pub(crate) struct CloudController"),
        "CloudController must own its provider engine and cached projection"
    );
    let checkpoint =
        balanced_block_after_marker(session, "struct SessionModelTransactionCheckpoint");
    assert!(
        checkpoint.contains("cloud: CloudModelCheckpoint"),
        "session transactions must checkpoint cloud state"
    );
    assert!(
        function_body(session, "rollback").contains("session.cloud.rollback_model"),
        "session transaction rollback must restore cloud state"
    );
    let snapshot = function_body(session, "try_snapshot_for_session");
    assert!(
        snapshot.contains("project_cloud_for_session")
            && !snapshot.contains("page_state_with_qr_scanner")
            && !snapshot.contains("status_summary("),
        "full snapshots must consume CloudController's cached projection"
    );
    let completion = function_body(session, "complete_cloud_provider_request_in_session");
    assert!(
        completion.contains("updates.offline_package_preferences")
            && completion.contains("updates.remote_flight_plan")
            && !completion.contains("remote_flight_plan()"),
        "cloud completion must expose typed domain updates to the session coordinator"
    );
    assert_eq!(
        session_text
            .match_indices("session.cloud.persistent()")
            .count(),
        1,
        "only aggregate persistence may read CloudController's persistent model"
    );
}

#[test]
fn data_status_state_actions_and_projection_are_owned_by_data_status_controller() {
    let session_text = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let session = strip_rust_tests(&session_text);
    let controller = read_repo_file("ui/core-rust/crates/app-core/src/data_status_controller.rs");
    let ui_session = balanced_block_after_marker(session, "struct UiSession {");
    let model = balanced_block_after_marker(session, "struct SessionCoordinatorModel");

    assert!(
        ui_session.contains("data_status: DataStatusController"),
        "UiSession must compose status behavior through one controller"
    );
    for field in [
        "data_status_records",
        "hushed_status_ids",
        "data_status_state",
    ] {
        assert!(
            !model.contains(field),
            "raw status field {field} must not return to SessionCoordinatorModel"
        );
    }
    assert!(
        controller.contains("struct DataStatusModel")
            && controller.contains("struct DataStatusStateCache")
            && controller.contains("struct DataStatusPageCache")
            && controller.contains("pub(crate) struct DataStatusController"),
        "DataStatusController must own status records and both cached projections"
    );
    for policy in [
        "fn project_data_status_page_state",
        "fn client_build_status_page_row",
        "fn live_feed_connection_status_page_row",
        "fn cycle_package_group_status_page_row",
        "fn chart_validity_condition",
    ] {
        assert!(
            controller.contains(policy) && !session.contains(policy),
            "status policy {policy} must remain inside DataStatusController"
        );
    }
    let checkpoint =
        balanced_block_after_marker(session, "struct SessionModelTransactionCheckpoint");
    assert!(
        checkpoint.contains("data_status: DataStatusModelCheckpoint"),
        "session transactions must checkpoint data-status state"
    );
    assert!(
        function_body(session, "rollback").contains("session.data_status.rollback_model"),
        "session transaction rollback must restore data-status state"
    );
    let action = function_body(session, "perform_status_action_in_session");
    assert!(
        action.contains(".data_status") && action.contains(".perform_action"),
        "status actions must delegate validation and hushing to DataStatusController"
    );
    let snapshot = function_body(session, "try_snapshot_for_session");
    assert!(
        snapshot.contains(".data_status.project_state")
            && snapshot.contains(".data_status.project_page"),
        "full snapshots must consume both cached status projections"
    );
    assert!(
        function_body(session, "sync_package_ui_warning_status_records")
            .contains(".replace_package_warnings"),
        "package warning interpretation must be owned by DataStatusController"
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
        "performMapSelectionUiAction",
        "insertWaypointAtFlightPlanRow",
        "appendFlightPlanEntry",
        "insertAirwayAtFlightPlanRow",
        "selectProcedureAtFlightPlanRow",
        "loadPlateProcedure",
        "performFlightPlanRowAction",
        "performFlightPlanControl",
    ] {
        let body = balanced_block_after_marker(&web, &format!("{method}: async"));
        if body.contains("syncGuidanceGeometry") {
            violations.push(format!(
                "web {method} resyncs guidance outside core mutation"
            ));
        }
    }

    for method in [
        "performMapSelectionUiAction",
        "insertWaypointAtFlightPlanRow",
        "appendFlightPlanEntry",
        "insertAirwayAtFlightPlanRow",
        "selectProcedureAtFlightPlanRow",
        "loadPlateProcedure",
        "performFlightPlanRowAction",
        "performFlightPlanControl",
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
        "perform_map_selection_ui_action_in_session",
        "perform_flight_plan_command_in_session",
        "perform_flight_plan_column_action_in_session",
        "perform_time_display_action_in_session",
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
            && !window.contains("runSessionResult")
            && !window.contains("runSessionSnapshot")
            && !window.contains("runSessionMutation")
            && !window.contains("runFlightPlanMutation")
        {
            violations.push(format!(
                "web calls {export} without a paged session operation helper"
            ));
        }
    }

    let paged_android_exports = [
        "loadRasterMapCatalogInSessionJson",
        "selectMapFamilyInSessionJson",
        "performMapSelectionUiActionInSessionJson",
        "performFlightPlanCommandInSessionJson",
        "performFlightPlanColumnActionInSessionJson",
        "performTimeDisplayActionInSessionJson",
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
        assert!(
            source.contains("performFlightPlanControl") && source.contains("perform_control"),
            "{platform} must send core-owned flight-plan controls through one generic command"
        );
        for legacy in [
            "fun activateNextLeg",
            "fun redoFlightPlanEdit",
            "fun restoreDirectTo",
            "fun sequenceActiveLeg",
            "fun stopNavigation",
            "fun suspendSequencing",
            "fun undoFlightPlanEdit",
            "fun unsuspendSequencing",
            "activateNextLeg: async",
            "redoFlightPlanEdit: async",
            "restoreDirectTo: async",
            "sequenceActiveLeg: async",
            "stopNavigation: async",
            "suspendSequencing: async",
            "undoFlightPlanEdit: async",
            "unsuspendSequencing: async",
        ] {
            assert!(
                !source.contains(legacy),
                "{platform} retains platform-specific flight-plan control method {legacy}"
            );
        }
    }

    for kind in [
        "insert_waypoint_at_row",
        "append_entry",
        "insert_airway_at_row",
        "select_procedure_at_row",
        "load_plate_procedure",
        "perform_control",
        "perform_row_action",
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
        session.contains("fn perform_flight_plan_control_in_session")
            && session.contains("FlightPlanSessionCommand::PerformControl"),
        "core must own flight-plan control dispatch"
    );
    assert!(
        !web_types.contains("export type FlightPlanControlId =")
            && !android_models.contains("enum class FlightPlanControlId")
            && !android_wire.contains("enum class WireFlightPlanControlId"),
        "platforms must consume the generated flight-plan control contract"
    );
    assert!(
        !android_plan.contains("when (controlId)") && !android_plan.contains(".coreId()"),
        "Android must not reconstruct flight-plan control dispatch or wire IDs"
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
fn flight_plan_row_action_policy_is_core_owned() {
    let session = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let wasm = read_repo_file("ui/core-rust/crates/app-wasm/src/lib.rs");
    let ffi = read_repo_file("ui/core-rust/crates/app-ffi/src/lib.rs");
    let web = read_repo_file("ui/web-app/src/App.tsx");
    let web_types = read_repo_file("ui/web-app/src/domain/types.ts");
    let android =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/FlightPlanPage.kt");
    let android_models =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/Models.kt");
    let android_wire =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/WireModels.kt");

    assert!(
        session.contains("fn registered_flight_plan_row_action")
            && session.contains("pub fn flight_plan_row_action_decision_in_session"),
        "core must register and interpret flight-plan row action UIDs"
    );
    assert!(
        wasm.contains("flight_plan_row_action_decision_in_session")
            && ffi.contains("flightPlanRowActionDecisionInSessionJson"),
        "both bridges must expose the same row-action decision boundary"
    );
    for (platform, source) in [("web", web.as_str()), ("Android", android.as_str())] {
        assert!(
            source.contains("flightPlanRowActionDecision"),
            "{platform} must ask core to interpret a selected row action"
        );
    }
    assert!(
        !web.contains("if (action.id === \"insert_before\"")
            && !web.contains("action.execution === \"core_session\"")
            && !android.contains("when (action.id)")
            && !android.contains("action.execution == \"core_session\""),
        "platform row-action dispatchers must not switch on core action IDs or execution kinds"
    );
    for (platform, source) in [
        ("web", web_types.as_str()),
        ("Android domain", android_models.as_str()),
        ("Android wire", android_wire.as_str()),
    ] {
        for internal_field in [
            "dismiss_tray_on_success",
            "dismissTrayOnSuccess",
            "airport_info_airport_id",
            "airportInfoAirportId",
            "FlightPlanRowNavigationAction",
            "WireFlightPlanRowNavigationAction",
        ] {
            assert!(
                !source.contains(internal_field),
                "{platform} retains core-only row-action field {internal_field}"
            );
        }
    }
}

#[test]
fn flight_plan_picker_presentation_is_core_owned() {
    let core = read_repo_file("ui/core-rust/crates/app-core/src/lib.rs");
    let session = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let had = read_repo_file("ui/core-rust/crates/app-core/src/had_ops.rs");
    let web = read_repo_file("ui/web-app/src/App.tsx");
    let android =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/FlightPlanPage.kt");

    assert!(
        core.contains("fn procedure_picker_choice_label")
            && core.contains("same_point_exit_disabled_reason")
            && had.contains("empty_message: \"No published routes are available.\""),
        "core must project procedure and airway option presentation"
    );
    assert!(
        session.contains("empty_message: match procedure_kind")
            && session.contains("crate::nav_ref_picker_label"),
        "core row-action effects must project picker headings and empty states"
    );
    for (platform, source) in [("web", web.as_str()), ("Android", android.as_str())] {
        for duplicated_policy in [
            "procedureChoiceLabel",
            "noPublishedProceduresLabel",
            "No published routes are available.",
            "That fix is the airway entry; choose an exit.",
            "navRefLabel(point.nav_ref)",
            "navRefLabel(point.navRef)",
        ] {
            assert!(
                !source.contains(duplicated_policy),
                "{platform} reconstructs core picker presentation: {duplicated_policy}"
            );
        }
    }
}

#[test]
fn weather_and_airport_detail_presentation_is_core_owned() {
    let weather = read_repo_file("ui/core-rust/crates/app-core/src/map_overlay.rs");
    let airport = read_repo_file("ui/core-rust/crates/app-core/src/airport_info.rs");
    let charts = read_repo_file("ui/core-rust/crates/app-core/src/chart_page.rs");
    let web = read_repo_file("ui/web-app/src/App.tsx");
    let android =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt");

    assert!(
        weather.contains("pub sections: Vec<WeatherDetailSectionUiView>")
            && airport.contains("pub fact_sections: Vec<AirportInfoFactSectionUiView>")
            && charts.contains("pub empty_text: String"),
        "core detail models must carry semantic sections, ordered facts, and empty states"
    );
    for (platform, source) in [("web", web.as_str()), ("Android", android.as_str())] {
        for duplicated_policy in [
            "Airport elevation",
            "Traffic pattern altitude",
            "Time at airport",
            "Time zone",
            "No airport NOTAMs available.",
            "No procedure NOTAMs available.",
        ] {
            assert!(
                !source.contains(duplicated_policy),
                "{platform} reconstructs core detail presentation: {duplicated_policy}"
            );
        }
    }
}

#[test]
fn chart_selector_and_procedure_load_controls_are_core_owned() {
    let core = read_repo_file("ui/core-rust/crates/app-core/src/chart_page.rs");
    let load = read_repo_file("ui/core-rust/crates/app-core/src/lib.rs");
    let web = read_repo_file("ui/web-app/src/App.tsx");
    let android = read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/ChartsPage.kt");

    assert!(
        core.contains("pub collection_control: ChartSelectorControlUiView")
            && core.contains("pub chart_control: ChartSelectorControlUiView")
            && core.contains("pub procedure_load_menu: crate::ProcedureLoadMenu")
            && load.contains("pub disabled_reason: Option<String>"),
        "core must project complete chart selector and procedure-load controls"
    );
    for (platform, source) in [("web", web.as_str()), ("Android", android.as_str())] {
        for duplicated_policy in [
            "selectedCollection.charts[0]",
            "collection.charts.firstOrNull()?.airportId",
            "No loadable procedure is available for this plate.",
            "No loadable procedure",
        ] {
            assert!(
                !source.contains(duplicated_policy),
                "{platform} reconstructs core chart control policy: {duplicated_policy}"
            );
        }
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
    assert_eq!(
        wire["ui_contract_version"],
        app_ui_contracts::UI_WIRE_CONTRACT_VERSION,
        "platform snapshots must identify the exact generated UI contract"
    );
    let plan_ui = &wire["app_ui_state"]["active_plan"];
    assert!(plan_ui.get("plan_id").is_some());
    assert!(plan_ui.get("plan_version").is_some());
    assert!(plan_ui.get("route_components").is_none());
    assert!(plan_ui.get("resolved_legs").is_none());
    assert!(plan_ui.get("guidance").is_some());
}

#[test]
fn generated_ui_contract_types_are_not_hand_copied_at_platform_boundaries() {
    let planning = read_repo_file("ui/core-rust/crates/app-core/src/planning.rs");
    let session = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let cloud = read_repo_file("ui/core-rust/crates/app-core/src/cloud.rs");
    let data_status = read_repo_file("ui/core-rust/crates/app-core/src/data_status.rs");
    let flight_data = read_repo_file("ui/core-rust/crates/app-core/src/flight_data.rs");
    let navdb_types = read_repo_file("ui/core-rust/crates/app-core/src/navdb_types.rs");
    let web = read_repo_file("ui/web-app/src/domain/appCoreAdapter.ts");
    let web_types = read_repo_file("ui/web-app/src/domain/types.ts");
    let web_work_runner = read_repo_file("ui/web-app/src/domain/uiSessionWorkRunner.ts");
    let android = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeAppCoreAdapter.kt",
    );
    let android_wire =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/WireModels.kt");
    let android_models =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/Models.kt");
    let android_work_runner =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/UiSessionWorkRunner.kt");

    assert!(
        android.contains("ignoreUnknownKeys = false"),
        "Android's core-session decoder must reject unmodeled fields instead of silently dropping them"
    );

    for (source_name, source, declarations) in [
        (
            "app-core session",
            session.as_str(),
            vec![
                "pub struct UiHomePageState",
                "pub struct NexradOverlayQueryResult",
                "pub struct UiChartPageState",
                "pub struct UiMapLayerState",
                "pub enum MapLayerId",
                "pub enum DebugFlagId",
                "pub struct UiDebugState",
                "pub struct UiSettingsPageState",
            ],
        ),
        (
            "app-core cloud",
            cloud.as_str(),
            vec![
                "pub enum CloudUiActionId",
                "pub enum CloudPlatformEffect",
                "pub struct UiCloudPageState",
                "pub struct CloudHttpRequest",
            ],
        ),
        (
            "app-core data status",
            data_status.as_str(),
            vec![
                "pub enum UiStatusSeverity",
                "pub struct UiDataStatusPageState",
            ],
        ),
        (
            "app-core flight data",
            flight_data.as_str(),
            vec![
                "pub struct FlightDataCell",
                "pub struct FlightDataBannerModel",
            ],
        ),
        (
            "app-core planning",
            planning.as_str(),
            vec![
                "pub enum FlightPlanControlId",
                "pub struct FlightPlanControlUiView",
            ],
        ),
        (
            "app-core nav query",
            navdb_types.as_str(),
            vec!["pub struct WaypointIdentifierSuggestion"],
        ),
        (
            "Android adapter",
            android.as_str(),
            vec![
                "private data class WireUiChartPageState",
                "private data class WireUiMapLayerState",
                "private data class WireUiDataStatusState",
                "private data class WireUiSettingsPageState",
                "enum class MapLayerId",
                "enum class DebugFlagId",
                "data class NavSymbolFeature",
            ],
        ),
        (
            "Android wire models",
            android_wire.as_str(),
            vec![
                "data class WireFlightDataCell",
                "data class WireFlightDataColumn",
                "data class WireWaypointIdentifierSuggestion",
                "data class WireNavSymbolFeature",
                "enum class WireFlightPlanControlId",
                "data class WireFlightPlanControlUiView",
            ],
        ),
        (
            "Android domain models",
            android_models.as_str(),
            vec![
                "data class WaypointIdentifierSuggestion",
                "enum class FlightPlanControlId",
                "data class FlightPlanControlUiView",
            ],
        ),
        (
            "web adapter",
            web.as_str(),
            vec![
                "export type UiDataStatusState =",
                "export type UiSettingsPageState =",
                "export type UiMapLayerState =",
                "export type UiChartPageState =",
                "export type MapLayerId =",
                "export type DebugFlagId =",
            ],
        ),
        (
            "web domain types",
            web_types.as_str(),
            vec![
                "export type FlightDataCell =",
                "export type FlightDataBannerModel =",
                "export type FlightEstimateKind =",
                "export type WaypointIdentifierSuggestion =",
                "export type NavSymbolFeature =",
                "export type FlightPlanControlId =",
                "export type FlightPlanControlUiView =",
            ],
        ),
        (
            "web session work runner",
            web_work_runner.as_str(),
            vec![
                "export type UiSessionWorkKind =",
                "type WorkRequest =",
                "type RequestDecision =",
                "type CompletionDecision =",
            ],
        ),
        (
            "Android session work runner",
            android_work_runner.as_str(),
            vec![
                "private data class WorkRequestWire(",
                "private sealed class RequestDecision",
                "private data class CompletionDecision(",
                "private sealed class ResultAction",
            ],
        ),
    ] {
        for declaration in declarations {
            assert!(
                !source.contains(declaration),
                "{source_name} redeclares generated UI contract type: {declaration}"
            );
        }
    }
}

#[test]
fn external_product_contract_dtos_are_not_redeclared_by_producers_or_consumers() {
    let publication_producer =
        read_repo_file("product/preprocessor/preprocessor-cli/src/product_build.rs");
    let live_feed_producer =
        read_repo_file("product/preprocessor/preprocessor-live-feeds/src/engine.rs");
    let live_feed_daemon = read_repo_file("product/preprocessor/live-feeds-daemon/src/main.rs");
    let live_feed_consumer = read_repo_file("ui/core-rust/crates/app-core/src/live_feeds.rs");
    let live_feed_cache = read_repo_file("ui/core-rust/crates/app-core/src/live_feed_cache.rs");

    for (source_name, source, declarations) in [
        (
            "publication producer",
            publication_producer.as_str(),
            vec![
                "struct BundleManifest",
                "struct CurrentArtifactsManifest",
                "struct BundlePackageArtifact",
            ],
        ),
        (
            "live-feed producer",
            live_feed_producer.as_str(),
            vec![
                "struct LiveFeedsCurrentManifest",
                "struct LiveFeedVersionManifest",
                "struct LivePayloadRef",
                "struct LiveDeltaRef",
            ],
        ),
        (
            "live-feed daemon",
            live_feed_daemon.as_str(),
            vec![
                "struct LiveFeedCurrentEvent",
                "struct LiveFeedVersionManifest",
            ],
        ),
        (
            "live-feed app consumer",
            live_feed_consumer.as_str(),
            vec![
                "struct CurrentManifest",
                "struct VersionManifest",
                "struct LiveFeedCurrentEvent",
                "struct LiveFeedRecordDelta",
            ],
        ),
        (
            "live-feed durable cache",
            live_feed_cache.as_str(),
            vec!["struct LiveFeedRecordDelta", "struct LiveFeedNavKvDelta"],
        ),
    ] {
        for declaration in declarations {
            assert!(
                !source.contains(declaration),
                "{source_name} redeclares canonical product contract type: {declaration}"
            );
        }
    }
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

#[test]
fn application_live_feed_clients_bootstrap_only_from_sse() {
    for path in [
        "ui/core-rust/crates/app-core/src/live_feeds.rs",
        "ui/core-rust/crates/app-core/src/live_feed_cache.rs",
        "ui/core-rust/crates/app-core/src/live_feed_runtime.rs",
        "ui/core-rust/crates/app-core/src/session.rs",
        "ui/core-rust/crates/app-wasm/src/lib.rs",
        "ui/core-rust/crates/app-ffi/src/lib.rs",
        "ui/web-app/src/domain/appCoreAdapter.ts",
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/LiveFeedCache.kt",
        "ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeBindings.kt",
    ] {
        let source_text = read_repo_file(path);
        let source = if path.ends_with(".rs") {
            strip_rust_tests(&source_text)
        } else {
            source_text.as_str()
        };
        for forbidden in [
            "current.json",
            "live_feeds/current",
            "refresh_live_feed_current",
            "refresh_current",
        ] {
            assert!(
                !source.contains(forbidden),
                "{path} exposes the daemon-owned current ledger to an application client: {forbidden}"
            );
        }
    }
}

#[test]
fn cross_platform_ui_policy_is_projected_by_core() {
    let contracts = read_repo_file("ui/core-rust/crates/app-ui-contracts/src/session.rs");
    let session = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let web = read_repo_file("ui/web-app/src/App.tsx");
    let android =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt");
    let web_status = web.as_str();
    let android_status =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/DataStatusPage.kt");

    assert!(
        contracts.contains("pub struct UiNavigationPageState")
            && contracts.contains("pub struct UiNavigationPageOption")
            && contracts.contains("default_chart_or_plate_return_target")
            && session.contains("fn navigation_page_state_for_platform"),
        "page ordering, labels, semantic roles, and history bounds must be projected by core"
    );
    assert!(
        web.contains("navigationPageOptionsFromCore")
            && web.contains("defaultChartOrPlateReturnPage")
            && android.contains("navigationPageOptionsFromCore")
            && android.contains("defaultChartOrPlateReturnPage"),
        "both platforms must render the core navigation policy"
    );
    for (platform, source, forbidden) in [
        ("web", web.as_str(), "const pageOptions = ["),
        ("Android", android.as_str(), "private val PageOptions ="),
    ] {
        assert!(
            !source.contains(forbidden),
            "{platform} must not restore a static navigation policy"
        );
    }

    for (platform, source) in [("web", web_status), ("Android", android_status.as_str())] {
        for forbidden in [
            "formatDataStatusDuration",
            "dataStatusRelativeTimeSuffix",
            "UiDataStatusPageTimeDisplay",
        ] {
            assert!(
                !source.contains(forbidden),
                "{platform} must render core-projected relative times, not define {forbidden}"
            );
        }
    }
}

#[test]
fn status_surface_composition_and_effects_are_core_owned() {
    let contracts = read_repo_file("ui/core-rust/crates/app-ui-contracts/src/session.rs");
    let status = read_repo_file("ui/core-rust/crates/app-core/src/data_status.rs");
    let web = read_repo_file("ui/web-app/src/App.tsx");
    let android_map =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt");
    let android_activity =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt");

    assert!(
        contracts.contains("pub struct UiSurfaceStatusState")
            && contracts.contains("pub enum UiStatusPlatformEffect")
            && status.contains("pub fn map_surface_status_state")
            && status.contains("pub fn charts_surface_status_state"),
        "core must project ordered per-surface controls and typed status effects"
    );
    assert!(
        web.contains("sessionSnapshot.map_status_controls")
            && web.contains("derivedChartPageState.status_controls")
            && android_map.contains("sessionSnapshot.mapStatusControls.controls")
            && android_activity.contains("derivedChartPageState.statusControls"),
        "both renderers must consume core's per-surface status composition"
    );
    for (platform, source) in [
        ("web", web.as_str()),
        ("Android map", android_map.as_str()),
        ("Android shell", android_activity.as_str()),
    ] {
        assert!(
            !source.contains("actionId == \"app:reload\"")
                && !source.contains("actionId === \"app:reload\""),
            "{platform} must execute core's typed reload effect, not interpret its action ID"
        );
    }
}

#[test]
fn map_selection_action_policy_is_core_owned() {
    let core = read_repo_file("ui/core-rust/crates/app-core/src/session.rs");
    let model = read_repo_file("ui/core-rust/crates/app-core/src/map_overlay.rs");
    let web = read_repo_file("ui/web-app/src/App.tsx");
    let android =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/MapExplorerPage.kt");
    let web_map = web
        .split("function MapPage(")
        .nth(1)
        .and_then(|source| source.split("function MapOrientationButton(").next())
        .expect("web MapPage source");
    let android_map = android
        .split("internal fun MapExplorerPage(")
        .nth(1)
        .and_then(|source| source.split("private fun RasterImageLayers(").next())
        .expect("Android MapExplorerPage source");
    let action_resolver = core
        .split("fn registered_map_selection_action_for_session(")
        .nth(1)
        .and_then(|source| {
            source
                .split("fn perform_map_selection_session_action(")
                .next()
        })
        .expect("map-selection action resolver source");
    let action_decoder = core
        .split("fn decode_map_selection_action_token(")
        .nth(1)
        .and_then(|source| {
            source
                .split("fn invalid_map_selection_action_token(")
                .next()
        })
        .expect("map-selection action decoder source");

    assert!(
        model.contains("pub action_uid: Option<String>")
            && model.contains("pub enum MapSelectionActionEffect")
            && core.contains("fn finalize_map_selection_actions")
            && core.contains("map_selection_action_key: Option<[u8; 32]>")
            && core.contains("fn encode_map_selection_action_token")
            && core.contains("fn decode_map_selection_action_token")
            && !core.contains("map_selection_actions:")
            && core.contains("map_selection_action_decision_in_session")
            && core.contains("perform_map_selection_ui_action_in_session"),
        "core must issue authenticated self-contained map-selection actions and return typed decisions"
    );
    for forbidden in [
        "materialize_map_selection",
        "query_map_selection",
        "session_nav_kv_store",
        "CoreResourceRequest",
        "NeedResources",
        "fetch",
    ] {
        assert!(
            !action_resolver.contains(forbidden) && !action_decoder.contains(forbidden),
            "bounded map-selection action decisions must not perform {forbidden} work"
        );
    }
    assert!(
        action_decoder.contains("ChaCha20Poly1305")
            && action_decoder.contains("serde_json::from_slice"),
        "bounded map-selection action decisions must only authenticate and decode core-issued commands"
    );
    assert!(
        web_map.contains("uiSession.mapSelectionActionDecision(action.action_uid)")
            && web_map.contains("uiSession.performMapSelectionUiAction(action.action_uid)")
            && android_map.contains("uiSession.mapSelectionActionDecision(actionUid)")
            && android_map.contains("uiSession.performMapSelectionUiAction(actionUid)"),
        "both renderers must use the generic map-selection action boundary"
    );
    for (platform, source) in [("web", web_map), ("Android", android_map)] {
        for forbidden in [
            "action.weather_detail",
            "action.airport_info_airport_id",
            "action.flight_plan_row_action",
            "action.session_action",
            "action.weatherDetail",
            "action.airportInfoAirportId",
            "action.flightPlanRowAction",
            "action.sessionAction",
        ] {
            assert!(
                !source.contains(forbidden),
                "{platform} must not dispatch map-selection action payload field {forbidden}"
            );
        }
    }
}

#[test]
fn pointer_rate_geometry_mirrors_share_conformance_vectors() {
    let core = read_repo_file("ui/core-rust/crates/app-core/src/ui_geometry.rs");
    let golden = read_repo_file(
        "ui/core-rust/crates/app-ui-contracts/tests/goldens/ui-geometry-conformance.json",
    );
    let web_map = read_repo_file("ui/web-app/src/domain/mapViewport.ts");
    let web_tests = read_repo_file("ui/web-app/src/domain/mapViewport.test.ts");
    let web_situation = read_repo_file("ui/web-app/src/domain/situationGeometry.ts");
    let web_route = read_repo_file("ui/web-app/src/domain/flightPlanRouteRender.ts");
    let web_pointer_tests =
        read_repo_file("ui/web-app/src/domain/pointerRateGeometryConformance.test.ts");
    let android_map =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/MapViewport.kt");
    let android_situation =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/MainActivity.kt");
    let android_route =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/RouteRendering.kt");
    let android_pills = read_repo_file(
        "ui/android-app/app/src/main/java/org/aerobag/app/RouteDistancePillLayout.kt",
    );
    let android_tests = read_repo_file(
        "ui/android-app/app/src/test/java/org/aerobag/app/UiGeometryConformanceTest.kt",
    );

    assert!(
        core.contains("ui-geometry-conformance.json")
            && web_tests.contains("uiGeometryConformance.json")
            && android_tests.contains("ui-geometry-conformance.json"),
        "core, web, and Android geometry must execute the same conformance vectors"
    );
    for vector in ["situation_overlay", "route_chevrons", "route_distance_pill"] {
        assert!(
            golden.contains(vector),
            "shared geometry golden is missing {vector}"
        );
        assert!(
            core.contains(vector)
                && web_pointer_tests.contains(vector)
                && android_tests.contains(vector),
            "core, web, and Android must all execute {vector}"
        );
    }
    for (platform, source) in [
        ("web map", web_map.as_str()),
        ("web situation", web_situation.as_str()),
        ("web route", web_route.as_str()),
        ("Android map", android_map.as_str()),
        ("Android situation", android_situation.as_str()),
        ("Android route", android_route.as_str()),
        ("Android distance pill", android_pills.as_str()),
    ] {
        assert!(
            source.contains("Pointer-rate mirror of app_core::ui_geometry"),
            "{platform} pointer-rate geometry must document its core authority and conformance fence"
        );
    }
}

#[test]
fn scheduler_viewport_inputs_have_wasm_and_ffi_parity() {
    let wasm = read_repo_file("ui/core-rust/crates/app-wasm/src/lib.rs");
    let ffi = read_repo_file("ui/core-rust/crates/app-ffi/src/lib.rs");
    let android =
        read_repo_file("ui/android-app/app/src/main/java/org/aerobag/app/domain/NativeBindings.kt");

    for operation in [
        "session_snapshot_refresh_scheduler_viewport_gesture_active_changed",
        "session_snapshot_refresh_scheduler_viewport_activity",
    ] {
        assert!(
            wasm.contains(&format!("pub fn {operation}")),
            "WASM is missing scheduler input {operation}"
        );
        assert!(
            ffi.contains(&format!("pub fn {operation}_json")),
            "FFI is missing scheduler input {operation}"
        );
    }
    assert!(
        android.contains("sessionSnapshotRefreshSchedulerViewportGestureActiveChangedJson")
            && android.contains("sessionSnapshotRefreshSchedulerViewportActivityJson"),
        "Android must forward both core scheduler viewport inputs"
    );
}
