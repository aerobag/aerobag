// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::cloud_acs_memory::InMemoryAcsProvider;

fn locator(engine: &CloudEngine) -> String {
    engine
        .account()
        .unwrap()
        .acs
        .as_ref()
        .unwrap()
        .account_locator
        .clone()
}

fn replace_root(
    engine: &CloudEngine,
    provider: &mut InMemoryAcsProvider,
    plaintext: serde_json::Value,
) {
    let locator = locator(engine);
    let root = provider.root(&locator).unwrap().unwrap();
    let replacement = engine
        .encrypt_acs_value(
            &plaintext,
            "state_node",
            AcsEncryptedValueKind::Root,
            ACS_FIXED_ROOT_ID,
            root.value.child_object_ids.clone(),
        )
        .unwrap();
    let result = provider
        .compare_and_swap_root(
            &locator,
            AcsCompareAndSwapRootRequest {
                contract_id: product_contracts::ACS_CONTRACT_ID.to_string(),
                expected_revision: root.revision,
                expected_root_hash: Some(root.root_hash),
                replacement,
            },
            2_000,
        )
        .unwrap();
    assert!(matches!(
        result,
        crate::cloud_acs_memory::AcsMemoryDelivery::Delivered(
            AcsCompareAndSwapRootResponse::Committed { .. }
        )
    ));
}

#[test]
fn future_format_is_a_pause_before_decoding_its_unknown_body() {
    let mut provider = InMemoryAcsProvider::default();
    let mut old = configured_engine();
    let code = create_account(&mut old, &mut provider, &plan(&["KRNT"]), 1_000);
    replace_root(
        &old,
        &mut provider,
        serde_json::json!({
            "version": 2, "future_body": { "deliberately_not_a_v1_node": true }
        }),
    );
    let (mut joining, adopted) = link_account(&mut provider, code, 3_000);
    assert!(adopted.is_empty());
    assert!(
        joining.persistent.last_provider_failure.is_none(),
        "a format mismatch must not become a latched permanent error"
    );
    let status = joining
        .status_record(3_000)
        .expect("persistent format caution");
    assert!(format!("{status:?}").contains("format 2"));
    assert!(joining
        .page_state(3_000)
        .sync_account_panels
        .iter()
        .any(|p| p.summary.as_deref().is_some_and(|s| s.contains("Update"))));
    assert!(
        joining.take_provider_request(63_001).unwrap().is_some(),
        "paused accounts must continue checking compatibility"
    );
}

#[test]
fn a_notification_during_a_paused_root_read_is_not_consumed_by_its_stale_response() {
    let (mut provider, _, mut newer) = fixture();
    let mut upgrader = as_next_client(newer.clone());
    read_only_pump(&mut upgrader, &mut provider, 3_000);
    newer
        .perform_action_at(CloudAction::SyncNow, &FlightPlan::default(), 4_000)
        .unwrap();
    let http = newer.take_provider_request(4_000).unwrap().unwrap();
    let request = newer.provider_request_in_flight.clone().unwrap();
    let stale = execute_acs(&mut provider, &request, &locator(&newer), 4_000);
    confirm_upgrade(&mut upgrader, 5_000);
    pump_acs(&mut upgrader, &mut provider, 5_000);
    newer
        .report_event_stream_event(
            CloudEventStreamEvent {
                stream_id: newer.event_stream_plan().unwrap().stream_id,
                kind: CloudEventStreamEventKind::Connected,
                data: None,
                detail: None,
            },
            5_000,
        )
        .unwrap();
    newer
        .complete_provider_request(http.request_id, stale, 5_001)
        .unwrap();
    pump_acs(&mut newer, &mut provider, 5_002);
    assert!(
        newer.compatibility_detail().is_none(),
        "the newer notification still requires a fresh root read"
    );
}

