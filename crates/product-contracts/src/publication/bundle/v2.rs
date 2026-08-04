// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub schema_version: u32,
    pub bundle_id: String,
    pub bundle_type: String,
    pub cycle: String,
    pub cycle_version: String,
    pub generated_at_utc: String,
    pub effective_date: String,
    pub expiration_date: String,
    pub start_valid: String,
    pub end_valid: String,
    pub packages: Vec<BundlePackageArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ancillary: Vec<BundleArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleArtifact {
    pub filename: String,
    pub relative_path: String,
    pub checksum_sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundlePackageArtifact {
    pub id: String,
    pub family_id: String,
    pub contract_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    pub filename: String,
    pub relative_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_version: Option<String>,
    pub checksum_sha256: String,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_generated_at_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_fetched_at_utc: Option<String>,
    pub effective_date: Option<String>,
    pub expiration_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning_text: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_manifest_requires_v2_integrity_fields() {
        let error = serde_json::from_value::<BundleManifest>(serde_json::json!({
            "schema_version": 2,
            "bundle_id": "cycle-2608",
            "bundle_type": "cycle",
            "cycle": "2608",
            "cycle_version": "01",
            "generated_at_utc": "2026-08-04T00:00:00Z",
            "effective_date": "2026-08-04",
            "expiration_date": "2026-09-01",
            "start_valid": "2026-08-04",
            "end_valid": "2026-09-01",
            "packages": [{
                "id": "NAV_DB_NAV16_2608_01",
                "family_id": "nav-db",
                "contract_id": "NAV16",
                "filename": "nav-db.zip",
                "relative_path": "nav-db.zip",
                "checksum_sha256": "abc",
                "effective_date": "2026-08-04",
                "expiration_date": "2026-09-01"
            }]
        }))
        .unwrap_err();
        assert!(error.to_string().contains("size_bytes"), "{error}");
    }
}
