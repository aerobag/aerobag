// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::cloud_acs_memory::InMemoryAcsProvider;

fn install_legacy_page(
    engine: &CloudEngine,
    provider: &mut InMemoryAcsProvider,
    version: u32,
    records: &BTreeMap<String, CloudRecord>,
) {
    let locator = engine
        .account()
        .unwrap()
        .acs
        .as_ref()
        .unwrap()
        .account_locator
        .clone();
    let root = provider.root(&locator).unwrap().unwrap();
    let mut node: CloudNode = engine
        .decrypt_acs_value(
            &root.value,
            "state_node",
            AcsEncryptedValueKind::Root,
            ACS_FIXED_ROOT_ID,
        )
        .unwrap();
    let page_id = format!("legacy-page-{}", root.revision);
    let page = CloudPage {
        version,
        records: records.clone(),
    };
    let value = engine
        .encrypt_acs_value(
            &page,
            "merkle_page",
            AcsEncryptedValueKind::Object,
            &page_id,
            Vec::new(),
        )
        .unwrap();
    node.merkle_root_hash = value
        .authenticated_hash(AcsEncryptedValueKind::Object, &page_id)
        .unwrap();
    provider
        .create_object(
            &locator,
            AcsCreateObjectRequest {
                contract_id: product_contracts::ACS_CONTRACT_ID.into(),
                object_id: page_id.clone(),
                value,
            },
            2_000,
        )
        .unwrap();
    node.version = version;
    node.generation += 1;
    node.merkle_root_id = page_id.clone();
    let replacement = engine
        .encrypt_acs_value(
            &node,
            "state_node",
            AcsEncryptedValueKind::Root,
            ACS_FIXED_ROOT_ID,
            vec![page_id],
        )
        .unwrap();
    let result = provider
        .compare_and_swap_root(
            &locator,
            AcsCompareAndSwapRootRequest {
                contract_id: product_contracts::ACS_CONTRACT_ID.into(),
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

fn root(engine: &CloudEngine, provider: &InMemoryAcsProvider) -> AcsRootSnapshot {
    provider
        .root(
            &engine
                .account()
                .unwrap()
                .acs
                .as_ref()
                .unwrap()
                .account_locator,
        )
        .unwrap()
        .unwrap()
}

fn cloud_records(
    engine: &CloudEngine,
    provider: &InMemoryAcsProvider,
    version: u32,
) -> BTreeMap<String, CloudRecord> {
    let root = root(engine, provider);
    let node: CloudNode = engine
        .decrypt_acs_value(
            &root.value,
            "state_node",
            AcsEncryptedValueKind::Root,
            ACS_FIXED_ROOT_ID,
        )
        .unwrap();
    assert_eq!(node.version, version);
    let object = provider
        .read_object(
            &engine
                .account()
                .unwrap()
                .acs
                .as_ref()
                .unwrap()
                .account_locator,
            &node.merkle_root_id,
        )
        .unwrap()
        .unwrap();
    let page = engine
        .decrypt_acs_value::<CloudPage>(
            &object.value,
            "merkle_page",
            AcsEncryptedValueKind::Object,
            &node.merkle_root_id,
        )
        .unwrap();
    assert_eq!(page.version, version);
    page.records
}

fn fixture(
    version: u32,
    schema: u32,
) -> (
    CloudEngine,
    InMemoryAcsProvider,
    BTreeMap<String, CloudRecord>,
) {
    let mut engine = configured_engine();
    engine
        .record_local_inactivity_sleep_timeout(InactivitySleepTimeout::TwoHours, 111)
        .unwrap();
    engine
        .record_local_nexrad_acquisition(NexradAcquisitionPreferences::default(), 112)
        .unwrap();
    engine
        .record_local_offline_package_preferences(
            &OfflinePackagePreferences {
                regions: BTreeMap::from([("northwest".into(), OfflinePackageSelection::Play)]),
                products: BTreeMap::from([("terrain".into(), OfflinePackageSelection::Pause)]),
            },
            113,
        )
        .unwrap();
    engine
        .record_local_debug_flag(all_debug_flags()[0], true, 114)
        .unwrap();
    let mut aircraft_records = BTreeMap::new();
    let mut initial_plan = plan(&["KRNT", "KPAE"]);
    let aircraft_schemas: &[u32] = if version == 1 { &[2, 3] } else { &[2] };
    for &aircraft_schema in aircraft_schemas {
        let mut aircraft = bundled_private_aircraft();
        aircraft.schema_version = aircraft_schema;
        if aircraft_schema == 2 {
            aircraft.glide = None;
        }
        let hash = aircraft.content_hash().unwrap();
        if aircraft_schema == 2 {
            initial_plan.aircraft = Some(product_contracts::AircraftSelection {
                definition_hash: hash.clone(),
                profile_id: aircraft.default_profile_id.clone(),
            });
        }
        engine.record_local_aircraft_definition(&aircraft).unwrap();
        let included = aircraft_schema == 3;
        let modified_at = 113 + i64::from(aircraft_schema);
        engine
            .record_local_aircraft_library_membership(
                &hash,
                product_contracts::AircraftLibraryMembership { included },
                modified_at,
            )
            .unwrap();
        // The current writer always emits envelope 3, even for inner schema 2.
        // Freeze a genuine pre-glide envelope instead of using that writer.
        aircraft_records.insert(
            format!(
                "{}{hash}",
                product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX
            ),
            CloudRecord::fixture(
                aircraft_schema,
                None,
                serde_json::to_value(&aircraft).unwrap(),
            ),
        );
        if version == 1 {
            aircraft_records.insert(
                format!("{AIRCRAFT_LIBRARY_RECORD_PREFIX}{hash}"),
                CloudRecord::fixture(
                    1,
                    Some(modified_at),
                    serde_json::json!({"included": included, "deleted": false}),
                ),
            );
        }
    }
    engine.record_service_read(&"a".repeat(64)).unwrap();
    let mut provider = InMemoryAcsProvider::default();
    create_account(&mut engine, &mut provider, &initial_plan, 1_000);
    let mut old = engine.persistent.records.cached.clone();
    old.extend(aircraft_records);
    if version == 1 {
        // Intentionally impossible to decode as today's runtime plan. Approval
        // must permit discarding it without invoking any historical planner.
        old.insert(
            FLIGHT_PLAN_RECORD_KEY.into(),
            CloudRecord::fixture(
                schema,
                Some(120),
                serde_json::json!({"old_route_shape": "not today's FlightPlan"}),
            ),
        );
    } else {
        assert_eq!(schema, 4);
        assert_eq!(old[FLIGHT_PLAN_RECORD_KEY].schema_version(), 4);
    }
    old.insert(
        "future/unrelated".into(),
        CloudRecord::fixture(
            79,
            Some(116),
            serde_json::json!({"preserve": ["opaque", "data"]}),
        ),
    );
    install_legacy_page(&engine, &mut provider, version, &old);
    engine.persistent.records_format = version;
    engine.persistent.records.cached = old.clone();
    engine.persistent.records.pending_keys = old.keys().cloned().collect();
    engine.persistent.records.deferred_adoption.insert(
        FLIGHT_PLAN_RECORD_KEY.into(),
        old[FLIGHT_PLAN_RECORD_KEY].clone(),
    );
    if version == 1 {
        engine.persistent.last_provider_failure = Some(CloudProviderFailure {
            kind: CloudProviderErrorKind::Permanent,
            detail: "unsupported cloud flight-plan schema 2".into(),
        });
    }
    (restart(&engine), provider, old)
}

fn restart(engine: &CloudEngine) -> CloudEngine {
    let bytes = serde_json::to_vec(&engine.persistent).unwrap();
    CloudEngine::new(serde_json::from_slice(&bytes).unwrap())
}

fn upgraded_records(
    old: &BTreeMap<String, CloudRecord>,
    version: u32,
) -> BTreeMap<String, CloudRecord> {
    let mut expected = old.clone();
    if version == 1 {
        expected.remove(FLIGHT_PLAN_RECORD_KEY);
    }
    for (key, record) in &mut expected {
        if key.starts_with(product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX) {
            *record =
                CloudRecord::fixture(3, record.modified_at_epoch_ms(), record.value().clone());
        } else if version == 1 && key.starts_with(AIRCRAFT_LIBRARY_RECORD_PREFIX) {
            *record = CloudRecord::fixture(
                2,
                record.modified_at_epoch_ms(),
                serde_json::json!({"included": record.value()["included"]}),
            );
        }
    }
    expected
}

fn assert_aircraft_preserved(
    old: &BTreeMap<String, CloudRecord>,
    current: &BTreeMap<String, CloudRecord>,
) {
    let mut count = 0;
    for (key, before) in old {
        let Some(hash) = key.strip_prefix(product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX) else {
            continue;
        };
        count += 1;
        let after = &current[key];
        assert_eq!(after.schema_version(), 3);
        assert_eq!(after.modified_at_epoch_ms(), before.modified_at_epoch_ms());
        assert_eq!(
            serde_json::to_vec(after.value()).unwrap(),
            serde_json::to_vec(before.value()).unwrap(),
            "content-addressed definition payload changed: {key}"
        );
        let definition = after.decode::<AircraftRecord>().unwrap();
        assert_eq!(definition.content_hash().unwrap(), hash);
        assert_eq!(definition.schema_version, before.schema_version());
        if before.schema_version() == 2 {
            assert!(definition.glide.is_none());
        }
    }
    assert!(count > 0);
}

fn click(engine: &mut CloudEngine, id: CloudUiActionId, current_plan: &FlightPlan, now: i64) {
    assert!(engine
        .page_state(now)
        .sync_account_panels
        .iter()
        .flat_map(|p| &p.actions)
        .any(|action| action.id == id && action.enabled));
    engine
        .perform_ui_action(id, &[], current_plan, now)
        .unwrap();
}

fn confirm_upgrade(engine: &mut CloudEngine, now: i64) {
    let current_plan = plan(&["KSEA", "KPDX"]);
    click(
        engine,
        CloudUiActionId::BeginAccountUpgrade,
        &current_plan,
        now,
    );
    click(
        engine,
        CloudUiActionId::ConfirmAccountUpgrade,
        &current_plan,
        now,
    );
}

fn read_only_pump(engine: &mut CloudEngine, provider: &mut InMemoryAcsProvider, now: i64) {
    for _ in 0..32 {
        let Some(http) = engine.take_provider_request(now).unwrap() else {
            return;
        };
        let request = engine.provider_request_in_flight.clone().unwrap();
        assert!(
            matches!(
                request.operation,
                CloudProviderOperation::AcsReadRoot
                    | CloudProviderOperation::AcsReadObject { .. }
                    | CloudProviderOperation::AcsCreateSseTicket { .. }
            ),
            "unexpected publication: {:?}",
            request.operation
        );
        let locator = engine
            .account()
            .unwrap()
            .acs
            .as_ref()
            .unwrap()
            .account_locator
            .clone();
        let response = execute_acs(provider, &request, &locator, now);
        let completion = engine
            .complete_provider_request(http.request_id, response, now)
            .unwrap();
        assert!(completion.changed_records.is_empty());
    }
    panic!("read-only upgrade pump did not quiesce");
}

#[test]
fn format_two_upgrade_preserves_aircraft_and_current_crossfill_only_after_consent() {
    let (mut engine, mut provider, old) = fixture(2, 4);
    let expected = upgraded_records(&old, 2);
    let initial = root(&engine, &provider);
    let shared_plan = flight_plan_from_record(&old[FLIGHT_PLAN_RECORD_KEY])
        .unwrap()
        .plan;
    let selection = shared_plan.aircraft.as_ref().unwrap();
    let definition_key =
        product_contracts::aircraft_definition_key(&selection.definition_hash).unwrap();
    assert_eq!(old[&definition_key].schema_version(), 2);
    assert_eq!(old[&definition_key].value()["schema_version"], 2);
    assert_eq!(cloud_records(&engine, &provider, 2), old);
    assert_eq!(engine.persistent.records.cached, expected);
    assert_eq!(engine.persistent.records_format, CLOUD_NODE_VERSION);
    assert_eq!(engine.cached_flight_plan(), Some(shared_plan.clone()));
    assert_eq!(
        engine.persistent.records.deferred_adoption[FLIGHT_PLAN_RECORD_KEY],
        old[FLIGHT_PLAN_RECORD_KEY]
    );
    read_only_pump(&mut engine, &mut provider, 3_000);
    assert_eq!(engine.status_summary(3_000).label, "PAUSED");
    assert_eq!(root(&engine, &provider), initial);

    let current_plan = plan(&["KSEA", "KPDX"]);
    click(
        &mut engine,
        CloudUiActionId::BeginAccountUpgrade,
        &current_plan,
        3_001,
    );
    let confirmation = engine
        .page_state(3_001)
        .sync_account_panels
        .into_iter()
        .find(|panel| panel.id == "confirm_upgrade")
        .unwrap()
        .summary
        .unwrap();
    assert!(confirmation.contains("aircraft glide-performance"));
    assert!(!confirmation.contains("removes the previously shared flight plan"));
    click(
        &mut engine,
        CloudUiActionId::CloseLinkedDetail,
        &current_plan,
        3_002,
    );
    click(&mut engine, CloudUiActionId::SyncNow, &current_plan, 3_003);
    read_only_pump(&mut engine, &mut provider, 3_003);
    assert_eq!(root(&engine, &provider), initial);
    engine = restart(&engine);
    read_only_pump(&mut engine, &mut provider, 3_004);
    assert_eq!(root(&engine, &provider), initial);
    assert_eq!(engine.status_summary(3_004).label, "PAUSED");

    confirm_upgrade(&mut engine, 3_005);
    assert!(pump_acs(&mut engine, &mut provider, 3_005).is_empty());
    assert!(engine.persistent.last_provider_failure.is_none());
    let current = cloud_records(&engine, &provider, CLOUD_NODE_VERSION);
    assert_eq!(current, expected);
    assert_aircraft_preserved(&old, &current);
    assert_eq!(current[FLIGHT_PLAN_RECORD_KEY].schema_version(), 4);
    assert_eq!(current[FLIGHT_PLAN_RECORD_KEY], old[FLIGHT_PLAN_RECORD_KEY]);
    assert!(current[&definition_key]
        .decode::<AircraftRecord>()
        .unwrap()
        .profile(&selection.profile_id)
        .is_some());
    assert!(engine.status_record(3_005).is_none());

    let committed = root(&engine, &provider);
    engine = restart(&engine);
    assert!(pump_acs(&mut engine, &mut provider, 4_000).is_empty());
    assert_eq!(root(&engine, &provider), committed);
    assert_eq!(
        cloud_records(&engine, &provider, CLOUD_NODE_VERSION),
        expected
    );
    assert!(engine.persistent.records.pending_keys.is_empty());
    assert_eq!(engine.take_pending_remote_flight_plan(), Some(shared_plan));
    assert_eq!(current_plan, plan(&["KSEA", "KPDX"]));
}

#[test]
fn interrupted_format_two_upgrade_preserves_records_and_does_not_reuse_consent() {
    for commit in [false, true] {
        let (mut engine, mut provider, old) = fixture(2, 4);
        let expected = upgraded_records(&old, 2);
        let initial = root(&engine, &provider);
        read_only_pump(&mut engine, &mut provider, 3_000);
        confirm_upgrade(&mut engine, 3_001);
        let locator = engine
            .account()
            .unwrap()
            .acs
            .as_ref()
            .unwrap()
            .account_locator
            .clone();
        let mut held_cas = None;
        for _ in 0..32 {
            let http = engine
                .take_provider_request(3_001)
                .unwrap()
                .expect("upgrade CAS expected");
            let request = engine.provider_request_in_flight.clone().unwrap();
            if matches!(
                request.operation,
                CloudProviderOperation::AcsCompareAndSwapRoot { .. }
            ) {
                held_cas = Some(request);
                break;
            }
            let response = execute_acs(&mut provider, &request, &locator, 3_001);
            engine
                .complete_provider_request(http.request_id, response, 3_001)
                .unwrap();
        }
        let held_cas = held_cas.expect("upgrade must stage a root CAS");
        assert_eq!(root(&engine, &provider), initial);
        if commit {
            // The server commits; the process stops before consuming the response.
            execute_acs(&mut provider, &held_cas, &locator, 3_002);
        }
        let before_restart = root(&engine, &provider);
        engine = restart(&engine);
        assert!(engine.upgrade_from.is_none());
        read_only_pump(&mut engine, &mut provider, 4_000);
        assert_eq!(root(&engine, &provider), before_restart);
        if !commit {
            assert_eq!(engine.status_summary(4_000).label, "PAUSED");
            assert_eq!(cloud_records(&engine, &provider, 2), old);
            confirm_upgrade(&mut engine, 4_001);
            assert!(pump_acs(&mut engine, &mut provider, 4_001).is_empty());
        }
        assert!(engine.persistent.last_provider_failure.is_none());
        assert_eq!(engine.status_summary(4_001).label, "LINKED");
        let current = cloud_records(&engine, &provider, CLOUD_NODE_VERSION);
        assert_eq!(current, expected);
        assert_aircraft_preserved(&old, &current);
    }
}

#[test]
fn invalid_aircraft_upgrade_leaves_encrypted_account_and_pending_records_unchanged() {
    for version in [1, 2] {
        for (schema, corruption) in [
            (0, "none"),
            (1, "none"),
            (4, "none"),
            (u32::MAX, "none"),
            (2, "missing field"),
            (3, "missing field"),
            (2, "unsupported inner schema"),
            (3, "unsupported inner schema"),
            (2, "schema two glide"),
            (3, "schema two glide"),
            (2, "hash mismatch"),
        ] {
            let (mut engine, mut provider, mut old) =
                fixture(version, if version == 1 { 2 } else { 4 });
            let (key, original) = old
                .iter()
                .find(|(key, record)| {
                    key.starts_with(product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX)
                        && record.schema_version() == 2
                })
                .unwrap();
            let key = key.clone();
            let mut value = original.value().clone();
            match corruption {
                "none" => {}
                "missing field" => {
                    value.as_object_mut().unwrap().remove("model");
                }
                "unsupported inner schema" => value["schema_version"] = serde_json::json!(4),
                "schema two glide" => {
                    value["glide"] = serde_json::json!({
                        "best_glide_ias_kt": 90.0, "glide_ratio": 10.0,
                        "reference_weight_lb": 3000.0, "source": "test"
                    })
                }
                "hash mismatch" => value["label"] = serde_json::json!("Different aircraft payload"),
                _ => unreachable!(),
            }
            old.insert(key, CloudRecord::fixture(schema, None, value));
            install_legacy_page(&engine, &mut provider, version, &old);
            let initial = root(&engine, &provider);
            let cached = engine.persistent.records.cached.clone();
            let pending = engine.persistent.records.pending_keys.clone();
            let deferred = engine.persistent.records.deferred_adoption.clone();
            read_only_pump(&mut engine, &mut provider, 3_000);
            confirm_upgrade(&mut engine, 3_001);
            read_only_pump(&mut engine, &mut provider, 3_001);
            assert!(
                engine.persistent.last_provider_failure.is_some(),
                "format {version}, aircraft envelope {schema}, {corruption} must be rejected"
            );
            assert!(engine.upgrade_from.is_none());
            assert_eq!(root(&engine, &provider), initial);
            assert_eq!(cloud_records(&engine, &provider, version), old);
            assert_eq!(engine.persistent.records.cached, cached);
            assert_eq!(engine.persistent.records.pending_keys, pending);
            assert_eq!(engine.persistent.records.deferred_adoption, deferred);
            engine = restart(&engine);
            read_only_pump(&mut engine, &mut provider, 4_000);
            assert_eq!(root(&engine, &provider), initial);
            assert_eq!(engine.persistent.records.pending_keys, pending);
        }
    }
}

#[test]
fn real_legacy_upgrade_discards_only_crossfill_after_consent_and_cannot_resurrect_it() {
    for schema in [1, 2, 3] {
        let (mut engine, mut provider, old) = fixture(1, schema);
        let expected = upgraded_records(&old, 1);
        assert_eq!(cloud_records(&engine, &provider, 1), old);
        let initial = root(&engine, &provider);
        assert!(engine.persistent.last_provider_failure.is_none());
        assert!(engine.cached_flight_plan().is_none());
        assert!(engine.take_pending_remote_flight_plan().is_none());
        assert!(!engine
            .persistent
            .records
            .pending_keys
            .contains(FLIGHT_PLAN_RECORD_KEY));
        assert!(pump_acs(&mut engine, &mut provider, 3_000).is_empty());
        assert_eq!(engine.status_summary(3_000).label, "PAUSED");
        assert_eq!(root(&engine, &provider), initial);
        let current_plan = plan(&["KSEA", "KPDX"]);
        click(
            &mut engine,
            CloudUiActionId::BeginAccountUpgrade,
            &current_plan,
            3_001,
        );
        let confirmation = engine
            .page_state(3_001)
            .sync_account_panels
            .into_iter()
            .find(|panel| panel.id == "confirm_upgrade")
            .unwrap()
            .summary
            .unwrap();
        assert!(confirmation.contains("removes the previously shared flight plan"));
        assert!(confirmation.contains("does not clear the flight plan currently open"));
        click(
            &mut engine,
            CloudUiActionId::CloseLinkedDetail,
            &current_plan,
            3_002,
        );
        click(&mut engine, CloudUiActionId::SyncNow, &current_plan, 3_003);
        assert!(pump_acs(&mut engine, &mut provider, 3_003).is_empty());
        assert_eq!(root(&engine, &provider), initial);
        click(
            &mut engine,
            CloudUiActionId::BeginAccountUpgrade,
            &current_plan,
            3_004,
        );
        click(
            &mut engine,
            CloudUiActionId::ConfirmAccountUpgrade,
            &current_plan,
            3_005,
        );
        assert!(
            pump_acs(&mut engine, &mut provider, 3_005).is_empty(),
            "upgrade must not clear/adopt a local plan"
        );
        assert!(
            engine.persistent.last_provider_failure.is_none(),
            "{:?}",
            engine.persistent.last_provider_failure
        );
        let current = cloud_records(&engine, &provider, CLOUD_NODE_VERSION);
        assert_eq!(current, expected);
        assert_aircraft_preserved(&old, &current);
        assert!(engine.status_record(3_005).is_none());
        let mut restarted = restart(&engine);
        assert!(pump_acs(&mut restarted, &mut provider, 4_000).is_empty());
        assert_eq!(
            cloud_records(&restarted, &provider, CLOUD_NODE_VERSION),
            expected
        );
        assert!(restarted.persistent.records.pending_keys.is_empty());
        assert_eq!(current_plan, plan(&["KSEA", "KPDX"]));
    }
}

#[test]
fn fresh_link_to_legacy_account_and_current_local_edit_survive_another_devices_upgrade() {
    let (mut upgrading, mut provider, _) = fixture(1, 2);
    let (mut joining, adopted) =
        link_account(&mut provider, upgrading.device_setup_code().unwrap(), 3_000);
    assert!(adopted.is_empty());
    assert_eq!(joining.status_summary(3_000).label, "PAUSED");
    let mine = plan(&["KPAE", "KSUS"]);
    joining
        .record_local_flight_plan_mutation(&FlightPlan::default(), &mine, 3_100)
        .unwrap();
    assert!(pump_acs(&mut joining, &mut provider, 3_100).is_empty());
    pump_acs(&mut upgrading, &mut provider, 3_200);
    click(
        &mut upgrading,
        CloudUiActionId::BeginAccountUpgrade,
        &mine,
        3_201,
    );
    click(
        &mut upgrading,
        CloudUiActionId::ConfirmAccountUpgrade,
        &mine,
        3_202,
    );
    pump_acs(&mut upgrading, &mut provider, 3_202);
    assert!(
        upgrading.persistent.last_provider_failure.is_none(),
        "{:?}",
        upgrading.persistent.last_provider_failure
    );
    assert!(!cloud_records(&upgrading, &provider, CLOUD_NODE_VERSION)
        .contains_key(FLIGHT_PLAN_RECORD_KEY));
    pump_acs(&mut joining, &mut provider, 64_000);
    let current = cloud_records(&joining, &provider, CLOUD_NODE_VERSION);
    assert_eq!(
        flight_plan_from_record(&current[FLIGHT_PLAN_RECORD_KEY])
            .unwrap()
            .plan,
        mine
    );
    assert_eq!(
        current[FLIGHT_PLAN_RECORD_KEY].modified_at_epoch_ms(),
        Some(3_100)
    );
}
