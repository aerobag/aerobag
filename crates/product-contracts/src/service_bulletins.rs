// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Cross-release public service bulletin contract. Never put operator secrets here.
use std::collections::BTreeSet;

use chrono::DateTime;
use serde::{Deserialize, Serialize};

pub const PATH: &str = "/service/bulletins-v1.json";
pub const EVENT: &str = "service-bulletins";
pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_NOTICES: usize = 256;
pub const MAX_RELEASES: usize = 1024;
pub const REFRESH_INTERVAL_MS: i64 = 4 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulletinDocument {
    pub schema_version: u32,
    /// Canonical absolute URL of this publisher's bulletin document.
    pub publisher: String,
    pub revision: u64,
    pub published_at_utc: String,
    pub releases: Vec<ReleaseSupport>,
    pub notices: Vec<Notice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseSupport {
    pub release: String,
    pub commit: String,
    pub role: ReleaseRole,
    pub support_until_utc: Option<String>,
    pub attention_revision: u64,
    pub web_update_url: String,
    pub android_update_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseRole {
    Production,
    Staging,
    Sunset,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeSeverity {
    Info,
    Caution,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notice {
    pub id: String,
    pub attention_revision: u64,
    pub title: String,
    pub body: String,
    pub severity: NoticeSeverity,
    pub published_at_utc: String,
    pub effective_at_utc: Option<String>,
    pub expires_at_utc: Option<String>,
    pub resolved: bool,
    pub audience: NoticeAudience,
    pub link: Option<NoticeLink>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoticeAudience {
    /// Empty means every client of this publisher. Identities are exact, not ordered.
    pub releases: Vec<String>,
    pub platforms: Vec<ClientPlatform>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientPlatform {
    Web,
    Android,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoticeLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BulletinHint {
    pub publisher: String,
    pub revision: u64,
}

pub fn epoch_ms(value: &str) -> Result<i64, String> {
    if !value.ends_with('Z') {
        return Err("bulletin timestamps must use UTC Z notation".into());
    }
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.timestamp_millis())
        .map_err(|_| format!("invalid bulletin timestamp: {value}"))
}

pub fn validate_url(value: &str) -> Result<(), String> {
    let rest = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .ok_or("bulletin URLs must be absolute HTTP(S) URLs")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if value.len() > 2048
        || authority.is_empty()
        || authority.contains('@')
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
    {
        return Err("invalid bulletin URL".into());
    }
    Ok(())
}

pub fn validate_publisher(value: &str) -> Result<(), String> {
    validate_url(value)?;
    if !value.ends_with(PATH) || value.contains(['?', '#']) {
        return Err(format!("publisher must be the stable {PATH} URL"));
    }
    Ok(())
}

fn identifier(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(format!("invalid bulletin identifier: {value}"));
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
        if self.schema_version != 1 || self.revision == 0 {
            return Err("unsupported bulletin schema or invalid revision".into());
        }
        validate_publisher(&self.publisher)?;
        epoch_ms(&self.published_at_utc)?;
        if self.notices.len() > MAX_NOTICES || self.releases.len() > MAX_RELEASES {
            return Err("bulletin entry limit exceeded".into());
        }
        let mut ids = BTreeSet::new();
        for notice in &self.notices {
            identifier(&notice.id)?;
            if !ids.insert(&notice.id) || notice.attention_revision == 0 {
                return Err("duplicate notice or invalid attention revision".into());
            }
            if notice.title.trim().is_empty()
                || notice.title.len() > 256
                || notice.body.trim().is_empty()
                || notice.body.len() > 16 * 1024
                || notice.title.chars().any(char::is_control)
            {
                return Err("invalid or oversized notice text".into());
            }
            let published = epoch_ms(&notice.published_at_utc)?;
            let effective = notice
                .effective_at_utc
                .as_deref()
                .map(epoch_ms)
                .transpose()?
                .unwrap_or(published);
            if notice
                .expires_at_utc
                .as_deref()
                .map(epoch_ms)
                .transpose()?
                .is_some_and(|end| end <= effective)
            {
                return Err("notice expiration must follow its effective time".into());
            }
            if notice.audience.releases.len() > MAX_RELEASES || notice.audience.platforms.len() > 2
            {
                return Err("notice audience limit exceeded".into());
            }
            for release in &notice.audience.releases {
                identifier(release)?;
            }
            if let Some(link) = &notice.link {
                validate_url(&link.url)?;
                if link.label.trim().is_empty() || link.label.len() > 128 {
                    return Err("invalid bulletin link label".into());
                }
            }
        }
        ids.clear();
        let mut production = 0;
        for release in &self.releases {
            identifier(&release.release)?;
            if !ids.insert(&release.release)
                || release.attention_revision == 0
                || release.commit.len() != 40
                || !release.commit.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err("invalid or duplicate release identity".into());
            }
            production += usize::from(release.role == ReleaseRole::Production);
            match (&release.support_until_utc, release.role) {
                (Some(time), ReleaseRole::Sunset | ReleaseRole::Retired) => {
                    epoch_ms(time)?;
                }
                (None, ReleaseRole::Production | ReleaseRole::Staging) => {}
                _ => return Err("release role and support deadline disagree".into()),
            }
            validate_url(&release.web_update_url)?;
            validate_url(&release.android_update_url)?;
        }
        if production > 1 {
            return Err("multiple production releases".into());
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_BYTES {
            return Err("bulletin document exceeds 256 KiB".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_contract_rejects_malformed_and_unsafe_input() {
        let mut document = BulletinDocument {
            schema_version: 1,
            publisher: format!("https://example.test{PATH}"),
            revision: 1,
            published_at_utc: "2026-09-13T00:00:00Z".into(),
            releases: vec![],
            notices: vec![],
        };
        assert!(document.validate().is_ok());
        document.schema_version = 2;
        assert!(document.validate().is_err());
        assert!(BulletinDocument::decode(&vec![b' '; MAX_BYTES + 1]).is_err());
        for bad in [
            "javascript:alert(1)",
            "https://user:pass@host/service/bulletins-v1.json",
            "https://x\\@evil/",
            "https:///empty",
        ] {
            assert!(validate_url(bad).is_err(), "{bad}");
        }
        assert!(epoch_ms("2026-09-13T00:00:00").is_err());
        assert!(validate_publisher("https://example.test/arbitrary.json").is_err());
    }
}
