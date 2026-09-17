// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{live_feeds::v3, *};

pub const LIVE_FEED_COMPATIBILITY_SCHEMA_VERSION: u32 = 1;
pub const METAR_PRODUCT_CONTRACT_VERSION: u32 = 9;
pub const METAR_SNAPSHOT_SCHEMA_VERSION: u32 = 4;
pub const TAF_PRODUCT_CONTRACT_VERSION: u32 = 1;
pub const PIREP_PRODUCT_CONTRACT_VERSION: u32 = 1;
pub const TFR_PRODUCT_CONTRACT_VERSION: u32 = 2;
pub const TFR_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const OBSTACLE_STATE_LAYOUT_VERSION: u32 = 2;
pub const OBSTACLE_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const OBSTACLE_TILE_SCHEMA_VERSION: u32 = 1;
pub const NEXRAD_MANIFEST_SCHEMA_VERSION: u32 = 2;
pub const NEXRAD_TILE_ENCODING: &str = "png-bounded-palette-v1";
pub const NEXRAD_OVERFLOW_ENCODING: &str = "rgba8";

// These identifiers version previously implicit framing/codec assumptions.
// They do not change the bytes or payload schemas already on the wire.
pub const LIVE_FEED_JSON_ENCODING: &str = "json";
pub const LIVE_FEED_JSON_XZ_ENCODING: &str = "json_xz";
pub const LIVE_FEED_RECORD_DELTA_ENCODING: &str = "record_json_delta_xz";
pub const LIVE_FEED_NAV_KV_ENCODING: &str = "nav_kv";
pub const LIVE_FEED_NAV_KV_DELTA_ENCODING: &str = "nav_kv_delta_xz";
pub const LIVE_FEED_NAV_KV_PACKAGE_ENCODING: &str = "nav_kv_package";
pub const LIVE_FEED_DIRECTORY_PACKAGE_ENCODING: &str = "directory_package";
pub const LIVE_FEED_NOTAM_CHECKPOINT_ENCODING: &str = "notam_checkpoint_xz";
pub const LIVE_FEED_NOTAM_DELTA_ENCODING: &str = "notam_ordered_delta_xz";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireFormatContract {
    pub schema_version: u32,
    pub encoding: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveFeedProductCompatibility {
    pub formats: BTreeMap<String, WireFormatContract>,
    pub parameters: BTreeMap<String, String>,
}

/// Complete compiled wire inventory, independent of availability, publication,
/// build identity and readiness. Those facts belong in the daemon envelope.
/// Schema 1 comparison is exact equality, never producer-superset matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiveFeedCompatibilityDescriptor {
    pub schema_version: u32,
    pub protocols: BTreeMap<String, WireFormatContract>,
    pub products: BTreeMap<String, LiveFeedProductCompatibility>,
}

fn format(schema_version: u32, encoding: impl Into<String>) -> WireFormatContract {
    WireFormatContract {
        schema_version,
        encoding: encoding.into(),
    }
}

fn formats<const N: usize>(
    entries: [(&str, WireFormatContract); N],
) -> BTreeMap<String, WireFormatContract> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

