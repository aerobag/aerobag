// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use crate::cloud_acs_memory::InMemoryAcsProvider;

fn install_legacy_page(
    engine: &CloudEngine,
    provider: &mut InMemoryAcsProvider,
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
        version: 1,
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
    node.version = 1;
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
    provider
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
    engine
        .decrypt_acs_value::<CloudPage>(
            &object.value,
            "merkle_page",
            AcsEncryptedValueKind::Object,
            &node.merkle_root_id,
        )
        .unwrap()
        .records
}

fn fixture(
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
    let aircraft = bundled_private_aircraft();
    let hash = aircraft.content_hash().unwrap();
    engine.record_local_aircraft_definition(&aircraft).unwrap();
    engine
        .record_local_aircraft_library_membership(
            &hash,
            product_contracts::AircraftLibraryMembership { included: true },
            115,
        )
        .unwrap();
    engine.record_service_read(&"a".repeat(64)).unwrap();
    let mut provider = InMemoryAcsProvider::default();
    create_account(&mut engine, &mut provider, &plan(&["KRNT", "KPAE"]), 1_000);
    let mut old = engine.persistent.records.cached.clone();
    old.insert(
        format!("{AIRCRAFT_LIBRARY_RECORD_PREFIX}{hash}"),
        CloudRecord::fixture(
            1,
            Some(115),
            serde_json::json!({"included": true, "deleted": false}),
        ),
    );
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
    old.insert(
        "future/unrelated".into(),
        CloudRecord::fixture(
            79,
            Some(116),
            serde_json::json!({"preserve": ["opaque", "data"]}),
        ),
    );
    install_legacy_page(&engine, &mut provider, &old);
    engine.persistent.records_format = 1;
    engine.persistent.records.cached = old.clone();
    engine.persistent.records.pending_keys = old.keys().cloned().collect();
    engine.persistent.records.deferred_adoption.insert(
        FLIGHT_PLAN_RECORD_KEY.into(),
        old[FLIGHT_PLAN_RECORD_KEY].clone(),
    );
    engine.persistent.last_provider_failure = Some(CloudProviderFailure {
        kind: CloudProviderErrorKind::Permanent,
        detail: "unsupported cloud flight-plan schema 2".into(),
    });
    (CloudEngine::new(engine.persistent), provider, old)
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

#[test]
fn real_legacy_upgrade_discards_only_crossfill_after_consent_and_cannot_resurrect_it() {
    for schema in [1, 2, 3] {
        let (mut engine, mut provider, mut expected) = fixture(schema);
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
        expected.remove(FLIGHT_PLAN_RECORD_KEY);
        for (key, record) in &mut expected {
            if key.starts_with(AIRCRAFT_LIBRARY_RECORD_PREFIX) {
                *record = CloudRecord::encode::<AircraftMembershipRecord>(
                    &wire::preferences::AircraftMembership { included: true },
                    Some(115),
                )
                .unwrap();
            }
        }
        assert_eq!(cloud_records(&engine, &provider), expected);
        assert!(engine.status_record(3_005).is_none());
        let mut restarted = CloudEngine::new(engine.persistent);
        assert!(pump_acs(&mut restarted, &mut provider, 4_000).is_empty());
        assert_eq!(cloud_records(&restarted, &provider), expected);
        assert!(restarted.persistent.records.pending_keys.is_empty());
        assert_eq!(current_plan, plan(&["KSEA", "KPDX"]));
    }
}

#[test]
fn fresh_link_to_legacy_account_and_current_local_edit_survive_another_devices_upgrade() {
    let (mut upgrading, mut provider, _) = fixture(2);
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
    assert!(!cloud_records(&upgrading, &provider).contains_key(FLIGHT_PLAN_RECORD_KEY));
    pump_acs(&mut joining, &mut provider, 64_000);
    let current = cloud_records(&joining, &provider);
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
