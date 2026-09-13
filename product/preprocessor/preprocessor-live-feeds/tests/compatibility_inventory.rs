// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs;

use app_core::live_feed_cache::{
    live_feed_product_registry, LiveFeedCache, LiveFeedFetchedPayload, LiveFeedProductDriver,
};
use app_core::live_feeds::{LiveFeedCacheRequest, LiveFeedCacheRequestKind, LiveFeedSseEvent};
use chrono::{TimeZone, Utc};
use preprocessor_live_feeds::engine::{
    live_feed_invalidation_from_update, read_live_feeds_current, BuiltLiveFeedState, DeltaPolicy,
    FileLiveFeedPublisher, FixedClock, LiveFeedPublisher, LiveFeedStatePayload,
    LiveFeedVersionManifest,
};
use product_contracts::{live_feed_compatibility_descriptor, LIVE_FEED_PRODUCT_POLICIES};
use serde_json::{json, Value};

#[test]
fn compatibility_inventory_covers_every_actual_core_sse_subscription() {
    use app_core::live_feed_runtime::{
        live_feed_runtime_decision, LiveFeedRuntimeEventKind, LiveFeedRuntimeInput,
        LiveFeedRuntimeState,
    };

    let inventory = live_feed_compatibility_descriptor();
    let decision = live_feed_runtime_decision(
        &mut LiveFeedRuntimeState::default(),
        LiveFeedRuntimeInput {
            kind: LiveFeedRuntimeEventKind::Start,
            now_ms: 0,
            message: None,
            source_url: None,
            status_url: None,
            network_status: None,
        },
    );
    let mut expected = inventory
        .protocols
        .iter()
        .filter(|(name, _)| name.starts_with("sse_") && name.as_str() != "sse_framing")
        .map(|(_, contract)| contract.encoding.clone())
        .collect::<Vec<_>>();
    expected.sort();
    let mut actual = decision.event_names;
    actual.sort();
    assert_eq!(actual, expected);
}

#[test]
fn inventory_bulletin_hint_reaches_the_shared_decoder_without_weather_cache_changes(
) -> anyhow::Result<()> {
    use product_contracts::service_bulletins::BulletinHint;

    let inventory = live_feed_compatibility_descriptor();
    let contract = &inventory.protocols["sse_service_bulletins"];
    assert_eq!(contract.schema_version, BulletinHint::SCHEMA_VERSION);
    assert!(live_feed_product_registry()
        .driver(&contract.encoding)
        .is_none());
    let hint = BulletinHint {
        publisher: "https://example.invalid/service/bulletins-v1.json".into(),
        revision: 7,
    };
    let event = LiveFeedSseEvent {
        id: None,
        event: Some(contract.encoding.clone()),
        data: serde_json::to_string(&Some(&hint))?,
    };
    let mut core = LiveFeedCache::default();
    core.set_source_root_url("https://example.invalid/live-feeds/v3/")?;
    let outcome = core.ingest_sse_event(&event)?;
    assert!(!outcome.cache_changed);
    assert!(core.missing_requests().is_empty());
    assert_eq!(outcome.session_events.len(), 1);
    let forwarded = &outcome.session_events[0];
    assert_eq!(forwarded.event, event.event);
    assert_eq!(forwarded.data, event.data);
    assert_eq!(serde_json::from_str::<BulletinHint>(&forwarded.data)?, hint);
    Ok(())
}

#[test]
fn compatibility_inventory_matches_every_actual_core_product_driver() {
    let inventory = live_feed_compatibility_descriptor();
    let registry = live_feed_product_registry();
    for policy in LIVE_FEED_PRODUCT_POLICIES {
        let product = &inventory.products[policy.product_id];
        match registry
            .driver(policy.product_id)
            .expect("registered core driver")
        {
            LiveFeedProductDriver::RecordJson {
                records_key,
                record_id_key,
                count_key,
                ..
            } => {
                assert_eq!(&product.parameters["records_key"], records_key);
                assert_eq!(
                    product.parameters.get("record_id_key"),
                    record_id_key.as_ref()
                );
                assert_eq!(product.parameters.get("count_key"), count_key.as_ref());
                assert_eq!(
                    product.parameters["records_shape"],
                    if record_id_key.is_some() {
                        "array"
                    } else {
                        "object"
                    }
                );
                assert_eq!(
                    product.formats["snapshot"].encoding,
                    product_contracts::LIVE_FEED_JSON_XZ_ENCODING
                );
                assert_eq!(
                    product.formats["delta"].encoding,
                    product_contracts::LIVE_FEED_RECORD_DELTA_ENCODING
                );
            }
            LiveFeedProductDriver::NavKv { .. } => {
                assert_eq!(
                    product.formats["snapshot"].encoding,
                    product_contracts::LIVE_FEED_NAV_KV_ENCODING
                );
                assert_eq!(
                    product.formats["install"].encoding,
                    product_contracts::LIVE_FEED_NAV_KV_PACKAGE_ENCODING
                );
            }
            LiveFeedProductDriver::NexradPackage { .. } => {
                assert_eq!(
                    product.formats["snapshot"].encoding,
                    product_contracts::LIVE_FEED_JSON_ENCODING
                );
                assert_eq!(
                    product.formats["install"].encoding,
                    product_contracts::LIVE_FEED_DIRECTORY_PACKAGE_ENCODING
                );
                assert_eq!(
                    product.formats["tile"].encoding,
                    product_contracts::NEXRAD_TILE_ENCODING
                );
            }
            LiveFeedProductDriver::Notam { .. } => {
                let checkpoint = notam_state::NotamState::default().checkpoint();
                assert_eq!(
                    product.formats["snapshot"].schema_version,
                    checkpoint.schema_version
                );
                assert_eq!(
                    product.formats["records"].schema_version,
                    checkpoint.contract_version
                );
            }
            LiveFeedProductDriver::FullJson { .. } => panic!("unrepresented full JSON driver"),
        }
    }
}

