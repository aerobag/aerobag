// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use app_ui_contracts::{cloud, home, nav_query, nexrad, session, work};

fn assert_golden<T: serde::Serialize>(value: &T, golden: &str) {
    let actual = serde_json::to_value(value).expect("serialize wire value");
    let expected = serde_json::from_str::<serde_json::Value>(golden).expect("decode golden JSON");
    assert_eq!(actual, expected);
}

#[test]
fn home_page_wire_matches_golden() {
    assert_golden(
        &home::UiHomePageState {
            buttons: vec![home::UiHomePageButton {
                destination: home::UiHomeDestination::OfflinePackages,
                label: "Offline Packages".to_string(),
                enabled: false,
                disabled_reason: Some("Downloads are unavailable on this platform.".to_string()),
            }],
        },
        include_str!("goldens/home-page.json"),
    );
}

#[test]
fn cloud_effect_wire_matches_golden() {
    assert_golden(
        &cloud::CloudPlatformEffect::CopyText {
            text: "setup-code".to_string(),
            completion_label: "Copied".to_string(),
        },
        include_str!("goldens/cloud-effect.json"),
    );
}

#[test]
fn nexrad_query_wire_matches_golden() {
    assert_golden(
        &nexrad::NexradOverlayQueryResult {
            status: nexrad::NexradOverlayStatus::Ready { count: 1 },
            tiles: Vec::new(),
            stats: nexrad::NexradOverlayStats::default(),
            animation: nexrad::NexradOverlayAnimation::idle(),
        },
        include_str!("goldens/nexrad-query.json"),
    );
}

#[test]
fn nav_query_suggestion_wire_matches_golden() {
    assert_golden(
        &nav_query::WaypointIdentifierSuggestion {
            identifier: "KRNT".to_string(),
            nav_ref: nav_query::WaypointSuggestionNavRef::Airport {
                code: "KRNT".to_string(),
            },
            kind: "airport".to_string(),
            display_name: "Renton Municipal Airport".to_string(),
            distance_from_anchor_nm: 12.4,
            distance_text: "12nm".to_string(),
            symbol_feature: Some(nav_query::NavSymbolFeature {
                kind: "airport".to_string(),
                label: "KRNT".to_string(),
                symbol_kind: "airport".to_string(),
                style_class: "airport".to_string(),
                obstacle_variant: None,
                obstacle_tone: None,
                towered: true,
                fuel_available: true,
                has_paved_runway: Some(true),
                heliport: None,
                has_water_runway: None,
                runway_length_ratio: 0.8,
                longest_runway_heading_true_deg: None,
            }),
        },
        include_str!("goldens/nav-query-suggestion.json"),
    );
}

#[test]
fn session_command_ids_match_golden() {
    assert_golden(
        &serde_json::json!({
            "debug_flag": session::DebugFlagId::GpsCapture,
            "map_layer": session::MapLayerId::TerrainWarning,
        }),
        include_str!("goldens/session-command-ids.json"),
    );
}

#[test]
fn session_update_wire_matches_golden() {
    assert_golden(
        &session::UiSessionUpdate {
            ui_contract_version: app_ui_contracts::UI_WIRE_CONTRACT_VERSION,
            session_revision: 7,
            nav_data: None,
            application_shell: None,
            flight_plan: None,
            ownship: None,
            flight_data: None,
            situation: None,
            charts: None,
            map: Some(session::UiSessionProjectionPatch {
                version: 3,
                assignments: vec![session::UiSessionProjectionAssignment {
                    path: vec!["map_layer_state".to_string()],
                    value: serde_json::json!({"example": true}),
                }],
            }),
            status: None,
            settings: None,
            cloud: None,
            packages: None,
            home: None,
            debug: None,
        },
        include_str!("goldens/session-update.json"),
    );
}

#[test]
fn session_work_wire_matches_golden() {
    assert_golden(
        &work::UiSessionWorkCompletionDecision {
            result_action: work::UiSessionWorkResultAction::Drop {
                reason: "superseded_by_newer_input".to_string(),
            },
            next: Some(work::UiSessionWorkRequest {
                id: 7,
                kind: work::UiSessionWorkKind::MapSelection,
                coalesce_key: Some("map_selection".to_string()),
                requested_at_ms: 1234,
            }),
        },
        include_str!("goldens/session-work.json"),
    );
}

#[test]
fn contract_decoders_reject_unknown_fields() {
    assert!(serde_json::from_str::<home::UiHomePageState>(
        r#"{"buttons":[],"platform_guess":true}"#,
    )
    .is_err());
    assert!(serde_json::from_value::<session::UiSessionProjectionPatch>(
        serde_json::json!({"version": 1, "assignments": {}}),
    )
    .is_err());
    assert!(serde_json::from_str::<nexrad::NexradOverlayStatus>(
        r#"{"state":"ready","count":1,"legacy_count":1}"#,
    )
    .is_err());
    assert!(serde_json::from_str::<session::MapLayerId>(r#""weather""#).is_err());
    assert!(serde_json::from_str::<session::DebugFlagId>(r#""diagnostics""#).is_err());
    assert!(serde_json::from_str::<session::UiSessionUpdate>(
        r#"{"ui_contract_version":3,"session_revision":1,"unknown":true}"#,
    )
    .is_err());
    assert!(serde_json::from_str::<work::UiSessionWorkRequestDecision>(
        r#"{"kind":"queued","replaced_request_id":null,"unknown":true}"#,
    )
    .is_err());
}
