// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;

use serde::de::DeserializeOwned;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDecodeError {
    message: String,
}

impl ContractDecodeError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ContractDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ContractDecodeError {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SchemaVersionHeader {
    schema_version: u32,
}

pub fn decode_exact<T: DeserializeOwned>(
    label: &str,
    payload: &[u8],
    required_schema_version: u32,
) -> Result<T, ContractDecodeError> {
    let value = serde_json::from_slice::<serde_json::Value>(payload)
        .map_err(|error| ContractDecodeError::new(format!("invalid {label} JSON: {error}")))?;
    decode_exact_value(label, value, required_schema_version)
}

pub fn decode_offered_list<T: DeserializeOwned>(
    label: &str,
    payload: &[u8],
    required_schema_version: u32,
) -> Result<Vec<T>, ContractDecodeError> {
    let values = serde_json::from_slice::<Vec<serde_json::Value>>(payload)
        .map_err(|error| ContractDecodeError::new(format!("invalid {label} list JSON: {error}")))?;
    if values.is_empty() {
        return Err(ContractDecodeError::new(format!("{label} list is empty")));
    }

    let mut offered_versions = Vec::with_capacity(values.len());
    let mut selected = Vec::new();
    for value in values {
        let schema_version = schema_version(label, &value)?;
        offered_versions.push(schema_version);
        if schema_version == required_schema_version {
            selected.push(decode_body(label, value)?);
        }
    }
    if selected.is_empty() {
        offered_versions.sort_unstable();
        offered_versions.dedup();
        return Err(ContractDecodeError::new(format!(
            "{label} offers schema versions {offered_versions:?}; client requires schema version {required_schema_version}"
        )));
    }
    Ok(selected)
}

fn decode_exact_value<T: DeserializeOwned>(
    label: &str,
    value: serde_json::Value,
    required_schema_version: u32,
) -> Result<T, ContractDecodeError> {
    let offered = schema_version(label, &value)?;
    if offered != required_schema_version {
        return Err(ContractDecodeError::new(format!(
            "{label} offers schema version {offered}; client requires schema version {required_schema_version}"
        )));
    }
    decode_body(label, value)
}

fn schema_version(label: &str, value: &serde_json::Value) -> Result<u32, ContractDecodeError> {
    let object = value
        .as_object()
        .ok_or_else(|| ContractDecodeError::new(format!("{label} must be a JSON object")))?;
    let header_value = serde_json::json!({
        "schema_version": object.get("schema_version").cloned().unwrap_or(serde_json::Value::Null),
    });
    serde_json::from_value::<SchemaVersionHeader>(header_value)
        .map(|header| header.schema_version)
        .map_err(|error| {
            ContractDecodeError::new(format!("{label} has no valid schema_version: {error}"))
        })
}

fn decode_body<T: DeserializeOwned>(
    label: &str,
    value: serde_json::Value,
) -> Result<T, ContractDecodeError> {
    serde_json::from_value(value)
        .map_err(|error| ContractDecodeError::new(format!("invalid {label}: {error}")))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, PartialEq, Eq, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ExampleV2 {
        schema_version: u32,
        value: String,
    }

    #[test]
    fn exact_decode_reports_schema_before_body_shape() {
        let error =
            decode_exact::<ExampleV2>("example", br#"{"schema_version":3,"unrelated":true}"#, 2)
                .unwrap_err();
        assert_eq!(
            error.to_string(),
            "example offers schema version 3; client requires schema version 2"
        );
    }

    #[test]
    fn offered_list_selects_only_the_required_schema() {
        let decoded = decode_offered_list::<ExampleV2>(
            "examples",
            br#"[
                {"schema_version":1,"legacy":true},
                {"schema_version":2,"value":"accepted"}
            ]"#,
            2,
        )
        .unwrap();
        assert_eq!(
            decoded,
            vec![ExampleV2 {
                schema_version: 2,
                value: "accepted".to_string(),
            }]
        );
    }

    #[test]
    fn offered_list_reports_all_available_versions() {
        let error = decode_offered_list::<ExampleV2>(
            "examples",
            br#"[
                {"schema_version":3},
                {"schema_version":1},
                {"schema_version":3}
            ]"#,
            2,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "examples offers schema versions [1, 3]; client requires schema version 2"
        );
    }

    #[test]
    fn exact_body_rejects_missing_and_unknown_fields() {
        assert!(decode_exact::<ExampleV2>("example", br#"{"schema_version":2}"#, 2).is_err());
        assert!(decode_exact::<ExampleV2>(
            "example",
            br#"{"schema_version":2,"value":"ok","extra":true}"#,
            2,
        )
        .is_err());
    }
}
