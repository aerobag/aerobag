// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::wire_enum;
use crate::settings_controller as runtime;
use serde::{Deserialize, Serialize};

wire_enum! {
    #[serde(rename_all = "snake_case")]
    PackageSelection => crate::OfflinePackageSelection { Unselected, Pause, Play }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) enum SleepTimeout {
    #[serde(rename = "30m")]
    ThirtyMinutes,
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "2h")]
    TwoHours,
    #[serde(rename = "4h")]
    FourHours,
    #[serde(rename = "never")]
    Never,
}
impl From<runtime::InactivitySleepTimeout> for SleepTimeout {
    fn from(value: runtime::InactivitySleepTimeout) -> Self {
        use runtime::InactivitySleepTimeout as R;
        match value {
            R::ThirtyMinutes => Self::ThirtyMinutes,
            R::OneHour => Self::OneHour,
            R::TwoHours => Self::TwoHours,
            R::FourHours => Self::FourHours,
            R::Never => Self::Never,
        }
    }
}
impl From<SleepTimeout> for runtime::InactivitySleepTimeout {
    fn from(value: SleepTimeout) -> Self {
        match value {
            SleepTimeout::ThirtyMinutes => Self::ThirtyMinutes,
            SleepTimeout::OneHour => Self::OneHour,
            SleepTimeout::TwoHours => Self::TwoHours,
            SleepTimeout::FourHours => Self::FourHours,
            SleepTimeout::Never => Self::Never,
        }
    }
}

wire_enum! {
    #[serde(rename_all = "snake_case")]
    Coverage => runtime::NexradCoverageMode { ViewportOnly, FullOffline }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(crate) enum Detail {
    #[serde(rename = "offline_0")]
    Offline0,
    #[serde(rename = "offline_low1")]
    OfflineLow1,
}
impl From<runtime::NexradOfflineProfile> for Detail {
    fn from(value: runtime::NexradOfflineProfile) -> Self {
        match value {
            runtime::NexradOfflineProfile::Offline0 => Self::Offline0,
            runtime::NexradOfflineProfile::OfflineLow1 => Self::OfflineLow1,
        }
    }
}
impl From<Detail> for runtime::NexradOfflineProfile {
    fn from(value: Detail) -> Self {
        match value {
            Detail::Offline0 => Self::Offline0,
            Detail::OfflineLow1 => Self::OfflineLow1,
        }
    }
}
wire_enum! {
    #[serde(rename_all = "snake_case")]
    Cadence => runtime::NexradUpdateCadence { Never, ThirtyMinutes, TenMinutes, EveryUpdate }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct NexradPreferences {
    pub coverage: Coverage,
    pub offline_profile: Detail,
    pub shown_cadence: Cadence,
    pub hidden_cadence: Cadence,
    pub asleep_cadence: Cadence,
}
impl From<runtime::NexradAcquisitionPreferences> for NexradPreferences {
    fn from(value: runtime::NexradAcquisitionPreferences) -> Self {
        let runtime::NexradAcquisitionPreferences {
            coverage,
            offline_profile,
            shown_cadence,
            hidden_cadence,
            asleep_cadence,
        } = value;
        Self {
            coverage: coverage.into(),
            offline_profile: offline_profile.into(),
            shown_cadence: shown_cadence.into(),
            hidden_cadence: hidden_cadence.into(),
            asleep_cadence: asleep_cadence.into(),
        }
    }
}
impl From<NexradPreferences> for runtime::NexradAcquisitionPreferences {
    fn from(value: NexradPreferences) -> Self {
        let NexradPreferences {
            coverage,
            offline_profile,
            shown_cadence,
            hidden_cadence,
            asleep_cadence,
        } = value;
        Self {
            coverage: coverage.into(),
            offline_profile: offline_profile.into(),
            shown_cadence: shown_cadence.into(),
            hidden_cadence: hidden_cadence.into(),
            asleep_cadence: asleep_cadence.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct AircraftMembership {
    pub included: bool,
}
impl From<product_contracts::AircraftLibraryMembership> for AircraftMembership {
    fn from(value: product_contracts::AircraftLibraryMembership) -> Self {
        let product_contracts::AircraftLibraryMembership { included } = value;
        Self { included }
    }
}
impl From<AircraftMembership> for product_contracts::AircraftLibraryMembership {
    fn from(value: AircraftMembership) -> Self {
        Self {
            included: value.included,
        }
    }
}