#[test]
fn restarting_with_pending_edits_reads_root_before_any_upload() {
    let mut provider = InMemoryAcsProvider::default();
    let mut old = configured_engine();
    create_account(&mut old, &mut provider, &plan(&["KRNT"]), 1_000);
    old.record_local_flight_plan_mutation(&plan(&["KRNT"]), &plan(&["KPAE"]), 2_000)
        .unwrap();
    let mut restarted = CloudEngine::new(old.persistent.clone());
    restarted.take_provider_request(3_000).unwrap().unwrap();
    assert!(
        matches!(
            restarted.provider_request_in_flight.unwrap().operation,
            CloudProviderOperation::AcsReadRoot
        ),
        "do not upload from an unverified persisted tip"
    );
}

#[test]
fn failed_page_verification_cannot_be_bypassed_by_sync_now() {
    let (mut provider, mut old, _) = fixture();
    old.record_local_flight_plan_mutation(&plan(&["KRNT"]), &plan(&["KPAE"]), 3_000)
        .unwrap();
    let mut restarted = CloudEngine::new(old.persistent);
    let http = restarted.take_provider_request(4_000).unwrap().unwrap();
    let request = restarted.provider_request_in_flight.clone().unwrap();
    let response = execute_acs(&mut provider, &request, &locator(&restarted), 4_000);
    restarted
        .complete_provider_request(http.request_id, response, 4_000)
        .unwrap();
    let page = restarted.take_provider_request(4_000).unwrap().unwrap();
    assert!(matches!(
        restarted
            .provider_request_in_flight
            .as_ref()
            .unwrap()
            .operation,
        CloudProviderOperation::AcsReadObject { .. }
    ));
    restarted
        .complete_provider_request(
            page.request_id,
            CloudHttpResponse::Completed {
                status_code: 404,
                body_base64: String::new(),
            },
            4_000,
        )
        .unwrap();
    assert!(restarted.persistent.last_provider_failure.is_some());
    restarted
        .perform_action_at(CloudAction::SyncNow, &FlightPlan::default(), 5_000)
        .unwrap();
    restarted.take_provider_request(5_000).unwrap().unwrap();
    assert!(
        matches!(
            restarted.provider_request_in_flight.unwrap().operation,
            CloudProviderOperation::AcsReadRoot
        ),
        "an authenticated header is not a verified account snapshot"
    );
}

#[test]
fn deferred_navigation_adoption_survives_client_update_and_account_upgrade() {
    let (mut provider, mut old, _) = fixture();
    // The session defers cloud adoption while guidance is active. Migration
    // must preserve that queue, not silently apply it or turn it into an edit.
    let remote = old.cached_flight_plan().unwrap();
    old.set_pending_remote_flight_plan(remote.clone()).unwrap();
    let mut newer = as_next_client(old);
    read_only_pump(&mut newer, &mut provider, 3_000);
    let deferred = newer.persistent.records.deferred_adoption.clone();
    confirm_upgrade(&mut newer, 4_000);
    pump_acs(&mut newer, &mut provider, 4_000);
    assert_eq!(newer.persistent.records.deferred_adoption, deferred);
    assert!(newer.persistent.records.pending_keys.is_empty());
    let mut restarted = as_next_client(newer);
    pump_acs(&mut restarted, &mut provider, 5_000);
    assert_eq!(restarted.take_pending_remote_flight_plan(), Some(remote));
    assert!(restarted.take_pending_remote_flight_plan().is_none());
}

#[test]
fn current_account_record_wire_shape_is_not_an_internal_model_version() {
    let value = serde_json::json!({
        "id": "frozen-format-1", "name": "TEST ONLY", "route_components": [],
        "route_component_uids": [], "route_component_uid_counter": 0,
        "resolved_legs": [], "guidance": null,
        "departure": null, "destination": null, "alternate": null,
        "aircraft": {"definition_hash": "d3bb0ffe0b906f2adc10f0e1aecb97ac449f65a8b081f0502cd547a25cc9fc9b", "profile_id": "normal-cruise"},
        "cruise_altitude_ft": 6500, "planned_departure_time_epoch_ms": 123000,
        "notes": "Frozen account format 1 / flight-plan schema 3", "updated_at_epoch_ms": 0,
        "version": 1
    });
    let page_value = serde_json::json!({"version": 1, "records": {
        "flight_plan/current": {"schema_version": 3, "modified_at_epoch_ms": 456000, "value": value}
    }});
    let page = (account_format::CURRENT.decode_page)(page_value.clone()).unwrap();
    let record = &page.records[FLIGHT_PLAN_RECORD_KEY];
    let decoded = flight_plan_from_record(record).unwrap();
    assert_eq!(cloud_record_for_flight_plan(&decoded).unwrap(), *record);
    assert_eq!(
        (account_format::CURRENT.encode_page)(&page).unwrap(),
        page_value
    );
}

