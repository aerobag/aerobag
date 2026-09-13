// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use app_core::{
    live_feed_product_registry, LiveFeedCache, LiveFeedCacheRequest, LiveFeedCacheRequestKind,
    LiveFeedFetchedPayload,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const SCHEMA: u32 = app_core::live_feeds::LIVE_FEEDS_SCHEMA_VERSION;

struct Weather {
    version: &'static str,
    bytes: Vec<u8>,
    sha: String,
    manifest: Value,
}

impl Weather {
    fn new(version: &'static str, marker: &str, stations: &[&str]) -> Self {
        let records = stations
            .iter()
            .map(|station| {
                (
                    station.to_string(),
                    json!({ "station_id": station, "raw_text": marker }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let state = json!({
            "schema_version": SCHEMA,
            "version_label": version,
            "metar_count": records.len(),
            "metars_by_station": records,
        });
        let bytes = serde_json::to_vec(&state).unwrap();
        let sha = format!("{:x}", Sha256::digest(&bytes));
        let manifest = json!({
            "schema_version": SCHEMA,
            "product": "metars", "version": version,
            "state": {
                "kind": "json", "url": format!("states/metars/{version}.json"),
                "bytes": bytes.len(), "blob_sha256": sha, "state_sha256": sha,
            },
        });
        Self {
            version,
            bytes,
            sha,
            manifest,
        }
    }

    fn advertise(&self, cache: &mut LiveFeedCache) -> LiveFeedCacheRequest {
        cache
            .ingest_catalog(
                &serde_json::to_vec(&json!({
                    "schema_version": SCHEMA,
                    "generated_at_utc": "2026-08-04T12:00:00Z",
                    "products": { "metars": {
                        "current": self.version,
                        "version_manifest_url": format!("versions/metars/{}.json", self.version),
                        "state_url": format!("states/metars/{}.json", self.version),
                        "state_sha256": self.sha,
                    }},
                }))
                .unwrap(),
            )
            .unwrap();
        cache
            .ingest_version_manifest(
                "metars",
                self.version,
                &serde_json::to_vec(&self.manifest).unwrap(),
            )
            .unwrap();
        let requests = cache.missing_requests();
        assert_eq!(requests.len(), 1);
        requests.into_iter().next().unwrap()
    }

    fn install(&self, cache: &mut LiveFeedCache) {
        let request = self.advertise(cache);
        assert!(matches!(
            request.kind,
            LiveFeedCacheRequestKind::Full { .. }
        ));
        cache
            .install_fetched_payload(
                &live_feed_product_registry(),
                &request,
                LiveFeedFetchedPayload::Bytes(self.bytes.clone()),
            )
            .unwrap();
    }

    fn assert_installed(&self, cache: &LiveFeedCache) {
        let installed = cache.installed("metars").unwrap();
        assert_eq!(installed.version, self.version);
        assert_eq!(installed.state_sha256, self.sha);
        let actual: Value = serde_json::from_slice(&installed.payload_bytes().unwrap()).unwrap();
        assert_eq!(
            actual,
            serde_json::from_slice::<Value>(&self.bytes).unwrap()
        );
    }
}

fn cache() -> LiveFeedCache {
    LiveFeedCache::with_source_root_url_and_installed(
        "http://fixture.invalid/releases/sunset/live-feeds/",
        [],
    )
    .unwrap()
}

#[test]
fn independent_provider_without_delta_base_converges_on_full_state_and_next_update() {
    let mut cache = cache();
    let old = Weather::new("old-1", "OLD", &["KSEA", "KOLM"]);
    old.install(&mut cache);
    old.assert_installed(&cache);

    let mut new = Weather::new("new-1", "NEW", &["KSEA", "KBFI"]);
    new.manifest["delta_from_previous"] = json!({
        "kind": "record_json_delta_xz",
        "from_version": "new-retired", "to_version": new.version,
        "from_state_sha256": "0".repeat(64), "to_state_sha256": new.sha,
        "url": "deltas/metars/new-retired__new-1.json.xz",
        "bytes": 1, "blob_sha256": "1".repeat(64),
    });
    // Even a cheaper delta must not be requested using the unrelated old state.
    new.install(&mut cache);
    new.assert_installed(&cache);
    assert!(cache.missing_requests().is_empty());

    let next = Weather::new("new-2", "NEXT", &["KSEA", "KRNT"]);
    next.install(&mut cache);
    next.assert_installed(&cache);
    assert!(cache.missing_requests().is_empty());
}

#[test]
fn old_provider_response_prepared_before_switch_cannot_replace_new_full_state() {
    let mut cache = cache();
    let old = Weather::new("old-1", "OLD", &["KSEA"]);
    old.install(&mut cache);
    let late = Weather::new("old-late", "OBSOLETE", &["KSEA", "KOLM"]);
    let pending = late.advertise(&mut cache);
    let plan = cache
        .full_install_plan(&live_feed_product_registry(), &pending)
        .unwrap()
        .unwrap();

    let new = Weather::new("new-1", "NEW", &["KSEA", "KBFI"]);
    new.install(&mut cache);
    // The old HTTP completion was already admitted and prepared independently.
    let obsolete = plan
        .install(LiveFeedFetchedPayload::Bytes(late.bytes.clone()))
        .unwrap();
    assert!(cache
        .commit_prepared_full_install(&pending, obsolete)
        .is_err());
    new.assert_installed(&cache);
    assert!(cache.missing_requests().is_empty());
    let next = Weather::new("new-2", "NEXT", &["KRNT"]);
    next.install(&mut cache);
    next.assert_installed(&cache);
}
