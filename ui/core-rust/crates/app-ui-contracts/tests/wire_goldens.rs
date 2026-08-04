// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use app_ui_contracts::{cloud, home, nexrad, session};

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
        &cloud::CloudPlatformEffect::BeginAuthorization {
            provider: cloud::CloudProviderKind::GoogleDrive,
            scopes: vec!["scope-a".to_string(), "scope-b".to_string()],
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
fn contract_decoders_reject_unknown_fields() {
    assert!(serde_json::from_str::<home::UiHomePageState>(
        r#"{"buttons":[],"platform_guess":true}"#,
    )
    .is_err());
    assert!(serde_json::from_str::<nexrad::NexradOverlayStatus>(
        r#"{"state":"ready","count":1,"legacy_count":1}"#,
    )
    .is_err());
    assert!(serde_json::from_str::<session::MapLayerId>(r#""weather""#).is_err());
    assert!(serde_json::from_str::<session::DebugFlagId>(r#""diagnostics""#).is_err());
}
