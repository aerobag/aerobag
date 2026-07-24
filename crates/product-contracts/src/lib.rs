// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductContract {
    pub family_id: &'static str,
    pub contract_id: &'static str,
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

pub const NAV_DB_CONTRACT_ID: &str = "NAV12";
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
pub const LIVE_FEEDS_SCHEMA_VERSION: u32 = 3;
pub const NOTAM_LIVE_FEED_CONTRACT_VERSION: u32 = 3;

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
}
