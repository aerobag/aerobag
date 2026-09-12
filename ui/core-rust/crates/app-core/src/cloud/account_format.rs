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
    pub migrate_records: fn(&mut BTreeMap<String, CloudRecord>) -> AppResult<()>,
}

pub(super) static CURRENT: AccountFormat = AccountFormat {
    version: CLOUD_NODE_VERSION,
    predecessor: None,
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
    migrate_records: |_| Err(cloud_error("Account format 1 has no predecessor")),
};

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
        prior.migrate(from, records)?;
        let stamps = records
            .iter()
            .map(|(key, record)| (key.clone(), record.modified_at_epoch_ms))
            .collect::<BTreeMap<_, _>>();
        (self.migrate_records)(records)?;
        if stamps.iter().any(|(key, stamp)| {
            records
                .get(key)
                .is_none_or(|record| record.modified_at_epoch_ms != *stamp)
        }) {
            return Err(cloud_error(
                "Account migration lost a record or changed its mutation timestamp",
            ));
        }
        Ok(())
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
