// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Stable client-facing service identity, independent of builds and publications.
use serde::{Deserialize, Serialize};

use crate::{ClientDataContractInventory, LiveFeedCompatibilityDescriptor};

pub const SERVICE_CONTRACT_ID: &str = "C1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceContract {
    pub schema_version: u32,
    pub contract: String,
    pub client_data: ClientDataContractInventory,
    pub live_feeds: LiveFeedCompatibilityDescriptor,
    pub behavior: Vec<String>,
}

pub fn service_contract() -> ServiceContract {
    ServiceContract {
        schema_version: 1,
        contract: SERVICE_CONTRACT_ID.into(),
        client_data: crate::client_data_contract_inventory(),
        live_feeds: crate::live_feed_compatibility_descriptor(),
        behavior: [
            "immutable-resource-identities-v1",
            "atomic-package-discovery-v1",
            "catalog-snapshot-delta-recovery-v1",
            "source-error-is-not-empty-data-v1",
            "utc-observation-and-validity-times-v1",
            "publication-bound-notam-projection-v1",
            "contract-support-bulletins-v2",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_service_matches_immutable_contract() {
        let frozen: ServiceContract =
            serde_json::from_str(include_str!("../contracts/service/C1.json")).unwrap();
        assert_eq!(
            service_contract(),
            frozen,
            "client/service contract changed: publish a new contract name"
        );
    }
}