/// Shared by the client export and the actual producer executable. Every
/// registered product must have an explicit entry; adding a policy alone fails
/// the fixture-free roster test (and inventory construction).
pub fn live_feed_compatibility_descriptor() -> LiveFeedCompatibilityDescriptor {
    let mut products = BTreeMap::new();
    for policy in LIVE_FEED_PRODUCT_POLICIES {
        let mut parameters = BTreeMap::new();
        match policy.cache {
            LiveFeedCachePolicy::RecordJson {
                records_key,
                count_key,
            } => {
                parameters.insert("records_shape".into(), "object".into());
                parameters.insert("records_key".into(), records_key.into());
                if let Some(key) = count_key {
                    parameters.insert("count_key".into(), key.into());
                }
            }
            LiveFeedCachePolicy::RecordJsonArray {
                records_key,
                record_id_key,
                count_key,
            } => {
                parameters.insert("records_shape".into(), "array".into());
                parameters.insert("records_key".into(), records_key.into());
                parameters.insert("record_id_key".into(), record_id_key.into());
                if let Some(key) = count_key {
                    parameters.insert("count_key".into(), key.into());
                }
            }
            _ => {}
        }
        let mut product_formats = match policy.product_id {
            "metars" | "tafs" | "pireps" | "tfrs" => {
                let (schema, manifest_schema) = match policy.product_id {
                    "metars" => (
                        METAR_SNAPSHOT_SCHEMA_VERSION,
                        METAR_PRODUCT_CONTRACT_VERSION,
                    ),
                    "tafs" => (TAF_PRODUCT_CONTRACT_VERSION, TAF_PRODUCT_CONTRACT_VERSION),
                    "pireps" => (
                        PIREP_PRODUCT_CONTRACT_VERSION,
                        PIREP_PRODUCT_CONTRACT_VERSION,
                    ),
                    "tfrs" => (TFR_PRODUCT_CONTRACT_VERSION, TFR_MANIFEST_SCHEMA_VERSION),
                    _ => unreachable!(),
                };
                formats([
                    ("snapshot", format(schema, LIVE_FEED_JSON_XZ_ENCODING)),
                    (
                        "product_manifest",
                        format(manifest_schema, LIVE_FEED_JSON_ENCODING),
                    ),
                ])
            }
            "notams" => formats([
                (
                    "snapshot",
                    format(
                        LIVE_FEEDS_SCHEMA_VERSION,
                        LIVE_FEED_NOTAM_CHECKPOINT_ENCODING,
                    ),
                ),
                (
                    "records",
                    format(NOTAM_LIVE_FEED_CONTRACT_VERSION, "tagged-json"),
                ),
            ]),
            "obstacles" => formats([
                (
                    "snapshot",
                    format(OBSTACLE_MANIFEST_SCHEMA_VERSION, LIVE_FEED_NAV_KV_ENCODING),
                ),
                (
                    "layout",
                    format(OBSTACLE_STATE_LAYOUT_VERSION, "obstacle-navkv-json"),
                ),
                (
                    "tile",
                    format(OBSTACLE_TILE_SCHEMA_VERSION, LIVE_FEED_JSON_ENCODING),
                ),
                ("install", format(1, LIVE_FEED_NAV_KV_PACKAGE_ENCODING)),
            ]),
            ATMOSPHERE_PRODUCT_ID => {
                parameters.insert("array_order".into(), ATMOSPHERE_ARRAY_ORDER.into());
                parameters.insert(
                    "wind_units_per_mps".into(),
                    ATMOSPHERE_WIND_UNITS_PER_MPS.to_string(),
                );
                parameters.insert(
                    "temperature_units_per_c".into(),
                    ATMOSPHERE_TEMPERATURE_UNITS_PER_C.to_string(),
                );
                parameters.insert(
                    "height_units_per_m".into(),
                    ATMOSPHERE_HEIGHT_UNITS_PER_M.to_string(),
                );
                formats([
                    (
                        "snapshot",
                        format(
                            ATMOSPHERE_MANIFEST_SCHEMA_VERSION,
                            LIVE_FEED_NAV_KV_ENCODING,
                        ),
                    ),
                    (
                        "tile",
                        format(ATMOSPHERE_TILE_SCHEMA_VERSION, ATMOSPHERE_TILE_ENCODING),
                    ),
                    ("install", format(1, LIVE_FEED_NAV_KV_PACKAGE_ENCODING)),
                ])
            }
            "nexrad" => {
                parameters.insert(
                    "offline_profile_0".into(),
                    v3::NEXRAD_OFFLINE_PROFILE_0.into(),
                );
                parameters.insert(
                    "offline_profile_low1".into(),
                    v3::NEXRAD_OFFLINE_PROFILE_LOW1.into(),
                );
                formats([
                    (
                        "snapshot",
                        format(NEXRAD_MANIFEST_SCHEMA_VERSION, LIVE_FEED_JSON_ENCODING),
                    ),
                    ("tile", format(1, NEXRAD_TILE_ENCODING)),
                    ("overflow_tile", format(1, NEXRAD_OVERFLOW_ENCODING)),
                    ("install", format(1, LIVE_FEED_DIRECTORY_PACKAGE_ENCODING)),
                ])
            }
            product => panic!("live-feed product {product} has no compatibility contract"),
        };
        match policy.delta {
            LiveFeedDeltaPolicy::None => {
                parameters.insert("delta".into(), "none".into());
            }
            delta => {
                let encoding = match delta {
                    LiveFeedDeltaPolicy::RecordJson => LIVE_FEED_RECORD_DELTA_ENCODING,
                    LiveFeedDeltaPolicy::NavKv => LIVE_FEED_NAV_KV_DELTA_ENCODING,
                    LiveFeedDeltaPolicy::Notam => LIVE_FEED_NOTAM_DELTA_ENCODING,
                    LiveFeedDeltaPolicy::None => unreachable!(),
                };
                product_formats.insert("delta".into(), format(LIVE_FEEDS_SCHEMA_VERSION, encoding));
            }
        }
        products.insert(
            policy.product_id.into(),
            LiveFeedProductCompatibility {
                formats: product_formats,
                parameters,
            },
        );
    }
    LiveFeedCompatibilityDescriptor {
        schema_version: LIVE_FEED_COMPATIBILITY_SCHEMA_VERSION,
        protocols: formats([
            (
                "discovery",
                format(v3::SCHEMA_VERSION, LIVE_FEED_JSON_ENCODING),
            ),
            (
                "sse_catalog",
                format(v3::SCHEMA_VERSION, v3::CATALOG_EVENT_NAME),
            ),
            (
                "sse_current",
                format(v3::SCHEMA_VERSION, v3::PRODUCT_EVENT_NAME),
            ),
            (
                "sse_service_bulletins",
                format(
                    service_bulletins::BulletinHint::SCHEMA_VERSION,
                    service_bulletins::EVENT,
                ),
            ),
            (
                "version_manifest",
                format(v3::SCHEMA_VERSION, LIVE_FEED_JSON_ENCODING),
            ),
            ("sse_framing", format(1, "text/event-stream;utf-8")),
            (
                "navkv",
                format(
                    had_nav_kv::VERSION,
                    format!("had-nav-kv-v{}", had_nav_kv::VERSION),
                ),
            ),
            ("navkv_pages", format(1, "raw-or-xz-lzma2")),
            (
                "navkv_package",
                format(1, "zip-stored-manifest-root-xz-pages"),
            ),
            ("directory_package", format(1, "zip-stored-or-deflate")),
            ("json_compression", format(1, "xz-lzma2")),
            (
                "state_integrity",
                format(1, "sha256-canonical-json-or-navkv-or-notam-state"),
            ),
        ]),
        products,
    }
}