fn as_next_client(engine: CloudEngine) -> CloudEngine {
    CloudEngine::new_with_format(engine.persistent, &account_format::test_format::NEXT)
}

fn root(provider: &InMemoryAcsProvider, engine: &CloudEngine) -> AcsRootSnapshot {
    provider.root(&locator(engine)).unwrap().unwrap()
}

fn root_plaintext(provider: &InMemoryAcsProvider, engine: &CloudEngine) -> serde_json::Value {
    engine
        .decrypt_acs_value(
            &root(provider, engine).value,
            "state_node",
            AcsEncryptedValueKind::Root,
            ACS_FIXED_ROOT_ID,
        )
        .unwrap()
}

fn cloud_records(
    provider: &InMemoryAcsProvider,
    engine: &CloudEngine,
) -> BTreeMap<String, CloudRecord> {
    let node = root_plaintext(provider, engine);
    let page_id = node["merkle_root_id"].as_str().unwrap();
    let object = provider
        .read_object(&locator(engine), page_id)
        .unwrap()
        .unwrap();
    let page = engine
        .decrypt_acs_value(
            &object.value,
            "merkle_page",
            AcsEncryptedValueKind::Object,
            page_id,
        )
        .unwrap();
    (engine.format.decode_page)(page).unwrap().records
}

fn click(engine: &mut CloudEngine, id: CloudUiActionId, now: i64) {
    let page = engine.page_state(now);
    assert!(
        page.sync_account_panels
            .iter()
            .flat_map(|panel| &panel.actions)
            .any(|action| action.id == id && action.enabled),
        "missing enabled {id:?}: {page:?}"
    );
    engine
        .perform_ui_action(id, &[], &FlightPlan::default(), now)
        .unwrap();
}

fn confirm_upgrade(engine: &mut CloudEngine, now: i64) {
    click(engine, CloudUiActionId::BeginAccountUpgrade, now);
    let panel = active_panel(engine);
    assert_eq!(panel.id, "confirm_upgrade");
    assert!(panel.summary.unwrap().contains("cannot be undone"));
    click(engine, CloudUiActionId::ConfirmAccountUpgrade, now);
}

fn read_only_pump(engine: &mut CloudEngine, provider: &mut InMemoryAcsProvider, now: i64) {
    for _ in 0..10 {
        let Some(http) = engine.take_provider_request(now).unwrap() else {
            return;
        };
        let request = engine.provider_request_in_flight.clone().unwrap();
        assert!(
            !matches!(
                request.operation,
                CloudProviderOperation::AcsCreateObject { .. }
                    | CloudProviderOperation::AcsCompareAndSwapRoot { .. }
            ),
            "paused client attempted a write"
        );
        let response = execute_acs(provider, &request, &locator(engine), now);
        let completion = engine
            .complete_provider_request(http.request_id, response, now)
            .unwrap();
        assert!(
            completion.changed_records.is_empty(),
            "paused client adopted data"
        );
    }
    panic!("paused client did not quiesce");
}

fn hold_cas(
    engine: &mut CloudEngine,
    provider: &mut InMemoryAcsProvider,
    now: i64,
) -> CloudProviderRequest {
    for _ in 0..10 {
        let http = engine
            .take_provider_request(now)
            .unwrap()
            .expect("CAS expected");
        let request = engine.provider_request_in_flight.clone().unwrap();
        if matches!(
            request.operation,
            CloudProviderOperation::AcsCompareAndSwapRoot { .. }
        ) {
            return request;
        }
        let response = execute_acs(provider, &request, &locator(engine), now);
        engine
            .complete_provider_request(http.request_id, response, now)
            .unwrap();
    }
    panic!("CAS never staged");
}