fn record_transport_round_trip(id: &str) -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("published");
    let publisher = FileLiveFeedPublisher::new(
        root.clone(),
        FixedClock::new(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()),
    );
    let inventory = live_feed_compatibility_descriptor();
    let contract = &inventory.products[id];
    let registry = live_feed_product_registry();
    let (records_key, record_id_key, count_key) = registry.record_json_delta_schema(id).unwrap();
    let mut core = LiveFeedCache::default();
    core.set_source_root_url("https://example.invalid/live-feeds/v3/")?;
    for (version, previous, value) in [("v1", None, 1), ("v2", Some("v1"), 2)] {
        let mut record = json!({"value": value});
        let (records, delta_policy) = if let Some(key) = record_id_key.as_ref() {
            record[key] = json!("KSFO");
            (
                json!([record]),
                DeltaPolicy::KeyedArrayRecords {
                    records_key: records_key.clone(),
                    record_id_key: key.clone(),
                    count_key: count_key.clone(),
                },
            )
        } else {
            (
                json!({"KSFO": record}),
                DeltaPolicy::KeyedRecords {
                    records_key: records_key.clone(),
                    count_key: count_key.clone(),
                },
            )
        };
        let mut state = json!({"schema_version":contract.formats["snapshot"].schema_version, "version_label":version});
        state[&records_key] = records;
        if let Some(key) = count_key.as_ref() {
            state[key] = json!(1);
        }
        let path = temporary.path().join(format!("{version}.json"));
        fs::write(&path, serde_json::to_vec(&state)?)?;
        let update = publisher.publish(BuiltLiveFeedState {
            product: id.into(),
            version: version.into(),
            payload: LiveFeedStatePayload::JsonFile {
                path,
                value: state.clone(),
            },
            state_sha256: None,
            state_payload_kind: None,
            status_timestamps: Default::default(),
            temporal_coverage: None,
            delta_policy,
            precomputed_delta: None,
            changed_count_if_no_delta: 1,
        })?;
        let catalog = read_live_feeds_current(&root)?.unwrap();
        assert_eq!(
            catalog.schema_version,
            inventory.protocols["discovery"].schema_version
        );
        if previous.is_none() {
            core.ingest_sse_event(&LiveFeedSseEvent {
                id: None,
                event: Some(inventory.protocols["sse_catalog"].encoding.clone()),
                data: serde_json::to_string(&catalog)?,
            })?;
        } else {
            let event = live_feed_invalidation_from_update(&update);
            assert_eq!(
                event.schema_version,
                inventory.protocols["sse_current"].schema_version
            );
            core.ingest_sse_event(&LiveFeedSseEvent {
                id: None,
                event: Some(inventory.protocols["sse_current"].encoding.clone()),
                data: serde_json::to_string(&event)?,
            })?;
        }
        let manifest_bytes = fs::read(&update.version_manifest_path)?;
        let manifest: LiveFeedVersionManifest = serde_json::from_slice(&manifest_bytes)?;
        assert_eq!(
            manifest.schema_version,
            inventory.protocols["version_manifest"].schema_version
        );
        assert_eq!(
            manifest.state.kind.as_deref(),
            Some(contract.formats["snapshot"].encoding.as_str())
        );
        core.ingest_version_manifest(id, version, &manifest_bytes)?;
        let (kind, url, bytes) = if let Some(previous) = previous {
            let delta = manifest.delta_from_previous.unwrap();
            assert_eq!(
                delta.kind.as_deref(),
                Some(contract.formats["delta"].encoding.as_str())
            );
            let bytes = fs::read(update.delta_path.unwrap())?;
            let decoded: Value = serde_json::from_slice(
                &nav_kv_package::decode_xz_if_needed(&bytes).map_err(anyhow::Error::msg)?,
            )?;
            assert_eq!(
                decoded["schema_version"],
                contract.formats["delta"].schema_version
            );
            (
                LiveFeedCacheRequestKind::Delta {
                    product: id.into(),
                    from_version: previous.into(),
                    to_version: version.into(),
                    payload_kind: delta.kind,
                },
                delta.url,
                bytes,
            )
        } else {
            (
                LiveFeedCacheRequestKind::Full {
                    product: id.into(),
                    version: version.into(),
                    payload_kind: manifest.state.kind,
                    install_profile: None,
                },
                manifest.state.url,
                fs::read(update.state_path)?,
            )
        };
        let request = LiveFeedCacheRequest {
            id: format!("live_feeds/test/{id}/{version}"),
            url,
            kind,
        };
        let installed = core
            .install_fetched_payload(&registry, &request, LiveFeedFetchedPayload::Bytes(bytes))?
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&installed.payload_bytes()?)?,
            state
        );
        core.acknowledge_install_candidate(id, version)?;
        assert_eq!(core.installed(id).unwrap().version, version);
    }
    Ok(())
}

#[test]
fn metar_producer_snapshot_and_delta_match_inventory_and_core_decoder() -> anyhow::Result<()> {
    record_transport_round_trip("metars")
}

#[test]
fn taf_producer_snapshot_and_delta_match_inventory_and_core_decoder() -> anyhow::Result<()> {
    record_transport_round_trip("tafs")
}

#[test]
fn pirep_producer_snapshot_and_delta_match_inventory_and_core_decoder() -> anyhow::Result<()> {
    record_transport_round_trip("pireps")
}

#[test]
fn tfr_producer_snapshot_and_delta_match_inventory_and_core_decoder() -> anyhow::Result<()> {
    record_transport_round_trip("tfrs")
}