impl LiveFeedCompatibilityDescriptor {
    /// Validate complete schema-v1 topology, including nested maps. This does
    /// not silently fill absent formats from this executable's inventory.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != LIVE_FEED_COMPATIBILITY_SCHEMA_VERSION {
            return Err("unsupported live-feed compatibility descriptor schema".into());
        }
        let expected = live_feed_compatibility_descriptor();
        validate_formats("protocols", &self.protocols, &expected.protocols)?;
        if !self.products.keys().eq(expected.products.keys()) {
            return Err("live-feed compatibility product roster is incomplete or unknown".into());
        }
        for (product, descriptor) in &self.products {
            let expected = &expected.products[product];
            validate_formats(product, &descriptor.formats, &expected.formats)?;
            if !descriptor.parameters.keys().eq(expected.parameters.keys())
                || descriptor.parameters.values().any(|value| value.is_empty())
            {
                return Err(format!(
                    "{product} compatibility parameters are incomplete or unknown"
                ));
            }
        }
        Ok(())
    }

    /// Contract equality only. Callers must separately establish actual product
    /// availability, publication/projection readiness and process identity.
    pub fn require_exact_match(&self, producer: &Self) -> Result<(), String> {
        self.validate()?;
        producer.validate()?;
        if self != producer {
            return Err("live-feed wire contracts differ".into());
        }
        if self != &live_feed_compatibility_descriptor() {
            return Err("live-feed wire contracts are unknown to this executable".into());
        }
        Ok(())
    }
}