fn deliver(
    engine: &mut CloudEngine,
    provider: &mut InMemoryAcsProvider,
    request: CloudProviderRequest,
    now: i64,
) {
    let response = execute_acs(provider, &request, &locator(engine), now);
    engine
        .complete_provider_request(request.request_id, response, now)
        .unwrap();
}

fn fixture() -> (InMemoryAcsProvider, CloudEngine, CloudEngine) {
    let mut provider = InMemoryAcsProvider::default();
    let mut old = configured_engine();
    let code = create_account(&mut old, &mut provider, &plan(&["KRNT"]), 1_000);
    let (peer, _) = link_account(&mut provider, code, 1_100);
    let mut newer = as_next_client(peer);
    read_only_pump(&mut newer, &mut provider, 2_000);
    (provider, old, newer)
}

#[test]
fn upgrade_is_explicit_and_two_instances_recover_without_losing_local_edits() {
    let (mut provider, mut old, mut newer) = fixture();
    let before = root(&provider, &old);
    assert_eq!(newer.status_summary(2_000).label, "PAUSED");
    assert!(newer
        .perform_ui_action(
            CloudUiActionId::ConfirmAccountUpgrade,
            &[],
            &FlightPlan::default(),
            2_001
        )
        .is_err());
    click(&mut newer, CloudUiActionId::BeginAccountUpgrade, 2_002);
    click(&mut newer, CloudUiActionId::CloseLinkedDetail, 2_003);
    click(&mut newer, CloudUiActionId::SyncNow, 2_004);
    read_only_pump(&mut newer, &mut provider, 2_004);
    assert_eq!(root(&provider, &old), before);
    let mut newer = as_next_client(newer);
    read_only_pump(&mut newer, &mut provider, 2_005);
    newer
        .record_local_flight_plan_mutation(&plan(&["KRNT"]), &plan(&["KPAE"]), 3_000)
        .unwrap();
    read_only_pump(&mut newer, &mut provider, 63_000);
    assert_eq!(root(&provider, &old), before);

    confirm_upgrade(&mut newer, 64_000);
    pump_acs(&mut newer, &mut provider, 64_000);
    assert_eq!(root_plaintext(&provider, &newer)["version"], 2);
    assert_eq!(newer.status_summary(64_000).label, "LINKED");
    assert!(newer.persistent.records.pending_keys.is_empty());
    assert_eq!(
        flight_plan_from_record(&cloud_records(&provider, &newer)[FLIGHT_PLAN_RECORD_KEY])
            .unwrap()
            .plan,
        plan(&["KPAE"])
    );

    old.record_local_flight_plan_mutation(&plan(&["KRNT"]), &plan(&["KPWT"]), 65_000)
        .unwrap();
    // The old instance still has a verified *old* tip. Its losing CAS must
    // reread compatibility, not retry by substituting a revision number.
    pump_acs(&mut old, &mut provider, 65_000);
    assert_eq!(old.status_summary(65_000).label, "PAUSED");
    assert!(old
        .compatibility_detail()
        .unwrap()
        .contains("Update this application"));
    assert!(old
        .persistent
        .records
        .pending_keys
        .contains(FLIGHT_PLAN_RECORD_KEY));
    read_only_pump(&mut old, &mut provider, 125_001);

    let mut updated = as_next_client(old);
    pump_acs(&mut updated, &mut provider, 126_000);
    assert_eq!(updated.status_summary(126_000).label, "LINKED");
    assert!(updated.status_record(126_000).is_none());
    assert_eq!(
        flight_plan_from_record(&cloud_records(&provider, &updated)[FLIGHT_PLAN_RECORD_KEY])
            .unwrap()
            .plan,
        plan(&["KPWT"])
    );
}

