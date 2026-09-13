// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    CoreResourceRequest, CoreResourceSource, DataStatusRecord, UiStatusAction, UiStatusActionStyle,
};
use app_ui_contracts::session::{
    ClientBuildInfo, UiServiceNotice, UiServiceNoticeAction, UiServiceNoticeLink,
    UiServiceNotificationsState, UiStatusSeverity,
};
use product_contracts::service_bulletins::{
    self as contract, BulletinDocument, BulletinHint, NoticeSeverity, ReleaseRole,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub const STATUS_ID: &str = "service:unread";
pub const OPEN_INBOX: &str = "service:inbox";
pub const RECEIPT_PREFIX: &str = "service/read/";
const RESOURCE_PREFIX: &str = "service-bulletin/";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ServicePersistentState {
    pub documents: BTreeMap<String, BulletinDocument>,
    pub checked_at: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Default)]
struct FetchState {
    pending: Option<(String, i64)>,
    due: i64,
    errors: u32,
    error: Option<String>,
    hinted_revision: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ServiceNotifications {
    persistent: Arc<ServicePersistentState>,
    sources: BTreeMap<String, FetchState>,
    next_request: u64,
    expanded: Option<String>,
    cache: Option<(
        i64,
        u64,
        Option<ClientBuildInfo>,
        Arc<UiServiceNotificationsState>,
    )>,
    projection_revision: u64,
}

fn key(publisher: &str, id: &str, revision: u64) -> String {
    let bytes =
        serde_json::to_vec(&(publisher, id, revision)).expect("receipt identity serializes");
    format!("{:x}", Sha256::digest(bytes))
}

fn age(milliseconds: i64) -> String {
    let seconds = milliseconds.max(0) / 1000;
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{:.1}h", seconds as f64 / 3600.)
    } else {
        format!("{:.1}d", seconds as f64 / 86400.)
    }
}

fn severity(value: NoticeSeverity) -> UiStatusSeverity {
    match value {
        NoticeSeverity::Info => UiStatusSeverity::Info,
        NoticeSeverity::Caution => UiStatusSeverity::Caution,
        NoticeSeverity::Warning => UiStatusSeverity::Warning,
    }
}

impl ServiceNotifications {
    pub fn persistent(&self) -> &ServicePersistentState {
        &self.persistent
    }

    pub fn restore(&mut self, mut value: ServicePersistentState) -> Result<(), String> {
        self.cache = None;
        let mut invalid = Vec::new();
        for (publisher, document) in &value.documents {
            let validation = document.validate().and_then(|()| {
                if publisher == &document.publisher {
                    Ok(())
                } else {
                    Err("cached bulletin publisher mismatch".into())
                }
            });
            if let Err(error) = validation {
                invalid.push(publisher.clone());
                self.sources.entry(publisher.clone()).or_default().error =
                    Some(format!("Invalid local bulletin cache: {error}"));
            }
        }
        // Cache damage must not make flight-plan/settings startup fail.
        // Diagnose the loss, retain unrelated user state, and request a fresh document.
        for publisher in invalid {
            value.documents.remove(&publisher);
            value.checked_at.remove(&publisher);
        }
        self.persistent = Arc::new(value);
        Ok(())
    }

    pub fn configure(&mut self, urls: &[String]) -> Result<(), String> {
        self.cache = None;
        if urls.len() > 4 {
            return Err("at most four bulletin publishers may be configured".into());
        }
        for url in urls {
            contract::validate_publisher(url)?;
        }
        self.sources.retain(|url, _| urls.contains(url));
        for url in urls {
            self.sources.entry(url.clone()).or_default();
        }
        Ok(())
    }

    pub fn handles_resource(id: &str) -> bool {
        id.starts_with(RESOURCE_PREFIX)
    }