fn validate_formats(
    label: &str,
    actual: &BTreeMap<String, WireFormatContract>,
    expected: &BTreeMap<String, WireFormatContract>,
) -> Result<(), String> {
    if !actual.keys().eq(expected.keys())
        || actual
            .values()
            .any(|format| format.schema_version == 0 || format.encoding.is_empty())
    {
        return Err(format!(
            "{label} compatibility formats are malformed, incomplete or unknown"
        ));
    }
    Ok(())
}

/// Evidence equality is necessary, not sufficient, for sharing. Never interpret
/// this as permission to route without the daemon's separate readiness facts.
pub fn require_live_feed_compatibility(
    client: &LiveFeedCompatibilityDescriptor,
    required_catalog: &NotamCatalogIdentity,
    producer: &LiveFeedCompatibilityDescriptor,
    loaded_catalog: &NotamCatalogIdentity,
) -> Result<(), String> {
    client.require_exact_match(producer)?;
    required_catalog.validate()?;
    loaded_catalog.validate()?;
    if required_catalog != loaded_catalog {
        return Err("NOTAM airport catalogs differ".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nodes(value: &serde_json::Value) -> Vec<Vec<String>> {
        fn visit(value: &serde_json::Value, path: &mut Vec<String>, found: &mut Vec<Vec<String>>) {
            found.push(path.clone());
            if let Some(object) = value.as_object() {
                for (key, child) in object {
                    path.push(key.clone());
                    visit(child, path, found);
                    path.pop();
                }
            }
        }
        let mut found = Vec::new();
        visit(value, &mut Vec::new(), &mut found);
        found
    }

    fn node_mut<'a>(
        value: &'a mut serde_json::Value,
        path: &[String],
    ) -> &'a mut serde_json::Value {
        path.iter()
            .fold(value, |value, key| value.get_mut(key).unwrap())
    }

    fn assert_invalid(value: serde_json::Value, context: &str) {
        if let Ok(descriptor) = serde_json::from_value::<LiveFeedCompatibilityDescriptor>(value) {
            assert!(descriptor.validate().is_err(), "{context}");
            assert!(
                descriptor.require_exact_match(&descriptor).is_err(),
                "{context}"
            );
        }
    }

    #[test]
    fn every_nested_inventory_member_is_required() {
        let original = serde_json::to_value(live_feed_compatibility_descriptor()).unwrap();
        for path in nodes(&original).into_iter().filter(|path| !path.is_empty()) {
            let mut changed = original.clone();
            let (key, parent) = path.split_last().unwrap();
            node_mut(&mut changed, parent)
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert_invalid(changed, &format!("missing {path:?}"));
        }
    }

    #[test]
    fn every_nested_inventory_object_rejects_unknown_members() {
        let original = serde_json::to_value(live_feed_compatibility_descriptor()).unwrap();
        for path in nodes(&original) {
            let mut changed = original.clone();
            if let Some(object) = node_mut(&mut changed, &path).as_object_mut() {
                object.insert(
                    "unknown".into(),
                    serde_json::json!({"schema_version":1,"encoding":"json"}),
                );
                assert_invalid(changed, &format!("unknown member in {path:?}"));
            }
        }
    }

    #[test]
    fn every_schema_and_encoding_rejects_malformed_scalar_values() {
        let original = serde_json::to_value(live_feed_compatibility_descriptor()).unwrap();
        for path in nodes(&original) {
            let mut changed = original.clone();
            let node = node_mut(&mut changed, &path);
            let malformed = if node.is_number() {
                vec![
                    serde_json::json!(0),
                    serde_json::json!(-1),
                    serde_json::json!(1.5),
                    serde_json::json!(u32::MAX as u64 + 1),
                ]
            } else if node.is_string() {
                vec![serde_json::json!("")]
            } else {
                continue;
            };
            for replacement in malformed.into_iter().chain([
                serde_json::Value::Null,
                serde_json::json!(true),
                serde_json::json!([]),
            ]) {
                let mut changed = original.clone();
                *node_mut(&mut changed, &path) = replacement;
                assert_invalid(changed, &format!("malformed {path:?}"));
            }
        }
    }

    #[test]
    fn compatibility_inventory_matches_checked_in_export() {
        let expected: LiveFeedCompatibilityDescriptor =
            serde_json::from_str(include_str!("../contracts/live-feed-compatibility.json"))
                .unwrap();
        assert_eq!(live_feed_compatibility_descriptor(), expected);
    }

    #[test]
    fn every_registered_product_and_delta_codec_is_represented() {
        let inventory = live_feed_compatibility_descriptor();
        inventory.validate().unwrap();
        assert_eq!(inventory.products.len(), LIVE_FEED_PRODUCT_POLICIES.len());
        for policy in LIVE_FEED_PRODUCT_POLICIES {
            let formats = &inventory.products[policy.product_id].formats;
            assert!(formats.contains_key("snapshot"));
            assert_eq!(
                formats.contains_key("delta"),
                policy.delta != LiveFeedDeltaPolicy::None
            );
        }
        assert_eq!(
            inventory.protocols["navkv"].schema_version,
            had_nav_kv::VERSION
        );
        assert_eq!(
            inventory.products["notams"].formats["records"].schema_version,
            NOTAM_LIVE_FEED_CONTRACT_VERSION
        );
        assert_eq!(
            inventory.products[ATMOSPHERE_PRODUCT_ID].formats["tile"].encoding,
            ATMOSPHERE_TILE_ENCODING
        );
    }

    #[test]
    fn bulletin_hint_is_a_versioned_protocol_not_a_required_weather_product() {
        let inventory = live_feed_compatibility_descriptor();
        assert_eq!(
            inventory.protocols["sse_service_bulletins"],
            format(
                service_bulletins::BulletinHint::SCHEMA_VERSION,
                service_bulletins::EVENT,
            )
        );
        assert_eq!(inventory.products.len(), LIVE_FEED_PRODUCT_POLICIES.len());
        assert!(!inventory.products.contains_key(service_bulletins::EVENT));
        assert!(!inventory.products.contains_key("service-bulletins"));
    }

    #[test]
    fn bulletin_hint_v1_keeps_the_shared_producer_and_decoder_shape() {
        use service_bulletins::BulletinHint;

        assert_eq!(BulletinHint::SCHEMA_VERSION, 1);
        let hint = BulletinHint {
            publisher: "https://example.invalid/service/bulletins-v1.json".into(),
            revision: u64::MAX,
        };
        // The daemon serializes Some(hint); the session decodes BulletinHint.
        let encoded = serde_json::to_value(Some(&hint)).unwrap();
        assert_eq!(
            encoded,
            serde_json::json!({"publisher": hint.publisher, "revision": u64::MAX})
        );
        assert_eq!(
            serde_json::from_value::<BulletinHint>(encoded.clone()).unwrap(),
            hint
        );
        for invalid in [
            serde_json::json!({"publisher": hint.publisher}),
            serde_json::json!({"revision": 1}),
            serde_json::json!({"publisher": hint.publisher, "revision": -1}),
            serde_json::json!({"publisher": hint.publisher, "revision": "1"}),
            serde_json::json!({"schema_version": 1, "publisher": hint.publisher, "revision": 1}),
            serde_json::Value::Null,
        ] {
            assert!(serde_json::from_value::<BulletinHint>(invalid).is_err());
        }
    }

    #[test]
    fn bulletin_hint_inventory_omission_or_mismatch_denies_sharing() {
        let client = live_feed_compatibility_descriptor();
        let mut producer = client.clone();
        producer.protocols.remove("sse_service_bulletins");
        assert!(client.require_exact_match(&producer).is_err());
        assert!(producer.require_exact_match(&producer).is_err());
        for replacement in [
            format(
                service_bulletins::BulletinHint::SCHEMA_VERSION + 1,
                service_bulletins::EVENT,
            ),
            format(
                service_bulletins::BulletinHint::SCHEMA_VERSION,
                "different-bulletin-event",
            ),
        ] {
            let mut producer = client.clone();
            producer
                .protocols
                .insert("sse_service_bulletins".into(), replacement);
            assert!(client.require_exact_match(&producer).is_err());
        }
    }

    #[test]
    fn every_schema_encoding_and_parameter_independently_denies_sharing() {
        let original = serde_json::to_value(live_feed_compatibility_descriptor()).unwrap();
        fn mutate_each(value: &serde_json::Value) -> Vec<serde_json::Value> {
            match value {
                serde_json::Value::Object(map) => map
                    .iter()
                    .flat_map(|(key, child)| {
                        mutate_each(child)
                            .into_iter()
                            .map(|changed| {
                                let mut map = map.clone();
                                map.insert(key.clone(), changed);
                                serde_json::Value::Object(map)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect(),
                serde_json::Value::Number(number) => {
                    vec![serde_json::json!(number.as_u64().unwrap() + 1)]
                }
                serde_json::Value::String(string) => {
                    vec![serde_json::json!(format!("{string}-changed"))]
                }
                other => panic!("unexpected descriptor value {other}"),
            }
        }
        let client = live_feed_compatibility_descriptor();
        client.require_exact_match(&client).unwrap();
        for mutated in mutate_each(&original) {
            let producer = serde_json::from_value(mutated).unwrap();
            assert!(client.require_exact_match(&producer).is_err());
            assert!(producer.require_exact_match(&producer).is_err());
        }
    }

    #[test]
    fn absent_unknown_and_malformed_inventory_never_match_even_themselves() {
        let original = serde_json::to_value(live_feed_compatibility_descriptor()).unwrap();
        for pointer in [
            "",
            "/protocols",
            "/products",
            "/products/notams",
            "/products/notams/formats",
            "/products/metars/parameters",
        ] {
            let map = original.pointer(pointer).unwrap().as_object().unwrap();
            for key in map.keys() {
                let mut changed = original.clone();
                changed
                    .pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
                if let Ok(descriptor) =
                    serde_json::from_value::<LiveFeedCompatibilityDescriptor>(changed)
                {
                    assert!(
                        descriptor.require_exact_match(&descriptor).is_err(),
                        "{pointer}/{key}"
                    );
                }
            }
            let mut changed = original.clone();
            changed
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".into(), serde_json::json!({}));
            if let Ok(descriptor) =
                serde_json::from_value::<LiveFeedCompatibilityDescriptor>(changed)
            {
                assert!(descriptor.validate().is_err());
            }
        }
        for malformed in [
            serde_json::Value::Null,
            serde_json::json!({}),
            serde_json::json!({"schema_version":1}),
        ] {
            assert!(serde_json::from_value::<LiveFeedCompatibilityDescriptor>(malformed).is_err());
        }
    }

    #[test]
    fn equal_count_catalog_substitution_denies_sharing() {
        let identity = |id: &str| {
            NotamAirportCatalog {
                aliases: Default::default(),
                schema_version: NotamAirportCatalog::SCHEMA_VERSION,
                airport_ids: [id.to_string()].into(),
            }
            .identity()
            .unwrap()
        };
        let inventory = live_feed_compatibility_descriptor();
        assert!(require_live_feed_compatibility(
            &inventory,
            &identity("KSFO"),
            &inventory,
            &identity("KSFO")
        )
        .is_ok());
        assert!(require_live_feed_compatibility(
            &inventory,
            &identity("KSFO"),
            &inventory,
            &identity("KLAX")
        )
        .is_err());
    }
}