#[test]
fn a_joining_newer_client_can_upgrade_without_an_old_format_tip() {
    let (mut provider, old, _) = fixture();
    let mut joining = as_next_client(configured_engine());
    joining
        .perform_action_at(
            CloudAction::AcceptDeviceSetupCode {
                setup_code: old.device_setup_code().unwrap(),
            },
            &FlightPlan::default(),
            3_000,
        )
        .unwrap();
    read_only_pump(&mut joining, &mut provider, 3_000);
    assert!(joining.account().unwrap().tip.is_none());
    assert!(joining.has_linked_account());
    confirm_upgrade(&mut joining, 4_000);
    pump_acs(&mut joining, &mut provider, 4_000);
    assert_eq!(joining.status_summary(4_000).label, "LINKED");
}

#[test]
fn stale_in_flight_old_cas_cannot_overwrite_a_completed_upgrade() {
    let (mut provider, mut old, mut newer) = fixture();
    old.record_local_flight_plan_mutation(&plan(&["KRNT"]), &plan(&["KSEA"]), 3_000)
        .unwrap();
    let pending = hold_cas(&mut old, &mut provider, 3_000);
    confirm_upgrade(&mut newer, 4_000);
    pump_acs(&mut newer, &mut provider, 4_000);
    let upgraded = root(&provider, &newer);
    deliver(&mut old, &mut provider, pending, 5_000);
    read_only_pump(&mut old, &mut provider, 5_000);
    assert_eq!(root(&provider, &newer), upgraded);
    assert_eq!(old.status_summary(5_000).label, "PAUSED");
}

#[test]
fn upgrader_losing_cas_remigrates_the_latest_cloud_edit() {
    let (mut provider, mut old, mut newer) = fixture();
    confirm_upgrade(&mut newer, 3_000);
    let pending = hold_cas(&mut newer, &mut provider, 3_000);
    old.record_local_flight_plan_mutation(&plan(&["KRNT"]), &plan(&["KSEA"]), 4_000)
        .unwrap();
    pump_acs(&mut old, &mut provider, 4_000);
    deliver(&mut newer, &mut provider, pending, 5_000);
    pump_acs(&mut newer, &mut provider, 5_000);
    assert_eq!(root_plaintext(&provider, &newer)["version"], 2);
    let record = &cloud_records(&provider, &newer)[FLIGHT_PLAN_RECORD_KEY];
    assert_eq!(
        flight_plan_from_record(record).unwrap().plan,
        plan(&["KSEA"])
    );
    assert_eq!(record.modified_at_epoch_ms, Some(4_000));
}

#[test]
fn another_instances_upgrade_resumes_a_paused_client_and_dismisses_stale_confirmation() {
    let (mut provider, old, mut newer) = fixture();
    let (peer, _) = link_account(&mut provider, old.device_setup_code().unwrap(), 2_001);
    let mut other = as_next_client(peer);
    read_only_pump(&mut other, &mut provider, 2_002);
    click(&mut other, CloudUiActionId::BeginAccountUpgrade, 2_003);
    confirm_upgrade(&mut newer, 3_000);
    pump_acs(&mut newer, &mut provider, 3_000);
    let upgraded = root(&provider, &newer);
    pump_acs(&mut other, &mut provider, 63_000);
    assert_eq!(other.status_summary(63_000).label, "LINKED");
    assert!(other.linked_account_detail.is_none());
    assert_eq!(root(&provider, &newer), upgraded);
}

#[test]
fn simultaneous_upgrades_commit_only_one_migration() {
    let (mut provider, old, mut newer) = fixture();
    let (peer, _) = link_account(&mut provider, old.device_setup_code().unwrap(), 2_001);
    let mut other = as_next_client(peer);
    read_only_pump(&mut other, &mut provider, 2_002);
    confirm_upgrade(&mut newer, 3_000);
    confirm_upgrade(&mut other, 3_000);
    let pending_a = hold_cas(&mut newer, &mut provider, 3_000);
    let pending_b = hold_cas(&mut other, &mut provider, 3_000);
    deliver(&mut newer, &mut provider, pending_a, 4_000);
    let winner = root(&provider, &newer);
    deliver(&mut other, &mut provider, pending_b, 4_001);
    pump_acs(&mut newer, &mut provider, 4_002);
    pump_acs(&mut other, &mut provider, 4_002);
    assert_eq!(root(&provider, &other), winner);
    assert_eq!(other.status_summary(4_002).label, "LINKED");
}