    pub fn hint(&mut self, hint: BulletinHint) {
        if let Some(source) = self.sources.get_mut(&hint.publisher) {
            let loaded = self
                .persistent
                .documents
                .get(&hint.publisher)
                .map_or(0, |d| d.revision);
            if hint.revision > loaded && hint.revision > source.hinted_revision {
                source.hinted_revision = hint.revision;
                source.due = 0;
            }
        }
    }

    pub fn prepare(&mut self, now: i64) -> Vec<CoreResourceRequest> {
        let mut requests = Vec::new();
        for (url, source) in &mut self.sources {
            if source
                .pending
                .as_ref()
                .is_some_and(|(_, deadline)| now >= *deadline)
            {
                source.pending = None;
                source.error = Some("Bulletin request timed out".into());
                self.cache = None;
                source.errors = source.errors.saturating_add(1);
                source.due = now
                    + product_contracts::AEROBAG_SSE_TRANSPORT_POLICY
                        .reconnect_delay_ms(source.errors);
            }
            if source.pending.is_none() && now >= source.due {
                self.next_request += 1;
                let id = format!("{RESOURCE_PREFIX}{}", self.next_request);
                source.pending = Some((id.clone(), now + 65_000));
                requests.push(CoreResourceRequest {
                    id,
                    source: CoreResourceSource::PublicUrl { url: url.clone() },
                    optional: false,
                    max_response_bytes: Some(contract::MAX_BYTES as u64),
                });
            }
        }
        requests
    }

    pub fn failed(&mut self, id: &str, message: &str, now: i64) {
        self.cache = None;
        for source in self.sources.values_mut() {
            if source
                .pending
                .as_ref()
                .is_some_and(|(pending, _)| pending == id)
            {
                source.pending = None;
                source.errors = source.errors.saturating_add(1);
                source.due = now
                    + product_contracts::AEROBAG_SSE_TRANSPORT_POLICY
                        .reconnect_delay_ms(source.errors);
                source.error = Some(message.chars().take(256).collect());
            }
        }
    }

    pub fn ingest(&mut self, id: &str, bytes: &[u8], now: i64) -> Result<bool, String> {
        let Some(url) = self.sources.iter().find_map(|(url, source)| {
            source
                .pending
                .as_ref()
                .filter(|(pending, _)| pending == id)
                .map(|_| url.clone())
        }) else {
            return Ok(false);
        };
        let document = BulletinDocument::decode(bytes)?;
        self.cache = None;
        if document.publisher != url {
            return Err("bulletin publisher does not match requested authority".into());
        }
        if let Some(old) = self.persistent.documents.get(&url) {
            if document.revision < old.revision
                || (document.revision == old.revision && old != &document)
            {
                return Err("stale or conflicting bulletin revision".into());
            }
        }
        let source = self.sources.get_mut(&url).expect("pending source exists");
        source.pending = None;
        source.errors = 0;
        source.error = None;
        source.due = if source.hinted_revision > document.revision {
            now + 5_000
        } else {
            now + contract::REFRESH_INTERVAL_MS
        };
        let persistent = Arc::make_mut(&mut self.persistent);
        persistent.checked_at.insert(url.clone(), now);
        persistent.documents.insert(url, document);
        Ok(true)
    }

    pub fn project_cached(
        &mut self,
        now: i64,
        build: Option<&ClientBuildInfo>,
        cloud: &crate::cloud_controller::CloudController,
    ) -> Result<Arc<UiServiceNotificationsState>, String> {
        if let Some((time, revision, cached_build, page)) = &self.cache {
            if now < *time && *revision == cloud.revision() && cached_build.as_ref() == build {
                return Ok(Arc::clone(page));
            }
        }
        let receipts = cloud
            .service_read_receipts()
            .map_err(|error| error.message)?;
        let page = Arc::new(self.project(now, build, &receipts));
        self.projection_revision = self.projection_revision.saturating_add(1);
        let until = self.next_refresh(now);
        self.cache = Some((until, cloud.revision(), build.cloned(), Arc::clone(&page)));
        Ok(page)
    }

