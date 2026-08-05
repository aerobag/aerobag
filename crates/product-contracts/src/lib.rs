// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

mod aerobag_cloud;
mod atmosphere;
pub mod live_feeds;
pub mod publication;
pub mod versioned_json;

pub use aerobag_cloud::*;
pub use atmosphere::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductContract {
    pub family_id: &'static str,
    pub contract_id: &'static str,
}

pub const WAYPOINT_SEARCH_MAX_RESULTS: usize = 100;
pub const CHART_PACKAGE_TIER_METADATA_KEY: &str = "chart_package_tier";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartPackageTier {
    Wide,
    Regional,
    Detail,
}

impl ChartPackageTier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wide => "wide",
            Self::Regional => "regional",
            Self::Detail => "detail",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaypointSearchMatchKind {
    Identifier,
    AirportName,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaypointSearchRecord {
    pub identifier: String,
    pub kind: String,
    pub display_name: String,
    pub lat: f64,
    pub lon: f64,
    pub matched_term: String,
    pub match_kind: WaypointSearchMatchKind,
}

/// Semantic effects extracted from airport-associated NOTAMs at ingestion time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AirportNotamEffect {
    AirportClosed,
    RunwayClosed,
    ProcedureUnavailable,
    RunwayRestricted,
    RunwayEquipmentUnavailable,
    TaxiwayClosed,
    ApronClosed,
    ProcedureRestricted,
    MovementAreaEquipmentUnavailable,
    SurfaceCondition,
    WorkInProgress,
    RoutineAdvisory,
    Other,
}

pub const NAV_DB_CONTRACT_ID: &str = "NAV18";
pub const SEC_CONTRACT_ID: &str = "SEC1";
pub const TAC_CONTRACT_ID: &str = "TAC1";
pub const ENR_L_CONTRACT_ID: &str = "ENL1";
pub const ENR_H_CONTRACT_ID: &str = "ENH1";
pub const TPP_CONTRACT_ID: &str = "TPP1";
pub const CSUP_CONTRACT_ID: &str = "CSUP1";
pub const TERRAIN_CONTRACT_ID: &str = "TER2";
pub const TERRAIN_TER2_MAX_ZOOM: u32 = 9;
pub const TERRAIN_TER2_HEIGHT_QUANTIZATION_FT: i16 = 64;
pub const SHADED_RELIEF_CONTRACT_ID: &str = "SHD1";
pub const WORLD_BASEMAP_CONTRACT_ID: &str = "WBM1";
pub const GEO_CONTRACT_ID: &str = "GEO1";
pub const LIVE_FEEDS_SCHEMA_VERSION: u32 = live_feeds::v3::SCHEMA_VERSION;
pub const NOTAM_LIVE_FEED_CONTRACT_VERSION: u32 = 3;

/// Transport timing shared by every Aerobag SSE producer and consumer.
///
/// Platforms execute this policy; they do not define their own timing values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SseTransportPolicy {
    pub heartbeat_interval_ms: i64,
    pub connect_timeout_ms: i64,
    pub idle_timeout_ms: i64,
    pub reconnect_initial_delay_ms: i64,
    pub reconnect_max_delay_ms: i64,
}

impl SseTransportPolicy {
    pub fn reconnect_delay_ms(self, consecutive_errors: u32) -> i64 {
        let mut delay_ms = self.reconnect_initial_delay_ms;
        for _ in 1..consecutive_errors {
            delay_ms = delay_ms.saturating_mul(2).min(self.reconnect_max_delay_ms);
        }
        delay_ms
    }
}

pub const AEROBAG_SSE_TRANSPORT_POLICY: SseTransportPolicy = SseTransportPolicy {
    heartbeat_interval_ms: 30_000,
    connect_timeout_ms: 5_000,
    idle_timeout_ms: 65_000,
    reconnect_initial_delay_ms: 5_000,
    reconnect_max_delay_ms: 65_000,
};

pub const PRODUCT_CONTRACTS: &[ProductContract] = &[
    ProductContract {
        family_id: "nav-db",
        contract_id: NAV_DB_CONTRACT_ID,
    },
    ProductContract {
        family_id: "sec",
        contract_id: SEC_CONTRACT_ID,
    },
    ProductContract {
        family_id: "tac",
        contract_id: TAC_CONTRACT_ID,
    },
    ProductContract {
        family_id: "enr-l",
        contract_id: ENR_L_CONTRACT_ID,
    },
    ProductContract {
        family_id: "enr-h",
        contract_id: ENR_H_CONTRACT_ID,
    },
    ProductContract {
        family_id: "tpp",
        contract_id: TPP_CONTRACT_ID,
    },
    ProductContract {
        family_id: "csup",
        contract_id: CSUP_CONTRACT_ID,
    },
    ProductContract {
        family_id: "terrain",
        contract_id: TERRAIN_CONTRACT_ID,
    },
    ProductContract {
        family_id: "shaded-relief",
        contract_id: SHADED_RELIEF_CONTRACT_ID,
    },
    ProductContract {
        family_id: "world-basemap",
        contract_id: WORLD_BASEMAP_CONTRACT_ID,
    },
    ProductContract {
        family_id: "geo",
        contract_id: GEO_CONTRACT_ID,
    },
];

pub fn contract_id_for_family(family_id: &str) -> Option<&'static str> {
    PRODUCT_CONTRACTS
        .iter()
        .find(|contract| contract.family_id == family_id)
        .map(|contract| contract.contract_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_lookup_returns_declared_contracts() {
        for contract in PRODUCT_CONTRACTS {
            assert_eq!(
                contract_id_for_family(contract.family_id),
                Some(contract.contract_id)
            );
        }
        assert_eq!(contract_id_for_family("missing"), None);
    }

    #[test]
    fn shared_sse_policy_preserves_the_existing_backoff_sequence() {
        let delays = (1..=6)
            .map(|failure| AEROBAG_SSE_TRANSPORT_POLICY.reconnect_delay_ms(failure))
            .collect::<Vec<_>>();
        assert_eq!(delays, vec![5_000, 10_000, 20_000, 40_000, 65_000, 65_000]);
        assert!(
            AEROBAG_SSE_TRANSPORT_POLICY.idle_timeout_ms
                > 2 * AEROBAG_SSE_TRANSPORT_POLICY.heartbeat_interval_ms
        );
    }
}