#[test]
fn migration_changes_real_wire_data_and_preserves_unrelated_records_and_stamps() {
    let (mut provider, mut old, _) = fixture();
    old.record_local_cloud_record(
        "test/format_marker",
        CloudRecord {
            schema_version: 1,
            modified_at_epoch_ms: Some(123),
            value: "old marker".into(),
        },
    );
    old.record_local_cloud_record(
        "future/unrelated",
        CloudRecord {
            schema_version: 79,
            modified_at_epoch_ms: Some(456),
            value: serde_json::json!({"keep": [1,2,3]}),
        },
    );
    pump_acs(&mut old, &mut provider, 3_000);
    let before = cloud_records(&provider, &old);
    let mut newer = as_next_client(old);
    read_only_pump(&mut newer, &mut provider, 4_000);
    confirm_upgrade(&mut newer, 5_000);
    pump_acs(&mut newer, &mut provider, 5_000);
    let after = cloud_records(&provider, &newer);
    assert_eq!(after["future/unrelated"], before["future/unrelated"]);
    assert_eq!(
        after[FLIGHT_PLAN_RECORD_KEY],
        before[FLIGHT_PLAN_RECORD_KEY]
    );
    assert_eq!(after["test/format_marker"].modified_at_epoch_ms, Some(123));
    assert_eq!(after["test/format_marker"].schema_version, 2);
    assert_eq!(
        after["test/format_marker"].value,
        serde_json::json!({"migrated_marker": "new marker"})
    );
    let node = root_plaintext(&provider, &newer);
    let id = node["merkle_root_id"].as_str().unwrap();
    let raw: serde_json::Value = newer
        .decrypt_acs_value(
            &provider
                .read_object(&locator(&newer), id)
                .unwrap()
                .unwrap()
                .value,
            "merkle_page",
            AcsEncryptedValueKind::Object,
            id,
        )
        .unwrap();
    assert!(raw.get("records").is_none());
    assert!(raw.get("entries").is_some());
    assert!(
        (account_format::CURRENT.decode_page)(raw).is_err(),
        "not a relabeled v1 page"
    );
}

#[test]
fn interruption_before_cas_requires_new_consent_but_after_commit_resumes() {
    for commit in [false, true] {
        let (mut provider, _, mut newer) = fixture();
        confirm_upgrade(&mut newer, 3_000);
        let pending = hold_cas(&mut newer, &mut provider, 3_000);
        if commit {
            // Server commits, client never receives the response.
            execute_acs(&mut provider, &pending, &locator(&newer), 4_000);
        }
        let before = root(&provider, &newer);
        let mut restarted = as_next_client(newer);
        if commit {
            pump_acs(&mut restarted, &mut provider, 5_000);
            assert_eq!(restarted.status_summary(5_000).label, "LINKED");
        } else {
            read_only_pump(&mut restarted, &mut provider, 5_000);
            assert_eq!(restarted.status_summary(5_000).label, "PAUSED");
            assert!(restarted.upgrade_from.is_none());
        }
        assert_eq!(root(&provider, &restarted), before);
    }
}

#[test]
fn malformed_headers_remain_integrity_failures() {
    for plaintext in [
        serde_json::json!({}),
        serde_json::json!({"version":"2"}),
        serde_json::json!({"version":0}),
    ] {
        let (mut provider, old, _) = fixture();
        replace_root(&old, &mut provider, plaintext);
        let (joining, _) = link_account(&mut provider, old.device_setup_code().unwrap(), 4_000);
        assert!(joining.persistent.last_provider_failure.is_some());
    }
}