    pub fn next_refresh(&self, now: i64) -> i64 {
        // A minute refreshes relative labels; effective/expiry deadlines are exact.
        let mut due = (now / 60_000 + 1) * 60_000;
        for (publisher, source) in &self.sources {
            let fetch_due = source
                .pending
                .as_ref()
                .map_or(source.due, |(_, deadline)| *deadline);
            if fetch_due > now {
                due = due.min(fetch_due);
            }
            if let Some(document) = self.persistent.documents.get(publisher) {
                for time in document
                    .notices
                    .iter()
                    .flat_map(|n| {
                        [
                            Some(n.effective_at_utc.as_deref().unwrap_or(&n.published_at_utc)),
                            n.expires_at_utc.as_deref(),
                        ]
                    })
                    .chain(
                        document
                            .releases
                            .iter()
                            .map(|r| r.support_until_utc.as_deref()),
                    )
                    .flatten()
                {
                    let boundary = contract::epoch_ms(time).expect("validated timestamp");
                    if boundary > now {
                        due = due.min(boundary);
                    }
                }
            }
        }
        due
    }

    pub fn projection_revision(&self) -> u64 {
        self.projection_revision
    }

    pub fn project(
        &self,
        now: i64,
        build: Option<&ClientBuildInfo>,
        receipts: &BTreeSet<String>,
    ) -> UiServiceNotificationsState {
        let platform = build
            .map(|build| build.platform.to_ascii_lowercase())
            .unwrap_or_default();
        let mut items = Vec::new();
        let mut published_times = BTreeMap::new();
        let mut source_status = Vec::new();
        for (publisher, source) in &self.sources {
            let checked = self
                .persistent
                .checked_at
                .get(publisher)
                .map(|time| format!("Last checked {} ago", age(now - time)))
                .unwrap_or_else(|| "Not checked yet".into());
            source_status.push(format!(
                "{publisher}\n{checked}{}",
                source
                    .error
                    .as_ref()
                    .map(|error| format!(
                        "; refresh failed: {error}. Showing cached notices, if available."
                    ))
                    .unwrap_or_default()
            ));
            let Some(document) = self.persistent.documents.get(publisher) else {
                continue;
            };
            let own_release = build.filter(|b| !b.dirty).and_then(|b| {
                document.releases.iter().find(|release| {
                    b.version == release.release
                        && b.commit.as_deref() == Some(release.commit.as_str())
                })
            });
            for notice in &document.notices {
                if !notice.audience.platforms.is_empty()
                    && !notice.audience.platforms.iter().any(|p| {
                        matches!(
                            (p, platform.as_str()),
                            (contract::ClientPlatform::Web, "web")
                                | (contract::ClientPlatform::Android, "android")
                        )
                    })
                {
                    continue;
                }
                if !notice.audience.releases.is_empty()
                    && !own_release
                        .is_some_and(|release| notice.audience.releases.contains(&release.release))
                {
                    continue;
                }
                let published =
                    contract::epoch_ms(&notice.published_at_utc).expect("validated timestamp");
                let effective = notice
                    .effective_at_utc
                    .as_deref()
                    .map(contract::epoch_ms)
                    .transpose()
                    .expect("validated timestamp")
                    .unwrap_or(published);
                if now < effective {
                    continue;
                }
                let archived = notice.resolved
                    || notice.expires_at_utc.as_deref().is_some_and(|time| {
                        contract::epoch_ms(time).expect("validated timestamp") <= now
                    });
                let id = key(
                    publisher,
                    &format!("notice/{}", notice.id),
                    notice.attention_revision,
                );
                published_times.insert(id.clone(), published);
                items.push(UiServiceNotice {
                    unread: !archived && !receipts.contains(&id),
                    expanded: self.expanded.as_ref() == Some(&id),
                    archived,
                    open_action: UiServiceNoticeAction {
                        action_id: format!("service:read:{id}"),
                        label: notice.title.clone(),
                    },
                    id,
                    title: notice.title.clone(),
                    body: notice.body.clone(),
                    timing: format!(
                        "Published {} ({} ago)",
                        notice.published_at_utc,
                        age(now - published)
                    ),
                    state_label: if archived { "History" } else { "Active" }.into(),
                    severity: severity(notice.severity),
                    link: notice.link.as_ref().map(|link| UiServiceNoticeLink {
                        label: link.label.clone(),
                        url: link.url.clone(),
                    }),
                });
            }
            if let Some(release) =
                own_release.filter(|r| matches!(r.role, ReleaseRole::Sunset | ReleaseRole::Retired))
            {
                let until = release
                    .support_until_utc
                    .as_deref()
                    .expect("validated release deadline");
                let deadline = contract::epoch_ms(until).expect("validated timestamp");
                let ended = deadline <= now;
                let milestone = if ended {
                    "support-ended"
                } else {
                    "support-ending"
                };
                let id = key(
                    publisher,
                    &format!("release/{}/{milestone}", release.release),
                    release.attention_revision,
                );
                published_times.insert(
                    id.clone(),
                    contract::epoch_ms(&document.published_at_utc).expect("validated timestamp"),
                );
                let current = document
                    .releases
                    .iter()
                    .find(|r| r.role == ReleaseRole::Production)
                    .map(|r| format!("Version {} is now current. ", r.release))
                    .unwrap_or_default();
                let (label, url) = if platform == "android" {
                    ("Update your app", &release.android_update_url)
                } else {
                    ("Reload with the current version", &release.web_update_url)
                };
                items.push(UiServiceNotice {
                    unread: !receipts.contains(&id), expanded: self.expanded.as_ref() == Some(&id), archived: false,
                    open_action: UiServiceNoticeAction { action_id: format!("service:read:{id}"), label: format!("Version {} support", release.release) },
                    id, title: if ended { "This application version is no longer supported" } else { "Update available; support for this version is ending" }.into(),
                    body: format!("{current}You are using version {}. Its cycle products and live feeds {} supported until {until}. Update when you are not navigating.", release.release, if ended { "were" } else { "will only be" }),
                    timing: if ended { format!("Support ended {} ago", age(now - deadline)) } else { format!("Support ends in {}", age(deadline - now)) },
                    state_label: "Active".into(), severity: if ended { UiStatusSeverity::Warning } else { UiStatusSeverity::Caution },
                    link: Some(UiServiceNoticeLink { label: label.into(), url: url.clone() }),
                });
            }
        }
        for item in &mut items {
            item.state_label = if item.archived {
                "History"
            } else if item.unread {
                "Unread"
            } else {
                "Read"
            }
            .into();
        }
        items.sort_by(|a, b| {
            b.unread
                .cmp(&a.unread)
                .then(a.archived.cmp(&b.archived))
                .then(b.severity.cmp(&a.severity))
                .then(published_times[&b.id].cmp(&published_times[&a.id]))
                .then(a.id.cmp(&b.id))
        });
        let unread = items
            .iter()
            .filter(|item| item.unread)
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>();
        let mark_all_read = (!unread.is_empty()).then(|| UiServiceNoticeAction {
            action_id: format!("service:read-all:{}", unread.join(",")),
            label: "Mark all read".into(),
        });
        UiServiceNotificationsState {
            title: "Service Notifications".into(),
            summary: if items.is_empty() {
                "No service notifications are available.".into()
            } else {
                format!("{} unread service notifications", unread.len())
            },
            source_status,
            mark_all_read,
            items,
        }
    }

