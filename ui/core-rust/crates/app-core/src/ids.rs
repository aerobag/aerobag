use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
pub enum ChartFamilyId {
    #[serde(rename = "sec")]
    Sectional,
    #[serde(rename = "tac")]
    Tac,
    #[serde(rename = "wac")]
    Wac,
    #[serde(rename = "enr-l")]
    IfrLow,
    #[serde(rename = "enr-h")]
    IfrHigh,
    #[serde(rename = "enr-a")]
    IfrArea,
    #[serde(rename = "flyway")]
    Flyway,
    #[serde(rename = "heli")]
    Heli,
    #[serde(rename = "misc")]
    Misc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum RegionId {
    Ne,
    Nc,
    Nw,
    Se,
    Sc,
    Sw,
    Ec,
    Ak,
    Pac,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AirportId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChartId {
    pub family: ChartFamilyId,
    pub name: String,
    pub cycle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PlateId {
    pub airport_id: AirportId,
    pub procedure_code: String,
    pub page: u16,
    pub cycle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
pub struct PackageId {
    pub region: RegionId,
    pub family: ChartFamilyId,
    pub cycle: String,
}

impl PackageId {
    pub fn package_name(&self) -> String {
        let region = match self.region {
            RegionId::Ne => "NE",
            RegionId::Nc => "NC",
            RegionId::Nw => "NW",
            RegionId::Se => "SE",
            RegionId::Sc => "SC",
            RegionId::Sw => "SW",
            RegionId::Ec => "EC",
            RegionId::Ak => "AK",
            RegionId::Pac => "PAC",
        };

        let family = match self.family {
            ChartFamilyId::Sectional => "SEC",
            ChartFamilyId::Tac => "TAC",
            ChartFamilyId::Wac => "WAC",
            ChartFamilyId::IfrLow => "ENR_L",
            ChartFamilyId::IfrHigh => "ENR_H",
            ChartFamilyId::IfrArea => "ENR_A",
            ChartFamilyId::Flyway => "TAC",
            ChartFamilyId::Heli => "HEL",
            ChartFamilyId::Misc => "MISC",
        };

        format!("{region}_{family}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_name_matches_sec_contract() {
        let id = PackageId {
            region: RegionId::Ne,
            family: ChartFamilyId::Sectional,
            cycle: "2026-04-16".to_string(),
        };

        assert_eq!(id.package_name(), "NE_SEC");
    }

    #[test]
    fn package_name_matches_enr_l_contract() {
        let id = PackageId {
            region: RegionId::Pac,
            family: ChartFamilyId::IfrLow,
            cycle: "2026-04-16".to_string(),
        };

        assert_eq!(id.package_name(), "PAC_ENR_L");
    }

    #[test]
    fn flyway_package_name_uses_shared_tac_package() {
        let id = PackageId {
            region: RegionId::Sw,
            family: ChartFamilyId::Flyway,
            cycle: "2026-04-16".to_string(),
        };

        assert_eq!(id.package_name(), "SW_TAC");
    }
}
