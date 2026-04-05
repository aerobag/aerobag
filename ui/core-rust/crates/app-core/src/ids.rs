use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum ChartFamilyId {
    Sectional,
    Tac,
    Wac,
    IfrLow,
    IfrHigh,
    IfrArea,
    Flyway,
    Heli,
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
            ChartFamilyId::Flyway => "FLY",
            ChartFamilyId::Heli => "HEL",
            ChartFamilyId::Misc => "MISC",
        };

        format!("{region}_{family}")
    }
}
