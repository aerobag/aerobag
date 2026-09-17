// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

#[cfg(any(test, feature = "cloud-format-test"))]
pub(super) mod test_format;

/// One descriptor owns the complete encrypted account contract. Register a
/// successor here when persisted meaning changes, not when app structs move.
#[derive(Debug)]
pub(super) struct AccountFormat {
    pub version: u32,
    pub predecessor: Option<&'static AccountFormat>,
    pub decode_node: fn(serde_json::Value) -> AppResult<CloudNode>,
    pub decode_page: fn(serde_json::Value) -> AppResult<CloudPage>,
    pub encode_page: fn(&CloudPage) -> AppResult<serde_json::Value>,
    pub migrations: &'static [RecordMigration],
}

#[derive(Debug)]
pub(super) struct RecordMigration {
    pub key: KeyPattern,
    pub source_versions: &'static [u32],
    pub target_version: Option<u32>,
    pub convert: fn(CloudRecord) -> AppResult<Option<CloudRecord>>,
    pub explanation: &'static str,
}

// Historical format 1 was unfortunately published with FP schemas 1, 2, and
// 3. Read its structural envelope only; the approved successor discards that
// crossfill without attempting to deserialize any historical FlightPlan.
pub(super) static LEGACY: AccountFormat = AccountFormat {
    version: 1,
    predecessor: None,
    decode_node: |value| serde_json::from_value(value).map_err(cloud_json_error),
    decode_page: |value| {
        let page: CloudPage = serde_json::from_value(value).map_err(cloud_json_error)?;
        if page.version != 1 {
            return Err(cloud_error("Expected account format 1 page"));
        }
        Ok(page)
    },
    encode_page: |_| Err(cloud_error("Historical account format 1 is read-only")),
    migrations: &[],
};

pub(super) static CURRENT: AccountFormat = AccountFormat {
    version: CLOUD_NODE_VERSION,
    predecessor: Some(&LEGACY),
    decode_node: |value| serde_json::from_value(value).map_err(cloud_json_error),
    decode_page: |value| {
        let page = serde_json::from_value(value).map_err(cloud_json_error)?;
        validate_cloud_page(&page)?;
        Ok(page)
    },
    encode_page: |page| {
        validate_cloud_page(page)?;
        serde_json::to_value(page).map_err(cloud_json_error)
    },
    migrations: &[RecordMigration {
        key: KeyPattern::Exact(FLIGHT_PLAN_RECORD_KEY),
        source_versions: &[1, 2, 3],
        target_version: None,
        convert: |_| Ok(None),
        explanation: "This upgrade removes the previously shared flight plan. Other synchronized data is retained. It does not clear the flight plan currently open on this device.",
    }, RecordMigration {
        key: KeyPattern::Prefix(AIRCRAFT_LIBRARY_RECORD_PREFIX),
        source_versions: &[1],
        target_version: Some(2),
        convert: migrate_legacy_membership,
        explanation: "",
    }],
};

fn migrate_legacy_membership(record: CloudRecord) -> AppResult<Option<CloudRecord>> {
    // Freeze the historical reader here, not in the current wire or runtime
    // model. The retired tombstone field did not affect the included choice.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LegacyMembership {
        included: bool,
        #[serde(default, rename = "deleted")]
        _deleted: bool,
    }
    let value: LegacyMembership =
        serde_json::from_value(record.value().clone()).map_err(cloud_json_error)?;
    CloudRecord::encode::<AircraftMembershipRecord>(
        &wire::preferences::AircraftMembership {
            included: value.included,
        },
        record.modified_at_epoch_ms(),
    )
    .map(Some)
}

impl AccountFormat {
    pub fn find(&'static self, version: u32) -> Option<&'static Self> {
        if version == self.version {
            Some(self)
        } else {
            self.predecessor
                .filter(|prior| prior.version < self.version)
                .and_then(|prior| prior.find(version))
        }
    }

    pub fn can_upgrade(&'static self, from: u32) -> bool {
        from < self.version && self.find(from).is_some()
    }

    pub fn migrate(
        &'static self,
        from: u32,
        records: &mut BTreeMap<String, CloudRecord>,
    ) -> AppResult<()> {
        if from == self.version {
            return Ok(());
        }
        let prior = self
            .predecessor
            .filter(|prior| prior.version < self.version)
            .ok_or_else(|| {
                cloud_error(format!(
                    "No account migration from format {from} to {}",
                    self.version
                ))
            })?;
        let mut migrated = records.clone();
        prior.migrate(from, &mut migrated)?;
        let prior_records = migrated.clone();
        for (key, original) in &prior_records {
            let rules = self
                .migrations
                .iter()
                .filter(|rule| rule.key.matches(key))
                .collect::<Vec<_>>();
            if rules.len() > 1 {
                return Err(cloud_error("Overlapping account migration rules"));
            }
            let Some(rule) = rules.first() else {
                continue;
            };
            if !rule.source_versions.contains(&original.schema_version()) {
                return Err(cloud_error(format!(
                    "No migration for {key} schema {}",
                    original.schema_version()
                )));
            }
            match (rule.convert)(original.clone())? {
                Some(record) => {
                    if rule.target_version != Some(record.schema_version())
                        || record.modified_at_epoch_ms() != original.modified_at_epoch_ms()
                    {
                        return Err(cloud_error(
                            "Account migration changed its declared schema or mutation timestamp",
                        ));
                    }
                    migrated.insert(key.clone(), record);
                }
                None => {
                    if rule.target_version.is_some() {
                        return Err(cloud_error(
                            "Account migration unexpectedly discarded a record",
                        ));
                    }
                    migrated.remove(key);
                }
            }
        }
        *records = migrated;
        Ok(())
    }

    pub fn upgrade_explanation(&'static self, from: u32) -> String {
        if from == self.version {
            return String::new();
        }
        let mut parts = self
            .predecessor
            .map(|prior| prior.upgrade_explanation(from))
            .unwrap_or_default();
        for rule in self.migrations {
            if !rule.explanation.is_empty() {
                if !parts.is_empty() {
                    parts.push(' ');
                }
                parts.push_str(rule.explanation);
            }
        }
        parts
    }
}

/// This header must remain readable independently of all future root bodies.
pub(super) fn read_version(value: &serde_json::Value) -> AppResult<u32> {
    #[derive(Deserialize)]
    struct Header {
        version: u32,
    }
    let header: Header = serde_json::from_value(value.clone()).map_err(cloud_json_error)?;
    if header.version == 0 {
        return Err(cloud_error("Invalid cloud account format 0"));
    }
    Ok(header.version)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ObservedRoot {
    pub version: u32,
    pub revision: u64,
    pub hash: String,
    pub records_verified: bool,
}

impl ObservedRoot {
    pub fn matches(&self, revision: u64, hash: Option<&str>) -> bool {
        self.revision == revision && Some(self.hash.as_str()) == hash
    }
}
