// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use product_contracts::{LiveFeedProductPolicy, LIVE_FEED_PRODUCT_POLICIES};
use serde::Deserialize;

use crate::{
    data_status::{parse_status_action_id, project_data_status_state, UiStatusActionCommand},
    freshness::{
        cycle_product_is_expired, evaluate_age, format_age, parse_utc_instant,
        required_live_feed_age_policy, FreshnessSeverity,
    },
    time_display::RelativeTimeStyle,
    BundlePackageArtifact, ClientBuildInfo, CloudStatusSummary, DataStatusRecord,
    UiDataStatusPageFact, UiDataStatusPageRow, UiDataStatusPageState, UiDataStatusState,
    UiStatusSeverity,
};

const PACKAGE_WARNING_STATUS_PREFIX: &str = "package_ui_warning:";
const WINDS_ALOFT_UNAVAILABLE_STATUS_ID: &str = "live_feed:winds-aloft_unavailable";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct DataStatusNavDbPackageRecord {
    pub id: String,
    pub family_id: String,
    #[serde(default)]
    pub effective_date: Option<String>,
    #[serde(default)]
    pub expiration_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct DataStatusNavDbFamilyRecord {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub warning_text: Option<String>,
    #[serde(default)]
    pub ui_warning: Option<DataStatusPackageUiWarning>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct DataStatusPackageUiWarning {
    pub severity: UiStatusSeverity,
    pub label: String,
    #[serde(default)]
    pub value: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataStatusNavDbArtifactInput {
    pub package_id: String,
    pub filename: String,
    pub cycle: Option<String>,
    pub cycle_version: Option<String>,
    pub contract_id: Option<String>,
    pub effective_date: Option<String>,
    pub expiration_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataStatusAttachedPackageWarning {
    pub package_id: String,
    pub warning_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DataStatusPublicationInput {
    pub bundle_count: usize,
    pub loaded_manifest_count: usize,
    pub as_of_utc: Option<String>,
    pub checked_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataStatusLiveFeedConnectionMode {
    Unknown,
    Connecting,
    Connected,
    Error,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataStatusLiveFeedConnectionInput {
    pub mode: DataStatusLiveFeedConnectionMode,
    pub source_url: Option<String>,
    pub status_url: Option<String>,
    pub last_heard_epoch_ms: Option<i64>,
    pub last_error_epoch_ms: Option<i64>,
    pub last_error_message: Option<String>,
    pub last_resource_error_epoch_ms: Option<i64>,
    pub last_resource_error_message: Option<String>,
    pub network_status: Option<crate::LiveFeedNetworkStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DataStatusLiveFeedProductInput {
    pub loaded: bool,
    pub listed: bool,
    pub index_loaded: bool,
    pub collected_utc: Option<DateTime<Utc>>,
    pub loaded_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataStatusForecastInput {
    pub loaded: bool,
    pub listed: bool,
    pub index_loaded: bool,
    pub version_label: Option<String>,
    pub model_id: Option<String>,
    pub cycle_time_epoch_ms: Option<i64>,
    pub valid_through_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataStatusSourcesInput {
    pub cycle_data_base_url: String,
    pub live_feeds_base_url: String,
    pub debug_log_sink_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DataStatusPageInput {
    pub now_epoch_ms: i64,
    pub time_display_mode: crate::TimeDisplayMode,
    pub local_time_zone: Tz,
    pub client_build: Option<ClientBuildInfo>,
    pub data_sources: Option<DataStatusSourcesInput>,
    pub nav_db_available: bool,
    pub active_nav_db: Option<DataStatusNavDbArtifactInput>,
    pub nav_db_packages: Vec<DataStatusNavDbPackageRecord>,
    pub published_packages: Vec<BundlePackageArtifact>,
    pub publication: Option<DataStatusPublicationInput>,
    pub cloud: Option<CloudStatusSummary>,
    pub live_feed_connection: DataStatusLiveFeedConnectionInput,
    pub live_feed_products: BTreeMap<String, DataStatusLiveFeedProductInput>,
    pub forecast: DataStatusForecastInput,
    pub nexrad_frame_age_summary: String,
}

#[derive(Clone, Default)]
struct DataStatusModel {
    records: BTreeMap<String, DataStatusRecord>,
    hushed_ids: BTreeSet<String>,
    revision: u64,
}

#[derive(Clone)]
pub(crate) struct DataStatusModelCheckpoint {
    model: Arc<DataStatusModel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataStatusActionIntent {
    ProjectionChanged,
    ReloadApplication,
}

#[derive(Clone)]
struct DataStatusStateCache {
    revision: u64,
    state: UiDataStatusState,
}

#[derive(Clone)]
struct DataStatusPageCache {
    revision: u64,
    input: DataStatusPageInput,
    state: UiDataStatusPageState,
}

pub(crate) struct DataStatusStateProjection {
    pub state: UiDataStatusState,
    pub rebuilt: bool,
}

pub(crate) struct DataStatusPageProjection {
    pub state: UiDataStatusPageState,
    pub rebuilt: bool,
}

pub(crate) struct DataStatusController {
    model: Arc<DataStatusModel>,
    state_cache: Option<DataStatusStateCache>,
    page_cache: Option<DataStatusPageCache>,
    page_projection_revision: u64,
}

impl Default for DataStatusController {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl DataStatusController {
    pub fn new(records: impl IntoIterator<Item = DataStatusRecord>) -> Self {
        let records = records
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();
        Self {
            model: Arc::new(DataStatusModel {
                records,
                ..DataStatusModel::default()
            }),
            state_cache: None,
            page_cache: None,
            page_projection_revision: 0,
        }
    }

    pub fn revision(&self) -> u64 {
        self.model.revision
    }

    pub fn page_projection_revision(&self) -> u64 {
        self.page_projection_revision
    }

    pub fn checkpoint_model(&self) -> DataStatusModelCheckpoint {
        DataStatusModelCheckpoint {
            model: Arc::clone(&self.model),
        }
    }

    pub fn rollback_model(&mut self, checkpoint: DataStatusModelCheckpoint) {
        self.model = checkpoint.model;
    }

    pub fn contains(&self, id: &str) -> bool {
        self.model.records.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.model.records.len()
    }

    pub fn upsert(&mut self, record: DataStatusRecord) -> bool {
        if self
            .model
            .records
            .get(&record.id)
            .is_some_and(|existing| existing == &record)
        {
            return false;
        }
        Arc::make_mut(&mut self.model)
            .records
            .insert(record.id.clone(), record);
        self.note_change();
        true
    }

    pub fn clear(&mut self, id: &str) -> bool {
        if !self.model.records.contains_key(id) {
            return false;
        }
        Arc::make_mut(&mut self.model).records.remove(id);
        self.note_change();
        true
    }

    pub fn replace_prefix(
        &mut self,
        prefix: &str,
        records: impl IntoIterator<Item = DataStatusRecord>,
    ) -> bool {
        let records = records
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect::<BTreeMap<_, _>>();
        let stale_ids = self
            .model
            .records
            .keys()
            .filter(|id| id.starts_with(prefix) && !records.contains_key(*id))
            .cloned()
            .collect::<Vec<_>>();
        let changed_records = records.iter().any(|(id, record)| {
            self.model
                .records
                .get(id)
                .is_none_or(|existing| existing != record)
        });
        if stale_ids.is_empty() && !changed_records {
            return false;
        }
        let model = Arc::make_mut(&mut self.model);
        for id in stale_ids {
            model.records.remove(&id);
        }
        model.records.extend(records);
        self.note_change();
        true
    }

    pub fn replace_package_warnings(
        &mut self,
        families: impl IntoIterator<Item = DataStatusNavDbFamilyRecord>,
        attached: Option<DataStatusAttachedPackageWarning>,
    ) -> bool {
        let mut records = BTreeMap::new();
        for family in families {
            if let Some(record) = nav_db_family_warning_status_record(&family) {
                records.insert(record.id.clone(), record);
            }
        }
        if let Some(attached) = attached {
            let record = DataStatusRecord::new(
                format!("{PACKAGE_WARNING_STATUS_PREFIX}{}", attached.package_id),
                "NAV DB",
                Some("WARNING".to_string()),
                UiStatusSeverity::Warning,
                true,
                attached.warning_text,
            );
            records.insert(record.id.clone(), record);
        }
        self.replace_prefix(PACKAGE_WARNING_STATUS_PREFIX, records.into_values())
    }

    pub fn perform_action(&mut self, action_id: &str) -> Option<DataStatusActionIntent> {
        match parse_status_action_id(action_id)? {
            UiStatusActionCommand::Hush(status_id) => {
                if !self.model.records.contains_key(&status_id) {
                    return None;
                }
                let changed = Arc::make_mut(&mut self.model).hushed_ids.insert(status_id);
                if changed {
                    self.note_change();
                }
                Some(DataStatusActionIntent::ProjectionChanged)
            }
            UiStatusActionCommand::Unhush(status_id) => {
                if !self.model.records.contains_key(&status_id) {
                    return None;
                }
                let changed = Arc::make_mut(&mut self.model).hushed_ids.remove(&status_id);
                if changed {
                    self.note_change();
                }
                Some(DataStatusActionIntent::ProjectionChanged)
            }
            UiStatusActionCommand::ReloadApplication => self
                .model
                .records
                .values()
                .flat_map(|record| &record.actions)
                .any(|action| action.id == action_id && action.enabled)
                .then_some(DataStatusActionIntent::ReloadApplication),
        }
    }

    pub(crate) fn action_is_enabled(&self, action_id: &str) -> bool {
        self.model
            .records
            .values()
            .flat_map(|record| &record.actions)
            .any(|action| action.id == action_id && action.enabled)
    }

    pub fn project_state(&mut self) -> DataStatusStateProjection {
        if let Some(cache) = self.state_cache.as_ref() {
            if cache.revision == self.model.revision {
                return DataStatusStateProjection {
                    state: cache.state.clone(),
                    rebuilt: false,
                };
            }
        }
        let state = project_data_status_state(&self.model.records, &self.model.hushed_ids);
        self.state_cache = Some(DataStatusStateCache {
            revision: self.model.revision,
            state: state.clone(),
        });
        DataStatusStateProjection {
            state,
            rebuilt: true,
        }
    }

    pub fn project_page(&mut self, input: DataStatusPageInput) -> DataStatusPageProjection {
        if let Some(cache) = self.page_cache.as_ref() {
            if cache.revision == self.model.revision && cache.input == input {
                return DataStatusPageProjection {
                    state: cache.state.clone(),
                    rebuilt: false,
                };
            }
        }
        let state = project_data_status_page_state(&self.model.records, &input);
        if self
            .page_cache
            .as_ref()
            .is_none_or(|cache| cache.state != state)
        {
            self.page_projection_revision = self.page_projection_revision.saturating_add(1);
        }
        self.page_cache = Some(DataStatusPageCache {
            revision: self.model.revision,
            input,
            state: state.clone(),
        });
        DataStatusPageProjection {
            state,
            rebuilt: true,
        }
    }

    #[cfg(test)]
    pub fn records(&self) -> &BTreeMap<String, DataStatusRecord> {
        &self.model.records
    }

    fn note_change(&mut self) {
        let model = Arc::make_mut(&mut self.model);
        model.revision = model.revision.saturating_add(1);
    }
}

fn project_data_status_page_state(
    records: &BTreeMap<String, DataStatusRecord>,
    input: &DataStatusPageInput,
) -> UiDataStatusPageState {
    let mut rows = Vec::new();
    if let Some(data_sources) = input.data_sources.as_ref() {
        rows.push(data_sources_status_page_row(data_sources));
    }
    rows.extend([
        client_build_status_page_row(input.client_build.as_ref(), input),
        publication_status_page_row(input),
        expected_contract_versions_status_page_row(),
        nav_db_status_page_row(input),
        cycle_package_group_status_page_row(
            input,
            "cycle:charts",
            "Charts",
            "charts",
            &[
                ("sec", "Sectional", 10),
                ("tac", "TAC", 20),
                ("enr-l", "IFR-L", 30),
                ("enr-h", "IFR-H", 40),
            ],
        ),
        cycle_package_group_status_page_row(
            input,
            "cycle:airport_docs",
            "Airport docs",
            "airport docs",
            &[("tpp", "TPP", 10), ("csup", "CSup", 20)],
        ),
        static_package_group_status_page_row(
            input,
            "static:base_data",
            "Static data",
            &[
                ("terrain", "Terrain", 10),
                ("shaded-relief", "Shaded relief", 20),
                ("world-basemap", "World basemap", 30),
                ("geo", "Geodesy", 40),
            ],
        ),
        live_feed_connection_status_page_row(input),
    ]);
    let mut live_feed_policies = LIVE_FEED_PRODUCT_POLICIES.iter().collect::<Vec<_>>();
    live_feed_policies.sort_by_key(|policy| policy.status_order);
    rows.extend(
        live_feed_policies
            .into_iter()
            .map(|policy| match policy.product_id {
                "nexrad" => nexrad_live_feed_status_page_row(input, policy),
                "winds-aloft" => winds_aloft_status_page_row(
                    &input.forecast,
                    records.get(WINDS_ALOFT_UNAVAILABLE_STATUS_ID),
                    input,
                ),
                _ => live_feed_product_status_page_row(input, policy),
            }),
    );
    if let Some(cloud) = input.cloud.as_ref() {
        rows.insert(3, cloud_status_page_row(cloud, input));
    }
    rows.extend(package_warning_status_page_rows(records));
    UiDataStatusPageState {
        title: "Status".to_string(),
        summary: data_status_page_summary(&rows),
        rows,
    }
}

fn data_sources_status_page_row(input: &DataStatusSourcesInput) -> UiDataStatusPageRow {
    let mut facts = vec![
        UiDataStatusPageFact {
            label: "Cycle Data".to_string(),
            value: input.cycle_data_base_url.clone(),
            action_id: None,
            link_url: Some(input.cycle_data_base_url.clone()),
            relative_value: None,
        },
        UiDataStatusPageFact {
            label: "Live Feeds".to_string(),
            value: input.live_feeds_base_url.clone(),
            action_id: None,
            link_url: Some(input.live_feeds_base_url.clone()),
            relative_value: None,
        },
    ];
    if let Some(url) = input.debug_log_sink_url.as_ref() {
        facts.push(UiDataStatusPageFact {
            label: "Debug log sink".to_string(),
            value: url.clone(),
            action_id: None,
            link_url: Some(url.clone()),
            relative_value: None,
        });
    }
    UiDataStatusPageRow {
        id: "data_sources".to_string(),
        label: "Data Sources".to_string(),
        value: "Config".to_string(),
        severity: UiStatusSeverity::Info,
        detail: "Base URLs used for remote aviation data.".to_string(),
        facts,
    }
}

fn cloud_status_page_row(
    summary: &CloudStatusSummary,
    input: &DataStatusPageInput,
) -> UiDataStatusPageRow {
    status_page_row(
        "cloud:status",
        "Cloud",
        summary.label.clone(),
        summary.severity,
        summary.detail.clone(),
        summary
            .facts
            .iter()
            .map(|fact| {
                fact.time_epoch_ms.map_or_else(
                    || status_fact(fact.label.clone(), fact.value.clone()),
                    |epoch_ms| {
                        status_time_fact(
                            fact.label.clone(),
                            utc_from_epoch_ms(epoch_ms),
                            RelativeTimeStyle::Ago,
                            input,
                        )
                    },
                )
            })
            .collect(),
    )
}

fn package_warning_status_page_rows(
    records: &BTreeMap<String, DataStatusRecord>,
) -> Vec<UiDataStatusPageRow> {
    let mut records = records
        .values()
        .filter(|record| record.id.starts_with(PACKAGE_WARNING_STATUS_PREFIX))
        .cloned()
        .collect::<Vec<_>>();
    records.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then_with(|| left.id.cmp(&right.id))
    });
    records
        .into_iter()
        .map(|record| {
            status_page_row(
                record.id,
                record.label,
                record.value.unwrap_or_else(|| "WARNING".to_string()),
                record.severity,
                record.detail,
                Vec::new(),
            )
        })
        .collect()
}

fn nav_db_family_warning_status_record(
    family: &DataStatusNavDbFamilyRecord,
) -> Option<DataStatusRecord> {
    if !matches!(
        family.id.as_str(),
        "sec" | "tac" | "flyway" | "enr-l" | "enr-h" | "tpp" | "csup"
    ) {
        return None;
    }
    let warning_id = format!("family:{}", family.id);
    if let Some(warning) = family.ui_warning.as_ref() {
        return Some(DataStatusRecord::new(
            format!("{PACKAGE_WARNING_STATUS_PREFIX}{warning_id}"),
            if warning.label.is_empty() {
                family.display_name.clone()
            } else {
                warning.label.clone()
            },
            warning.value.clone(),
            warning.severity,
            matches!(
                warning.severity,
                UiStatusSeverity::Caution
                    | UiStatusSeverity::Warning
                    | UiStatusSeverity::Unavailable
            ),
            warning.detail.clone(),
        ));
    }
    family.warning_text.as_ref().map(|warning_text| {
        DataStatusRecord::new(
            format!("{PACKAGE_WARNING_STATUS_PREFIX}{warning_id}"),
            package_warning_label(&family.id),
            Some("WARNING".to_string()),
            UiStatusSeverity::Warning,
            true,
            warning_text.clone(),
        )
    })
}

fn data_status_page_summary(rows: &[UiDataStatusPageRow]) -> String {
    let warnings = rows
        .iter()
        .filter(|row| matches!(row.severity, UiStatusSeverity::Warning))
        .count();
    let unavailable = rows
        .iter()
        .filter(|row| matches!(row.severity, UiStatusSeverity::Unavailable))
        .count();
    let cautions = rows
        .iter()
        .filter(|row| matches!(row.severity, UiStatusSeverity::Caution))
        .count();
    if warnings + unavailable + cautions == 0 {
        return "All tracked systems are usable.".to_string();
    }
    let mut parts = Vec::new();
    if warnings > 0 {
        parts.push(format!("{warnings} warning{}", plural_s(warnings)));
    }
    if cautions > 0 {
        parts.push(format!("{cautions} caution{}", plural_s(cautions)));
    }
    if unavailable > 0 {
        parts.push(format!(
            "{unavailable} unavailable source{}",
            plural_s(unavailable)
        ));
    }
    parts.join(", ")
}

fn client_build_status_page_row(
    build: Option<&ClientBuildInfo>,
    input: &DataStatusPageInput,
) -> UiDataStatusPageRow {
    let Some(build) = build else {
        return status_page_row(
            "client",
            "Client",
            "UNKNOWN",
            UiStatusSeverity::Info,
            "Client build identity was not provided by this platform.",
            Vec::new(),
        );
    };

    let mut facts = vec![status_fact("Platform", build.platform.clone())];
    if let Some(built_at_utc) = build.built_at_utc.as_deref() {
        if let Some(instant) = parse_utc_instant(built_at_utc) {
            facts.push(status_time_fact(
                "Built",
                instant,
                RelativeTimeStyle::Ago,
                input,
            ));
        } else {
            facts.push(status_fact("Built", built_at_utc.to_string()));
        }
    }
    if let Some(commit) = build.commit.as_deref().filter(|commit| !commit.is_empty()) {
        facts.push(status_fact("Commit", commit.to_string()));
    }
    if build.dirty {
        facts.push(status_fact("Worktree", "dirty"));
    }
    let detail = if build.dirty {
        format!(
            "Running the {} client build {} from a dirty worktree.",
            build.platform, build.version
        )
    } else {
        format!(
            "Running the {} client build {}.",
            build.platform, build.version
        )
    };
    status_page_row(
        "client",
        "Client",
        build.version.clone(),
        UiStatusSeverity::Ok,
        detail,
        facts,
    )
}

fn expected_contract_versions_status_page_row() -> UiDataStatusPageRow {
    let facts = product_contracts::PRODUCT_CONTRACTS
        .iter()
        .map(|contract| {
            status_fact(
                package_warning_label(contract.family_id),
                contract.contract_id,
            )
        })
        .collect::<Vec<_>>();
    status_page_row(
        "contracts:expected",
        "Contract versions",
        product_contracts::PRODUCT_CONTRACTS.len().to_string(),
        UiStatusSeverity::Ok,
        "Core will only accept packages that match these product contract ids.",
        facts,
    )
}

fn publication_status_page_row(input: &DataStatusPageInput) -> UiDataStatusPageRow {
    let publication = input.publication.as_ref();
    let Some(publication) = publication else {
        return status_page_row(
            "publication:current_artifacts",
            "Package library",
            "NEVER",
            UiStatusSeverity::Info,
            "current_artifacts.json has not been checked in this session.",
            Vec::new(),
        );
    };
    let mut facts = vec![
        status_fact("Bundles", publication.bundle_count.to_string()),
        status_fact(
            "Loaded manifests",
            publication.loaded_manifest_count.to_string(),
        ),
    ];
    if let Some(as_of) = publication.as_of_utc.as_deref().and_then(parse_utc_instant) {
        facts.push(status_time_fact(
            "Published",
            as_of,
            RelativeTimeStyle::Ago,
            input,
        ));
    }
    if let Some(checked_at) = publication.checked_epoch_ms {
        let checked_utc = utc_from_epoch_ms(checked_at);
        facts.push(status_time_fact(
            "Checked",
            checked_utc,
            RelativeTimeStyle::Ago,
            input,
        ));
        return status_page_row(
            "publication:current_artifacts",
            "Package library",
            "OK",
            UiStatusSeverity::Ok,
            format!(
                "current_artifacts.json checked at {}.",
                format_status_time(checked_utc, input),
            ),
            facts,
        );
    }
    status_page_row(
        "publication:current_artifacts",
        "Package library",
        "LOADED",
        UiStatusSeverity::Ok,
        "current_artifacts.json is loaded; check time was not recorded.",
        facts,
    )
}

fn live_feed_connection_status_page_row(input: &DataStatusPageInput) -> UiDataStatusPageRow {
    let connection = &input.live_feed_connection;
    let mut facts = Vec::new();
    if let Some(source_url) = connection.source_url.as_deref() {
        facts.push(status_link_fact(
            "Server",
            source_url,
            connection.status_url.as_deref().unwrap_or(source_url),
        ));
    }
    if let Some(last_heard) = connection.last_heard_epoch_ms {
        facts.push(status_time_fact(
            "Last server event",
            utc_from_epoch_ms(last_heard),
            RelativeTimeStyle::Ago,
            input,
        ));
    }
    if let Some(status) = connection.network_status {
        facts.push(status_fact(
            "Network",
            live_feed_network_status_label(status),
        ));
    }
    let last_error_epoch = connection
        .last_resource_error_epoch_ms
        .or(connection.last_error_epoch_ms);
    let network_issue = live_feed_network_status_issue(connection.network_status);
    let last_error_message = connection
        .last_resource_error_message
        .as_deref()
        .or_else(|| {
            matches!(connection.mode, DataStatusLiveFeedConnectionMode::Error)
                .then_some(network_issue)
                .flatten()
        })
        .or(connection.last_error_message.as_deref());
    if let Some(last_error) = last_error_epoch {
        facts.push(status_time_fact(
            "Last error",
            utc_from_epoch_ms(last_error),
            RelativeTimeStyle::Ago,
            input,
        ));
    }
    if let Some(message) = last_error_message {
        facts.push(status_fact("Error", message.to_string()));
    }
    let (value, severity, detail) = if let Some(message) =
        connection.last_resource_error_message.as_ref()
    {
        (
            "ERROR",
            UiStatusSeverity::Unavailable,
            match connection.mode {
                DataStatusLiveFeedConnectionMode::Connected => format!(
                    "The live-feed event stream is connected, but live-feed data is unavailable: {message}"
                ),
                DataStatusLiveFeedConnectionMode::Closed => {
                    format!("The live-feed event stream is closed. Last live-feed error: {message}")
                }
                _ => format!("Live-feed data is unavailable: {message}"),
            },
        )
    } else if let Some(message) = network_issue {
        (
            match connection.network_status {
                Some(crate::LiveFeedNetworkStatus::Metered) => "METERED",
                Some(crate::LiveFeedNetworkStatus::NoActiveNetwork) => "NO NETWORK",
                _ => "NETWORK",
            },
            UiStatusSeverity::Unavailable,
            message.to_string(),
        )
    } else {
        match connection.mode {
            DataStatusLiveFeedConnectionMode::Unknown => (
                "UNKNOWN",
                UiStatusSeverity::Info,
                "No live-feed connection state has been reported.".to_string(),
            ),
            DataStatusLiveFeedConnectionMode::Connecting => (
                "CONNECTING",
                UiStatusSeverity::Info,
                "The live-feed event stream is connecting.".to_string(),
            ),
            DataStatusLiveFeedConnectionMode::Connected => {
                let heard = connection
                    .last_heard_epoch_ms
                    .map(|epoch| {
                        format!(
                            " Last server event was at {}.",
                            format_status_time(utc_from_epoch_ms(epoch), input),
                        )
                    })
                    .unwrap_or_default();
                (
                    "CONNECTED",
                    UiStatusSeverity::Ok,
                    format!("The live-feed event stream is connected.{heard}"),
                )
            }
            DataStatusLiveFeedConnectionMode::Error => (
                "ERROR",
                UiStatusSeverity::Unavailable,
                connection
                    .last_error_message
                    .as_ref()
                    .map(|message| {
                        format!("The live-feed event stream reported an error: {message}.")
                    })
                    .unwrap_or_else(|| "The live-feed event stream reported an error.".to_string()),
            ),
            DataStatusLiveFeedConnectionMode::Closed => (
                "CLOSED",
                UiStatusSeverity::Unavailable,
                connection
                    .last_error_message
                    .as_ref()
                    .map(|message| {
                        format!(
                            "The live-feed event stream is closed. Last live-feed error: {message}"
                        )
                    })
                    .unwrap_or_else(|| "The live-feed event stream is closed.".to_string()),
            ),
        }
    };
    status_page_row(
        "live_feed:connection",
        "Live-feed connection",
        value,
        severity,
        detail,
        facts,
    )
}

fn live_feed_network_status_label(status: crate::LiveFeedNetworkStatus) -> &'static str {
    match status {
        crate::LiveFeedNetworkStatus::Unmetered => "Unmetered",
        crate::LiveFeedNetworkStatus::Metered => "Metered",
        crate::LiveFeedNetworkStatus::NoActiveNetwork => "No active network",
        crate::LiveFeedNetworkStatus::Unknown => "Unknown",
    }
}

fn live_feed_network_status_issue(
    status: Option<crate::LiveFeedNetworkStatus>,
) -> Option<&'static str> {
    match status {
        Some(crate::LiveFeedNetworkStatus::Metered) => Some(
            "The active network is metered. Live feeds are allowed, but this network condition can explain live-feed connectivity failures.",
        ),
        Some(crate::LiveFeedNetworkStatus::NoActiveNetwork) => {
            Some("Android reports no active network for live feeds.")
        }
        _ => None,
    }
}

fn live_feed_product_status_page_row(
    input: &DataStatusPageInput,
    product: &LiveFeedProductPolicy,
) -> UiDataStatusPageRow {
    let product_key = product.product_id;
    let label = product.display_name;
    let source = input
        .live_feed_products
        .get(product_key)
        .cloned()
        .unwrap_or_default();
    let mut facts = Vec::new();
    if let Some(version) = source.loaded_version {
        facts.push(status_fact("Version", version));
    }
    if let Some(collected) = source.collected_utc {
        facts.push(status_time_fact(
            "Collected At",
            collected,
            RelativeTimeStyle::Old,
            input,
        ));
    }
    if !source.loaded {
        let detail = if source.index_loaded {
            if source.listed {
                format!("{label} is listed in the live-feed index but no current state is loaded.")
            } else {
                format!("{label} is not listed in the live-feed index.")
            }
        } else {
            "The live-feed index has not loaded.".to_string()
        };
        return status_page_row(
            format!("live_feed:{product_key}"),
            label,
            "MISSING",
            UiStatusSeverity::Unavailable,
            detail,
            facts,
        );
    }
    let Some(collected_utc) = source.collected_utc else {
        return status_page_row(
            format!("live_feed:{product_key}"),
            label,
            "CACHED",
            UiStatusSeverity::Info,
            format!("Cached {label} live-feed data is available, but source timestamp is unknown."),
            facts,
        );
    };
    if let Some(violation) = evaluate_age(
        required_live_feed_age_policy(product_key),
        collected_utc,
        utc_from_epoch_ms(input.now_epoch_ms),
    ) {
        return status_page_row(
            format!("live_feed:{product_key}"),
            label,
            "OLD",
            match violation.severity {
                FreshnessSeverity::Info => UiStatusSeverity::Info,
                FreshnessSeverity::Warning => UiStatusSeverity::Warning,
            },
            format!("{label} data is {} old.", format_age(violation.age_ms)),
            facts,
        );
    }
    status_page_row(
        format!("live_feed:{product_key}"),
        label,
        "OK",
        UiStatusSeverity::Ok,
        format!("{label} is loaded."),
        facts,
    )
}

fn winds_aloft_status_page_row(
    forecast: &DataStatusForecastInput,
    unavailable: Option<&DataStatusRecord>,
    input: &DataStatusPageInput,
) -> UiDataStatusPageRow {
    let mut facts = Vec::new();
    if let Some(version) = forecast.version_label.as_ref() {
        facts.push(status_fact("Version", version.clone()));
    }
    if let Some(model) = forecast.model_id.as_ref() {
        facts.push(status_fact("Model", model.clone()));
    }
    if let Some(cycle) = forecast
        .cycle_time_epoch_ms
        .and_then(DateTime::<Utc>::from_timestamp_millis)
    {
        facts.push(status_time_fact(
            "Model Cycle",
            cycle,
            RelativeTimeStyle::Old,
            input,
        ));
    }
    if let Some(valid_through) = forecast
        .valid_through_epoch_ms
        .and_then(DateTime::<Utc>::from_timestamp_millis)
    {
        facts.push(status_time_fact(
            "Valid Through",
            valid_through,
            RelativeTimeStyle::Until,
            input,
        ));
    }
    if let Some(unavailable) = unavailable {
        return status_page_row(
            "live_feed:winds-aloft",
            "Winds aloft",
            unavailable
                .value
                .clone()
                .unwrap_or_else(|| "UNAVAIL".to_string()),
            unavailable.severity,
            unavailable.detail.clone(),
            facts,
        );
    }
    if forecast.loaded {
        return status_page_row(
            "live_feed:winds-aloft",
            "Winds aloft",
            "OK",
            UiStatusSeverity::Ok,
            "NOAA forecast atmosphere is loaded.",
            facts,
        );
    }
    let detail = if forecast.index_loaded {
        if forecast.listed {
            "Winds aloft is listed in the live-feed index but its current NavKv state is not loaded."
        } else {
            "Winds aloft is not listed in the live-feed index."
        }
    } else {
        "The live-feed index has not loaded."
    };
    status_page_row(
        "live_feed:winds-aloft",
        "Winds aloft",
        "MISSING",
        UiStatusSeverity::Unavailable,
        detail,
        facts,
    )
}

fn nexrad_live_feed_status_page_row(
    input: &DataStatusPageInput,
    policy: &LiveFeedProductPolicy,
) -> UiDataStatusPageRow {
    let mut row = live_feed_product_status_page_row(input, policy);
    row.facts.push(status_fact(
        "Frames",
        input.nexrad_frame_age_summary.clone(),
    ));
    row
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CycleWindow {
    effective: Option<DateTime<Utc>>,
    expiration: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ChartValidityViolationKind {
    NotYetEffective,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChartValidityViolation {
    family_label: &'static str,
    family_sort_key: u8,
    kind: ChartValidityViolationKind,
}

pub(crate) fn collect_chart_validity_violations(
    violations: &mut Vec<ChartValidityViolation>,
    seen: &mut BTreeSet<(u8, ChartValidityViolationKind)>,
    family_label: &'static str,
    family_sort_key: u8,
    effective_date: Option<&str>,
    expiration_date: Option<&str>,
    now_utc: DateTime<Utc>,
) -> Option<i64> {
    let mut next_check_epoch_ms = None;
    if let Some(effective_utc) = effective_date.and_then(parse_utc_instant) {
        if now_utc < effective_utc {
            push_chart_validity_violation(
                violations,
                seen,
                family_label,
                family_sort_key,
                ChartValidityViolationKind::NotYetEffective,
            );
            next_check_epoch_ms = Some(effective_utc.timestamp_millis());
        }
    }
    if let Some(expiration_utc) = expiration_date.and_then(parse_utc_instant) {
        if cycle_product_is_expired(expiration_utc, now_utc) {
            push_chart_validity_violation(
                violations,
                seen,
                family_label,
                family_sort_key,
                ChartValidityViolationKind::Expired,
            );
        } else {
            next_check_epoch_ms = Some(
                next_check_epoch_ms
                    .map(|current| current.min(expiration_utc.timestamp_millis()))
                    .unwrap_or_else(|| expiration_utc.timestamp_millis()),
            );
        }
    }
    next_check_epoch_ms
}

fn push_chart_validity_violation(
    violations: &mut Vec<ChartValidityViolation>,
    seen: &mut BTreeSet<(u8, ChartValidityViolationKind)>,
    family_label: &'static str,
    family_sort_key: u8,
    kind: ChartValidityViolationKind,
) {
    if seen.insert((family_sort_key, kind)) {
        violations.push(ChartValidityViolation {
            family_label,
            family_sort_key,
            kind,
        });
    }
}

pub(crate) fn chart_validity_value(violations: &[ChartValidityViolation]) -> &'static str {
    let kinds = violations
        .iter()
        .map(|violation| violation.kind)
        .collect::<BTreeSet<_>>();
    if kinds.len() > 1 {
        return "INVALID";
    }
    match kinds.first() {
        Some(ChartValidityViolationKind::Expired) => "EXPIRED",
        Some(ChartValidityViolationKind::NotYetEffective) => "EARLY",
        None => "INVALID",
    }
}

pub(crate) fn chart_validity_detail(violations: &[ChartValidityViolation]) -> String {
    let family_list = violations
        .iter()
        .map(|violation| (violation.family_sort_key, violation.family_label))
        .collect::<BTreeSet<_>>();
    format!(
        "{} charts {}.",
        status_family_list(family_list),
        chart_validity_condition(violations)
    )
}

fn chart_validity_condition(violations: &[ChartValidityViolation]) -> &'static str {
    let kinds = violations
        .iter()
        .map(|violation| violation.kind)
        .collect::<BTreeSet<_>>();
    if kinds.len() > 1 {
        return "not valid";
    }
    match kinds.first() {
        Some(ChartValidityViolationKind::Expired) => "expired",
        Some(ChartValidityViolationKind::NotYetEffective) => "not valid yet",
        None => "not valid",
    }
}

fn cycle_package_group_status_page_row(
    input: &DataStatusPageInput,
    id: &str,
    label: &str,
    noun: &str,
    families: &[(&'static str, &'static str, u8)],
) -> UiDataStatusPageRow {
    let packages = package_group_packages(input, families);
    if packages.is_empty() {
        return status_page_row(
            id,
            label,
            "MISSING",
            if input.nav_db_available {
                UiStatusSeverity::Info
            } else {
                UiStatusSeverity::Unavailable
            },
            format!("No {noun} package rows are present in the attached nav-db."),
            Vec::new(),
        );
    }

    let now_utc = utc_from_epoch_ms(input.now_epoch_ms);
    let mut seen_violations = BTreeSet::new();
    let mut violations = Vec::new();
    let mut family_set = BTreeSet::new();
    let mut earliest_expiration: Option<DateTime<Utc>> = None;
    let mut latest_effective: Option<DateTime<Utc>> = None;
    let mut missing_expiration_families = BTreeSet::new();
    let mut packages_by_family: BTreeMap<u8, (&'static str, Vec<&DataStatusNavDbPackageRecord>)> =
        BTreeMap::new();

    for package in &packages {
        let Some((_, family_label, family_sort_key)) = family_spec_for_package(families, package)
        else {
            continue;
        };
        family_set.insert((family_sort_key, family_label));
        packages_by_family
            .entry(family_sort_key)
            .or_insert_with(|| (family_label, Vec::new()))
            .1
            .push(package);
    }

    for (family_sort_key, (family_label, family_packages)) in &packages_by_family {
        let current_packages = family_packages
            .iter()
            .copied()
            .filter(|package| cycle_package_is_currently_valid(package, now_utc))
            .collect::<Vec<_>>();
        if current_packages.is_empty() {
            for package in family_packages {
                collect_chart_validity_violations(
                    &mut violations,
                    &mut seen_violations,
                    family_label,
                    *family_sort_key,
                    package.effective_date.as_deref(),
                    package.expiration_date.as_deref(),
                    now_utc,
                );
            }
            continue;
        }
        let mut family_has_expiration = false;
        for package in current_packages {
            if let Some(effective_utc) = package
                .effective_date
                .as_deref()
                .and_then(parse_utc_instant)
            {
                latest_effective = Some(
                    latest_effective
                        .map(|current| current.max(effective_utc))
                        .unwrap_or(effective_utc),
                );
            }
            if let Some(expiration_utc) = package
                .expiration_date
                .as_deref()
                .and_then(parse_utc_instant)
            {
                family_has_expiration = true;
                earliest_expiration = Some(
                    earliest_expiration
                        .map(|current| current.min(expiration_utc))
                        .unwrap_or(expiration_utc),
                );
            }
        }
        if !family_has_expiration {
            missing_expiration_families.insert((*family_sort_key, *family_label));
        }
    }

    let family_list = status_family_list(family_set.iter().copied());
    if !violations.is_empty() {
        return status_page_row(
            id,
            label,
            chart_validity_value(&violations),
            UiStatusSeverity::Warning,
            format!(
                "{family_list} {noun} {}.",
                chart_validity_condition(&violations)
            ),
            vec![status_fact("Products", family_list)],
        );
    }

    let mut facts = vec![
        status_fact("Products", family_list.clone()),
        status_fact("Packages", packages.len().to_string()),
    ];
    if let Some(effective) = latest_effective {
        facts.push(status_time_fact(
            "Effective",
            effective,
            RelativeTimeStyle::Ago,
            input,
        ));
    }
    let next_cycle_window = next_published_cycle_window_for_families(input, families, now_utc)
        .or_else(|| next_cycle_window_for_package_groups(&packages_by_family, now_utc));
    if let Some(expiration) = earliest_expiration {
        facts.push(status_time_fact(
            "Expires",
            expiration,
            RelativeTimeStyle::Until,
            input,
        ));
        push_next_cycle_window_facts(&mut facts, next_cycle_window, input);
        return status_page_row(
            id,
            label,
            "OK",
            UiStatusSeverity::Ok,
            format!(
                "{family_list} {noun} valid until {}.",
                format_status_time(expiration, input),
            ),
            facts,
        );
    }
    if !missing_expiration_families.is_empty() {
        facts.push(status_fact(
            "Missing dates",
            status_family_list(missing_expiration_families.iter().copied()),
        ));
    }
    push_next_cycle_window_facts(&mut facts, next_cycle_window, input);
    status_page_row(
        id,
        label,
        "UNKNOWN",
        UiStatusSeverity::Info,
        format!("{family_list} {noun} validity metadata is not available."),
        facts,
    )
}

fn next_published_cycle_window_for_families(
    input: &DataStatusPageInput,
    families: &[(&'static str, &'static str, u8)],
    now_utc: DateTime<Utc>,
) -> Option<CycleWindow> {
    let mut latest_effective: Option<DateTime<Utc>> = None;
    let mut earliest_expiration: Option<DateTime<Utc>> = None;
    for (family_id, _, _) in families {
        let Some((package, effective)) = input
            .published_packages
            .iter()
            .filter(|package| {
                package.family_id == *family_id
                    && crate::package_management::package_contract_is_supported(package)
            })
            .filter_map(|package| {
                let effective = package
                    .effective_date
                    .as_deref()
                    .and_then(parse_utc_instant)?;
                (now_utc < effective).then_some((package, effective))
            })
            .min_by(
                |(left_package, left_effective), (right_package, right_effective)| {
                    left_effective
                        .cmp(right_effective)
                        .then_with(|| left_package.id.cmp(&right_package.id))
                },
            )
        else {
            continue;
        };
        latest_effective = Some(
            latest_effective
                .map(|current| current.max(effective))
                .unwrap_or(effective),
        );
        if let Some(expiration) = package
            .expiration_date
            .as_deref()
            .and_then(parse_utc_instant)
        {
            earliest_expiration = Some(
                earliest_expiration
                    .map(|current| current.min(expiration))
                    .unwrap_or(expiration),
            );
        }
    }
    (latest_effective.is_some() || earliest_expiration.is_some()).then_some(CycleWindow {
        effective: latest_effective,
        expiration: earliest_expiration,
    })
}

fn next_cycle_window_for_package_groups(
    packages_by_family: &BTreeMap<u8, (&'static str, Vec<&DataStatusNavDbPackageRecord>)>,
    now_utc: DateTime<Utc>,
) -> Option<CycleWindow> {
    let mut latest_effective: Option<DateTime<Utc>> = None;
    let mut earliest_expiration: Option<DateTime<Utc>> = None;
    for (_, family_packages) in packages_by_family.values() {
        let Some((package, effective)) = family_packages
            .iter()
            .filter_map(|package| {
                let effective = package
                    .effective_date
                    .as_deref()
                    .and_then(parse_utc_instant)?;
                (now_utc < effective).then_some((*package, effective))
            })
            .min_by(|(_, left), (_, right)| left.cmp(right))
        else {
            continue;
        };
        latest_effective = Some(
            latest_effective
                .map(|current| current.max(effective))
                .unwrap_or(effective),
        );
        if let Some(expiration) = package
            .expiration_date
            .as_deref()
            .and_then(parse_utc_instant)
        {
            earliest_expiration = Some(
                earliest_expiration
                    .map(|current| current.min(expiration))
                    .unwrap_or(expiration),
            );
        }
    }
    (latest_effective.is_some() || earliest_expiration.is_some()).then_some(CycleWindow {
        effective: latest_effective,
        expiration: earliest_expiration,
    })
}

fn push_next_cycle_window_facts(
    facts: &mut Vec<UiDataStatusPageFact>,
    window: Option<CycleWindow>,
    input: &DataStatusPageInput,
) {
    let Some(window) = window else {
        return;
    };
    if let Some(effective) = window.effective {
        facts.push(status_time_fact(
            "Next effective",
            effective,
            RelativeTimeStyle::Until,
            input,
        ));
    }
    if let Some(expiration) = window.expiration {
        facts.push(status_time_fact(
            "Next expires",
            expiration,
            RelativeTimeStyle::Until,
            input,
        ));
    }
}

fn cycle_package_is_currently_valid(
    package: &DataStatusNavDbPackageRecord,
    now_utc: DateTime<Utc>,
) -> bool {
    if package
        .effective_date
        .as_deref()
        .and_then(parse_utc_instant)
        .is_some_and(|effective| now_utc < effective)
    {
        return false;
    }
    !package
        .expiration_date
        .as_deref()
        .and_then(parse_utc_instant)
        .is_some_and(|expiration| cycle_product_is_expired(expiration, now_utc))
}

fn static_package_group_status_page_row(
    input: &DataStatusPageInput,
    id: &str,
    label: &str,
    families: &[(&'static str, &'static str, u8)],
) -> UiDataStatusPageRow {
    let packages = package_group_packages(input, families);
    if packages.is_empty() {
        return status_page_row(
            id,
            label,
            "MISSING",
            if input.nav_db_available {
                UiStatusSeverity::Info
            } else {
                UiStatusSeverity::Unavailable
            },
            "No static package rows are present in the attached nav-db.",
            Vec::new(),
        );
    }
    let mut newest_by_family: BTreeMap<u8, (&'static str, DateTime<Utc>)> = BTreeMap::new();
    let mut family_set = BTreeSet::new();
    for package in &packages {
        let Some((_, family_label, family_sort_key)) = family_spec_for_package(families, package)
        else {
            continue;
        };
        family_set.insert((family_sort_key, family_label));
        if let Some(effective_utc) = package
            .effective_date
            .as_deref()
            .and_then(parse_utc_instant)
        {
            newest_by_family
                .entry(family_sort_key)
                .and_modify(|(_, current)| *current = (*current).max(effective_utc))
                .or_insert((family_label, effective_utc));
        }
    }
    let family_list = status_family_list(family_set.iter().copied());
    let mut facts = vec![
        status_fact("Products", family_list.clone()),
        status_fact("Packages", packages.len().to_string()),
    ];
    for (family_label, effective_utc) in newest_by_family.values() {
        facts.push(status_time_fact(
            *family_label,
            *effective_utc,
            RelativeTimeStyle::Old,
            input,
        ));
    }
    if newest_by_family.is_empty() {
        return status_page_row(
            id,
            label,
            "LOADED",
            UiStatusSeverity::Info,
            format!("{family_list} packages are loaded, but source age metadata is not available."),
            facts,
        );
    }
    let oldest = newest_by_family
        .values()
        .map(|(_, effective)| *effective)
        .min()
        .unwrap_or_else(|| utc_from_epoch_ms(input.now_epoch_ms));
    status_page_row(
        id,
        label,
        "OK",
        UiStatusSeverity::Ok,
        format!(
            "{family_list} source data dates back to {}.",
            format_status_time(oldest, input),
        ),
        facts,
    )
}

fn package_group_packages<'a>(
    input: &'a DataStatusPageInput,
    families: &[(&'static str, &'static str, u8)],
) -> Vec<&'a DataStatusNavDbPackageRecord> {
    let family_ids = families
        .iter()
        .map(|(family_id, _, _)| *family_id)
        .collect::<BTreeSet<_>>();
    input
        .nav_db_packages
        .iter()
        .filter(|package| family_ids.contains(package.family_id.as_str()))
        .collect()
}

fn family_spec_for_package(
    families: &[(&'static str, &'static str, u8)],
    package: &DataStatusNavDbPackageRecord,
) -> Option<(&'static str, &'static str, u8)> {
    families
        .iter()
        .find(|(family_id, _, _)| *family_id == package.family_id.as_str())
        .copied()
}

fn nav_db_status_page_row(input: &DataStatusPageInput) -> UiDataStatusPageRow {
    let Some(artifact) = input.active_nav_db.as_ref() else {
        return status_page_row(
            "nav_db",
            "NAV DB",
            "MISSING",
            UiStatusSeverity::Unavailable,
            "No nav-db package is attached.",
            Vec::new(),
        );
    };
    let now_utc = utc_from_epoch_ms(input.now_epoch_ms);
    let latest_effective = artifact
        .effective_date
        .as_deref()
        .and_then(parse_utc_instant);
    let earliest_expiration = artifact
        .expiration_date
        .as_deref()
        .and_then(parse_utc_instant);
    let not_yet_effective = latest_effective.is_some_and(|effective| now_utc < effective);
    let expired =
        earliest_expiration.is_some_and(|expiration| cycle_product_is_expired(expiration, now_utc));
    let mut facts = vec![
        status_fact("Package", artifact.package_id.clone()),
        status_fact("File", artifact.filename.clone()),
    ];
    if let Some(cycle) = artifact.cycle.as_ref() {
        facts.push(status_fact("Cycle", cycle.clone()));
    }
    if let Some(version) = artifact.cycle_version.as_ref() {
        facts.push(status_fact("Cycle version", version.clone()));
    }
    if let Some(contract) = artifact.contract_id.as_ref() {
        facts.push(status_fact("Contract", contract.clone()));
    }
    if let Some(effective) = latest_effective {
        facts.push(status_time_fact(
            "Effective",
            effective,
            RelativeTimeStyle::Ago,
            input,
        ));
    }
    if let Some(expiration) = earliest_expiration {
        facts.push(status_time_fact(
            "Expires",
            expiration,
            RelativeTimeStyle::Until,
            input,
        ));
    }
    push_next_cycle_window_facts(&mut facts, next_nav_db_cycle_window(input, now_utc), input);
    if expired || not_yet_effective {
        let (value, condition) = match (expired, not_yet_effective) {
            (true, true) => ("INVALID", "not valid"),
            (true, false) => ("EXPIRED", "expired"),
            (false, true) => ("EARLY", "not valid yet"),
            (false, false) => ("INVALID", "not valid"),
        };
        return status_page_row(
            "nav_db",
            "NAV DB",
            value,
            UiStatusSeverity::Warning,
            format!(
                "Attached nav-db package {} is {condition}.",
                artifact.package_id
            ),
            facts,
        );
    }
    if let Some(earliest_expiration) = earliest_expiration {
        return status_page_row(
            "nav_db",
            "NAV DB",
            "OK",
            UiStatusSeverity::Ok,
            format!(
                "NAV DB valid until {}.",
                format_status_time(earliest_expiration, input),
            ),
            facts,
        );
    }
    status_page_row(
        "nav_db",
        "NAV DB",
        "UNKNOWN",
        UiStatusSeverity::Info,
        "NAV DB package metadata does not include an expiration date.",
        facts,
    )
}

fn next_nav_db_cycle_window(
    input: &DataStatusPageInput,
    now_utc: DateTime<Utc>,
) -> Option<CycleWindow> {
    let mut candidates = input
        .published_packages
        .iter()
        .filter(|package| {
            package.family_id == "nav-db"
                && crate::package_management::package_contract_is_supported(package)
        })
        .filter_map(|package| {
            let effective = package
                .effective_date
                .as_deref()
                .and_then(parse_utc_instant)?;
            (now_utc < effective).then_some((package, effective))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(
        |(left_package, left_effective), (right_package, right_effective)| {
            left_effective
                .cmp(right_effective)
                .then_with(|| left_package.id.cmp(&right_package.id))
        },
    );
    let (package, effective) = candidates.first().copied()?;
    Some(CycleWindow {
        effective: Some(effective),
        expiration: package
            .expiration_date
            .as_deref()
            .and_then(parse_utc_instant),
    })
}

fn status_page_row(
    id: impl Into<String>,
    label: impl Into<String>,
    value: impl Into<String>,
    severity: UiStatusSeverity,
    detail: impl Into<String>,
    facts: Vec<UiDataStatusPageFact>,
) -> UiDataStatusPageRow {
    UiDataStatusPageRow {
        id: id.into(),
        label: label.into(),
        value: value.into(),
        severity,
        detail: detail.into(),
        facts,
    }
}

fn status_fact(label: impl Into<String>, value: impl Into<String>) -> UiDataStatusPageFact {
    UiDataStatusPageFact {
        label: label.into(),
        value: value.into(),
        action_id: None,
        link_url: None,
        relative_value: None,
    }
}

fn status_link_fact(
    label: impl Into<String>,
    value: impl Into<String>,
    link_url: impl Into<String>,
) -> UiDataStatusPageFact {
    UiDataStatusPageFact {
        label: label.into(),
        value: value.into(),
        action_id: None,
        link_url: Some(link_url.into()),
        relative_value: None,
    }
}

fn status_time_fact(
    label: impl Into<String>,
    instant: DateTime<Utc>,
    display: RelativeTimeStyle,
    input: &DataStatusPageInput,
) -> UiDataStatusPageFact {
    UiDataStatusPageFact {
        label: label.into(),
        value: crate::format_dated_time(
            instant.timestamp_millis(),
            input.time_display_mode,
            input.local_time_zone,
            crate::DatedTimeStyle::IsoMinute,
        ),
        action_id: Some(crate::TOGGLE_TIME_DISPLAY_MODE_ACTION_ID.to_string()),
        link_url: None,
        relative_value: Some(crate::time_display::format_relative_time(
            instant.timestamp_millis(),
            input.now_epoch_ms,
            display,
            false,
        )),
    }
}

fn package_warning_label(family_id: &str) -> String {
    match family_id {
        "nav-db" => "NAV DB".to_string(),
        "sec" => "Sectional".to_string(),
        "tac" => "TAC".to_string(),
        "flyway" => "Flyway".to_string(),
        "enr-l" => "IFR-L".to_string(),
        "enr-h" => "IFR-H".to_string(),
        "tpp" => "TPP".to_string(),
        "csup" => "CSup".to_string(),
        "terrain" => "Terrain".to_string(),
        "shaded-relief" => "Shaded relief".to_string(),
        "world-basemap" => "World basemap".to_string(),
        "geo" => "Geodesy".to_string(),
        other => other.to_ascii_uppercase(),
    }
}

fn status_family_list<'a>(families: impl IntoIterator<Item = (u8, &'a str)>) -> String {
    families
        .into_iter()
        .map(|(_, label)| label)
        .collect::<Vec<_>>()
        .join(", ")
}

fn utc_from_epoch_ms(epoch_ms: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(epoch_ms).unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

fn format_status_time(instant: DateTime<Utc>, input: &DataStatusPageInput) -> String {
    crate::format_dated_time(
        instant.timestamp_millis(),
        input.time_display_mode,
        input.local_time_zone,
        crate::DatedTimeStyle::IsoMinute,
    )
}

fn plural_s(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UiStatusSeverity;

    fn record(id: &str) -> DataStatusRecord {
        DataStatusRecord::new(
            id,
            id,
            Some("WARNING".to_string()),
            UiStatusSeverity::Warning,
            true,
            format!("{id} detail"),
        )
    }

    fn page_input() -> DataStatusPageInput {
        DataStatusPageInput {
            now_epoch_ms: 1_000,
            time_display_mode: crate::TimeDisplayMode::Local,
            local_time_zone: chrono_tz::UTC,
            client_build: None,
            data_sources: None,
            nav_db_available: false,
            active_nav_db: None,
            nav_db_packages: Vec::new(),
            published_packages: Vec::new(),
            publication: None,
            cloud: None,
            live_feed_connection: DataStatusLiveFeedConnectionInput {
                mode: DataStatusLiveFeedConnectionMode::Unknown,
                source_url: None,
                status_url: None,
                last_heard_epoch_ms: None,
                last_error_epoch_ms: None,
                last_error_message: None,
                last_resource_error_epoch_ms: None,
                last_resource_error_message: None,
                network_status: None,
            },
            live_feed_products: BTreeMap::new(),
            forecast: DataStatusForecastInput {
                loaded: false,
                listed: false,
                index_loaded: false,
                version_label: None,
                model_id: None,
                cycle_time_epoch_ms: None,
                valid_through_epoch_ms: None,
            },
            nexrad_frame_age_summary: "---".to_string(),
        }
    }

    #[test]
    fn state_projection_is_revision_cached() {
        let mut controller = DataStatusController::new([record("one")]);
        assert!(controller.project_state().rebuilt);
        assert!(!controller.project_state().rebuilt);
        controller.upsert(record("two"));
        assert!(controller.project_state().rebuilt);
    }

    #[test]
    fn data_sources_row_uses_configured_base_urls() {
        let mut input = page_input();
        input.data_sources = Some(DataStatusSourcesInput {
            cycle_data_base_url: "https://cycle.example/packages".to_string(),
            live_feeds_base_url: "https://feeds.example/live-feeds".to_string(),
            debug_log_sink_url: Some("https://debug.example/logs".to_string()),
        });

        let page = DataStatusController::default().project_page(input).state;
        let row = page.rows.first().expect("data sources row");
        assert_eq!(row.id, "data_sources");
        assert_eq!(row.facts[0].value, "https://cycle.example/packages");
        assert_eq!(row.facts[1].value, "https://feeds.example/live-feeds");
        assert_eq!(
            row.facts[1].link_url.as_deref(),
            Some("https://feeds.example/live-feeds")
        );
        assert_eq!(row.facts[2].value, "https://debug.example/logs");
    }

    #[test]
    fn checkpoint_is_copy_on_write_and_rollback_restores_records() {
        let mut controller = DataStatusController::new([record("one")]);
        let model_address = Arc::as_ptr(&controller.model);
        let checkpoint = controller.checkpoint_model();
        controller.upsert(record("two"));
        assert_ne!(Arc::as_ptr(&controller.model), model_address);
        controller.rollback_model(checkpoint);
        assert_eq!(Arc::as_ptr(&controller.model), model_address);
        assert!(controller.contains("one"));
        assert!(!controller.contains("two"));
    }

    #[test]
    fn replacing_a_prefix_removes_stale_records_and_keeps_other_domains() {
        let mut controller = DataStatusController::new([
            record("package:old"),
            record("package:keep"),
            record("weather:keep"),
        ]);
        assert!(controller.replace_prefix("package:", [record("package:new")]));
        assert!(!controller.contains("package:old"));
        assert!(!controller.contains("package:keep"));
        assert!(controller.contains("package:new"));
        assert!(controller.contains("weather:keep"));
    }

    #[test]
    fn replacing_a_prefix_preserves_hushing_when_a_record_returns() {
        let mut controller = DataStatusController::new([record("package:old")]);
        assert_eq!(
            controller.perform_action("status:hush:package:old"),
            Some(DataStatusActionIntent::ProjectionChanged)
        );

        assert!(controller.replace_prefix("package:", [record("package:new")]));
        assert!(controller.replace_prefix("package:", [record("package:old")]));

        assert!(controller
            .project_state()
            .state
            .boxes
            .iter()
            .find(|box_| box_.id == "package:old")
            .is_some_and(|box_| box_.hushed));
    }

    #[test]
    fn page_projection_is_cached_by_model_and_typed_inputs() {
        let mut controller = DataStatusController::default();
        let input = page_input();
        assert!(controller.project_page(input.clone()).rebuilt);
        let initial_projection_revision = controller.page_projection_revision();
        assert!(!controller.project_page(input.clone()).rebuilt);
        assert!(
            controller
                .project_page(DataStatusPageInput {
                    now_epoch_ms: input.now_epoch_ms + 1,
                    ..input.clone()
                })
                .rebuilt
        );
        assert_eq!(
            controller.page_projection_revision(),
            initial_projection_revision,
        );
        controller.upsert(record("warning"));
        assert!(controller.project_page(input).rebuilt);
        assert_eq!(
            controller.page_projection_revision(),
            initial_projection_revision,
        );
        controller.upsert(record(&format!("{PACKAGE_WARNING_STATUS_PREFIX}warning")));
        assert!(controller.project_page(page_input()).rebuilt);
        assert!(controller.page_projection_revision() > initial_projection_revision);
    }

    #[test]
    fn status_page_has_a_row_for_every_public_live_feed_product() {
        let mut controller = DataStatusController::default();
        let page = controller.project_page(page_input()).state;

        for policy in LIVE_FEED_PRODUCT_POLICIES {
            assert!(
                page.rows
                    .iter()
                    .any(|row| row.id == format!("live_feed:{}", policy.product_id)),
                "missing Status row for {}",
                policy.product_id
            );
        }
    }

    #[test]
    fn actions_validate_against_controller_records_and_update_hushing() {
        let mut controller = DataStatusController::new([record("warning")]);
        assert_eq!(
            controller.perform_action("status:hush:warning"),
            Some(DataStatusActionIntent::ProjectionChanged)
        );
        assert!(controller
            .project_state()
            .state
            .boxes
            .iter()
            .find(|box_| box_.id == "warning")
            .is_some_and(|box_| box_.hushed));
        assert_eq!(controller.perform_action("status:hush:missing"), None);
        assert_eq!(controller.perform_action("app:reload"), None);
    }
}
