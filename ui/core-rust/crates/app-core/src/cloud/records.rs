// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The only construction boundary for application cloud records. The registry
//! drives codecs, key matching, validation, and the frozen contract inventory.

use super::{cloud_error, cloud_json_error, wire, AppResult};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
pub(super) struct CloudRecord {
    schema_version: u32,
    #[serde(default)]
    modified_at_epoch_ms: Option<i64>,
    value: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KeyPattern {
    Exact(&'static str),
    Prefix(&'static str),
    Prefixes(&'static [&'static str]),
}
impl KeyPattern {
    pub fn matches(self, key: &str) -> bool {
        match self {
            Self::Exact(v) => v == key,
            Self::Prefix(v) => key.starts_with(v),
            Self::Prefixes(values) => values.iter().any(|v| key.starts_with(v)),
        }
    }
    #[cfg(test)]
    pub(super) fn description(self) -> serde_json::Value {
        match self {
            Self::Exact(v) => serde_json::json!({"exact": v}),
            Self::Prefix(v) => serde_json::json!({"prefix": v}),
            Self::Prefixes(v) => serde_json::json!({"prefixes": v}),
        }
    }
}

mod sealed {
    pub trait Sealed {}
}
pub(super) trait RecordSpec: sealed::Sealed {
    type Wire: Serialize + DeserializeOwned;
    const KEY: KeyPattern;
    const VERSION: u32;
    const MUTABLE: bool;
    fn validate(value: &Self::Wire) -> AppResult<()>;
}

impl CloudRecord {
    pub fn encode<S: RecordSpec>(value: &S::Wire, modified: Option<i64>) -> AppResult<Self> {
        Self::check_timestamp::<S>(modified)?;
        S::validate(value)?;
        Ok(Self {
            schema_version: S::VERSION,
            modified_at_epoch_ms: modified,
            value: serde_json::to_value(value).map_err(cloud_json_error)?,
        })
    }
    pub fn decode<S: RecordSpec>(&self) -> AppResult<S::Wire> {
        if self.schema_version != S::VERSION {
            return Err(cloud_error(format!(
                "unsupported cloud record schema {} for {:?}; requires {}",
                self.schema_version,
                S::KEY,
                S::VERSION
            )));
        }
        Self::check_timestamp::<S>(self.modified_at_epoch_ms)?;
        let value = serde_json::from_value(self.value.clone()).map_err(cloud_json_error)?;
        S::validate(&value)?;
        Ok(value)
    }
    fn check_timestamp<S: RecordSpec>(modified: Option<i64>) -> AppResult<()> {
        if modified.is_some() != S::MUTABLE {
            return Err(cloud_error(format!(
                "invalid mutation timestamp for {:?}",
                S::KEY
            )));
        }
        Ok(())
    }
    pub fn modified_at_epoch_ms(&self) -> Option<i64> {
        self.modified_at_epoch_ms
    }
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub fn value(&self) -> &serde_json::Value {
        &self.value
    }

    // Malformed and historical fixtures must not be expressible by the current
    // production writer. These constructors never ship in ordinary clients.
    #[cfg(any(test, feature = "cloud-format-test"))]
    pub fn fixture(
        schema_version: u32,
        modified_at_epoch_ms: Option<i64>,
        value: serde_json::Value,
    ) -> Self {
        Self {
            schema_version,
            modified_at_epoch_ms,
            value,
        }
    }
}

macro_rules! record_specs {
    ($($name:ident => ($key:expr, $version:expr, $wire:ty, $mutable:expr, $validate:expr)),+ $(,)?) => {
        $(pub(super) struct $name;
        impl sealed::Sealed for $name {}
        impl RecordSpec for $name {
            type Wire = $wire;
            const KEY: KeyPattern = $key;
            const VERSION: u32 = $version;
            const MUTABLE: bool = $mutable;
            fn validate(value: &Self::Wire) -> AppResult<()> { ($validate)(value) }
        })+

        pub(super) fn validate(key: &str, record: &CloudRecord) -> AppResult<()> {
            $(if $name::KEY.matches(key) { record.decode::<$name>()?; return validate_key_binding(key, record); })+
            // Unknown families remain opaque so an older client cannot erase them.
            Ok(())
        }

        #[cfg(test)]
        pub(super) fn contract_inventory() -> serde_json::Value {
            serde_json::json!([$( {
                "key": $name::KEY.description(), "schema_version": $name::VERSION,
                "mutable": $name::MUTABLE, "payload": schemars::schema_for!($wire),
            } ),+])
        }
    };
}

pub(super) const FLIGHT_PLAN_RECORD_KEY: &str = "flight_plan/current";
pub(super) const OFFLINE_PACKAGE_REGION_RECORD_PREFIX: &str = "offline_packages/region/";
pub(super) const OFFLINE_PACKAGE_PRODUCT_RECORD_PREFIX: &str = "offline_packages/product/";
pub(super) const INACTIVITY_SLEEP_TIMEOUT_RECORD_KEY: &str = "settings/inactivity_sleep_timeout";
pub(super) const NEXRAD_ACQUISITION_RECORD_KEY: &str = "settings/nexrad_acquisition";
pub(super) const DEBUG_FLAG_RECORD_PREFIX: &str = "settings/debug/";
pub(super) const AIRCRAFT_LIBRARY_RECORD_PREFIX: &str = "aircraft/library/";
pub(super) const SERVICE_RECEIPT_PREFIX: &str = crate::service_notifications::RECEIPT_PREFIX;

record_specs! {
    FlightPlanRecord => (KeyPattern::Exact(FLIGHT_PLAN_RECORD_KEY), 4,
        wire::flight_plan::StoredFlightPlan, true, |value: &wire::flight_plan::StoredFlightPlan| {
            crate::FlightPlan::try_from(value.clone()).map(|_| ())
        }),
    PackageSelectionRecord => (KeyPattern::Prefixes(&[OFFLINE_PACKAGE_REGION_RECORD_PREFIX, OFFLINE_PACKAGE_PRODUCT_RECORD_PREFIX]), 1,
        wire::preferences::PackageSelection, true, |_| Ok(())),
    SleepRecord => (KeyPattern::Exact(INACTIVITY_SLEEP_TIMEOUT_RECORD_KEY), 1,
        wire::preferences::SleepTimeout, true, |_| Ok(())),
    NexradRecord => (KeyPattern::Exact(NEXRAD_ACQUISITION_RECORD_KEY), 1,
        wire::preferences::NexradPreferences, true, |_| Ok(())),
    DebugRecord => (KeyPattern::Prefix(DEBUG_FLAG_RECORD_PREFIX), 1, bool, true, |_| Ok(())),
    AircraftRecord => (KeyPattern::Prefix(product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX),
        product_contracts::AIRCRAFT_DEFINITION_SCHEMA_VERSION, product_contracts::AircraftDefinition,
        false, |value: &product_contracts::AircraftDefinition| { value.validate().map_err(cloud_error) }),
    AircraftMembershipRecord => (KeyPattern::Prefix(AIRCRAFT_LIBRARY_RECORD_PREFIX), 2,
        wire::preferences::AircraftMembership, true, |_| Ok(())),
    ServiceReceiptRecord => (KeyPattern::Prefix(SERVICE_RECEIPT_PREFIX), 1, bool, false,
        |value: &bool| if *value { Ok(()) } else { Err(cloud_error("read receipt must be true")) }),
}

fn validate_key_binding(key: &str, record: &CloudRecord) -> AppResult<()> {
    if let Some(hash) = key.strip_prefix(product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX) {
        let definition = record.decode::<AircraftRecord>()?;
        if definition.content_hash().map_err(cloud_error)? != hash {
            return Err(cloud_error("aircraft definition record hash mismatch"));
        }
    } else if let Some(hash) = key.strip_prefix(AIRCRAFT_LIBRARY_RECORD_PREFIX) {
        product_contracts::validate_aircraft_definition_hash(hash).map_err(cloud_error)?;
    } else if let Some(id) = key.strip_prefix(SERVICE_RECEIPT_PREFIX) {
        if id.len() != 64 || !id.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(cloud_error("invalid immutable service read receipt"));
        }
    }
    Ok(())
}