    /// The action contains exactly the displayed revision keys. Arrivals during
    /// the click must not be acknowledged by a fresh 'all current' lookup.
    pub fn read_action(&mut self, action: &str) -> Result<Vec<String>, String> {
        self.cache = None;
        let (ids, single) = if let Some(id) = action.strip_prefix("service:read:") {
            (id, true)
        } else if let Some(ids) = action.strip_prefix("service:read-all:") {
            (ids, false)
        } else {
            return Err("unknown service notification action".into());
        };
        let keys: Vec<_> = ids.split(',').map(str::to_string).collect();
        if (single && keys.len() != 1)
            || keys.len() > 4 * (contract::MAX_NOTICES + contract::MAX_RELEASES)
            || keys.iter().any(|key| {
                key.len() != 64
                    || !key
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
        {
            return Err("invalid service read receipt".into());
        }
        if single {
            self.expanded = keys.first().cloned();
        }
        Ok(keys)
    }
}

pub fn status_record(page: &UiServiceNotificationsState) -> Option<DataStatusRecord> {
    let unread: Vec<_> = page.items.iter().filter(|item| item.unread).collect();
    if unread.is_empty() {
        return None;
    }
    let mut record = DataStatusRecord::new(
        STATUS_ID,
        "SERVICE NOTIFICATIONS",
        Some(unread.len().to_string()),
        unread
            .iter()
            .map(|item| item.severity)
            .max()
            .unwrap_or(UiStatusSeverity::Info),
        true,
        page.summary.clone(),
    );
    record.hushable = false;
    record.actions.push(UiStatusAction {
        id: OPEN_INBOX.into(),
        label: "Read notifications".into(),
        enabled: true,
        style: UiStatusActionStyle::Normal,
    });
    Some(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use contract::{Notice, NoticeAudience, ReleaseSupport};

    fn document() -> BulletinDocument {
        BulletinDocument {
            schema_version: 1,
            publisher: format!("https://service.test{}", contract::PATH),
            revision: 1,
            published_at_utc: "2026-09-13T00:00:00Z".into(),
            releases: vec![],
            notices: vec![Notice {
                id: "notam-quality".into(),
                attention_revision: 1,
                title: "NOTAM distribution quality".into(),
                body: "Please use caution.".into(),
                severity: NoticeSeverity::Caution,
                published_at_utc: "2026-09-13T00:00:00Z".into(),
                effective_at_utc: None,
                expires_at_utc: None,
                resolved: false,
                audience: NoticeAudience::default(),
                link: None,
            }],
        }
    }
    fn now() -> i64 {
        contract::epoch_ms("2026-09-13T00:00:01Z").unwrap()
    }
    fn install(state: &mut ServiceNotifications, document: &BulletinDocument, time: i64) {
        state
            .configure(std::slice::from_ref(&document.publisher))
            .unwrap();
        state.hint(BulletinHint {
            publisher: document.publisher.clone(),
            revision: document.revision,
        });
        let request = state.prepare(time).pop().expect("fetch effect");
        assert_eq!(request.max_response_bytes, Some(contract::MAX_BYTES as u64));
        assert!(state
            .ingest(&request.id, &serde_json::to_vec(document).unwrap(), time)
            .unwrap());
    }

    #[test]
    fn reading_displayed_batch_does_not_read_a_new_notice() {
        let mut state = ServiceNotifications::default();
        let mut doc = document();
        install(&mut state, &doc, now());
        let shown = state.project(now(), None, &BTreeSet::new());
        assert_eq!(
            status_record(&shown).unwrap().severity,
            UiStatusSeverity::Caution
        );
        let all = shown.mark_all_read.unwrap().action_id;
        let mut other = doc.notices[0].clone();
        other.id = "other".into();
        doc.notices.push(other);
        doc.revision += 1;
        install(&mut state, &doc, now() + 1000);
        let receipts = state.read_action(&all).unwrap().into_iter().collect();
        let remaining = state.project(now() + 1000, None, &receipts);
        assert_eq!(remaining.items.iter().filter(|n| n.unread).count(), 1);
        assert_eq!(remaining.items.iter().filter(|n| !n.unread).count(), 1);
        assert_eq!(status_record(&remaining).unwrap().id, STATUS_ID);
    }

    #[test]
    fn receipt_identity_changes_only_for_attention_revision_or_publisher() {
        let mut state = ServiceNotifications::default();
        let mut doc = document();
        install(&mut state, &doc, now());
        let first = state.project(now(), None, &BTreeSet::new());
        let read: BTreeSet<_> = state
            .read_action(&first.items[0].open_action.action_id)
            .unwrap()
            .into_iter()
            .collect();
        doc.revision += 1;
        doc.notices[0].body = "Corrected wording.".into();
        install(&mut state, &doc, now() + 1000);
        assert!(!state.project(now(), None, &read).items[0].unread);
        doc.revision += 1;
        doc.notices[0].attention_revision += 1;
        install(&mut state, &doc, now() + 2000);
        assert!(state.project(now(), None, &read).items[0].unread);
        let mut other = state.clone();
        doc.publisher = format!("https://other.test{}", contract::PATH);
        install(&mut other, &doc, now());
        assert_ne!(
            other.project(now(), None, &read).items[0].id,
            state.project(now(), None, &read).items[0].id
        );
    }

    #[test]
    fn offline_cache_survives_failed_fetch_and_time_changes_create_exact_milestones() {
        let mut state = ServiceNotifications::default();
        let mut doc = document();
        doc.notices.clear();
        doc.releases.push(ReleaseSupport {
            release: "2026-09-12.1".into(),
            commit: "a".repeat(40),
            role: ReleaseRole::Sunset,
            support_until_utc: Some("2026-09-13T00:00:15Z".into()),
            attention_revision: 1,
            web_update_url: "https://service.test/".into(),
            android_update_url: "https://service.test/about".into(),
        });
        install(&mut state, &doc, now());
        let build = ClientBuildInfo {
            platform: "Web".into(),
            version: "2026-09-12.1".into(),
            built_at_utc: None,
            commit: Some("a".repeat(40)),
            dirty: false,
        };
        let mut cloud = crate::cloud_controller::CloudController::default();
        let before = state.project_cached(now(), Some(&build), &cloud).unwrap();
        assert_eq!(before.items[0].severity, UiStatusSeverity::Caution);
        cloud.record_service_read(&before.items[0].id).unwrap();
        assert!(
            !state
                .project_cached(now() + 1000, Some(&build), &cloud)
                .unwrap()
                .items[0]
                .unread
        );
        let mut restarted = ServiceNotifications::default();
        restarted
            .restore(
                serde_json::from_slice(&serde_json::to_vec(state.persistent()).unwrap()).unwrap(),
            )
            .unwrap();
        restarted
            .configure(std::slice::from_ref(&doc.publisher))
            .unwrap();
        let request = restarted.prepare(now()).pop().unwrap();
        restarted.failed(&request.id, "network unreachable", now());
        let deadline = contract::epoch_ms("2026-09-13T00:00:15Z").unwrap();
        assert_eq!(state.next_refresh(now()), deadline);
        let after = state
            .project_cached(deadline, Some(&build), &cloud)
            .unwrap();
        assert!(after.items[0].unread);
        assert_eq!(after.items[0].severity, UiStatusSeverity::Warning);
        assert!(
            restarted
                .project(
                    deadline,
                    Some(&build),
                    &cloud.service_read_receipts().unwrap()
                )
                .items[0]
                .unread
        );
        assert!(state
            .project(
                deadline,
                Some(&ClientBuildInfo {
                    dirty: true,
                    ..build.clone()
                }),
                &BTreeSet::new()
            )
            .items
            .is_empty());
        let android = state.project(
            deadline,
            Some(&ClientBuildInfo {
                platform: "Android".into(),
                ..build
            }),
            &BTreeSet::new(),
        );
        assert_eq!(
            android.items[0].link.as_ref().unwrap().label,
            "Update your app"
        );
    }

    #[test]
    fn hints_coalesce_and_untrusted_documents_do_not_replace_good_cache() {
        let mut state = ServiceNotifications::default();
        let doc = document();
        install(&mut state, &doc, now());
        assert!(state.prepare(now() + 1).is_empty());
        state.hint(BulletinHint {
            publisher: "https://evil.test".into(),
            revision: 1000,
        });
        assert!(state.prepare(now() + 1).is_empty());
        state.hint(BulletinHint {
            publisher: doc.publisher.clone(),
            revision: 2,
        });
        let request = state.prepare(now() + 1).pop().unwrap();
        for revision in 3..100 {
            state.hint(BulletinHint {
                publisher: doc.publisher.clone(),
                revision,
            });
        }
        assert!(state.prepare(now() + 2).is_empty());
        assert!(state
            .ingest(&request.id, b"<html>not a bulletin</html>", now())
            .is_err());
        assert_eq!(state.persistent.documents[&doc.publisher], doc);
        state.failed(&request.id, "invalid response", now());
        assert!(state.prepare(now() + 4999).is_empty());
        assert_eq!(state.prepare(now() + 5000).len(), 1);
        assert!(state.read_action("service:read:bogus").is_err());
        assert!(state
            .read_action(&format!(
                "service:read:{},{}",
                "a".repeat(64),
                "b".repeat(64)
            ))
            .is_err());
    }

    #[test]
    fn release_identity_uses_both_tag_and_commit_and_cache_damage_is_isolated() {
        let mut state = ServiceNotifications::default();
        let mut doc = document();
        doc.notices.clear();
        let release = ReleaseSupport {
            release: "old-tag".into(),
            commit: "a".repeat(40),
            role: ReleaseRole::Sunset,
            support_until_utc: Some("2026-09-14T00:00:00Z".into()),
            attention_revision: 1,
            web_update_url: "https://service.test/".into(),
            android_update_url: "https://service.test/about".into(),
        };
        doc.releases = vec![
            release.clone(),
            ReleaseSupport {
                release: "new-tag".into(),
                role: ReleaseRole::Production,
                support_until_utc: None,
                ..release
            },
        ];
        install(&mut state, &doc, now());
        let build = ClientBuildInfo {
            platform: "Web".into(),
            version: "new-tag".into(),
            commit: Some("a".repeat(40)),
            built_at_utc: None,
            dirty: false,
        };
        assert!(state
            .project(now(), Some(&build), &BTreeSet::new())
            .items
            .is_empty());
        let old = ClientBuildInfo {
            version: "old-tag".into(),
            ..build
        };
        assert_eq!(
            state
                .project(now(), Some(&old), &BTreeSet::new())
                .items
                .len(),
            1
        );
        let mut persisted = state.persistent().clone();
        persisted
            .documents
            .get_mut(&doc.publisher)
            .unwrap()
            .schema_version = 0;
        let mut restarted = ServiceNotifications::default();
        restarted.restore(persisted).unwrap();
        restarted.configure(&[doc.publisher]).unwrap();
        let page = restarted.project(now(), Some(&old), &BTreeSet::new());
        assert!(page.items.is_empty());
        assert!(page.source_status[0].contains("Invalid local bulletin cache"));
        assert_eq!(restarted.prepare(now()).len(), 1);
    }

    #[test]
    fn scheduled_and_resolved_notices_remain_readable_without_unread_alarm() {
        let mut state = ServiceNotifications::default();
        let mut doc = document();
        doc.notices[0].effective_at_utc = Some("2026-09-13T00:00:15Z".into());
        doc.notices[0].expires_at_utc = Some("2026-09-13T00:00:20Z".into());
        install(&mut state, &doc, now());
        assert!(state
            .project(now(), None, &BTreeSet::new())
            .items
            .is_empty());
        let deadline = contract::epoch_ms("2026-09-13T00:00:15Z").unwrap();
        let cloud = crate::cloud_controller::CloudController::default();
        state.project_cached(now(), None, &cloud).unwrap();
        assert_eq!(
            state
                .project_cached(deadline, None, &cloud)
                .unwrap()
                .items
                .len(),
            1
        );
        let expired = state.project_cached(deadline + 5000, None, &cloud).unwrap();
        assert!(expired.items[0].archived);
        assert!(status_record(&expired).is_none());
    }
}
