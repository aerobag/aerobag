// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Support follows the service contract, independently of client/server builds.
pub use super::{
    epoch_ms, BulletinHint, ClientPlatform, Notice, NoticeAudience, NoticeSeverity, ReleaseRole,
    MAX_BYTES, MAX_NOTICES, MAX_RELEASES, REFRESH_INTERVAL_MS,
};
use serde::{Deserialize, Serialize};

pub const PATH: &str = "/service/bulletins-v2.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSupport {
    pub contract: String,
    pub role: ReleaseRole,
    pub support_until_utc: Option<String>,
    pub attention_revision: u64,
    pub web_update_url: String,
    pub android_update_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulletinDocument {
    pub schema_version: u32,
    pub publisher: String,
    pub revision: u64,
    pub published_at_utc: String,
    pub contracts: Vec<ContractSupport>,
    pub notices: Vec<Notice>,
}

pub fn validate_publisher(value: &str) -> Result<(), String> {
    super::validate_url(value)?;
    if !value.ends_with(PATH) || value.contains(['?', '#']) {
        return Err(format!("publisher must be the stable {PATH} URL"));
    }
    Ok(())
}

impl BulletinDocument {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("bulletin document exceeds 256 KiB".into());
        }
        let document: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 2 {
            return Err("unsupported contract bulletin schema".into());
        }
        validate_publisher(&self.publisher)?;
        // The notice, URL, role, timestamp and attention rules are unchanged.
        // Reuse their validator without expanding the immutable v1 decoder.
        let mut releases = Vec::new();
        for entry in &self.contracts {
            let number = entry.contract.strip_prefix('C').unwrap_or_default();
            if number.is_empty()
                || number.starts_with('0')
                || !number.bytes().all(|b| b.is_ascii_digit())
            {
                return Err("invalid service contract name".into());
            }
            releases.push(super::ReleaseSupport {
                release: entry.contract.clone(),
                commit: "0".repeat(40),
                role: entry.role,
                support_until_utc: entry.support_until_utc.clone(),
                attention_revision: entry.attention_revision,
                web_update_url: entry.web_update_url.clone(),
                android_update_url: entry.android_update_url.clone(),
            });
        }
        super::BulletinDocument {
            schema_version: 1,
            publisher: format!(
                "{}{}",
                self.publisher.strip_suffix(PATH).unwrap(),
                super::PATH
            ),
            revision: self.revision,
            published_at_utc: self.published_at_utc.clone(),
            releases,
            notices: self.notices.clone(),
        }
        .validate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_support_is_strict_and_does_not_extend_the_v1_decoder() {
        let mut document = BulletinDocument {
            schema_version: 2,
            publisher: format!("https://example.test{PATH}"),
            revision: 1,
            published_at_utc: "2026-11-18T00:00:00Z".into(),
            contracts: vec![ContractSupport {
                contract: "C1".into(),
                role: ReleaseRole::Production,
                support_until_utc: None,
                attention_revision: 1,
                web_update_url: "https://example.test/".into(),
                android_update_url: "https://example.test/about".into(),
            }],
            notices: vec![],
        };
        let bytes = serde_json::to_vec(&document).unwrap();
        assert_eq!(BulletinDocument::decode(&bytes).unwrap(), document);
        assert!(super::super::BulletinDocument::decode(&bytes).is_err());
        document.contracts.push(document.contracts[0].clone());
        assert!(document.validate().is_err());
        document.contracts.pop();
        document.contracts[0].role = ReleaseRole::Sunset;
        assert!(document.validate().is_err());
        document.contracts[0].support_until_utc = Some("2026-11-19T00:00:00Z".into());
        assert!(document.validate().is_ok());
        document.contracts[0].contract = "C01".into();
        assert!(document.validate().is_err());
    }
}