#[test]
fn failed_migration_writes_nothing_and_does_not_consume_pending_edits() {
    let (mut provider, mut old, mut newer) = fixture();
    old.record_local_cloud_record(
        "test/format_marker",
        CloudRecord {
            schema_version: 1,
            modified_at_epoch_ms: Some(123),
            value: "invalid marker".into(),
        },
    );
    pump_acs(&mut old, &mut provider, 3_000);
    newer
        .record_local_flight_plan_mutation(&plan(&["KRNT"]), &plan(&["KPAE"]), 4_000)
        .unwrap();
    let before = root(&provider, &old);
    confirm_upgrade(&mut newer, 5_000);
    read_only_pump(&mut newer, &mut provider, 5_000);
    assert!(newer
        .persistent
        .last_provider_failure
        .as_ref()
        .unwrap()
        .detail
        .contains("migration marker"));
    assert!(newer.upgrade_from.is_none());
    assert!(
        newer
            .page_state(5_000)
            .overall_status
            .summary
            .unwrap()
            .contains("migration marker"),
        "explain why the upgrade failed, not only that sync is paused"
    );
    assert!(newer
        .persistent
        .records
        .pending_keys
        .contains(FLIGHT_PLAN_RECORD_KEY));
    assert_eq!(root(&provider, &old), before);
}

#[test]
fn joining_while_paused_does_not_lose_edits_when_another_client_upgrades() {
    let (mut provider, old, mut newer) = fixture();
    let mut joining = as_next_client(configured_engine());
    joining
        .perform_action_at(
            CloudAction::AcceptDeviceSetupCode {
                setup_code: old.device_setup_code().unwrap(),
            },
            &FlightPlan::default(),
            3_000,
        )
        .unwrap();
    read_only_pump(&mut joining, &mut provider, 3_000);
    joining
        .record_local_flight_plan_mutation(&FlightPlan::default(), &plan(&["KPAE"]), 4_000)
        .unwrap();
    confirm_upgrade(&mut newer, 5_000);
    pump_acs(&mut newer, &mut provider, 5_000);
    pump_acs(&mut joining, &mut provider, 64_000);
    assert_eq!(
        flight_plan_from_record(&cloud_records(&provider, &joining)[FLIGHT_PLAN_RECORD_KEY])
            .unwrap()
            .plan,
        plan(&["KPAE"])
    );
}

#[test]
fn unknown_older_format_disables_upgrade_without_guessing_a_migration() {
    let (mut provider, old, _) = fixture();
    replace_root(
        &old,
        &mut provider,
        serde_json::json!({"version": 3, "future_body": true}),
    );
    let mut newer = CloudEngine::new_with_format(
        old.persistent,
        Box::leak(Box::new(AccountFormat {
            version: 4,
            predecessor: Some(&account_format::CURRENT),
            decode_node: account_format::CURRENT.decode_node,
            decode_page: account_format::CURRENT.decode_page,
            encode_page: account_format::CURRENT.encode_page,
            migrate_records: |_| Ok(()),
        })),
    );
    read_only_pump(&mut newer, &mut provider, 3_000);
    let panel = newer.compatibility_panel().unwrap();
    let upgrade = &panel.actions[0];
    assert!(!upgrade.enabled);
    assert!(upgrade
        .disabled_reason
        .as_deref()
        .unwrap()
        .contains("no migration"));
    assert!(newer
        .perform_ui_action(
            CloudUiActionId::BeginAccountUpgrade,
            &[],
            &FlightPlan::default(),
            3_001
        )
        .is_err());
}

#[test]
fn corrupted_encrypted_root_is_not_misreported_as_a_format_mismatch() {
    let (mut provider, mut old, _) = fixture();
    let current = root(&provider, &old);
    provider
        .compare_and_swap_root(
            &locator(&old),
            AcsCompareAndSwapRootRequest {
                contract_id: product_contracts::ACS_CONTRACT_ID.to_string(),
                expected_revision: current.revision,
                expected_root_hash: Some(current.root_hash),
                replacement: AcsEncryptedValue::from_ciphertext(
                    b"not an encrypted envelope",
                    current.value.child_object_ids,
                ),
            },
            3_000,
        )
        .unwrap();
    old.perform_action_at(CloudAction::SyncNow, &FlightPlan::default(), 4_000)
        .unwrap();
    pump_acs(&mut old, &mut provider, 4_000);
    assert!(old.persistent.last_provider_failure.is_some());
    assert!(old.compatibility_detail().is_none());
}
