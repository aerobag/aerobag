// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 3;
pub const NEXRAD_OFFLINE_PROFILE_0: &str = "offline_0";
pub const NEXRAD_OFFLINE_PROFILE_LOW1: &str = "offline_low1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentManifest {
    pub schema_version: u32,
    pub generated_at_utc: String,
    pub products: BTreeMap<String, CurrentProduct>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentProduct {
    pub current: String,
    pub version_manifest_url: String,
    pub state_url: String,
    pub state_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collected_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<CurrentHistoryEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentHistoryEntry {
    pub version: String,
    pub version_manifest_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionManifest {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    /// Small, product-neutral timing summary that can be inspected before
    /// downloading the state or install payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporal_coverage: Option<TemporalCoverage>,
    pub state: PayloadRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_state: Option<PayloadRef>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub install_profiles: BTreeMap<String, PayloadRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_from_previous: Option<DeltaRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_deltas: Vec<DeltaRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemporalCoverage {
    /// Product-specific reference time used to describe the age of this data
    /// (for example, the forecast model cycle time).
    pub reference_time_epoch_ms: i64,
    pub valid_from_epoch_ms: i64,
    pub valid_through_epoch_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub url: String,
    pub bytes: u64,
    pub blob_sha256: String,
    pub state_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeltaRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub from_version: String,
    pub from_state_sha256: String,
    pub to_version: String,
    pub to_state_sha256: String,
    pub url: String,
    pub bytes: u64,
    pub blob_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutation_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordDelta {
    pub schema_version: u32,
    pub product: String,
    pub from_version: String,
    pub to_version: String,
    pub top_level_changed: BTreeMap<String, serde_json::Value>,
    pub top_level_removed: Vec<String>,
    pub changed: BTreeMap<String, serde_json::Value>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavKvDelta {
    pub schema_version: u32,
    pub product: String,
    pub from_version: String,
    pub to_version: String,
    pub from_state_sha256: String,
    pub to_state_sha256: String,
    pub entries: Vec<NavKvDeltaEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavKvDeltaEntry {
    pub key: String,
    pub value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentEvent {
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    pub version_manifest_url: String,
    pub state_url: String,
    pub state_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collected_at_utc: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<CurrentHistoryEntry>,
}

#[cfg(test)]
mod tests {
    use crate::versioned_json::decode_exact;

    use super::*;

    #[test]
    fn current_manifest_requires_v3_integrity_fields() {
        let error = decode_exact::<CurrentManifest>(
            "live-feed current manifest",
            br#"{
                "schema_version":3,
                "generated_at_utc":"2026-08-04T00:00:00Z",
                "products":{
                    "metars":{
                        "current":"v1",
                        "version_manifest_url":"versions/metars/v1.json",
                        "state_url":"states/metars/v1.json"
                    }
                }
            }"#,
            SCHEMA_VERSION,
        )
        .unwrap_err();
        assert!(error.to_string().contains("state_sha256"), "{error}");
    }
}
