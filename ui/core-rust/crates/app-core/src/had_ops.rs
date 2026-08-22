// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
};

use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use procedure_geometry_types as pgt;
#[cfg(test)]
use product_contracts::WaypointSearchMatchKind;
use product_contracts::WaypointSearchRecord;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;

use crate::navdb_types::{
    AirwayBranch, AirwayEntryCandidate, AirwayExitCandidate, AirwaySpatialPoint,
};
use crate::planning::FlightPlanRowActionId;
use crate::{
    chart_page::{
        chart_page_airport_candidates, derive_chart_page_state_from_collections,
        ChartReferenceFamilyRecord, ChartReferenceFamilySummary, PlateAirportRecord,
    },
    describe_plate_procedure_load_menu, describe_show_plate_for_procedure, flight_leg_distance_nm,
    flight_plan_contains_nav_ref, flight_plan_has_direct_to_overlay, insert_airway_after_airway,
    insert_airway_after_waypoint, insert_waypoint, prepare_airway_presentation,
    project_flight_plan_route_with_resolver, AirportId, AirwayPresentationPlan,
    AirwayPresentationSelection, AirwaySegment, AirwaySuggestion, AppError, AppErrorKind,
    AppResult, CifpTppMatchRow, FlightPlan, FlightPlanRouteSegment, FlightPlanUiState, LatLon,
    LegDisplayElement, LegDisplayPath, LegDisplayPathStyle, MaterializedProcedure, NavKvLookup,
    NavKvQuery, NavKvRoot, NavKvStore, NavRef, NavSymbolFeature, PathTermination,
    PlateProcedureLoadCandidateInput, PolygonRecord, ProcedureDiscontinuity, ProcedureKind,
    ProcedureLegProvenance, ProcedureLoadMenu, ProcedureOptions, ProcedureSegment,
    ProcedureSegmentRole, ProcedureSummary, ResolvedLeg, ResolvedLegSource, RouteComponent,
    SequencingMode, WaypointIdentifierSuggestion, REQUIRED_NAV_DB_CONTRACT_ID,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct FlightPlanLiveData {
    pub ownship_position: Option<LatLon>,
    pub ownship_altitude_ft: Option<f64>,
    pub now_epoch_ms: Option<i64>,
    pub local_time_zone: Tz,
    pub time_display_mode: crate::TimeDisplayMode,
}

impl Default for FlightPlanLiveData {
    fn default() -> Self {
        Self {
            ownship_position: None,
            ownship_altitude_ft: None,
            now_epoch_ms: None,
            local_time_zone: chrono_tz::UTC,
            time_display_mode: crate::TimeDisplayMode::Local,
        }
    }
}

const NAV_DB_ROOT_MEMBER_PATH: &str = "root";
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HadOperationOutcome {
    Complete {
        result: Value,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        invalidations: Vec<UiInvalidation>,
    },
    NeedResources {
        resources: Vec<CoreResourceRequest>,
    },
    NeedSnapshotResources {
        resources: Vec<CoreResourceRequest>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        invalidations: Vec<UiInvalidation>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiInvalidation {
    NavData,
    SessionSnapshot,
    RasterTiles,
    MapOverlay,
    NexradOverlay,
    TerrainOverlay,
    FlightPlanRoute,
    DebugPanel,
}

impl HadOperationOutcome {
    pub fn complete(result: Value) -> Self {
        Self::Complete {
            result,
            invalidations: Vec::new(),
        }
    }

    pub fn complete_with_invalidations(
        result: Value,
        invalidations: impl Into<Vec<UiInvalidation>>,
    ) -> Self {
        Self::Complete {
            result,
            invalidations: invalidations.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreResourceRequest {
    pub id: String,
    pub source: CoreResourceSource,
    #[serde(default)]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_response_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoreResourceSource {
    PublicUrl {
        url: String,
    },
    PackageMember {
        package_id: String,
        filename: String,
        member_path: String,
    },
    LiveFeedPackageMember {
        product: String,
        version: String,
        blob_sha256: String,
        member_path: String,
    },
    InstalledArtifactMember {
        filename: String,
        member_path: String,
    },
    NavKvMember {
        member_path: String,
    },
    Unavailable {
        message: String,
    },
}

impl CoreResourceRequest {
    pub fn public_url(id: impl Into<String>, url: impl Into<String>, optional: bool) -> Self {
        Self {
            id: id.into(),
            source: CoreResourceSource::PublicUrl { url: url.into() },
            optional,
            max_response_bytes: None,
        }
    }

    pub fn package_member(
        id: impl Into<String>,
        package_id: impl Into<String>,
        filename: impl Into<String>,
        member_path: impl Into<String>,
        optional: bool,
    ) -> Self {
        Self {
            id: id.into(),
            source: CoreResourceSource::PackageMember {
                package_id: package_id.into(),
                filename: filename.into(),
                member_path: member_path.into(),
            },
            optional,
            max_response_bytes: None,
        }
    }

    pub fn installed_artifact_member(
        id: impl Into<String>,
        filename: impl Into<String>,
        member_path: impl Into<String>,
        optional: bool,
    ) -> Self {
        Self {
            id: id.into(),
            source: CoreResourceSource::InstalledArtifactMember {
                filename: filename.into(),
                member_path: member_path.into(),
            },
            optional,
            max_response_bytes: None,
        }
    }

    pub fn live_feed_package_member(
        id: impl Into<String>,
        product: impl Into<String>,
        version: impl Into<String>,
        blob_sha256: impl Into<String>,
        member_path: impl Into<String>,
        optional: bool,
    ) -> Self {
        Self {
            id: id.into(),
            source: CoreResourceSource::LiveFeedPackageMember {
                product: product.into(),
                version: version.into(),
                blob_sha256: blob_sha256.into(),
                member_path: member_path.into(),
            },
            optional,
            max_response_bytes: None,
        }
    }

    pub fn nav_kv_member(
        id: impl Into<String>,
        member_path: impl Into<String>,
        optional: bool,
    ) -> Self {
        Self {
            id: id.into(),
            source: CoreResourceSource::NavKvMember {
                member_path: member_path.into(),
            },
            optional,
            max_response_bytes: None,
        }
    }

    pub fn unavailable(id: impl Into<String>, message: impl Into<String>, optional: bool) -> Self {
        Self {
            id: id.into(),
            source: CoreResourceSource::Unavailable {
                message: message.into(),
            },
            optional,
            max_response_bytes: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavDbArtifactCandidate {
    pub package_id: String,
    pub filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_source: Option<CoreResourceSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavDbArtifactOpenStatus {
    pub package_id: String,
    pub filename: String,
    pub readable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavDbOpenResult {
    pub selected_package_id: String,
    pub selected_filename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_contract_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_cycle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_cycle_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_effective_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_expiration_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_warning_text: Option<String>,
    pub statuses: Vec<NavDbArtifactOpenStatus>,
}

#[derive(Debug, Clone)]
pub struct NavDbOpenController {
    candidates: Vec<NavDbArtifactCandidate>,
    statuses: Vec<Option<NavDbArtifactOpenStatus>>,
    stores: Vec<Option<NavKvStore>>,
    root_prefetch_attempted: Vec<bool>,
    now_epoch_ms: i64,
}

impl NavDbOpenController {
    pub fn new_at_epoch_ms(candidates: Vec<NavDbArtifactCandidate>, now_epoch_ms: i64) -> Self {
        let len = candidates.len();
        Self {
            candidates,
            statuses: vec![None; len],
            stores: vec![None; len],
            root_prefetch_attempted: vec![false; len],
            now_epoch_ms,
        }
    }

    pub fn step(&mut self) -> Result<HadOperationOutcome, String> {
        for (index, candidate) in self.candidates.iter().enumerate() {
            match &self.statuses[index] {
                Some(_) => continue,
                None => {
                    if let Some(store) = self.stores.get(index).and_then(Option::as_ref) {
                        if !self.root_prefetch_attempted[index] {
                            self.root_prefetch_attempted[index] = true;
                            let pages = store.root().prefetch_pages().to_vec();
                            if !pages.is_empty() {
                                return Ok(HadOperationOutcome::NeedResources {
                                    resources: nav_db_artifact_page_resources(
                                        index, candidate, pages,
                                    ),
                                });
                            }
                        }
                        match validate_nav_db_contract(store)? {
                            NavDbContractValidation::Valid => {
                                self.statuses[index] = Some(NavDbArtifactOpenStatus {
                                    package_id: candidate.package_id.clone(),
                                    filename: candidate.filename.clone(),
                                    readable: true,
                                    message: None,
                                });
                                continue;
                            }
                            NavDbContractValidation::NeedPages(pages) => {
                                return Ok(HadOperationOutcome::NeedResources {
                                    resources: nav_db_artifact_page_resources(
                                        index, candidate, pages,
                                    ),
                                });
                            }
                            NavDbContractValidation::Invalid(message) => {
                                self.statuses[index] = Some(NavDbArtifactOpenStatus {
                                    package_id: candidate.package_id.clone(),
                                    filename: candidate.filename.clone(),
                                    readable: false,
                                    message: Some(message),
                                });
                                continue;
                            }
                        }
                    }
                    return Ok(HadOperationOutcome::NeedResources {
                        resources: vec![nav_db_artifact_root_resource(index, candidate)],
                    });
                }
            }
        }
        if let Some(index) = self.selected_candidate_index() {
            return Ok(HadOperationOutcome::complete(
                serde_json::to_value(self.open_result(index)).map_err(|err| err.to_string())?,
            ));
        }
        Err(self.no_readable_candidate_message())
    }

    fn no_readable_candidate_message(&self) -> String {
        if self.candidates.is_empty() {
            return "no installed nav-db package candidates".to_string();
        }
        let rejected = self
            .statuses
            .iter()
            .filter_map(|status| status.as_ref())
            .filter(|status| !status.readable)
            .map(|status| {
                let message = status.message.as_deref().unwrap_or("unreadable");
                format!("{}: {message}", status.filename)
            })
            .collect::<Vec<_>>();
        if rejected.is_empty() {
            if self
                .statuses
                .iter()
                .any(|status| status.as_ref().is_some_and(|status| status.readable))
            {
                return "no readable nav-db package is effective at the current time".to_string();
            }
            "no readable installed nav-db package".to_string()
        } else {
            format!(
                "no readable installed nav-db package; rejected {}",
                rejected.join("; ")
            )
        }
    }

    pub fn ingest_resource(
        &mut self,
        resource_id: &str,
        resource_bytes: &[u8],
    ) -> Result<(), String> {
        let Some(resource) = nav_db_artifact_resource_index(resource_id) else {
            return Err(format!(
                "unsupported nav_db open resource id: {resource_id}"
            ));
        };
        let index = resource.index;
        let candidate = self
            .candidates
            .get(index)
            .ok_or_else(|| format!("nav_db open resource index out of range: {index}"))?;
        if resource_bytes.is_empty() {
            self.statuses[index] = Some(NavDbArtifactOpenStatus {
                package_id: candidate.package_id.clone(),
                filename: candidate.filename.clone(),
                readable: false,
                message: Some(
                    resource
                        .page_index
                        .map(|page| format!("missing page {page:04}"))
                        .unwrap_or_else(|| "missing root".to_string()),
                ),
            });
            return Ok(());
        }
        if let Some(page_index) = resource.page_index {
            let decoded_bytes = decode_nav_db_page_resource_bytes(resource_id, resource_bytes)?;
            let store = self
                .stores
                .get_mut(index)
                .and_then(Option::as_mut)
                .ok_or_else(|| format!("nav_db open page arrived before root for index {index}"))?;
            store.insert_page(page_index, decoded_bytes.as_ref().to_vec());
            return Ok(());
        }
        match NavKvRoot::parse(resource_bytes) {
            Ok(root) => {
                self.stores[index] = Some(NavKvStore::new(root));
                self.root_prefetch_attempted[index] = false;
            }
            Err(message) => {
                self.statuses[index] = Some(NavDbArtifactOpenStatus {
                    package_id: candidate.package_id.clone(),
                    filename: candidate.filename.clone(),
                    readable: false,
                    message: Some(message),
                });
            }
        }
        Ok(())
    }

    pub fn selected_store(&self) -> Option<&NavKvStore> {
        self.selected_candidate_index()
            .and_then(|index| self.stores.get(index))
            .and_then(Option::as_ref)
    }

    pub fn statuses(&self) -> Vec<NavDbArtifactOpenStatus> {
        self.statuses.iter().filter_map(Clone::clone).collect()
    }

    fn selected_candidate_index(&self) -> Option<usize> {
        select_preferred_effective_nav_db_candidate_index(
            &self.candidates,
            self.statuses
                .iter()
                .enumerate()
                .filter(|(_, status)| status.as_ref().is_some_and(|status| status.readable))
                .map(|(index, _)| index),
            self.now_epoch_ms,
        )
    }

    fn open_result(&self, index: usize) -> NavDbOpenResult {
        let candidate = &self.candidates[index];
        NavDbOpenResult {
            selected_package_id: candidate.package_id.clone(),
            selected_filename: candidate.filename.clone(),
            selected_contract_id: candidate.contract_id.clone(),
            selected_cycle: candidate.cycle.clone(),
            selected_cycle_version: candidate.cycle_version.clone(),
            selected_effective_date: candidate.effective_date.clone(),
            selected_expiration_date: candidate.expiration_date.clone(),
            selected_warning_text: candidate.warning_text.clone(),
            statuses: self.statuses.iter().filter_map(Clone::clone).collect(),
        }
    }
}

pub fn select_preferred_nav_db_candidate(
    candidates: &[NavDbArtifactCandidate],
    now_epoch_ms: i64,
) -> Option<&NavDbArtifactCandidate> {
    select_preferred_effective_nav_db_candidate_index(candidates, 0..candidates.len(), now_epoch_ms)
        .and_then(|index| candidates.get(index))
}

fn select_preferred_effective_nav_db_candidate_index(
    candidates: &[NavDbArtifactCandidate],
    indexes: impl Iterator<Item = usize>,
    now_epoch_ms: i64,
) -> Option<usize> {
    select_preferred_nav_db_candidate_index(
        candidates,
        indexes.filter(|index| {
            candidates[*index]
                .effective_date
                .as_deref()
                .and_then(parse_nav_db_timestamp)
                .is_none_or(|effective| effective <= now_epoch_ms)
        }),
        now_epoch_ms,
    )
}

fn select_preferred_nav_db_candidate_index(
    candidates: &[NavDbArtifactCandidate],
    indexes: impl Iterator<Item = usize>,
    now_epoch_ms: i64,
) -> Option<usize> {
    indexes.max_by(|left, right| {
        compare_nav_db_candidates(&candidates[*left], &candidates[*right], now_epoch_ms)
    })
}

fn compare_nav_db_candidates(
    left: &NavDbArtifactCandidate,
    right: &NavDbArtifactCandidate,
    now_epoch_ms: i64,
) -> Ordering {
    nav_db_candidate_score(left, now_epoch_ms)
        .cmp(&nav_db_candidate_score(right, now_epoch_ms))
        .then_with(|| left.filename.cmp(&right.filename))
}

fn nav_db_candidate_score(candidate: &NavDbArtifactCandidate, now_epoch_ms: i64) -> (u8, i64, i64) {
    let effective_epoch_ms = candidate
        .effective_date
        .as_deref()
        .and_then(parse_nav_db_timestamp);
    let expiration_epoch_ms = candidate
        .expiration_date
        .as_deref()
        .and_then(parse_nav_db_timestamp);
    match (effective_epoch_ms, expiration_epoch_ms) {
        (Some(effective), Some(expiration))
            if effective <= now_epoch_ms && now_epoch_ms < expiration =>
        {
            (3, effective, expiration)
        }
        (Some(effective), _) if now_epoch_ms < effective => (1, effective.saturating_neg(), 0),
        (_, Some(expiration)) if expiration <= now_epoch_ms => (0, expiration, 0),
        _ => (2, 0, 0),
    }
}

fn parse_nav_db_timestamp(value: &str) -> Option<i64> {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Some(timestamp.with_timezone(&Utc).timestamp_millis());
    }
    DateTime::parse_from_rfc3339(&format!("{value}T00:00:00Z"))
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc).timestamp_millis())
}

fn nav_db_artifact_root_resource(
    index: usize,
    candidate: &NavDbArtifactCandidate,
) -> CoreResourceRequest {
    let id = format!("nav_db/artifact/{index}/root");
    if let Some(source) = candidate.root_source.clone() {
        CoreResourceRequest {
            id,
            source,
            optional: true,
            max_response_bytes: None,
        }
    } else {
        CoreResourceRequest::installed_artifact_member(
            id,
            candidate.filename.clone(),
            NAV_DB_ROOT_MEMBER_PATH,
            true,
        )
    }
}

fn nav_db_artifact_page_resources(
    index: usize,
    candidate: &NavDbArtifactCandidate,
    mut pages: Vec<u32>,
) -> Vec<CoreResourceRequest> {
    pages.sort_unstable();
    pages.dedup();
    pages
        .into_iter()
        .map(|page| {
            let id = format!("nav_db/artifact/{index}/page/{page:04}");
            CoreResourceRequest {
                id,
                source: nav_db_artifact_page_source(candidate, page),
                optional: true,
                max_response_bytes: None,
            }
        })
        .collect()
}

fn nav_db_artifact_page_source(
    candidate: &NavDbArtifactCandidate,
    page: u32,
) -> CoreResourceSource {
    let page_member = format!("page_{page:04}");
    match candidate.root_source.as_ref() {
        Some(CoreResourceSource::PublicUrl { url }) => {
            if let Some(prefix) = url.strip_suffix(NAV_DB_ROOT_MEMBER_PATH) {
                CoreResourceSource::PublicUrl {
                    url: format!("{prefix}{page_member}"),
                }
            } else {
                CoreResourceSource::Unavailable {
                    message: format!(
                        "nav_db root URL does not end with {NAV_DB_ROOT_MEMBER_PATH}: {url}"
                    ),
                }
            }
        }
        Some(CoreResourceSource::PackageMember {
            package_id,
            filename,
            ..
        }) => CoreResourceSource::PackageMember {
            package_id: package_id.clone(),
            filename: filename.clone(),
            member_path: page_member,
        },
        Some(CoreResourceSource::InstalledArtifactMember { filename, .. }) => {
            CoreResourceSource::InstalledArtifactMember {
                filename: filename.clone(),
                member_path: page_member,
            }
        }
        Some(CoreResourceSource::NavKvMember { .. }) => CoreResourceSource::Unavailable {
            message: "nav_db artifact root cannot be a nav_kv member".to_string(),
        },
        Some(CoreResourceSource::LiveFeedPackageMember { .. }) => CoreResourceSource::Unavailable {
            message: "nav_db artifact root cannot be a live-feed package member".to_string(),
        },
        Some(CoreResourceSource::Unavailable { message }) => CoreResourceSource::Unavailable {
            message: message.clone(),
        },
        None => CoreResourceSource::InstalledArtifactMember {
            filename: candidate.filename.clone(),
            member_path: page_member,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NavDbArtifactResource {
    index: usize,
    page_index: Option<u32>,
}

fn nav_db_artifact_resource_index(resource_id: &str) -> Option<NavDbArtifactResource> {
    if let Some(index) = resource_id
        .strip_prefix("nav_db/artifact/")
        .and_then(|rest| rest.strip_suffix("/root"))
        .and_then(|index| index.parse::<usize>().ok())
    {
        return Some(NavDbArtifactResource {
            index,
            page_index: None,
        });
    }
    let rest = resource_id.strip_prefix("nav_db/artifact/")?;
    let (index, page) = rest.split_once("/page/")?;
    Some(NavDbArtifactResource {
        index: index.parse().ok()?,
        page_index: Some(page.parse().ok()?),
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct NavDbContractRecord {
    contract_id: String,
}

enum NavDbContractValidation {
    Valid,
    NeedPages(Vec<u32>),
    Invalid(String),
}

fn validate_nav_db_contract(store: &NavKvStore) -> Result<NavDbContractValidation, String> {
    match store.get_bytes(crate::NAV_DB_CONTRACT_KEY)? {
        NavKvLookup::Hit(bytes) => match serde_json::from_slice::<NavDbContractRecord>(&bytes) {
            Ok(record) if record.contract_id == REQUIRED_NAV_DB_CONTRACT_ID => {
                Ok(NavDbContractValidation::Valid)
            }
            Ok(record) => Ok(NavDbContractValidation::Invalid(format!(
                "unsupported nav-db contract {}; required {}",
                record.contract_id, REQUIRED_NAV_DB_CONTRACT_ID
            ))),
            Err(err) => Ok(NavDbContractValidation::Invalid(format!(
                "invalid nav_db contract record: {err}"
            ))),
        },
        NavKvLookup::MissingKey => Ok(NavDbContractValidation::Invalid(format!(
            "missing nav-db contract {}; required {}",
            crate::NAV_DB_CONTRACT_KEY,
            REQUIRED_NAV_DB_CONTRACT_ID
        ))),
        NavKvLookup::MissingPages(pages) => Ok(NavDbContractValidation::NeedPages(pages)),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HadOperation {
    VectorManifest,
    PlateAirport {
        airport_id: String,
    },
    PlateById {
        plate_id: String,
    },
    ResolveWaypointIdentifier {
        identifier: String,
    },
    ResolveNavRefPosition {
        nav_ref: NavRef,
    },
    ResolveNavSymbolFeature {
        nav_ref: NavRef,
    },
    SuggestWaypointIdentifiersNear {
        anchor: LatLon,
        query: String,
        limit: usize,
    },
    SuggestAirwaysNearAnchor {
        anchor: NavRef,
        limit: usize,
    },
    ListProcedures {
        airport_id: String,
        procedure_kind: ProcedureKind,
    },
    DescribeProcedureOptions {
        airport_id: String,
        procedure_id: String,
        procedure_kind: ProcedureKind,
    },
    FindProcedurePlateMatch {
        airport_id: String,
        cifp_id: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HadReadError {
    NeedPages(Vec<u32>),
    Fatal(String),
}

#[derive(Debug, Default)]
struct HadReadPageCollector {
    pages: Vec<u32>,
}

impl HadReadPageCollector {
    fn collect<T>(&mut self, result: Result<T, HadReadError>) -> Result<Option<T>, HadReadError> {
        match result {
            Ok(value) => Ok(Some(value)),
            Err(HadReadError::NeedPages(pages)) => {
                self.pages.extend(pages);
                Ok(None)
            }
            Err(HadReadError::Fatal(message)) => Err(HadReadError::Fatal(message)),
        }
    }

    fn into_pages(mut self) -> Vec<u32> {
        self.pages.sort_unstable();
        self.pages.dedup();
        self.pages
    }
}

impl From<AppError> for HadReadError {
    fn from(err: AppError) -> Self {
        Self::Fatal(err.to_string())
    }
}

pub fn run_had_operation(store: &NavKvStore, op: HadOperation) -> AppResult<HadOperationOutcome> {
    let operation = had_operation_trace(&op);
    let started = crate::CoreDebugTimer::start();
    match run_had_operation_value(store, op) {
        Ok(result) => {
            crate::core_debug_log(
                "core.had_operation.core_done",
                &serde_json::json!({
                    "operation": operation,
                    "state": "complete",
                    "elapsed_ms": started.elapsed_ms(),
                }),
            );
            Ok(HadOperationOutcome::complete(result))
        }
        Err(HadReadError::NeedPages(pages)) => {
            crate::core_debug_log(
                "core.had_operation.core_done",
                &serde_json::json!({
                    "operation": operation,
                    "state": "need_resources",
                    "resource_count": pages.len(),
                    "pages": pages,
                    "elapsed_ms": started.elapsed_ms(),
                }),
            );
            Ok(HadOperationOutcome::NeedResources {
                resources: nav_kv_page_resources(pages),
            })
        }
        Err(HadReadError::Fatal(message)) => Err(AppError {
            kind: AppErrorKind::InvalidFlightPlan,
            message,
        }),
    }
}

fn had_operation_trace(op: &HadOperation) -> serde_json::Value {
    match op {
        HadOperation::ListProcedures {
            airport_id,
            procedure_kind,
        } => serde_json::json!({
            "kind": "list_procedures",
            "airport_id": airport_id,
            "procedure_kind": procedure_kind,
        }),
        HadOperation::DescribeProcedureOptions {
            airport_id,
            procedure_id,
            procedure_kind,
        } => serde_json::json!({
            "kind": "describe_procedure_options",
            "airport_id": airport_id,
            "procedure_id": procedure_id,
            "procedure_kind": procedure_kind,
        }),
        _ => serde_json::json!({
            "kind": "other",
        }),
    }
}

pub(crate) fn nav_kv_page_resources(mut pages: Vec<u32>) -> Vec<CoreResourceRequest> {
    pages.sort_unstable();
    pages.dedup();
    pages.into_iter().map(nav_kv_page_resource).collect()
}

pub(crate) fn nav_kv_page_resource(page: u32) -> CoreResourceRequest {
    CoreResourceRequest::nav_kv_member(
        format!("nav_kv/page/{page:04}"),
        format!("page_{page:04}"),
        false,
    )
}

pub fn nav_kv_page_index_from_resource_id(resource_id: &str) -> Option<u32> {
    resource_id
        .strip_prefix("nav_kv/page/")
        .and_then(|value| value.parse::<u32>().ok())
}

pub fn decode_nav_db_page_resource_bytes<'a>(
    resource_id: &str,
    resource_bytes: &'a [u8],
) -> Result<Cow<'a, [u8]>, String> {
    let is_nav_db_page_resource = nav_kv_page_index_from_resource_id(resource_id).is_some()
        || nav_db_artifact_page_resource_id(resource_id);
    if !is_nav_db_page_resource {
        return Ok(Cow::Borrowed(resource_bytes));
    }
    nav_kv_package::decode_xz_if_needed(resource_bytes)
        .map_err(|err| format!("{resource_id}: {err}"))
}

fn nav_db_artifact_page_resource_id(resource_id: &str) -> bool {
    let Some(rest) = resource_id.strip_prefix("nav_db/artifact/") else {
        return false;
    };
    let Some((index, page)) = rest.split_once("/page/") else {
        return false;
    };
    index.parse::<usize>().is_ok()
        && page.len() == 4
        && page.bytes().all(|byte| byte.is_ascii_digit())
}

fn run_had_operation_value(store: &NavKvStore, op: HadOperation) -> Result<Value, HadReadError> {
    let value = match op {
        HadOperation::VectorManifest => {
            read_required::<Value>(store, NavKvQuery::VectorManifest, "vector manifest")?
        }
        HadOperation::PlateAirport { airport_id } => {
            serde_json::to_value(resolve_plate_airport(store, &airport_id)?)?
        }
        HadOperation::PlateById { plate_id } => serde_json::to_value(read_optional::<Value>(
            store,
            NavKvQuery::PlateById { plate_id },
        )?)?,
        HadOperation::ResolveWaypointIdentifier { identifier } => {
            serde_json::to_value(resolve_waypoint_identifier_for_ui(store, &identifier)?)?
        }
        HadOperation::ResolveNavRefPosition { nav_ref } => {
            serde_json::to_value(nav_ref_position(store, &nav_ref, None)?)?
        }
        HadOperation::ResolveNavSymbolFeature { nav_ref } => {
            serde_json::to_value(nav_symbol_feature(store, &nav_ref)?)?
        }
        HadOperation::SuggestWaypointIdentifiersNear {
            anchor,
            query,
            limit,
        } => serde_json::to_value(suggest_waypoint_identifiers_near(
            store, anchor, &query, limit,
        )?)?,
        HadOperation::SuggestAirwaysNearAnchor { anchor, limit } => {
            serde_json::to_value(suggest_airways_near_anchor(store, &anchor, limit)?)?
        }
        HadOperation::ListProcedures {
            airport_id,
            procedure_kind,
        } => serde_json::to_value(list_procedures(store, &airport_id, procedure_kind)?)?,
        HadOperation::DescribeProcedureOptions {
            airport_id,
            procedure_id,
            procedure_kind,
        } => serde_json::to_value(describe_procedure_options(
            store,
            &airport_id,
            &procedure_id,
            procedure_kind,
        )?)?,
        HadOperation::FindProcedurePlateMatch {
            airport_id,
            cifp_id,
        } => {
            let rows = read_optional::<Vec<CifpTppMatchRow>>(
                store,
                NavKvQuery::PlateCifpMatch {
                    airport_id,
                    cifp_id,
                },
            )?;
            serde_json::to_value(rows.and_then(describe_show_plate_for_procedure))?
        }
    };
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapSelectorState {
    selected_map_id: String,
    selected_map: Option<MapViewOptionRecord>,
    available_maps: Vec<MapViewOptionRecord>,
    displayed_maps: Vec<MapViewOptionRecord>,
    geometry: DisplayGeometryRecord,
    family_options: Vec<MapFamilyOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapFamilyOption {
    id: String,
    label: String,
    launcher_label: String,
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    disabled_reason: Option<String>,
    active: bool,
    #[serde(default)]
    has_references: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapViewOptionRecord {
    id: String,
    label: String,
    region_id: String,
    #[serde(default)]
    reference_assets: Vec<crate::RasterChartReferenceAsset>,
    map_view: MapViewRecord,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DisplayGeometryRecord {
    schema_version: u32,
    polygons: Vec<PolygonRecord>,
    polygon_sets: Vec<DisplayPolygonSetRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DisplayPolygonSetRecord {
    id: String,
    polygon_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapViewRecord {
    chart_family: String,
    chart_name: String,
    chart_index: i64,
    tile_root: String,
    tile_url_root: String,
    tile_path_template: String,
    tile_size: i64,
    min_zoom: f64,
    max_zoom: f64,
    #[serde(default)]
    max_source_zoom: Option<i64>,
    #[serde(default)]
    max_display_zoom: Option<f64>,
    storage_kind: String,
    package_name: Option<String>,
    #[serde(default)]
    package_relative_path: Option<String>,
    #[serde(default)]
    package_effective_date: Option<String>,
    #[serde(default)]
    package_expiration_date: Option<String>,
    full_coverage_zoom: Option<f64>,
    #[serde(default)]
    wide_angle: Option<WideAngleMapViewRecord>,
    #[serde(default)]
    detail: Option<DetailMapViewRecord>,
    initial_viewport: MapInitialViewportRecord,
    levels: Vec<MapViewLevelRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct DetailMapViewRecord {
    package_name: String,
    #[serde(default)]
    package_relative_path: Option<String>,
    #[serde(default)]
    package_effective_date: Option<String>,
    #[serde(default)]
    package_expiration_date: Option<String>,
    tile_url_root: String,
    tile_path_template: String,
    levels: Vec<MapViewLevelRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct WideAngleMapViewRecord {
    region_id: String,
    max_zoom: f64,
    package_name: String,
    #[serde(default)]
    package_relative_path: Option<String>,
    #[serde(default)]
    package_effective_date: Option<String>,
    #[serde(default)]
    package_expiration_date: Option<String>,
    tile_url_root: String,
    tile_path_template: String,
    levels: Vec<MapViewLevelRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PackageRecord {
    id: String,
    family_id: String,
    region_id: Option<String>,
    #[serde(default)]
    relative_path: Option<String>,
    #[serde(default)]
    effective_date: Option<String>,
    #[serde(default)]
    expiration_date: Option<String>,
    #[serde(default)]
    metadata: Option<PackageMetadataRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PackageMetadataRecord {
    full_coverage_zoom: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapInitialViewportRecord {
    lat: f64,
    lon: f64,
    zoom: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapViewLevelRecord {
    zoom: i64,
    boxes: Vec<MapViewTileBoundsRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MapViewTileBoundsRecord {
    x_min: i64,
    x_max: i64,
    y_tms_min: i64,
    y_tms_max: i64,
}

fn read_required<T: DeserializeOwned>(
    store: &NavKvStore,
    query: NavKvQuery,
    family: &str,
) -> Result<T, HadReadError> {
    read_optional(store, query.clone())?.ok_or_else(|| {
        let key = crate::nav_kv_key_for_query(&query).unwrap_or_else(|| "<no-key>".to_string());
        HadReadError::Fatal(format!("HAD missing required {family} key: {key}"))
    })
}

fn read_required_key<T: DeserializeOwned>(
    store: &NavKvStore,
    key: &str,
    family: &str,
) -> Result<T, HadReadError> {
    match store.get_bytes(key).map_err(HadReadError::Fatal)? {
        NavKvLookup::Hit(bytes) => serde_json::from_slice(&bytes)
            .map_err(|err| HadReadError::Fatal(format!("HAD JSON decode failed for {key}: {err}"))),
        NavKvLookup::MissingKey => Err(HadReadError::Fatal(format!(
            "HAD missing required {family} key: {key}"
        ))),
        NavKvLookup::MissingPages(pages) => {
            log_had_key_page_fault(key, &pages);
            Err(HadReadError::NeedPages(pages))
        }
    }
}

fn read_optional<T: DeserializeOwned>(
    store: &NavKvStore,
    query: NavKvQuery,
) -> Result<Option<T>, HadReadError> {
    let Some(key) = crate::nav_kv_key_for_query(&query) else {
        return Ok(None);
    };
    match store.get_bytes(&key).map_err(HadReadError::Fatal)? {
        NavKvLookup::Hit(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|err| HadReadError::Fatal(format!("HAD JSON decode failed for {key}: {err}"))),
        NavKvLookup::MissingKey => Ok(None),
        NavKvLookup::MissingPages(pages) => {
            log_had_key_page_fault(&key, &pages);
            Err(HadReadError::NeedPages(pages))
        }
    }
}

fn log_had_key_page_fault(key: &str, pages: &[u32]) {
    crate::core_debug_log(
        "core.had.key_page_fault",
        &serde_json::json!({
            "key": key,
            "pages": pages,
            "page_count": pages.len(),
        }),
    );
}

pub(crate) fn nav_ref_position(
    store: &NavKvStore,
    nav_ref: &NavRef,
    procedure_airport_id: Option<&str>,
) -> Result<LatLon, HadReadError> {
    if let NavRef::LatLon(position) | NavRef::Spot(position) = nav_ref {
        return Ok(*position);
    }
    read_required(
        store,
        NavKvQuery::NavRefPosition {
            nav_ref: nav_ref.clone(),
            procedure_airport_id: procedure_airport_id.map(str::to_string),
        },
        "navref position",
    )
}

pub(crate) fn true_to_magnetic_course_deg_optional(
    store: &NavKvStore,
    true_course_deg: f64,
    position: LatLon,
) -> Result<Option<f64>, HadReadError> {
    magnetic_variation_degrees_optional(store, position).map(|variation| {
        variation.map(|variation| normalize_course_degrees(true_course_deg - variation))
    })
}

pub(crate) fn magnetic_variation_degrees_optional(
    store: &NavKvStore,
    position: LatLon,
) -> Result<Option<f64>, HadReadError> {
    let lat = position.lat.clamp(-90.0, 89.0);
    let lon = normalize_magvar_longitude(position.lon);
    let lat0 = lat.floor() as i32;
    let lat1 = (lat0 + 1).min(89);
    let lon0 = lon.floor() as i32;
    let lon1 = wrap_magvar_longitude(lon0 + 1);
    let lat_t = lat - f64::from(lat0);
    let lon_t = lon - f64::from(lon0);

    let Some(sw) = read_magvar_corner_optional(store, lat0, lon0)? else {
        return Ok(None);
    };
    let Some(se) = read_magvar_corner_optional(store, lat0, lon1)? else {
        return Ok(None);
    };
    let Some(nw) = read_magvar_corner_optional(store, lat1, lon0)? else {
        return Ok(None);
    };
    let Some(ne) = read_magvar_corner_optional(store, lat1, lon1)? else {
        return Ok(None);
    };
    let south = sw * (1.0 - lon_t) + se * lon_t;
    let north = nw * (1.0 - lon_t) + ne * lon_t;
    Ok(Some(south * (1.0 - lat_t) + north * lat_t))
}

fn read_magvar_corner_optional(
    store: &NavKvStore,
    lat: i32,
    lon: i32,
) -> Result<Option<f64>, HadReadError> {
    read_optional(store, NavKvQuery::MagneticVariation { lat, lon })
}

fn normalize_magvar_longitude(longitude: f64) -> f64 {
    let mut lon = longitude;
    while lon < -180.0 {
        lon += 360.0;
    }
    while lon >= 180.0 {
        lon -= 360.0;
    }
    lon
}

fn wrap_magvar_longitude(longitude: i32) -> i32 {
    if longitude >= 180 {
        longitude - 360
    } else if longitude < -180 {
        longitude + 360
    } else {
        longitude
    }
}

fn normalize_course_degrees(course_deg: f64) -> f64 {
    course_deg.rem_euclid(360.0)
}

pub(crate) fn nav_symbol_feature(
    store: &NavKvStore,
    nav_ref: &NavRef,
) -> Result<Option<NavSymbolFeature>, HadReadError> {
    read_optional(
        store,
        NavKvQuery::NavRefSymbol {
            nav_ref: nav_ref.clone(),
        },
    )
}

pub(crate) struct FlightPlanUiProjection {
    pub ui_state: FlightPlanUiState,
    pub materialized: crate::flight_plan_materialization::MaterializedFlightPlan,
    pub aircraft_plan_view_path: String,
}

const DEFAULT_CRUISE_ALTITUDE_FT: i32 = 12_000;
const MIN_PLANNER_CRUISE_ALTITUDE_FT: i32 = 2_000;
const PLANNER_CRUISE_ALTITUDE_STEP_FT: i32 = 2_000;
const ALTITUDE_ACTION_PREFIX: &str = "altitude-planner:select:";
const AIRCRAFT_ACTION_PREFIX: &str = "altitude-planner:aircraft:";
pub(crate) const SELECT_NO_WIND_ACTION_UID: &str = "altitude-planner:wind:no-wind";
pub(crate) const SELECT_GFS_WIND_ACTION_UID: &str = "altitude-planner:wind:gfs";
static NO_WIND_ATMOSPHERE: crate::NoWindIsaAtmosphere = crate::NoWindIsaAtmosphere;

#[derive(Clone, Copy)]
pub(crate) struct PlannerAtmosphereSelection<'a> {
    model: Option<&'a dyn crate::AtmosphereModel>,
    manifest: Option<&'a product_contracts::AtmosphereManifest>,
    label: &'static str,
    forecast_selected: bool,
    alternate_available: bool,
    action_uid: Option<&'static str>,
}

impl PlannerAtmosphereSelection<'static> {
    pub(crate) fn no_wind(forecast_available: bool) -> Self {
        Self {
            model: Some(&NO_WIND_ATMOSPHERE),
            manifest: None,
            label: "NO WIND",
            forecast_selected: false,
            alternate_available: forecast_available,
            action_uid: forecast_available.then_some(SELECT_GFS_WIND_ACTION_UID),
        }
    }
}

impl<'a> PlannerAtmosphereSelection<'a> {
    pub(crate) fn gfs(
        model: Option<&'a dyn crate::AtmosphereModel>,
        manifest: Option<&'a product_contracts::AtmosphereManifest>,
    ) -> Self {
        Self {
            model,
            manifest,
            label: "FORECAST",
            forecast_selected: true,
            // Even an unavailable selected model must allow returning to no-wind.
            alternate_available: true,
            action_uid: Some(SELECT_NO_WIND_ACTION_UID),
        }
    }
}

#[derive(Debug, Clone)]
struct PlannerAircraft {
    selection: product_contracts::AircraftSelection,
    definition: product_contracts::AircraftDefinition,
    profile: crate::AircraftPerformanceProfile,
    definitions: BTreeMap<String, product_contracts::AircraftDefinition>,
    advisory: Option<String>,
}

pub(crate) fn planner_aircraft_action_uid(
    selection: &product_contracts::AircraftSelection,
) -> String {
    format!(
        "{AIRCRAFT_ACTION_PREFIX}{}/{}",
        selection.definition_hash,
        had_key::component(&selection.profile_id)
    )
}

pub(crate) fn planner_aircraft_from_action_uid(
    action_uid: &str,
) -> Option<product_contracts::AircraftSelection> {
    let encoded = action_uid.strip_prefix(AIRCRAFT_ACTION_PREFIX)?;
    let (definition_hash, profile_id) = encoded.split_once('/')?;
    product_contracts::validate_aircraft_definition_hash(definition_hash).ok()?;
    Some(product_contracts::AircraftSelection {
        definition_hash: definition_hash.to_string(),
        profile_id: had_key::decode_component(profile_id).ok()?,
    })
}

pub(crate) fn planner_aircraft_selection_cruise_altitude(
    store: &NavKvStore,
    private_definitions: &BTreeMap<String, product_contracts::AircraftDefinition>,
    selection: &product_contracts::AircraftSelection,
    current_altitude_ft: Option<i32>,
) -> Result<Option<i32>, HadReadError> {
    let resolved = planner_aircraft(store, Some(selection), private_definitions)?;
    if resolved.advisory.is_some() || resolved.selection != *selection {
        return Ok(None);
    }
    let requested_altitude_ft = current_altitude_ft.unwrap_or(DEFAULT_CRUISE_ALTITUDE_FT);
    let nearest_altitude_ft = planner_cruise_altitudes(&resolved.profile)
        .into_iter()
        .min_by_key(|candidate| {
            (
                (i64::from(*candidate) - i64::from(requested_altitude_ft)).abs(),
                *candidate,
            )
        })
        .ok_or_else(|| {
            HadReadError::Fatal(format!(
                "aircraft profile {} has no selectable cruise altitude",
                selection.profile_id
            ))
        })?;
    Ok(Some(nearest_altitude_ft))
}

fn aircraft_control_options(
    aircraft: &PlannerAircraft,
    library_memberships: &BTreeMap<String, product_contracts::AircraftLibraryMembership>,
) -> (
    Vec<crate::AltitudePlannerControlOptionUiView>,
    Vec<crate::AltitudePlannerControlOptionUiView>,
) {
    let mut definitions = aircraft.definitions.iter().collect::<Vec<_>>();
    definitions.sort_by(|left, right| {
        left.1
            .label
            .cmp(&right.1.label)
            .then_with(|| left.0.cmp(right.0))
    });
    let superseded = aircraft
        .definitions
        .values()
        .flat_map(|definition| definition.supersedes.iter().cloned())
        .collect::<BTreeSet<_>>();
    let definitions = definitions
        .into_iter()
        .filter(|(hash, _)| {
            hash.as_str() == aircraft.selection.definition_hash
                || (!superseded.contains(*hash)
                    && library_memberships
                        .get(*hash)
                        .map(|membership| membership.included)
                        .unwrap_or(true))
        })
        .collect::<Vec<_>>();
    let aircraft_options = if definitions.len() > 1 {
        definitions
            .into_iter()
            .map(|(hash, definition)| {
                let selection = product_contracts::AircraftSelection {
                    definition_hash: hash.clone(),
                    profile_id: definition.default_profile_id.clone(),
                };
                crate::AltitudePlannerControlOptionUiView {
                    label: definition.label.clone(),
                    action_uid: planner_aircraft_action_uid(&selection),
                    selected: hash == &aircraft.selection.definition_hash,
                    trailing_symbol: Some(crate::aircraft_library::aircraft_symbol(definition)),
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let profile_options = if aircraft.definition.profiles.len() > 1 {
        aircraft
            .definition
            .profiles
            .iter()
            .map(|profile| {
                let selection = product_contracts::AircraftSelection {
                    definition_hash: aircraft.selection.definition_hash.clone(),
                    profile_id: profile.id.clone(),
                };
                crate::AltitudePlannerControlOptionUiView {
                    label: profile.label.clone(),
                    action_uid: planner_aircraft_action_uid(&selection),
                    selected: profile.id == aircraft.selection.profile_id,
                    trailing_symbol: None,
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    (aircraft_options, profile_options)
}

fn planner_aircraft(
    store: &NavKvStore,
    requested: Option<&product_contracts::AircraftSelection>,
    private_definitions: &BTreeMap<String, product_contracts::AircraftDefinition>,
) -> Result<PlannerAircraft, HadReadError> {
    let default_selection = product_contracts::default_aircraft_selection();
    let mut definitions = system_aircraft_definitions(store)?;
    for (hash, definition) in private_definitions {
        validate_aircraft_definition(hash, definition)?;
        definitions.insert(hash.clone(), definition.clone());
    }

    let requested = requested.unwrap_or(&default_selection);
    let superseding_hashes = if definitions.contains_key(&requested.definition_hash) {
        Vec::new()
    } else {
        definitions
            .iter()
            .filter_map(|(hash, definition)| {
                definition
                    .supersedes
                    .contains(&requested.definition_hash)
                    .then_some(hash)
            })
            .collect::<Vec<_>>()
    };
    if superseding_hashes.len() > 1 {
        return Err(HadReadError::Fatal(format!(
            "aircraft definition {} is superseded ambiguously by {}",
            requested.definition_hash,
            superseding_hashes
                .iter()
                .map(|hash| hash.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let resolved_requested = superseding_hashes
        .first()
        .map(|definition_hash| product_contracts::AircraftSelection {
            definition_hash: (*definition_hash).clone(),
            profile_id: requested.profile_id.clone(),
        })
        .unwrap_or_else(|| requested.clone());
    let (selection, advisory) = match definitions.get(&resolved_requested.definition_hash) {
        Some(definition) if definition.profile(&requested.profile_id).is_some() => {
            (resolved_requested, None)
        }
        Some(_) => (
            default_selection.clone(),
            Some(format!(
                "Selected aircraft profile {} is unavailable; using the default Cessna 172 profile.",
                requested.profile_id
            )),
        ),
        None => (
            default_selection.clone(),
            Some(format!(
                "Selected aircraft definition {} is unavailable; using the default Cessna 172 profile.",
                requested.definition_hash
            )),
        ),
    };
    let definition = definitions
        .get(&selection.definition_hash)
        .cloned()
        .ok_or_else(|| HadReadError::Fatal("default aircraft definition is missing".to_string()))?;
    let profile = crate::performance_profile_from_definition(
        &selection.definition_hash,
        &definition,
        &selection.profile_id,
    )
    .map_err(HadReadError::Fatal)?;
    Ok(PlannerAircraft {
        selection,
        definition,
        profile,
        definitions,
        advisory,
    })
}

pub(crate) fn system_aircraft_definitions(
    store: &NavKvStore,
) -> Result<BTreeMap<String, product_contracts::AircraftDefinition>, HadReadError> {
    let mut definitions = BTreeMap::new();
    for key in nav_kv_prefix_keys(store, product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX)? {
        let hash = key
            .strip_prefix(product_contracts::AIRCRAFT_DEFINITION_KEY_PREFIX)
            .ok_or_else(|| {
                HadReadError::Fatal(format!("invalid aircraft definition key: {key}"))
            })?;
        let definition: product_contracts::AircraftDefinition =
            read_required_key(store, &key, "aircraft definition")?;
        validate_aircraft_definition(hash, &definition)?;
        definitions.insert(hash.to_string(), definition);
    }
    Ok(definitions)
}

pub(crate) fn default_aircraft_plan_view_path(
    store: &NavKvStore,
    private_definitions: &BTreeMap<String, product_contracts::AircraftDefinition>,
) -> Result<String, HadReadError> {
    let selection = product_contracts::default_aircraft_selection();
    let definition = match private_definitions.get(&selection.definition_hash) {
        Some(definition) => definition.clone(),
        None => {
            let key = product_contracts::aircraft_definition_key(&selection.definition_hash)
                .map_err(HadReadError::Fatal)?;
            read_required_key(store, &key, "default aircraft definition")?
        }
    };
    validate_aircraft_definition(&selection.definition_hash, &definition)?;
    Ok(definition.plan_view_path)
}

fn validate_aircraft_definition(
    expected_hash: &str,
    definition: &product_contracts::AircraftDefinition,
) -> Result<(), HadReadError> {
    definition.validate().map_err(HadReadError::Fatal)?;
    let actual_hash = definition.content_hash().map_err(HadReadError::Fatal)?;
    if actual_hash != expected_hash {
        return Err(HadReadError::Fatal(format!(
            "aircraft definition hash mismatch: key has {expected_hash}, value has {actual_hash}"
        )));
    }
    Ok(())
}

fn planner_cruise_altitudes(profile: &crate::AircraftPerformanceProfile) -> Vec<i32> {
    let maximum = profile
        .cruise
        .last()
        .map(|point| point.pressure_altitude_ft.floor() as i32)
        .unwrap_or(MIN_PLANNER_CRUISE_ALTITUDE_FT);
    let step_count =
        maximum.saturating_sub(MIN_PLANNER_CRUISE_ALTITUDE_FT) / PLANNER_CRUISE_ALTITUDE_STEP_FT;
    (0..=step_count)
        .rev()
        .map(|step| MIN_PLANNER_CRUISE_ALTITUDE_FT + step * PLANNER_CRUISE_ALTITUDE_STEP_FT)
        .collect()
}

pub(crate) fn planner_altitude_action_uid(altitude_ft: i32) -> Option<String> {
    (altitude_ft >= MIN_PLANNER_CRUISE_ALTITUDE_FT
        && altitude_ft % PLANNER_CRUISE_ALTITUDE_STEP_FT == 0)
        .then(|| format!("{ALTITUDE_ACTION_PREFIX}{altitude_ft}"))
}

pub(crate) fn planner_altitude_from_action_uid(action_uid: &str) -> Option<i32> {
    let altitude_ft = action_uid
        .strip_prefix(ALTITUDE_ACTION_PREFIX)?
        .parse()
        .ok()?;
    (planner_altitude_action_uid(altitude_ft).as_deref() == Some(action_uid)).then_some(altitude_ft)
}

fn endpoint_airport_id(
    plan: &FlightPlan,
    materialized: &crate::flight_plan_materialization::MaterializedFlightPlan,
    departure: bool,
) -> Option<String> {
    let declared = if departure {
        plan.departure.as_ref()
    } else {
        plan.destination.as_ref()
    };
    declared.map(|airport| airport.0.clone()).or_else(|| {
        let row_id = if departure {
            materialized.order.first()
        } else {
            materialized.order.last()
        }?;
        materialized
            .rows
            .get(row_id)?
            .location
            .as_ref()?
            .airport_code()
            .map(str::to_string)
    })
}

fn trajectory_route_legs(
    materialized: &crate::flight_plan_materialization::MaterializedFlightPlan,
    active_start: Option<LatLon>,
) -> Vec<crate::TrajectoryRouteLeg> {
    let active_row_id = materialized.active.as_ref().map(|active| &active.row_id);
    let active_index = active_row_id.and_then(|row_id| {
        materialized
            .order
            .iter()
            .position(|candidate| candidate == row_id)
    });

    materialized
        .order
        .iter()
        .enumerate()
        .filter_map(|(index, row_id)| {
            if active_index.is_some_and(|active_index| index < active_index) {
                return None;
            }
            let row = materialized.rows.get(row_id)?;
            let geometry = row.estimate_geometry.as_ref()?;
            let mut path = crate::flight_plan_materialization::geometry_points(geometry);
            if active_index == Some(index) {
                if let Some(start) = active_start {
                    let end = path.last().copied()?;
                    path = vec![start, end];
                }
            }
            Some(crate::TrajectoryRouteLeg {
                row_id: row_id.clone(),
                path,
            })
        })
        .collect()
}

fn modeled_prediction(
    profile: &crate::AircraftPerformanceProfile,
    legs: &[crate::TrajectoryRouteLeg],
    start_altitude_ft: f64,
    cruise_altitude_ft: f64,
    destination_altitude_ft: f64,
    departure_epoch_ms: i64,
    atmosphere: &dyn crate::AtmosphereModel,
) -> Result<crate::TrajectoryPrediction, crate::TrajectoryPlannerError> {
    crate::TrajectoryPlanner::new(profile, atmosphere).predict(&crate::TrajectoryPlanInput {
        legs: legs.to_vec(),
        start_pressure_altitude_ft: start_altitude_ft,
        cruise_pressure_altitude_ft: cruise_altitude_ft,
        destination_pressure_altitude_ft: destination_altitude_ft,
        departure_epoch_ms,
    })
}

#[derive(Debug)]
struct PlannerPrediction {
    prediction: crate::TrajectoryPrediction,
    fallback_reason: Option<String>,
}

fn planner_wind_fallback(
    prediction: &PlannerPrediction,
    atmosphere: PlannerAtmosphereSelection<'_>,
    departure_epoch_ms: i64,
) -> Option<crate::AltitudePlannerWindFallback> {
    let reason = prediction.fallback_reason.as_ref()?;
    if reason == crate::forecast_atmosphere::TIME_OUTSIDE_FORECAST_COVERAGE {
        if let Some((valid_from_epoch_ms, valid_through_epoch_ms)) =
            atmosphere.manifest.and_then(|manifest| {
                manifest
                    .valid_times_epoch_ms
                    .first()
                    .copied()
                    .zip(manifest.valid_times_epoch_ms.last().copied())
            })
        {
            return Some(crate::AltitudePlannerWindFallback::ForecastCoverage {
                valid_from_epoch_ms,
                valid_through_epoch_ms,
                flight_start_epoch_ms: departure_epoch_ms,
                flight_end_epoch_ms: departure_epoch_ms.saturating_add(
                    (prediction.prediction.total_ete_seconds * 1_000.0).round() as i64,
                ),
            });
        }
    }
    Some(crate::AltitudePlannerWindFallback::Other {
        reason: reason.clone(),
    })
}

fn modeled_prediction_for_selection(
    profile: &crate::AircraftPerformanceProfile,
    legs: &[crate::TrajectoryRouteLeg],
    start_altitude_ft: f64,
    cruise_altitude_ft: f64,
    destination_altitude_ft: f64,
    departure_epoch_ms: i64,
    atmosphere: PlannerAtmosphereSelection<'_>,
) -> Result<PlannerPrediction, crate::TrajectoryPlannerError> {
    if !atmosphere.forecast_selected {
        return modeled_prediction(
            profile,
            legs,
            start_altitude_ft,
            cruise_altitude_ft,
            destination_altitude_ft,
            departure_epoch_ms,
            atmosphere.model.unwrap_or(&NO_WIND_ATMOSPHERE),
        )
        .map(|prediction| PlannerPrediction {
            prediction,
            fallback_reason: None,
        });
    }

    let forecast_result = atmosphere.model.map_or_else(
        || {
            Err(crate::TrajectoryPlannerError::AtmosphereUnavailable(
                "the forecast product is unavailable".to_string(),
            ))
        },
        |model| {
            modeled_prediction(
                profile,
                legs,
                start_altitude_ft,
                cruise_altitude_ft,
                destination_altitude_ft,
                departure_epoch_ms,
                model,
            )
        },
    );
    match forecast_result {
        Ok(prediction) => Ok(PlannerPrediction {
            prediction,
            fallback_reason: None,
        }),
        Err(crate::TrajectoryPlannerError::AtmosphereUnavailable(reason)) => modeled_prediction(
            profile,
            legs,
            start_altitude_ft,
            cruise_altitude_ft,
            destination_altitude_ft,
            departure_epoch_ms,
            &NO_WIND_ATMOSPHERE,
        )
        .map(|prediction| PlannerPrediction {
            prediction,
            fallback_reason: Some(reason),
        }),
        Err(error) => Err(error),
    }
}

fn planner_forecast_ui(
    atmosphere: PlannerAtmosphereSelection<'_>,
    now_epoch_ms: i64,
    time_display_mode: crate::TimeDisplayMode,
    local_time_zone: chrono_tz::Tz,
) -> Option<crate::AltitudePlannerForecastUiView> {
    let manifest = atmosphere.manifest?;
    let cycle = DateTime::<Utc>::from_timestamp_millis(manifest.cycle_time_epoch_ms)?;
    let valid_from = manifest
        .valid_times_epoch_ms
        .first()
        .and_then(|epoch_ms| DateTime::<Utc>::from_timestamp_millis(*epoch_ms))?;
    let valid_through = manifest
        .valid_times_epoch_ms
        .last()
        .and_then(|epoch_ms| DateTime::<Utc>::from_timestamp_millis(*epoch_ms))?;
    let age_ms = now_epoch_ms
        .saturating_sub(manifest.cycle_time_epoch_ms)
        .max(0);
    let age_hours = age_ms as f64 / 3_600_000.0;
    let age = if age_hours < 1.0 {
        format!("{}m old", (age_ms / 60_000).max(0))
    } else {
        format!("{age_hours:.1}h old")
    };
    Some(crate::AltitudePlannerForecastUiView {
        summary: format!(
            "NOAA {} cycle {} ({age}); valid {} through {}.",
            manifest.model_id.to_uppercase(),
            crate::format_dated_time(
                cycle.timestamp_millis(),
                time_display_mode,
                local_time_zone,
                crate::DatedTimeStyle::MonthDayMinute,
            ),
            crate::format_dated_time(
                valid_from.timestamp_millis(),
                time_display_mode,
                local_time_zone,
                crate::DatedTimeStyle::MonthDayMinute,
            ),
            crate::format_dated_time(
                valid_through.timestamp_millis(),
                time_display_mode,
                local_time_zone,
                crate::DatedTimeStyle::MonthDayMinute,
            ),
        ),
    })
}

struct AltitudeComparisonInput<'a> {
    profile: &'a crate::AircraftPerformanceProfile,
    materialized: &'a crate::flight_plan_materialization::MaterializedFlightPlan,
    live_data: FlightPlanLiveData,
    origin_altitude_ft: Option<f64>,
    destination_altitude_ft: Option<f64>,
    selected_altitude_ft: i32,
    departure_epoch_ms: i64,
    atmosphere: PlannerAtmosphereSelection<'a>,
}

fn altitude_comparisons(
    input: AltitudeComparisonInput<'_>,
) -> Vec<crate::AltitudeComparisonUiView> {
    let AltitudeComparisonInput {
        profile,
        materialized,
        live_data,
        origin_altitude_ft,
        destination_altitude_ft,
        selected_altitude_ft,
        departure_epoch_ms,
        atmosphere,
    } = input;
    let navigation_active = materialized.active.is_some();
    let start_altitude_ft = if navigation_active {
        live_data.ownship_altitude_ft
    } else {
        origin_altitude_ft
    };
    let active_start = navigation_active
        .then_some(live_data.ownship_position)
        .flatten();
    let legs = if navigation_active && active_start.is_none() {
        Vec::new()
    } else {
        trajectory_route_legs(materialized, active_start)
    };

    planner_cruise_altitudes(profile)
        .into_iter()
        .map(|altitude_ft| {
            let selected = altitude_ft == selected_altitude_ft;
            let unavailable = if start_altitude_ft.is_none() {
                Some(if navigation_active {
                    "Active navigation has no ownship altitude."
                } else {
                    "The flight plan origin has no known elevation."
                })
            } else if navigation_active && live_data.ownship_position.is_none() {
                Some("Active navigation has no ownship position.")
            } else if destination_altitude_ft.is_none() {
                Some("The flight plan destination has no known elevation.")
            } else if legs.is_empty() {
                Some("The flight plan has no flyable route geometry.")
            } else if !navigation_active
                && start_altitude_ft.is_some_and(|start| altitude_ft as f64 <= start)
            {
                Some("Cruise altitude must be above the departure elevation.")
            } else if destination_altitude_ft
                .is_some_and(|destination| altitude_ft as f64 <= destination)
            {
                Some("Cruise altitude must be above the destination elevation.")
            } else {
                None
            };
            let prediction = unavailable.is_none().then(|| {
                modeled_prediction_for_selection(
                    profile,
                    &legs,
                    start_altitude_ft.expect("availability checked above"),
                    altitude_ft as f64,
                    destination_altitude_ft.expect("availability checked above"),
                    departure_epoch_ms,
                    atmosphere,
                )
            });
            let (prediction, fallback_reason, disabled_reason) = match prediction {
                Some(Ok(outcome)) => (Some(outcome.prediction), outcome.fallback_reason, None),
                Some(Err(error)) => (None, None, Some(error.to_string())),
                None => (None, None, unavailable.map(str::to_string)),
            };
            let enabled = prediction.is_some();
            let estimate = prediction
                .as_ref()
                .map(|prediction| crate::FlightTimeFuelEstimate {
                    leg_ete_seconds: None,
                    cumulative_ete_seconds: Some(prediction.total_ete_seconds),
                    cumulative_fuel_gal: Some(prediction.total_fuel_gal),
                    estimate_kind: crate::FlightEstimateKind::Modeled,
                });
            let wind = prediction.as_ref().map(crate::format_trajectory_wind);
            crate::AltitudeComparisonUiView {
                action_uid: enabled
                    .then(|| planner_altitude_action_uid(altitude_ft))
                    .flatten(),
                selected,
                enabled,
                disabled_reason,
                advisory: fallback_reason
                    .filter(|reason| {
                        reason != crate::forecast_atmosphere::TIME_OUTSIDE_FORECAST_COVERAGE
                    })
                    .map(|reason| {
                        format!(
                            "Forecast unavailable for this comparison ({reason}); using no-wind/ISA."
                        )
                    }),
                cells: crate::altitude_comparison_cells(altitude_ft, estimate, wind),
            }
        })
        .collect()
}

pub(crate) fn flight_plan_ui_projection(
    store: &NavKvStore,
    private_aircraft_definitions: &BTreeMap<String, product_contracts::AircraftDefinition>,
    aircraft_library_memberships: &BTreeMap<String, product_contracts::AircraftLibraryMembership>,
    plan: FlightPlan,
    current_ui_state: FlightPlanUiState,
    computer: crate::FlightDataComputer,
    live_data: FlightPlanLiveData,
    atmosphere: PlannerAtmosphereSelection<'_>,
) -> Result<FlightPlanUiProjection, HadReadError> {
    let plan = crate::build_flight_plan(plan)?;
    let aircraft = planner_aircraft(store, plan.aircraft.as_ref(), private_aircraft_definitions)?;
    let mut ui_state = current_ui_state;
    ui_state
        .display_rows
        .retain(|row| row.row_kind != crate::FlightPlanDisplayRowKind::Summary);
    let mut missing_pages = HadReadPageCollector::default();
    let route = missing_pages
        .collect(project_flight_plan_route(store, &plan))?
        .unwrap_or_default();
    let geometry_by_id = crate::flight_plan_materialization::geometry_map_from_route(&route);
    let estimate_geometry_by_leg_id =
        crate::flight_plan_materialization::estimate_geometry_by_leg_id_with_resolver(
            &plan,
            &geometry_by_id,
            |nav_ref, procedure_airport_id| nav_ref_position(store, nav_ref, procedure_airport_id),
        )?;
    let materialized =
        crate::flight_plan_materialization::MaterializedFlightPlan::build_with_estimate_geometries(
            &plan,
            &geometry_by_id,
            &estimate_geometry_by_leg_id,
            live_data.ownship_position,
        )
        .map_err(|err| HadReadError::Fatal(err.message))?;
    let now_epoch_ms = live_data.now_epoch_ms.unwrap_or_default();
    let departure_epoch_ms = if materialized.active.is_some() {
        now_epoch_ms
    } else {
        plan.planned_departure_time_epoch_ms.unwrap_or(now_epoch_ms)
    };
    let cruise_altitude_ft = plan
        .cruise_altitude_ft
        .unwrap_or(DEFAULT_CRUISE_ALTITUDE_FT);
    let cruise_performance = aircraft
        .profile
        .cruise_performance_at(cruise_altitude_ft as f64);
    let computer = computer.with_fuel_flow_gph(
        cruise_performance
            .as_ref()
            .ok()
            .map(|performance| performance.fuel_flow_gph),
    );
    let use_live_eta = live_data.ownship_position.is_some()
        && materialized.active.is_some()
        && computer.has_usable_ground_speed();
    let origin_airport_id = endpoint_airport_id(&plan, &materialized, true);
    let destination_airport_id = endpoint_airport_id(&plan, &materialized, false);
    let origin_altitude_ft = match origin_airport_id.as_deref() {
        Some(airport_id) => missing_pages
            .collect(crate::airport_info::airport_elevation_msl_ft(
                store, airport_id,
            ))?
            .flatten(),
        None => None,
    };
    let destination_altitude_ft = match destination_airport_id.as_deref() {
        Some(airport_id) => missing_pages
            .collect(crate::airport_info::airport_elevation_msl_ft(
                store, airport_id,
            ))?
            .flatten(),
        None => None,
    };
    let modeled_route_legs = trajectory_route_legs(
        &materialized,
        materialized.active.as_ref().and(live_data.ownship_position),
    );
    let mut modeled_prediction_error = None;
    let wind_model_available = true;
    let modeled_prediction = if use_live_eta {
        None
    } else {
        let start_altitude_ft = if materialized.active.is_some() {
            live_data
                .ownship_altitude_ft
                .unwrap_or(cruise_altitude_ft as f64)
        } else {
            origin_altitude_ft.unwrap_or(cruise_altitude_ft as f64)
        };
        let end_altitude_ft = destination_altitude_ft.unwrap_or(cruise_altitude_ft as f64);
        match modeled_prediction_for_selection(
            &aircraft.profile,
            &modeled_route_legs,
            start_altitude_ft,
            cruise_altitude_ft as f64,
            end_altitude_ft,
            departure_epoch_ms,
            atmosphere,
        ) {
            Ok(prediction) => Some(prediction),
            Err(error) => {
                modeled_prediction_error =
                    Some(crate::altitude_planner::format_trajectory_planner_error(
                        &aircraft.profile,
                        &error,
                    ));
                None
            }
        }
    };
    let estimate_basis = if use_live_eta {
        crate::FlightPlanEstimateBasis::LiveGroundSpeed
    } else if modeled_prediction.is_some() {
        crate::FlightPlanEstimateBasis::AircraftModel
    } else {
        crate::FlightPlanEstimateBasis::Unavailable
    };
    let (aircraft_options, aircraft_profile_options) =
        aircraft_control_options(&aircraft, aircraft_library_memberships);
    ui_state.altitude_planner = crate::project_altitude_planner_ui(crate::AltitudePlannerUiInput {
        aircraft_label: Some(aircraft.definition.label.clone()),
        aircraft_profile_label: Some(aircraft.profile.profile_label.clone()),
        aircraft_options,
        aircraft_profile_options,
        cruise_altitude_ft: Some(cruise_altitude_ft),
        navigation_active: materialized.active.is_some(),
        ownship_altitude_available: live_data.ownship_altitude_ft.is_some(),
        plan_origin_altitude_available: origin_altitude_ft.is_some(),
        plan_destination_altitude_available: destination_altitude_ft.is_some(),
        wind_model_label: atmosphere.label.to_string(),
        wind_model_selected: atmosphere.forecast_selected,
        wind_model_available,
        wind_model_selectable: atmosphere.alternate_available,
        wind_model_action_uid: atmosphere.action_uid.map(str::to_string),
        modeled_prediction_error,
        estimate_basis,
        departure_time_epoch_ms: plan.planned_departure_time_epoch_ms,
        effective_departure_time_epoch_ms: departure_epoch_ms,
        now_epoch_ms,
        time_display_mode: live_data.time_display_mode,
        local_time_zone: live_data.local_time_zone,
        forecast: planner_forecast_ui(
            atmosphere,
            now_epoch_ms,
            live_data.time_display_mode,
            live_data.local_time_zone,
        ),
        wind_fallback: modeled_prediction.as_ref().and_then(|prediction| {
            planner_wind_fallback(prediction, atmosphere, departure_epoch_ms)
        }),
        aircraft_advisory: aircraft.advisory,
    });
    let modeled_by_row_id = modeled_prediction
        .as_ref()
        .map_or_else(BTreeMap::new, |prediction| {
            let mut previous_ete_seconds = 0.0;
            prediction
                .prediction
                .legs
                .iter()
                .map(|leg| {
                    let estimate = crate::FlightTimeFuelEstimate {
                        leg_ete_seconds: Some(
                            (leg.cumulative_ete_seconds - previous_ete_seconds).max(0.0),
                        ),
                        cumulative_ete_seconds: Some(leg.cumulative_ete_seconds),
                        cumulative_fuel_gal: Some(leg.cumulative_fuel_gal),
                        estimate_kind: crate::FlightEstimateKind::Modeled,
                    };
                    previous_ete_seconds = leg.cumulative_ete_seconds;
                    (leg.row_id.clone(), estimate)
                })
                .collect::<BTreeMap<_, _>>()
        });
    for row in ui_state.display_rows.iter_mut() {
        let mut course_deg = None;
        let materialized_row = materialized
            .rows
            .get(&crate::FlightPlanRowId(row.uid.clone()));
        row.symbol_feature = match &row.nav_ref {
            Some(nav_ref) => missing_pages
                .collect(nav_symbol_feature(store, nav_ref))?
                .flatten(),
            None => None,
        };
        if let Some(geometry) = materialized_row.and_then(|row| row.geometry.as_ref()) {
            let points = crate::flight_plan_materialization::geometry_points(geometry);
            if let Some(segment) = points.windows(2).next() {
                course_deg = missing_pages
                    .collect(true_to_magnetic_course_deg_optional(
                        store,
                        crate::initial_course_deg(segment[0], segment[1]),
                        crate::great_circle_intermediate(segment[0], segment[1], 0.5),
                    ))?
                    .flatten();
            }
        }
        let row_has_data = row.row_kind != crate::FlightPlanDisplayRowKind::Group;
        let distance_nm = materialized_row.and_then(|row| row.distance_remaining_nm);
        let cumulative_distance_nm =
            materialized_row.and_then(|row| row.cumulative_distance_remaining_nm);
        let tone = materialized_row
            .map(|row| row.tone)
            .unwrap_or(crate::FlightDataCellTone::Planned);
        let eta = if use_live_eta {
            cumulative_distance_nm.and_then(|distance_nm| {
                live_data
                    .now_epoch_ms
                    .and_then(|now_epoch_ms| computer.format_eta_at(distance_nm, now_epoch_ms))
            })
        } else {
            None
        };
        row.data_cells = if let Some(estimate) =
            modeled_by_row_id.get(&crate::FlightPlanRowId(row.uid.clone()))
        {
            computer.flight_plan_row_cells_with_estimate(
                row_has_data,
                distance_nm,
                None,
                course_deg,
                tone,
                *estimate,
            )
        } else {
            computer.flight_plan_row_cells(
                row_has_data,
                distance_nm,
                cumulative_distance_nm,
                eta,
                course_deg,
                tone,
            )
        };
        if crate::planning::flight_plan_row_actions(row)
            .any(|action| action.id == FlightPlanRowActionId::ShowPlate)
        {
            let match_rows = match (&row.chart_airport_id, &row.procedure_id) {
                (Some(airport_id), Some(procedure_id)) => missing_pages
                    .collect(read_optional::<Vec<CifpTppMatchRow>>(
                        store,
                        NavKvQuery::PlateCifpMatch {
                            airport_id: airport_id.clone(),
                            cifp_id: procedure_id.clone(),
                        },
                    ))?
                    .flatten(),
                _ => None,
            };
            let plate_match = match_rows.and_then(describe_show_plate_for_procedure);
            row.show_plate_target_id = plate_match.as_ref().map(|matched| matched.plate_id.clone());
            for action in crate::planning::flight_plan_row_actions_mut(row) {
                if action.id == FlightPlanRowActionId::ShowPlate {
                    crate::planning::set_flight_plan_row_action_enabled(
                        action,
                        plate_match.is_some(),
                    );
                }
            }
            crate::planning::refresh_flight_plan_row_action_navigation(row);
        }
    }
    if let Some(total_remaining_distance_nm) = materialized.total_distance_remaining_nm {
        let summary_tone = if materialized.active.is_some() {
            crate::FlightDataCellTone::Active
        } else {
            crate::FlightDataCellTone::Planned
        };
        ui_state.display_rows.push(flight_plan_summary_row(
            &computer,
            total_remaining_distance_nm,
            summary_tone,
            modeled_prediction
                .as_ref()
                .map(|prediction| crate::FlightTimeFuelEstimate {
                    leg_ete_seconds: None,
                    cumulative_ete_seconds: Some(prediction.prediction.total_ete_seconds),
                    cumulative_fuel_gal: Some(prediction.prediction.total_fuel_gal),
                    estimate_kind: crate::FlightEstimateKind::Modeled,
                }),
        ));
    }
    ui_state.data_columns = computer.flight_plan_columns();
    let pages = missing_pages.into_pages();
    if !pages.is_empty() {
        return Err(HadReadError::NeedPages(pages));
    }
    Ok(FlightPlanUiProjection {
        ui_state,
        materialized,
        aircraft_plan_view_path: aircraft.definition.plan_view_path,
    })
}

pub(crate) fn altitude_comparison_panel(
    store: &NavKvStore,
    private_aircraft_definitions: &BTreeMap<String, product_contracts::AircraftDefinition>,
    plan: FlightPlan,
    live_data: FlightPlanLiveData,
    atmosphere: PlannerAtmosphereSelection<'_>,
) -> Result<crate::AltitudeComparisonPanelUiView, HadReadError> {
    let plan = crate::build_flight_plan(plan)?;
    let aircraft = planner_aircraft(store, plan.aircraft.as_ref(), private_aircraft_definitions)?;
    let mut missing_pages = HadReadPageCollector::default();
    let route = missing_pages
        .collect(project_flight_plan_route(store, &plan))?
        .unwrap_or_default();
    let geometry_by_id = crate::flight_plan_materialization::geometry_map_from_route(&route);
    let estimate_geometry_by_leg_id =
        crate::flight_plan_materialization::estimate_geometry_by_leg_id_with_resolver(
            &plan,
            &geometry_by_id,
            |nav_ref, procedure_airport_id| nav_ref_position(store, nav_ref, procedure_airport_id),
        )?;
    let materialized =
        crate::flight_plan_materialization::MaterializedFlightPlan::build_with_estimate_geometries(
            &plan,
            &geometry_by_id,
            &estimate_geometry_by_leg_id,
            live_data.ownship_position,
        )
        .map_err(|error| HadReadError::Fatal(error.message))?;
    let origin_airport_id = endpoint_airport_id(&plan, &materialized, true);
    let destination_airport_id = endpoint_airport_id(&plan, &materialized, false);
    let origin_altitude_ft = match origin_airport_id.as_deref() {
        Some(airport_id) => missing_pages
            .collect(crate::airport_info::airport_elevation_msl_ft(
                store, airport_id,
            ))?
            .flatten(),
        None => None,
    };
    let destination_altitude_ft = match destination_airport_id.as_deref() {
        Some(airport_id) => missing_pages
            .collect(crate::airport_info::airport_elevation_msl_ft(
                store, airport_id,
            ))?
            .flatten(),
        None => None,
    };
    let missing_pages = missing_pages.into_pages();
    if !missing_pages.is_empty() {
        return Err(HadReadError::NeedPages(missing_pages));
    }

    let departure_epoch_ms = if materialized.active.is_some() {
        live_data.now_epoch_ms.unwrap_or_default()
    } else {
        plan.planned_departure_time_epoch_ms
            .unwrap_or_else(|| live_data.now_epoch_ms.unwrap_or_default())
    };
    let rows = altitude_comparisons(AltitudeComparisonInput {
        profile: &aircraft.profile,
        materialized: &materialized,
        live_data,
        origin_altitude_ft,
        destination_altitude_ft,
        selected_altitude_ft: plan
            .cruise_altitude_ft
            .unwrap_or(DEFAULT_CRUISE_ALTITUDE_FT),
        departure_epoch_ms,
        atmosphere,
    });
    let mut advisories = rows
        .iter()
        .filter_map(|row| row.advisory.clone())
        .map(|message| (message, ()))
        .collect::<BTreeMap<_, ()>>()
        .into_keys()
        .collect::<Vec<_>>();
    advisories.extend(aircraft.advisory);
    Ok(crate::AltitudeComparisonPanelUiView {
        columns: crate::altitude_comparison_columns(),
        rows,
        advisories,
    })
}

fn flight_plan_summary_row(
    computer: &crate::FlightDataComputer,
    total_distance_nm: f64,
    tone: crate::FlightDataCellTone,
    modeled_estimate: Option<crate::FlightTimeFuelEstimate>,
) -> crate::planning::FlightPlanDisplayRowUiView {
    crate::planning::FlightPlanDisplayRowUiView {
        uid: "flight-plan:summary".to_string(),
        label: "TOTAL".to_string(),
        row_kind: crate::FlightPlanDisplayRowKind::Summary,
        component_kind: None,
        component_uid: None,
        component_index: None,
        procedure_id: None,
        procedure_kind: None,
        leg_index: None,
        data_cells: modeled_estimate.map_or_else(
            || computer.flight_plan_summary_cells(Some(total_distance_nm), tone),
            |estimate| {
                computer.flight_plan_summary_cells_with_estimate(
                    Some(total_distance_nm),
                    tone,
                    estimate,
                )
            },
        ),
        show_plate_target_id: None,
        chart_airport_id: None,
        nav_ref: None,
        symbol_feature: None,
        weather_badge: None,
        depth: 0,
        active: false,
        enabled: false,
        disabled_reason: None,
        synthetic_direct_to: false,
        can_add_airway_after: false,
        can_add_procedure_before: false,
        can_remove_component: false,
        can_reorder_component: false,
        can_reorder_up: false,
        can_reorder_down: false,
        origin_anchor: None,
        destination_anchor: None,
        preceding_waypoint: None,
        following_waypoint: None,
        action_matrix: Vec::new(),
    }
}

pub(crate) struct ChartPageQuery<'a> {
    pub(crate) plan: &'a FlightPlan,
    pub(crate) stored_recent_airport_ids: &'a [String],
    pub(crate) plate_target_airport_id: Option<&'a str>,
    pub(crate) candidate_airport_id: Option<&'a str>,
    pub(crate) selected_reference_family_id: Option<&'a str>,
    pub(crate) candidate_chart_id: Option<&'a str>,
    pub(crate) suggested_chart_ids: &'a [String],
    pub(crate) notam_display_index: Option<&'a crate::NotamDisplayIndex>,
}

pub(crate) fn chart_page_state(
    store: &NavKvStore,
    query: ChartPageQuery<'_>,
) -> Result<crate::DerivedChartPageState, HadReadError> {
    let ChartPageQuery {
        plan,
        stored_recent_airport_ids,
        plate_target_airport_id,
        candidate_airport_id,
        selected_reference_family_id,
        candidate_chart_id,
        suggested_chart_ids,
        notam_display_index,
    } = query;
    let mut airports = Vec::new();
    for airport_id in chart_page_airport_candidates(
        plan,
        stored_recent_airport_ids,
        plate_target_airport_id,
        candidate_airport_id,
    ) {
        if let Some(airport) = resolve_plate_airport(store, &airport_id)? {
            airports.push(airport);
        }
    }
    let reference_families = resolve_chart_reference_families(store, selected_reference_family_id)?;
    Ok(derive_chart_page_state_from_collections(
        crate::chart_page::ChartPageCollectionsInput {
            plan,
            airports,
            reference_families,
            stored_recent_airport_ids,
            plate_target_airport_id,
            selected_airport_id: candidate_airport_id,
            selected_reference_family_id,
            candidate_chart_id,
            suggested_chart_ids,
            notam_display_index,
        },
    ))
}

fn resolve_chart_reference_families(
    store: &NavKvStore,
    selected_family_id: Option<&str>,
) -> Result<Vec<crate::DerivedChartReferenceFamily>, HadReadError> {
    let summaries = read_optional::<Vec<ChartReferenceFamilySummary>>(
        store,
        NavKvQuery::ChartReferenceFamilyIndex,
    )?
    .unwrap_or_default();
    let mut ranked_summaries = summaries
        .into_iter()
        .map(|summary| {
            let rank = chart_family_order(&summary.id).ok_or_else(|| {
                HadReadError::Fatal(format!(
                    "chart reference family index contains unsupported family {}",
                    summary.id
                ))
            })?;
            Ok((rank, summary))
        })
        .collect::<Result<Vec<_>, HadReadError>>()?;
    ranked_summaries.sort_by_key(|(rank, _)| *rank);

    let mut families = Vec::with_capacity(ranked_summaries.len());
    for (_, summary) in ranked_summaries {
        if selected_family_id != Some(summary.id.as_str()) {
            families.push(crate::DerivedChartReferenceFamily {
                id: summary.id,
                label: summary.label,
                charts: Vec::new(),
            });
            continue;
        }
        let Some(record) = read_optional::<ChartReferenceFamilyRecord>(
            store,
            NavKvQuery::ChartReferenceFamily {
                family_id: summary.id.clone(),
            },
        )?
        else {
            return Err(HadReadError::Fatal(format!(
                "chart reference family index names missing family {}",
                summary.id
            )));
        };
        let mut charts = Vec::with_capacity(record.chart_ids.len());
        let mut missing_pages = Vec::new();
        for chart_id in &record.chart_ids {
            match read_chart_asset_by_id(store, chart_id)? {
                ChartAssetByIdRead::Hit(chart) => charts.push((*chart).into()),
                ChartAssetByIdRead::MissingPages(pages) => missing_pages.extend(pages),
            }
        }
        if !missing_pages.is_empty() {
            return Err(HadReadError::NeedPages(missing_pages));
        }
        families.push(crate::DerivedChartReferenceFamily {
            id: record.id,
            label: record.label,
            charts,
        });
    }
    Ok(families)
}

fn resolve_plate_airport(
    store: &NavKvStore,
    airport_id: &str,
) -> Result<Option<crate::DerivedChartAirport>, HadReadError> {
    let Some(record) = read_optional::<PlateAirportRecord>(
        store,
        NavKvQuery::PlateAirport {
            airport_id: airport_id.to_string(),
        },
    )?
    else {
        return Ok(None);
    };

    let mut charts = Vec::with_capacity(record.chart_ids.len());
    let mut missing_pages = Vec::new();
    for plate_id in &record.chart_ids {
        match read_chart_asset_by_id(store, plate_id)? {
            ChartAssetByIdRead::Hit(chart) => charts.push((*chart).into()),
            ChartAssetByIdRead::MissingPages(pages) => missing_pages.extend(pages),
        }
    }
    if !missing_pages.is_empty() {
        return Err(HadReadError::NeedPages(missing_pages));
    }

    Ok(Some(crate::DerivedChartAirport {
        id: record.id,
        label: record.label,
        charts,
        unmatched_procedure_notam_badge: None,
    }))
}

enum ChartAssetByIdRead {
    Hit(Box<crate::ChartAssetRecord>),
    MissingPages(Vec<u32>),
}

fn read_chart_asset_by_id(
    store: &NavKvStore,
    chart_id: &str,
) -> Result<ChartAssetByIdRead, HadReadError> {
    let query = NavKvQuery::PlateById {
        plate_id: chart_id.to_string(),
    };
    let key = crate::nav_kv_key_for_query(&query)
        .ok_or_else(|| HadReadError::Fatal("invalid plate id query".to_string()))?;
    match store.get_bytes(&key).map_err(HadReadError::Fatal)? {
        NavKvLookup::Hit(bytes) => serde_json::from_slice(&bytes)
            .map(Box::new)
            .map(ChartAssetByIdRead::Hit)
            .map_err(|err| HadReadError::Fatal(format!("HAD JSON decode failed for {key}: {err}"))),
        NavKvLookup::MissingKey => Err(HadReadError::Fatal(format!(
            "HAD missing required chart asset key: {key}"
        ))),
        NavKvLookup::MissingPages(pages) => Ok(ChartAssetByIdRead::MissingPages(pages)),
    }
}

const NO_RASTER_FAMILY_ID: &str = "none";
const NO_RASTER_SELECTED_MAP_ID: &str = "none";

fn map_selector_state(
    store: &NavKvStore,
    selected_map_id: Option<&str>,
    selected_family_id: Option<&str>,
) -> Result<MapSelectorState, HadReadError> {
    let mut map_views = chart_catalog(store)?;
    normalize_map_views(&mut map_views);
    let no_raster_selected = selected_family_id == Some(NO_RASTER_FAMILY_ID)
        || selected_map_id == Some(NO_RASTER_SELECTED_MAP_ID);
    let seed_map = map_views
        .iter()
        .find(|view| Some(view.id.as_str()) == selected_map_id);
    let seed_region_id = seed_map.map(|view| view.region_id.as_str()).or(Some("nw"));
    let selected_map = if no_raster_selected {
        seed_map
            .or_else(|| preferred_family_map(&map_views, "tac", Some("nw")))
            .or_else(|| map_views.first())
            .cloned()
    } else {
        selected_family_id
            .and_then(|family_id| preferred_family_map(&map_views, family_id, seed_region_id))
            .or(seed_map)
            .or_else(|| preferred_family_map(&map_views, "tac", Some("nw")))
            .or_else(|| map_views.first())
            .cloned()
    };
    let selected_family_id = if no_raster_selected {
        NO_RASTER_FAMILY_ID
    } else {
        selected_map
            .as_ref()
            .map(|view| view.map_view.chart_family.as_str())
            .unwrap_or("sec")
    };
    let mut displayed_maps: Vec<MapViewOptionRecord> = if no_raster_selected {
        Vec::new()
    } else {
        displayed_family_maps(&map_views, selected_family_id)
            .into_iter()
            .cloned()
            .collect()
    };
    if !no_raster_selected {
        let mut displayed_map_ids = displayed_maps
            .iter()
            .map(|view| view.id.clone())
            .collect::<HashSet<_>>();
        displayed_maps.extend(
            background_maps(&map_views)
                .into_iter()
                .filter(|view| displayed_map_ids.insert(view.id.clone()))
                .cloned(),
        );
    }
    let geometry = displayed_geometry();
    let family_options = supported_chart_families()
        .into_iter()
        .map(|(id, label, launcher_label)| {
            let enabled = id == NO_RASTER_FAMILY_ID
                || map_views
                    .iter()
                    .any(|view| view.map_view.chart_family == id);
            MapFamilyOption {
                id: id.to_string(),
                label: label.to_string(),
                launcher_label: launcher_label.to_string(),
                enabled,
                disabled_reason: (!enabled).then(|| format!("{label} charts are not available.")),
                active: selected_family_id == id,
                has_references: map_views.iter().any(|view| {
                    view.map_view.chart_family == id && !view.reference_assets.is_empty()
                }),
            }
        })
        .collect();
    Ok(MapSelectorState {
        selected_map_id: if no_raster_selected {
            NO_RASTER_SELECTED_MAP_ID.to_string()
        } else {
            selected_map
                .as_ref()
                .map(|view| view.id.clone())
                .unwrap_or_default()
        },
        selected_map,
        available_maps: map_views,
        displayed_maps,
        geometry,
        family_options,
    })
}

fn normalize_map_views(map_views: &mut [MapViewOptionRecord]) {
    for view in map_views {
        if view.map_view.max_source_zoom.is_none() {
            view.map_view.max_source_zoom = view
                .map_view
                .levels
                .iter()
                .chain(
                    view.map_view
                        .detail
                        .iter()
                        .flat_map(|detail| detail.levels.iter()),
                )
                .map(|level| level.zoom)
                .max();
        }
        if view.map_view.max_display_zoom.is_none() {
            view.map_view.max_display_zoom = Some(view.map_view.max_zoom);
        }
    }
}

pub(crate) fn raster_map_catalog_from_nav_kv(
    store: &NavKvStore,
    selected_map_id: Option<&str>,
    selected_family_id: Option<&str>,
) -> Result<crate::RasterMapCatalog, HadReadError> {
    let state = map_selector_state(store, selected_map_id, selected_family_id)?;
    serde_json::from_value(serde_json::to_value(state)?)
        .map_err(|err| HadReadError::Fatal(format!("failed to decode raster map catalog: {err}")))
}

fn chart_catalog(store: &NavKvStore) -> Result<Vec<MapViewOptionRecord>, HadReadError> {
    let mut map_views = read_required::<Vec<MapViewOptionRecord>>(
        store,
        NavKvQuery::ChartCatalog,
        "chart catalog",
    )?;
    enrich_map_views_with_package_metadata(store, &mut map_views)?;
    Ok(map_views)
}

fn enrich_map_views_with_package_metadata(
    store: &NavKvStore,
    map_views: &mut [MapViewOptionRecord],
) -> Result<(), HadReadError> {
    for view in map_views {
        enrich_map_view_with_package_metadata(store, &mut view.map_view, &view.id)?;
        if let Some(wide_angle) = view.map_view.wide_angle.as_mut() {
            enrich_wide_angle_map_view_with_package_metadata(store, wide_angle, &view.id)?;
        }
        if let Some(detail) = view.map_view.detail.as_mut() {
            enrich_detail_map_view_with_package_metadata(store, detail, &view.id)?;
        }
    }
    Ok(())
}

fn enrich_detail_map_view_with_package_metadata(
    store: &NavKvStore,
    detail: &mut DetailMapViewRecord,
    map_view_id: &str,
) -> Result<(), HadReadError> {
    let package = package_record_for_raster_source(store, &detail.package_name, map_view_id)?;
    detail.package_relative_path = Some(package_zip_relative_path(&package)?.to_string());
    detail.package_effective_date = package.effective_date.clone();
    detail.package_expiration_date = package.expiration_date.clone();
    Ok(())
}

fn enrich_map_view_with_package_metadata(
    store: &NavKvStore,
    map_view: &mut MapViewRecord,
    map_view_id: &str,
) -> Result<(), HadReadError> {
    let Some(package_id) = map_view.package_name.as_ref() else {
        return Ok(());
    };
    let package = package_record_for_raster_source(store, package_id, map_view_id)?;
    map_view.package_relative_path = Some(package_zip_relative_path(&package)?.to_string());
    map_view.package_effective_date = package.effective_date.clone();
    map_view.package_expiration_date = package.expiration_date.clone();
    map_view.full_coverage_zoom = package
        .metadata
        .and_then(|metadata| metadata.full_coverage_zoom);
    Ok(())
}

fn enrich_wide_angle_map_view_with_package_metadata(
    store: &NavKvStore,
    wide_angle: &mut WideAngleMapViewRecord,
    map_view_id: &str,
) -> Result<(), HadReadError> {
    let package = package_record_for_raster_source(store, &wide_angle.package_name, map_view_id)?;
    wide_angle.package_relative_path = Some(package_zip_relative_path(&package)?.to_string());
    wide_angle.package_effective_date = package.effective_date.clone();
    wide_angle.package_expiration_date = package.expiration_date.clone();
    Ok(())
}

fn package_record_for_raster_source(
    store: &NavKvStore,
    package_id: &str,
    map_view_id: &str,
) -> Result<PackageRecord, HadReadError> {
    read_optional::<PackageRecord>(
        store,
        NavKvQuery::PackageById {
            package_id: package_id.to_string(),
        },
    )?
    .ok_or_else(|| {
        HadReadError::Fatal(format!(
            "raster map view {map_view_id} references package {package_id}, but package/by-id/{package_id} is missing"
        ))
    })
}

fn package_zip_relative_path(package: &PackageRecord) -> Result<&str, HadReadError> {
    let relative_path = package.relative_path.as_ref().ok_or_else(|| {
        HadReadError::Fatal(format!(
            "package {} missing relative_path required for raster resources",
            package.id
        ))
    })?;
    if !relative_path.ends_with(".zip") {
        return Err(HadReadError::Fatal(format!(
            "package {} relative_path is not a zip: {}",
            package.id, relative_path
        )));
    }
    Ok(relative_path)
}

fn displayed_geometry() -> DisplayGeometryRecord {
    DisplayGeometryRecord {
        schema_version: 1,
        polygons: Vec::new(),
        polygon_sets: Vec::new(),
    }
}

fn supported_chart_families() -> [(&'static str, &'static str, &'static str); 7] {
    [
        (NO_RASTER_FAMILY_ID, "NONE", "NONE"),
        ("sec", "SECTIONAL", "SEC"),
        ("tac", "TAC", "TAC"),
        ("flyway", "FLYWAY", "FLYWAY"),
        ("enr-l", "IFR-LOW", "IFR L"),
        ("enr-h", "IFR-HIGH", "IFR H"),
        ("shaded-relief", "SHADED RELIEF", "RELIEF"),
    ]
}

fn chart_family_order(family_id: &str) -> Option<usize> {
    supported_chart_families()
        .iter()
        .position(|(id, _, _)| *id == family_id)
}

fn displayed_family_maps<'a>(
    map_views: &'a [MapViewOptionRecord],
    family_id: &str,
) -> Vec<&'a MapViewOptionRecord> {
    if matches!(family_id, "tac" | "flyway") {
        return map_views
            .iter()
            .filter(|view| {
                let chart_family = view.map_view.chart_family.as_str();
                chart_family == "sec" || chart_family == family_id
            })
            .collect();
    }
    map_views
        .iter()
        .filter(|view| view.map_view.chart_family == family_id)
        .collect()
}

fn background_maps(map_views: &[MapViewOptionRecord]) -> Vec<&MapViewOptionRecord> {
    map_views
        .iter()
        .filter(|view| view.map_view.chart_family == "world-basemap")
        .collect()
}

fn preferred_family_map<'a>(
    map_views: &'a [MapViewOptionRecord],
    family_id: &str,
    fallback_region_id: Option<&str>,
) -> Option<&'a MapViewOptionRecord> {
    let family_maps = map_views
        .iter()
        .filter(|view| view.map_view.chart_family == family_id)
        .collect::<Vec<_>>();
    family_maps
        .iter()
        .find(|view| Some(view.region_id.as_str()) == fallback_region_id)
        .copied()
        .or_else(|| family_maps.first().copied())
}

pub(crate) fn project_flight_plan_route(
    store: &NavKvStore,
    plan: &FlightPlan,
) -> Result<Vec<FlightPlanRouteSegment>, HadReadError> {
    let plan = crate::build_flight_plan(plan.clone())?;
    ensure_route_position_pages_loaded(store, &plan)?;
    project_flight_plan_route_with_resolver(&plan, |nav_ref, procedure_airport_id| {
        nav_ref_position(store, nav_ref, procedure_airport_id)
    })
}

pub(crate) fn rebuild_flight_plan_from_nav_kv(
    store: &NavKvStore,
    plan: &FlightPlan,
) -> Result<FlightPlan, HadReadError> {
    let plan = crate::build_flight_plan(plan.clone())?;
    let mut missing_pages = HadReadPageCollector::default();
    let mut replacements = Vec::new();

    for airport_id in [
        plan.departure.as_ref(),
        plan.destination.as_ref(),
        plan.alternate.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        missing_pages.collect(nav_ref_position(
            store,
            &NavRef::Airport(airport_id.0.clone()),
            None,
        ))?;
    }

    for (component_index, component) in plan.route_components.iter().enumerate() {
        match component {
            RouteComponent::Waypoint { waypoint } => {
                missing_pages.collect(nav_ref_position(store, waypoint, None))?;
            }
            RouteComponent::Airway { airway } => {
                let branches = missing_pages.collect(read_required::<Vec<AirwayBranch>>(
                    store,
                    NavKvQuery::AirwayBranches {
                        airway_name: airway.name.clone(),
                    },
                    "airway branches",
                ))?;
                if let Some(branches) = branches {
                    let materialized = exact_airway_materialization(
                        &airway.name,
                        &branches,
                        &airway.entry,
                        &airway.exit,
                        component_index,
                    )?;
                    replacements.push((
                        component_index,
                        RouteComponent::Airway {
                            airway: materialized.airway,
                        },
                        materialized.resolved_legs,
                    ));
                }
            }
            RouteComponent::Procedure { procedure } => {
                let materialized = missing_pages.collect(materialize_procedure(
                    store,
                    &procedure.airport_id.0,
                    &procedure.procedure_id,
                    procedure.kind.clone(),
                    procedure.runway_transition.as_deref(),
                    procedure.enroute_transition.as_deref(),
                    component_index,
                ))?;
                if let Some(materialized) = materialized {
                    replacements.push((
                        component_index,
                        RouteComponent::Procedure {
                            procedure: materialized.procedure,
                        },
                        materialized.resolved_legs,
                    ));
                }
            }
        }
    }

    let pages = missing_pages.into_pages();
    if !pages.is_empty() {
        return Err(HadReadError::NeedPages(pages));
    }
    crate::planning::rebuild_plan_with_nav_materializations(&plan, replacements)
        .map_err(HadReadError::from)
}

fn ensure_route_position_pages_loaded(
    store: &NavKvStore,
    plan: &FlightPlan,
) -> Result<(), HadReadError> {
    let keys = route_position_keys(plan);
    if keys.is_empty() {
        return Ok(());
    }
    let pages = store
        .missing_pages_for_keys(&keys)
        .map_err(HadReadError::Fatal)?;
    if pages.is_empty() {
        return Ok(());
    }
    crate::core_debug_log(
        "core.had.route_position_page_fault",
        &serde_json::json!({
            "key_count": keys.len(),
            "page_count": pages.len(),
            "pages": pages,
        }),
    );
    Err(HadReadError::NeedPages(pages))
}

fn route_position_keys(plan: &FlightPlan) -> Vec<String> {
    let mut keys = Vec::new();
    for leg in &plan.resolved_legs {
        let procedure_airport_id = leg.procedure_provenance.as_ref().and_then(|provenance| {
            (!provenance.airport_id.is_empty()).then_some(provenance.airport_id.as_str())
        });
        if leg
            .procedure_provenance
            .as_ref()
            .and_then(|provenance| provenance.display_path.as_ref())
            .is_some()
            && !crate::planning::resolved_leg_ends_in_manual_sequence(leg)
        {
            continue;
        }
        push_nav_ref_position_key(&mut keys, &leg.from, procedure_airport_id);
        push_nav_ref_position_key(&mut keys, &leg.to, procedure_airport_id);
    }
    if let Some(direct_to) = plan
        .guidance
        .as_ref()
        .filter(|guidance| guidance.sequencing_mode == SequencingMode::DirectTo)
        .and_then(|guidance| guidance.direct_to.as_ref())
    {
        push_nav_ref_position_key(&mut keys, &direct_to.start, None);
        push_nav_ref_position_key(&mut keys, &direct_to.target, None);
    }
    keys.sort();
    keys.dedup();
    keys
}

fn push_nav_ref_position_key(
    keys: &mut Vec<String>,
    nav_ref: &NavRef,
    procedure_airport_id: Option<&str>,
) {
    if let Some(key) = crate::nav_kv_key_for_query(&NavKvQuery::NavRefPosition {
        nav_ref: nav_ref.clone(),
        procedure_airport_id: procedure_airport_id.map(str::to_string),
    }) {
        keys.push(key);
    }
}

fn component_insert_anchor(
    plan: &FlightPlan,
    component_index: usize,
    before: bool,
) -> Result<NavRef, HadReadError> {
    let plan = plan.clone().normalized();
    let component = plan.route_components.get(component_index).ok_or_else(|| {
        HadReadError::Fatal(format!("component index out of bounds: {component_index}"))
    })?;
    let waypoint = match component {
        RouteComponent::Waypoint { waypoint } => Some(waypoint.clone()),
        RouteComponent::Airway { airway } => {
            if before {
                Some(airway.entry.clone())
            } else {
                Some(airway.exit.clone())
            }
        }
        RouteComponent::Procedure { .. } => {
            let mut legs = plan.resolved_legs.iter().filter(|leg| {
                matches!(
                    leg.source,
                    ResolvedLegSource::RouteComponent { component_index: index } if index == component_index
                )
            });
            if before {
                legs.next().map(|leg| leg.from.clone())
            } else {
                legs.next_back().map(|leg| leg.to.clone())
            }
        }
    };
    waypoint
        .ok_or_else(|| HadReadError::Fatal("selected component has no waypoint anchor".to_string()))
}

pub(crate) fn suggest_waypoint_identifiers(
    store: &NavKvStore,
    plan: &FlightPlan,
    component_index: usize,
    before: bool,
    query: &str,
    limit: usize,
) -> Result<Vec<WaypointIdentifierSuggestion>, HadReadError> {
    let anchor = component_insert_anchor(plan, component_index, before)?;
    let anchor_position = nav_ref_position(store, &anchor, None)?;
    suggest_waypoint_identifier_candidates(store, query, limit, anchor_position)
}

fn suggest_waypoint_identifiers_near(
    store: &NavKvStore,
    anchor: LatLon,
    query: &str,
    limit: usize,
) -> Result<Vec<WaypointIdentifierSuggestion>, HadReadError> {
    suggest_waypoint_identifier_candidates(store, query, limit, anchor)
}

fn suggest_waypoint_identifier_candidates(
    store: &NavKvStore,
    query: &str,
    limit: usize,
    anchor_position: LatLon,
) -> Result<Vec<WaypointIdentifierSuggestion>, HadReadError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    if let Some(position) = parse_spot_coordinates(query) {
        let distance_from_anchor_nm = flight_leg_distance_nm(anchor_position, position);
        return Ok(vec![WaypointIdentifierSuggestion {
            identifier: "SPOT".to_string(),
            nav_ref: waypoint_suggestion_nav_ref(&NavRef::Spot(position)),
            kind: "spot".to_string(),
            display_name: crate::planning::format_spot_coordinates(position),
            distance_text: format!("{:.0}nm", distance_from_anchor_nm),
            distance_from_anchor_nm,
            symbol_feature: None,
        }]);
    }
    let mut terms = had_key::search_terms(query);
    terms.sort();
    terms.dedup();
    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let lookup_prefixes = terms
        .iter()
        .map(|term| {
            let chars = term.chars().collect::<Vec<_>>();
            (1..=chars.len())
                .rev()
                .map(|length| chars.iter().take(length).collect::<String>())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let lookup_keys = lookup_prefixes
        .iter()
        .flatten()
        .filter_map(|prefix| {
            crate::nav_kv_key_for_query(&NavKvQuery::WaypointSearchPrefix {
                prefix: prefix.clone(),
            })
        })
        .collect::<Vec<_>>();
    let missing_pages = store
        .missing_pages_for_keys(&lookup_keys)
        .map_err(HadReadError::Fatal)?;
    if !missing_pages.is_empty() {
        return Err(HadReadError::NeedPages(missing_pages));
    }

    let mut matches_by_term = Vec::<BTreeMap<(String, String), WaypointSearchRecord>>::new();
    for (term, prefixes) in terms.iter().zip(&lookup_prefixes) {
        let mut bucket = None;
        for prefix in prefixes {
            if let Some(records) = read_optional::<Vec<WaypointSearchRecord>>(
                store,
                NavKvQuery::WaypointSearchPrefix {
                    prefix: prefix.clone(),
                },
            )? {
                bucket = Some(records);
                break;
            }
        }
        let Some(bucket) = bucket else {
            // No shard means the term is deliberately too broad. Other terms
            // can still constrain the query.
            continue;
        };
        let mut term_matches = BTreeMap::new();
        for candidate in bucket
            .into_iter()
            .filter(|candidate| candidate.matched_term.starts_with(term))
        {
            let key = (candidate.kind.clone(), candidate.identifier.clone());
            term_matches
                .entry(key)
                .and_modify(|current: &mut WaypointSearchRecord| {
                    if candidate.match_kind < current.match_kind {
                        *current = candidate.clone();
                    }
                })
                .or_insert(candidate);
        }
        if term_matches.is_empty() {
            return Ok(Vec::new());
        }
        matches_by_term.push(term_matches);
    }
    let Some(first_term_matches) = matches_by_term.first() else {
        return Ok(Vec::new());
    };
    let candidate_keys = first_term_matches
        .keys()
        .filter(|key| {
            matches_by_term
                .iter()
                .skip(1)
                .all(|term_matches| term_matches.contains_key(*key))
        })
        .cloned()
        .collect::<Vec<_>>();

    let ranked_candidates = candidate_keys
        .into_iter()
        .filter_map(|key| {
            matches_by_term
                .iter()
                .filter_map(|term_matches| term_matches.get(&key))
                .min_by_key(|candidate| candidate.match_kind)
                .cloned()
                .and_then(|candidate| {
                    waypoint_search_record_nav_ref(&candidate)
                        .map(|nav_ref| (candidate.match_kind, candidate, nav_ref))
                })
        })
        .collect::<Vec<_>>();
    let symbol_keys = ranked_candidates
        .iter()
        .filter_map(|(_, _, nav_ref)| {
            crate::nav_kv_key_for_query(&NavKvQuery::NavRefSymbol {
                nav_ref: nav_ref.clone(),
            })
        })
        .collect::<Vec<_>>();
    let missing_pages = store
        .missing_pages_for_keys(&symbol_keys)
        .map_err(HadReadError::Fatal)?;
    if !missing_pages.is_empty() {
        return Err(HadReadError::NeedPages(missing_pages));
    }

    let mut suggestions = ranked_candidates
        .into_iter()
        .filter_map(|(match_kind, candidate, nav_ref)| {
            let symbol_feature = match nav_symbol_feature(store, &nav_ref) {
                Ok(symbol_feature) => symbol_feature,
                Err(err) => return Some(Err(err)),
            };
            if matches!(
                nav_ref,
                NavRef::Navaid(_) | NavRef::ArincNavaid { .. } | NavRef::TerminalNavaid { .. }
            ) && symbol_feature.is_none()
            {
                return None;
            }
            let distance_from_anchor_nm = flight_leg_distance_nm(
                anchor_position,
                LatLon {
                    lat: candidate.lat,
                    lon: candidate.lon,
                },
            );
            Some(Ok((
                match_kind,
                WaypointIdentifierSuggestion {
                    identifier: candidate.identifier,
                    nav_ref: waypoint_suggestion_nav_ref(&nav_ref),
                    kind: candidate.kind,
                    display_name: candidate.display_name,
                    distance_text: format!("{:.0}nm", distance_from_anchor_nm),
                    distance_from_anchor_nm,
                    symbol_feature,
                },
            )))
        })
        .collect::<Result<Vec<_>, HadReadError>>()?;
    suggestions.sort_by(|(left_match, left), (right_match, right)| {
        left_match
            .cmp(right_match)
            .then_with(|| {
                left.distance_from_anchor_nm
                    .partial_cmp(&right.distance_from_anchor_nm)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.identifier.cmp(&right.identifier))
            .then_with(|| {
                waypoint_suggestion_nav_ref_kind_order(&left.nav_ref)
                    .cmp(&waypoint_suggestion_nav_ref_kind_order(&right.nav_ref))
            })
    });
    suggestions.truncate(limit);
    Ok(suggestions
        .into_iter()
        .map(|(_, suggestion)| suggestion)
        .collect())
}

fn waypoint_suggestion_nav_ref(
    nav_ref: &NavRef,
) -> app_ui_contracts::nav_query::WaypointSuggestionNavRef {
    use app_ui_contracts::nav_query::{
        WaypointSuggestionNavRef as UiNavRef, WaypointSuggestionPosition,
    };

    let position = |position: &LatLon| WaypointSuggestionPosition {
        lat: position.lat,
        lon: position.lon,
    };
    match nav_ref {
        NavRef::Airport(code) => UiNavRef::Airport { code: code.clone() },
        NavRef::Navaid(code) => UiNavRef::Navaid { code: code.clone() },
        NavRef::ArincNavaid {
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => UiNavRef::ArincNavaid {
            identifier: identifier.clone(),
            icao_code: icao_code.clone(),
            section_code: section_code.clone(),
            subsection_code: subsection_code.clone(),
        },
        NavRef::TerminalNavaid {
            airport_id,
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => UiNavRef::TerminalNavaid {
            airport_id: airport_id.clone(),
            identifier: identifier.clone(),
            icao_code: icao_code.clone(),
            section_code: section_code.clone(),
            subsection_code: subsection_code.clone(),
        },
        NavRef::Fix(code) => UiNavRef::Fix { code: code.clone() },
        NavRef::LatLon(value) => UiNavRef::LatLon {
            position: position(value),
        },
        NavRef::Spot(value) => UiNavRef::Spot {
            position: position(value),
        },
    }
}

fn waypoint_suggestion_nav_ref_kind_order(
    nav_ref: &app_ui_contracts::nav_query::WaypointSuggestionNavRef,
) -> usize {
    use app_ui_contracts::nav_query::WaypointSuggestionNavRef;

    match nav_ref {
        WaypointSuggestionNavRef::Airport { .. } => 0,
        WaypointSuggestionNavRef::Navaid { .. }
        | WaypointSuggestionNavRef::ArincNavaid { .. }
        | WaypointSuggestionNavRef::TerminalNavaid { .. } => 1,
        WaypointSuggestionNavRef::Fix { .. } => 2,
        WaypointSuggestionNavRef::LatLon { .. } | WaypointSuggestionNavRef::Spot { .. } => 3,
    }
}

fn waypoint_search_record_nav_ref(record: &WaypointSearchRecord) -> Option<NavRef> {
    let identifier = record.identifier.trim().to_ascii_uppercase();
    if identifier.is_empty() {
        return None;
    }
    match record.kind.trim() {
        "airport" => Some(NavRef::Airport(identifier)),
        "navaid" => Some(NavRef::Navaid(identifier)),
        "fix" => Some(NavRef::Fix(identifier)),
        _ => None,
    }
}

fn resolve_waypoint_identifier_for_ui(
    store: &NavKvStore,
    identifier: &str,
) -> Result<Option<NavRef>, HadReadError> {
    if let Some(position) = parse_spot_coordinates(identifier) {
        return Ok(Some(NavRef::Spot(position)));
    }
    let normalized_identifier = identifier.trim().to_ascii_uppercase();
    let nav_ref = read_optional::<NavRef>(
        store,
        NavKvQuery::WaypointIdentifier {
            identifier: normalized_identifier.clone(),
        },
    )?;
    let Some(nav_ref) = nav_ref else {
        return Ok(None);
    };
    if !waypoint_identifier_is_canonical_for_ui(&normalized_identifier, &nav_ref) {
        return Ok(None);
    }
    if !waypoint_identifier_nav_ref_is_acceptable_for_ui(store, &nav_ref)? {
        return Ok(None);
    }
    Ok(Some(nav_ref))
}

fn parse_spot_coordinates(value: &str) -> Option<LatLon> {
    let (lat, lon) = value.trim().split_once(',')?;
    if lon.contains(',') {
        return None;
    }
    let lat = lat.trim().parse::<f64>().ok()?;
    let lon = lon.trim().parse::<f64>().ok()?;
    if !lat.is_finite()
        || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return None;
    }
    Some(LatLon { lat, lon })
}

fn waypoint_identifier_nav_ref_is_acceptable_for_ui(
    store: &NavKvStore,
    nav_ref: &NavRef,
) -> Result<bool, HadReadError> {
    match nav_ref {
        NavRef::Navaid(_) | NavRef::ArincNavaid { .. } | NavRef::TerminalNavaid { .. } => {
            Ok(nav_symbol_feature(store, nav_ref)?.is_some())
        }
        NavRef::Airport(_) | NavRef::Fix(_) | NavRef::LatLon(_) | NavRef::Spot(_) => Ok(true),
    }
}

fn waypoint_identifier_is_canonical_for_ui(identifier: &str, nav_ref: &NavRef) -> bool {
    match nav_ref {
        NavRef::Airport(code) => identifier.trim().eq_ignore_ascii_case(code.trim()),
        NavRef::Navaid(_)
        | NavRef::ArincNavaid { .. }
        | NavRef::TerminalNavaid { .. }
        | NavRef::Fix(_)
        | NavRef::LatLon(_)
        | NavRef::Spot(_) => true,
    }
}

fn suggest_airways_near_anchor(
    store: &NavKvStore,
    anchor: &NavRef,
    limit: usize,
) -> Result<Vec<AirwaySuggestion>, HadReadError> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let anchor_position = nav_ref_position(store, anchor, None)?;
    let mut points = Vec::new();
    for radius_nm in [25.0, 50.0, 100.0, 200.0, 400.0] {
        for (lat_tile, lon_tile) in airway_spatial_tiles(anchor_position, radius_nm) {
            if let Some(tile_points) = read_optional::<Vec<AirwaySpatialPoint>>(
                store,
                NavKvQuery::AirwaySpatial { lat_tile, lon_tile },
            )? {
                points.extend(tile_points);
            }
        }
        let mut suggestions = suggestions_from_airway_points(anchor_position, &points, limit);
        if suggestions.len() >= limit || radius_nm == 400.0 {
            suggestions.truncate(limit);
            return Ok(suggestions);
        }
    }
    Ok(Vec::new())
}

fn airway_spatial_tiles(anchor: LatLon, radius_nm: f64) -> Vec<(i32, i32)> {
    let lat_delta = radius_nm / 60.0;
    let lon_delta = radius_nm / (60.0 * anchor.lat.to_radians().cos().abs().max(0.1));
    let min_lat = (anchor.lat - lat_delta).floor() as i32;
    let max_lat = (anchor.lat + lat_delta).floor() as i32;
    let min_lon = (anchor.lon - lon_delta).floor() as i32;
    let max_lon = (anchor.lon + lon_delta).floor() as i32;
    let mut tiles = Vec::new();
    for lat_tile in min_lat..=max_lat {
        for lon_tile in min_lon..=max_lon {
            tiles.push((lat_tile, lon_tile));
        }
    }
    tiles
}

fn suggestions_from_airway_points(
    anchor_position: LatLon,
    points: &[AirwaySpatialPoint],
    limit: usize,
) -> Vec<AirwaySuggestion> {
    let mut seen = HashMap::<String, AirwaySuggestion>::new();
    for point in points {
        let distance_from_anchor_nm = flight_leg_distance_nm(anchor_position, point.position);
        let suggestion = AirwaySuggestion {
            airway_name: point.airway_name.clone(),
            nearest_branch_key: Some(point.branch_key.clone()),
            nearest_nav_ref: point.nav_ref.clone(),
            nearest_sequence: point.sequence,
            distance_from_anchor_nm,
        };
        match seen.get(&point.airway_name) {
            Some(existing) if existing.distance_from_anchor_nm <= distance_from_anchor_nm => {}
            _ => {
                seen.insert(point.airway_name.clone(), suggestion);
            }
        }
    }
    let mut suggestions = seen.into_values().collect::<Vec<_>>();
    suggestions.sort_by(|left, right| {
        left.distance_from_anchor_nm
            .partial_cmp(&right.distance_from_anchor_nm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.airway_name.cmp(&right.airway_name))
    });
    suggestions.truncate(limit);
    suggestions
}

pub(crate) fn prepare_airway_presentation_for_anchors(
    store: &NavKvStore,
    airway_name: &str,
    origin_anchor: &NavRef,
    destination_anchor: Option<&NavRef>,
) -> Result<AirwayPresentationPlan, HadReadError> {
    let branches = read_required::<Vec<AirwayBranch>>(
        store,
        NavKvQuery::AirwayBranches {
            airway_name: airway_name.to_string(),
        },
        "airway branches",
    )?;
    let origin_position = nav_ref_position(store, origin_anchor, None)?;
    let destination_position = destination_anchor
        .map(|anchor| nav_ref_position(store, anchor, None))
        .transpose()?;
    prepare_airway_presentation(airway_name, branches, origin_position, destination_position)
        .map_err(Into::into)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MaterializedAirwayResponse {
    pub(crate) airway: AirwaySegment,
    pub(crate) resolved_legs: Vec<ResolvedLeg>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FlightPlanEntryTokenState {
    Neutral,
    Recognized,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct FlightPlanEntryToken {
    start: usize,
    end: usize,
    state: FlightPlanEntryTokenState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct FlightPlanEntryIssue {
    start: usize,
    end: usize,
    message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct FlightPlanEntryPreview {
    can_commit: bool,
    tokens: Vec<FlightPlanEntryToken>,
    issues: Vec<FlightPlanEntryIssue>,
}

#[derive(Debug, Clone, PartialEq)]
struct ParsedInputToken {
    text: String,
    start: usize,
    end: usize,
    terminated: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum RecognizedInputToken {
    Waypoint(NavRef),
    Airway {
        airway_name: String,
        branches: Vec<AirwayBranch>,
    },
}

#[derive(Debug, Clone, PartialEq)]
struct EvaluatedInputToken {
    parsed: ParsedInputToken,
    token_state: FlightPlanEntryTokenState,
    recognized: Option<RecognizedInputToken>,
}

#[derive(Debug, Clone, PartialEq)]
struct ExactAirwayMaterialization {
    airway: AirwaySegment,
    resolved_legs: Vec<ResolvedLeg>,
}

pub(crate) fn materialize_airway_presentation_selection(
    store: &NavKvStore,
    start_component_index: usize,
    selection: AirwayPresentationSelection,
) -> Result<MaterializedAirwayResponse, HadReadError> {
    if selection.entry_point_uid == selection.exit_point_uid {
        return Err(HadReadError::Fatal(
            "airway presentation exit cannot be the entry point".to_string(),
        ));
    }
    let branches = read_required::<Vec<AirwayBranch>>(
        store,
        NavKvQuery::AirwayBranches {
            airway_name: selection.airway_name.clone(),
        },
        "airway branches",
    )?;
    let branch = branches
        .iter()
        .find(|branch| {
            branch.branch_key == selection.branch_key
                && branch.display_name == selection.airway_name
        })
        .ok_or_else(|| HadReadError::Fatal("selected airway presentation is stale".to_string()))?;
    let point_index_for_uid = |uid: &str| {
        branch
            .points
            .iter()
            .enumerate()
            .find(|(index, point)| {
                crate::airway_presentation_point_uid(&branch.branch_key, *index, point.sequence)
                    == uid
            })
            .map(|(index, _)| index)
    };
    let entry_index = point_index_for_uid(&selection.entry_point_uid)
        .ok_or_else(|| HadReadError::Fatal("selected airway entry choice is stale".to_string()))?;
    let exit_index = point_index_for_uid(&selection.exit_point_uid)
        .ok_or_else(|| HadReadError::Fatal("selected airway exit choice is stale".to_string()))?;
    let direction = if exit_index > entry_index {
        1_isize
    } else {
        -1
    };
    let entry_point = &branch.points[entry_index];
    let exit_point = &branch.points[exit_index];
    let adjacent_nav_ref = |index: usize, offset: isize| {
        index
            .checked_add_signed(offset)
            .and_then(|index| branch.points.get(index))
            .map(|point| point.nav_ref.clone())
    };
    let entry = AirwayEntryCandidate {
        airway_name: selection.airway_name.clone(),
        branch_key: selection.branch_key.clone(),
        branch_point_index: entry_index,
        sequence: entry_point.sequence,
        nav_ref: entry_point.nav_ref.clone(),
        distance_from_anchor_nm: 0.0,
        previous_nav_ref: adjacent_nav_ref(entry_index, -direction),
        next_nav_ref: adjacent_nav_ref(entry_index, direction),
    };
    let exit = AirwayExitCandidate {
        airway_name: selection.airway_name,
        branch_key: selection.branch_key,
        branch_point_index: exit_index,
        sequence: exit_point.sequence,
        nav_ref: exit_point.nav_ref.clone(),
        leg_offset_from_entry: exit_index as isize - entry_index as isize,
        is_entry: false,
        distance_from_target_nm: None,
    };
    let (airway, resolved_legs) =
        materialize_airway_from_branches(start_component_index, &entry, &exit, &branches)?;
    Ok(MaterializedAirwayResponse {
        airway,
        resolved_legs,
    })
}

fn materialize_airway_from_branches(
    component_index: usize,
    entry: &AirwayEntryCandidate,
    exit: &AirwayExitCandidate,
    branches: &[AirwayBranch],
) -> Result<(AirwaySegment, Vec<ResolvedLeg>), HadReadError> {
    if entry.airway_name != exit.airway_name || entry.branch_key != exit.branch_key {
        return Err(HadReadError::Fatal(format!(
            "entry airway {} branch {} does not match exit airway {} branch {}",
            entry.airway_name, entry.branch_key, exit.airway_name, exit.branch_key
        )));
    }
    let branch = branches
        .iter()
        .find(|branch| branch.branch_key == entry.branch_key)
        .ok_or_else(|| {
            HadReadError::Fatal(format!(
                "unknown airway branch {} {}",
                entry.airway_name, entry.branch_key
            ))
        })?;
    let entry_point = branch.points.get(entry.branch_point_index).ok_or_else(|| {
        HadReadError::Fatal(format!(
            "entry index {} is out of bounds for airway {} branch {}",
            entry.branch_point_index, entry.airway_name, entry.branch_key
        ))
    })?;
    let exit_point = branch.points.get(exit.branch_point_index).ok_or_else(|| {
        HadReadError::Fatal(format!(
            "exit index {} is out of bounds for airway {} branch {}",
            exit.branch_point_index, entry.airway_name, entry.branch_key
        ))
    })?;
    if entry.branch_point_index == exit.branch_point_index {
        return Err(HadReadError::Fatal(
            "airway entry and exit cannot be the same point".to_string(),
        ));
    }
    let slice = if entry.branch_point_index < exit.branch_point_index {
        &branch.points[entry.branch_point_index..=exit.branch_point_index]
    } else {
        &branch.points[exit.branch_point_index..=entry.branch_point_index]
    };
    let traversed = if entry.branch_point_index < exit.branch_point_index {
        slice.to_vec()
    } else {
        slice.iter().rev().cloned().collect::<Vec<_>>()
    };
    let resolved_legs = traversed
        .windows(2)
        .enumerate()
        .map(|(index, pair)| ResolvedLeg {
            id: format!("airway-{}-{index}", branch.branch_key),
            from: pair[0].nav_ref.clone(),
            to: pair[1].nav_ref.clone(),
            source: ResolvedLegSource::RouteComponent { component_index },
            procedure_provenance: None,
        })
        .collect::<Vec<_>>();
    Ok((
        AirwaySegment {
            name: branch.display_name.clone(),
            branch_key: Some(branch.branch_key.clone()),
            entry: entry_point.nav_ref.clone(),
            exit: exit_point.nav_ref.clone(),
        },
        resolved_legs,
    ))
}

fn describe_procedure_options(
    store: &NavKvStore,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
) -> Result<ProcedureOptions, HadReadError> {
    describe_procedure_options_from_geometry_keys(store, airport_id, procedure_id, kind)
}

pub(crate) fn materialize_procedure(
    store: &NavKvStore,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
    runway_transition: Option<&str>,
    enroute_transition: Option<&str>,
    component_index: usize,
) -> Result<MaterializedProcedure, HadReadError> {
    let mut record = read_required::<pgt::ProcedureGeometryRecord>(
        store,
        NavKvQuery::ProcedureGeometry {
            airport_id: airport_id.to_string(),
            procedure_kind: kind.clone(),
            procedure_id: procedure_id.to_string(),
            runway_transition: runway_transition.map(str::to_string),
            enroute_transition: enroute_transition.map(str::to_string),
        },
        "procedure geometry",
    )?;
    record.key = pgt::ProcedureGeometryKey {
        airport_id: airport_id.trim().to_string(),
        procedure_id: procedure_id.trim().to_string(),
        kind: procedure_kind_to_geometry(&kind),
        runway_transition: runway_transition.map(str::to_string),
        enroute_transition: enroute_transition.map(str::to_string),
    };
    expand_procedure_geometry_segments(store, &mut record)?;
    pgt::populate_derived_procedure_geometry_fields(&mut record);
    let display_label = procedure_display_label(store, airport_id, procedure_id, &kind)?;
    let mut materialized = materialized_procedure_from_geometry_record(record, component_index)
        .map_err(HadReadError::from)?;
    materialized.procedure.display_label = Some(display_label);
    Ok(materialized)
}

fn expand_procedure_geometry_segments(
    store: &NavKvStore,
    record: &mut pgt::ProcedureGeometryRecord,
) -> Result<(), HadReadError> {
    if record.components.is_empty() {
        return Ok(());
    }

    let mut leg_bundles = Vec::new();
    for component in &record.components {
        match component {
            pgt::ProcedureGeometryComponent::LegBundles {
                leg_bundles: inline,
            } => leg_bundles.extend(inline.clone()),
            pgt::ProcedureGeometryComponent::SegmentRef { segment_ref } => {
                let key = pgt::procedure_geometry_segment_navdb_key(segment_ref);
                let segment = read_required_key::<pgt::ProcedureGeometrySegmentRecord>(
                    store,
                    &key,
                    "procedure geometry segment",
                )?;
                leg_bundles.extend(segment.leg_bundles);
            }
        }
    }
    record.leg_bundles = leg_bundles;
    Ok(())
}

const NO_PROCEDURE_GEOMETRY_REASON: &str = "No geometry; use Plates page.";

fn list_procedures(
    store: &NavKvStore,
    airport_id: &str,
    kind: ProcedureKind,
) -> Result<Vec<ProcedureSummary>, HadReadError> {
    let geometry_prefix = crate::navkv::procedure_geometry_kind_prefix(airport_id, &kind);
    let plate_prefix = format!("plate/cifp/{}/", airport_id.trim().to_ascii_uppercase());
    let mut missing_pages = HadReadPageCollector::default();
    let geometry_keys = missing_pages.collect(
        nav_kv_prefix_keys(store, &geometry_prefix).map(|keys| keys.collect::<Vec<_>>()),
    )?;
    let plate_keys = missing_pages
        .collect(nav_kv_prefix_keys(store, &plate_prefix).map(|keys| keys.collect::<Vec<_>>()))?;
    let plate_airport = missing_pages.collect(read_optional::<PlateAirportRecord>(
        store,
        NavKvQuery::PlateAirport {
            airport_id: airport_id.to_string(),
        },
    ))?;
    let pages = missing_pages.into_pages();
    if !pages.is_empty() {
        return Err(HadReadError::NeedPages(pages));
    }

    let geometry_ids = geometry_keys
        .unwrap_or_default()
        .into_iter()
        .filter_map(|key| {
            let suffix = key.strip_prefix(&geometry_prefix)?;
            let procedure_id = suffix.split('/').next()?;
            decode_key_component(procedure_id).ok()
        })
        .collect::<BTreeSet<_>>();

    let mut plate_rows_by_procedure = BTreeMap::<String, Vec<CifpTppMatchRow>>::new();
    let mut missing_pages = HadReadPageCollector::default();
    for key in plate_keys.unwrap_or_default() {
        let Some(procedure_id) = key
            .strip_prefix(&plate_prefix)
            .and_then(|component| decode_key_component(component).ok())
        else {
            continue;
        };
        let Some(rows) = missing_pages.collect(read_optional::<Vec<CifpTppMatchRow>>(
            store,
            NavKvQuery::PlateCifpMatch {
                airport_id: airport_id.to_string(),
                cifp_id: procedure_id,
            },
        ))?
        else {
            continue;
        };
        for row in rows.unwrap_or_default().into_iter().filter(|row| {
            row.airport_id
                .trim()
                .eq_ignore_ascii_case(airport_id.trim())
                && row.procedure_kind == kind
                && !row.cifp_id.trim().is_empty()
        }) {
            plate_rows_by_procedure
                .entry(row.cifp_id.trim().to_string())
                .or_default()
                .push(row);
        }
    }
    let pages = missing_pages.into_pages();
    if !pages.is_empty() {
        return Err(HadReadError::NeedPages(pages));
    }

    let charted_labels_by_procedure = plate_airport
        .flatten()
        .into_iter()
        .flat_map(|airport| airport.charted_procedures)
        .filter(|procedure| procedure.kind == kind)
        .map(|procedure| (procedure.procedure_id, procedure.display_label))
        .collect::<BTreeMap<_, _>>();

    let procedure_ids = geometry_ids
        .iter()
        .cloned()
        .chain(plate_rows_by_procedure.keys().cloned())
        .chain(charted_labels_by_procedure.keys().cloned())
        .collect::<BTreeSet<_>>();
    let mut procedures = Vec::with_capacity(procedure_ids.len());
    for procedure_id in procedure_ids {
        let match_rows = plate_rows_by_procedure.remove(&procedure_id);
        let display_label = if match_rows.is_some() {
            procedure_display_label_from_match_rows(airport_id, &procedure_id, &kind, match_rows)?
        } else if let Some(label) = charted_labels_by_procedure.get(&procedure_id) {
            label.clone()
        } else {
            procedure_display_label_from_match_rows(airport_id, &procedure_id, &kind, None)?
        };
        let enabled = geometry_ids.contains(&procedure_id);
        procedures.push(ProcedureSummary {
            airport_id: airport_id.trim().to_string(),
            procedure_id,
            display_label,
            accent_category: kind.accent_category().to_string(),
            kind: kind.clone(),
            enabled,
            disabled_reason: (!enabled).then(|| NO_PROCEDURE_GEOMETRY_REASON.to_string()),
        });
    }
    crate::disambiguate_duplicate_procedure_display_labels(&mut procedures);
    Ok(procedures)
}

pub(crate) fn procedure_display_label(
    store: &NavKvStore,
    airport_id: &str,
    procedure_id: &str,
    kind: &ProcedureKind,
) -> Result<String, HadReadError> {
    let rows = read_optional::<Vec<CifpTppMatchRow>>(
        store,
        NavKvQuery::PlateCifpMatch {
            airport_id: airport_id.to_string(),
            cifp_id: procedure_id.to_string(),
        },
    )?;
    procedure_display_label_from_match_rows(airport_id, procedure_id, kind, rows)
}

fn procedure_display_label_from_match_rows(
    airport_id: &str,
    procedure_id: &str,
    kind: &ProcedureKind,
    rows: Option<Vec<CifpTppMatchRow>>,
) -> Result<String, HadReadError> {
    let matched = rows.and_then(|rows| {
        crate::select_preferred_cifp_tpp_match(
            rows.into_iter()
                .filter(|row| row.procedure_kind == *kind)
                .collect(),
        )
    });
    let Some(matched) = matched else {
        if *kind != ProcedureKind::Approach {
            return Ok(procedure_id.trim().to_string());
        }
        return Err(HadReadError::Fatal(format!(
            "approach {airport_id} {procedure_id} has no preferred plate label"
        )));
    };
    let label = matched.plate_label.trim();
    if label.is_empty() {
        return Err(HadReadError::Fatal(format!(
            "approach {airport_id} {procedure_id} has an empty plate label"
        )));
    }
    Ok(label.to_string())
}

fn describe_procedure_options_from_geometry_keys(
    store: &NavKvStore,
    airport_id: &str,
    procedure_id: &str,
    kind: ProcedureKind,
) -> Result<ProcedureOptions, HadReadError> {
    let prefix = crate::navkv::procedure_geometry_prefix(airport_id, &kind, procedure_id);
    let mut valid_choices = nav_kv_prefix_keys(store, &prefix)?
        .filter_map(|key| {
            let suffix = key.strip_prefix(&prefix)?;
            let mut parts = suffix.split('/');
            let runway_transition = decode_optional_transition_component(parts.next()?)?;
            let enroute_transition = decode_optional_transition_component(parts.next()?)?;
            if parts.next().is_some() {
                return None;
            }
            Some(crate::ProcedureSpecChoice {
                runway_transition,
                enroute_transition,
                label: String::new(),
            })
        })
        .collect::<Vec<_>>();
    valid_choices.sort_by(|left, right| {
        left.runway_transition
            .cmp(&right.runway_transition)
            .then_with(|| left.enroute_transition.cmp(&right.enroute_transition))
    });
    valid_choices.dedup();

    let mut runway_transitions = valid_choices
        .iter()
        .filter_map(|choice| choice.runway_transition.clone())
        .collect::<Vec<_>>();
    runway_transitions.sort();
    runway_transitions.dedup();

    let mut enroute_transitions = valid_choices
        .iter()
        .filter_map(|choice| choice.enroute_transition.clone())
        .collect::<Vec<_>>();
    enroute_transitions.sort();
    enroute_transitions.dedup();

    for choice in &mut valid_choices {
        choice.label = crate::procedure_picker_choice_label(
            &kind,
            choice.runway_transition.as_deref(),
            choice.enroute_transition.as_deref(),
        );
    }

    Ok(ProcedureOptions {
        airport_id: airport_id.trim().to_string(),
        procedure_id: procedure_id.trim().to_string(),
        kind,
        runway_transitions,
        enroute_transitions,
        has_common_segment: valid_choices.len() > 1,
        empty_message: "No published routes are available.".to_string(),
        valid_choices,
    })
}

fn nav_kv_prefix_keys(
    store: &NavKvStore,
    prefix: &str,
) -> Result<std::vec::IntoIter<String>, HadReadError> {
    match store
        .keys_with_prefix_lookup(prefix)
        .map_err(HadReadError::Fatal)?
    {
        NavKvLookup::Hit(bytes) => {
            let text =
                String::from_utf8(bytes).map_err(|err| HadReadError::Fatal(err.to_string()))?;
            Ok(text
                .lines()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
                .into_iter())
        }
        NavKvLookup::MissingPages(pages) => Err(HadReadError::NeedPages(pages)),
        NavKvLookup::MissingKey => Ok(Vec::new().into_iter()),
    }
}

fn decode_optional_transition_component(component: &str) -> Option<Option<String>> {
    if component == "_" {
        Some(None)
    } else {
        decode_key_component(component).ok().map(Some)
    }
}

fn decode_key_component(component: &str) -> Result<String, String> {
    let bytes = component.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().ok_or("truncated escape")?;
            let low = bytes.get(index + 2).copied().ok_or("truncated escape")?;
            out.push((hex_value(high)? << 4) | hex_value(low)?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|err| err.to_string())
}

fn hex_value(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex digit {}", byte as char)),
    }
}

fn materialized_procedure_from_geometry_record(
    record: pgt::ProcedureGeometryRecord,
    component_index: usize,
) -> AppResult<MaterializedProcedure> {
    let kind = procedure_kind_from_geometry(record.key.kind.clone());
    let terminal_discontinuity = record
        .terminal_discontinuity
        .clone()
        .map(procedure_discontinuity_from_geometry);
    let resolved_legs = record
        .leg_bundles
        .iter()
        .enumerate()
        .map(|(bundle_index, bundle)| {
            resolved_leg_from_geometry_bundle(&record, bundle, bundle_index, component_index)
        })
        .collect::<AppResult<Vec<_>>>()?;
    let data_quality = record
        .data_quality
        .into_iter()
        .map(|annotation| annotation.message)
        .collect::<Vec<_>>();

    Ok(MaterializedProcedure {
        procedure: ProcedureSegment {
            airport_id: AirportId(record.key.airport_id.trim().to_string()),
            procedure_id: record.key.procedure_id.trim().to_string(),
            display_label: None,
            kind,
            runway_transition: record.key.runway_transition,
            enroute_transition: record.key.enroute_transition,
            terminal_discontinuity,
            data_quality: data_quality.clone(),
        },
        resolved_legs,
    })
}

fn resolved_leg_from_geometry_bundle(
    record: &pgt::ProcedureGeometryRecord,
    bundle: &pgt::ProcedureGeometryLegBundle,
    bundle_index: usize,
    component_index: usize,
) -> AppResult<ResolvedLeg> {
    Ok(ResolvedLeg {
        id: derived_procedure_geometry_leg_id(record, bundle_index),
        from: nav_ref_from_geometry(bundle.from.clone()),
        to: nav_ref_from_geometry(bundle.to.clone()),
        source: ResolvedLegSource::RouteComponent { component_index },
        procedure_provenance: Some(ProcedureLegProvenance {
            airport_id: record.key.airport_id.clone(),
            procedure_id: record.key.procedure_id.clone(),
            kind: procedure_kind_from_geometry(record.key.kind.clone()),
            role: procedure_segment_role_from_geometry(bundle.role.clone()),
            path_termination: path_termination_from_geometry(bundle.path_termination.clone()),
            leg_sequence: bundle.leg_sequence,
            discontinuity_after: bundle
                .discontinuity_after
                .clone()
                .map(procedure_discontinuity_from_geometry),
            display_path: Some(display_path_from_geometry(bundle.path.clone())),
        }),
    })
}

fn derived_procedure_geometry_leg_id(
    record: &pgt::ProcedureGeometryRecord,
    bundle_index: usize,
) -> String {
    format!(
        "procedure-{}-{}-{}-{}-{}-{}",
        record.key.airport_id.trim(),
        pgt::procedure_kind_component(&record.key.kind),
        record.key.procedure_id.trim(),
        procedure_transition_id_component(record.key.runway_transition.as_deref()),
        procedure_transition_id_component(record.key.enroute_transition.as_deref()),
        bundle_index
    )
}

fn procedure_transition_id_component(value: Option<&str>) -> &str {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("_")
}

fn display_path_from_geometry(path: pgt::ProcedureGeometryPath) -> LegDisplayPath {
    LegDisplayPath {
        style: match path.style {
            pgt::ProcedureGeometryPathStyle::Solid => LegDisplayPathStyle::Solid,
            pgt::ProcedureGeometryPathStyle::Dashed => LegDisplayPathStyle::Dashed,
        },
        elements: path
            .elements
            .into_iter()
            .map(|element| match element {
                pgt::ProcedureGeometryElement::Segment { start, end } => {
                    LegDisplayElement::Segment {
                        start: lat_lon_from_geometry(start),
                        end: lat_lon_from_geometry(end),
                    }
                }
                pgt::ProcedureGeometryElement::Arc {
                    center,
                    radius_nm,
                    start,
                    end,
                    clockwise,
                    sweep_degrees,
                } => LegDisplayElement::Arc {
                    center: lat_lon_from_geometry(center),
                    radius_nm,
                    start: lat_lon_from_geometry(start),
                    end: lat_lon_from_geometry(end),
                    clockwise,
                    sweep_degrees,
                },
            })
            .collect(),
        effective_terminal_course_deg: path.effective_terminal_course_deg,
        debug_element_sources: Vec::new(),
        debug_element_roles: Vec::new(),
    }
}

fn lat_lon_from_geometry(value: pgt::ProcedureLatLon) -> LatLon {
    LatLon {
        lat: value.lat,
        lon: value.lon,
    }
}

fn nav_ref_from_geometry(value: pgt::ProcedureNavRef) -> NavRef {
    match value {
        pgt::ProcedureNavRef::Airport(id) => NavRef::Airport(id),
        pgt::ProcedureNavRef::Navaid(id) => NavRef::Navaid(id),
        pgt::ProcedureNavRef::Fix(id) => NavRef::Fix(id),
        pgt::ProcedureNavRef::ArincNavaid {
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => NavRef::ArincNavaid {
            identifier,
            icao_code,
            section_code,
            subsection_code,
        },
        pgt::ProcedureNavRef::TerminalNavaid {
            airport_id,
            identifier,
            icao_code,
            section_code,
            subsection_code,
        } => NavRef::TerminalNavaid {
            airport_id,
            identifier,
            icao_code,
            section_code,
            subsection_code,
        },
        pgt::ProcedureNavRef::LatLon(value) => NavRef::LatLon(lat_lon_from_geometry(value)),
    }
}

fn procedure_kind_from_geometry(kind: pgt::ProcedureKind) -> ProcedureKind {
    match kind {
        pgt::ProcedureKind::Sid => ProcedureKind::Sid,
        pgt::ProcedureKind::Star => ProcedureKind::Star,
        pgt::ProcedureKind::Approach => ProcedureKind::Approach,
    }
}

fn procedure_kind_to_geometry(kind: &ProcedureKind) -> pgt::ProcedureKind {
    match kind {
        ProcedureKind::Sid => pgt::ProcedureKind::Sid,
        ProcedureKind::Star => pgt::ProcedureKind::Star,
        ProcedureKind::Approach => pgt::ProcedureKind::Approach,
    }
}

fn procedure_discontinuity_from_geometry(
    discontinuity: pgt::ProcedureDiscontinuity,
) -> ProcedureDiscontinuity {
    match discontinuity {
        pgt::ProcedureDiscontinuity::Vectors => ProcedureDiscontinuity::Vectors,
        pgt::ProcedureDiscontinuity::Hold => ProcedureDiscontinuity::Hold,
        pgt::ProcedureDiscontinuity::Other(label) => ProcedureDiscontinuity::Other(label),
    }
}

fn procedure_segment_role_from_geometry(role: pgt::ProcedureSegmentRole) -> ProcedureSegmentRole {
    match role {
        pgt::ProcedureSegmentRole::EnrouteTransition => ProcedureSegmentRole::EnrouteTransition,
        pgt::ProcedureSegmentRole::Common => ProcedureSegmentRole::Common,
        pgt::ProcedureSegmentRole::RunwayTransition => ProcedureSegmentRole::RunwayTransition,
    }
}

fn path_termination_from_geometry(path: pgt::ProcedurePathTermination) -> PathTermination {
    match path {
        pgt::ProcedurePathTermination::InitialFix => PathTermination::InitialFix,
        pgt::ProcedurePathTermination::TrackToFix => PathTermination::TrackToFix,
        pgt::ProcedurePathTermination::CourseToFix => PathTermination::CourseToFix,
        pgt::ProcedurePathTermination::DirectToFix => PathTermination::DirectToFix,
        pgt::ProcedurePathTermination::HeadingToManual => PathTermination::HeadingToManual,
        pgt::ProcedurePathTermination::HeadingToAltitude => PathTermination::HeadingToAltitude,
        pgt::ProcedurePathTermination::Other(value) => PathTermination::Other(value),
    }
}

pub(crate) fn preview_flight_plan_entry(
    store: &NavKvStore,
    plan: &FlightPlan,
    input: &str,
) -> Result<FlightPlanEntryPreview, HadReadError> {
    let tokens = tokenize_flight_plan_entry(input);
    let evaluated = evaluate_flight_plan_entry_tokens(store, &tokens)?;
    let issues = validate_flight_plan_entry(plan, &evaluated)?;
    let can_commit = !input.trim().is_empty()
        && issues.is_empty()
        && evaluated.iter().all(|token| {
            token.recognized.is_some()
                && (token.parsed.terminated
                    || token.token_state == FlightPlanEntryTokenState::Recognized)
        })
        && append_flight_plan_tokens(plan, &evaluated).is_ok();
    Ok(FlightPlanEntryPreview {
        can_commit,
        tokens: evaluated
            .iter()
            .map(|token| FlightPlanEntryToken {
                start: token.parsed.start,
                end: token.parsed.end,
                state: token.token_state.clone(),
            })
            .collect(),
        issues,
    })
}

pub(crate) fn append_flight_plan_entry(
    store: &NavKvStore,
    plan: &FlightPlan,
    input: &str,
) -> Result<FlightPlan, HadReadError> {
    let tokens = tokenize_flight_plan_entry(input);
    let evaluated = evaluate_flight_plan_entry_tokens(store, &tokens)?;
    let issues = validate_flight_plan_entry(plan, &evaluated)?;
    if !issues.is_empty() {
        return Err(HadReadError::Fatal(
            issues
                .first()
                .map(|issue| issue.message.clone())
                .unwrap_or_else(|| "invalid route entry".to_string()),
        ));
    }
    append_flight_plan_tokens(plan, &evaluated)
}

pub(crate) fn insert_waypoint_best_position(
    store: &NavKvStore,
    plan: &FlightPlan,
    waypoint: NavRef,
) -> Result<FlightPlan, HadReadError> {
    if let Some(message) = insert_waypoint_best_position_rejection(plan, &waypoint) {
        return Err(HadReadError::Fatal(message));
    }

    let insertion_index = best_top_level_insertion_index(store, plan, &waypoint)?;
    insert_waypoint_at_top_level(plan, insertion_index, waypoint)
}

pub(crate) fn insert_waypoint_best_position_rejection(
    plan: &FlightPlan,
    waypoint: &NavRef,
) -> Option<String> {
    if flight_plan_has_direct_to_overlay(plan) {
        return Some(crate::planning::OFF_PLAN_DIRECT_TO_EDIT_DISABLED_REASON.to_string());
    }
    if !matches!(
        waypoint,
        NavRef::Airport(_)
            | NavRef::Navaid(_)
            | NavRef::Fix(_)
            | NavRef::LatLon(_)
            | NavRef::Spot(_)
    ) {
        return Some("only positioned waypoints can be inserted from map selection".to_string());
    }
    if flight_plan_contains_nav_ref(plan, waypoint) {
        return Some(format!(
            "{} is already in the flight plan",
            nav_ref_display_label(waypoint)
        ));
    }
    if !(0..=plan.route_components.len()).any(|insertion_index| {
        crate::planning::validate_waypoint_insertion_index_attachments(plan, insertion_index)
            .is_ok()
    }) {
        return Some("The flight plan has no valid waypoint insertion point.".to_string());
    }
    None
}

fn best_top_level_insertion_index(
    store: &NavKvStore,
    plan: &FlightPlan,
    waypoint: &NavRef,
) -> Result<usize, HadReadError> {
    let plan = plan.clone().normalized();
    if plan.route_components.len() <= 1 {
        return Ok(plan.route_components.len());
    }
    let waypoint_position = nav_ref_position(store, waypoint, None)?;
    let mut best: Option<(usize, f64)> = None;
    for insertion_index in 0..=plan.route_components.len() {
        if crate::planning::validate_waypoint_insertion_index_attachments(&plan, insertion_index)
            .is_err()
        {
            continue;
        }
        let prev = plan.route_components[..insertion_index]
            .iter()
            .rev()
            .find_map(route_component_last_nav_ref);
        let next = plan.route_components[insertion_index..]
            .iter()
            .find_map(route_component_first_nav_ref);
        let cost = insertion_added_length_nm(store, prev, next, waypoint_position)?;
        match best {
            Some((_, best_cost)) if cost >= best_cost => {}
            _ => best = Some((insertion_index, cost)),
        }
    }
    best.map(|(index, _)| index)
        .ok_or_else(|| HadReadError::Fatal("could not place waypoint in flight plan".to_string()))
}

fn insertion_added_length_nm(
    store: &NavKvStore,
    prev: Option<&NavRef>,
    next: Option<&NavRef>,
    waypoint_position: LatLon,
) -> Result<f64, HadReadError> {
    let prev_position = match prev {
        Some(nav_ref) => Some(nav_ref_position(store, nav_ref, None)?),
        None => None,
    };
    let next_position = match next {
        Some(nav_ref) => Some(nav_ref_position(store, nav_ref, None)?),
        None => None,
    };
    Ok(match (prev_position, next_position) {
        (Some(prev), Some(next)) => {
            flight_leg_distance_nm(prev, waypoint_position)
                + flight_leg_distance_nm(waypoint_position, next)
                - flight_leg_distance_nm(prev, next)
        }
        (Some(prev), None) => flight_leg_distance_nm(prev, waypoint_position),
        (None, Some(next)) => flight_leg_distance_nm(waypoint_position, next),
        (None, None) => 0.0,
    })
}

fn insert_waypoint_at_top_level(
    plan: &FlightPlan,
    insertion_index: usize,
    waypoint: NavRef,
) -> Result<FlightPlan, HadReadError> {
    let plan = plan.clone().normalized();
    if plan.route_components.is_empty() {
        let mut next_plan = plan;
        next_plan
            .route_components
            .push(RouteComponent::Waypoint { waypoint });
        next_plan.resolved_legs.clear();
        return Ok(next_plan.normalized());
    }
    if insertion_index >= plan.route_components.len() {
        insert_waypoint(&plan, plan.route_components.len() - 1, false, waypoint).map_err(Into::into)
    } else {
        insert_waypoint(&plan, insertion_index, true, waypoint).map_err(Into::into)
    }
}

fn route_component_first_nav_ref(component: &RouteComponent) -> Option<&NavRef> {
    match component {
        RouteComponent::Waypoint { waypoint } => Some(waypoint),
        RouteComponent::Airway { airway } => Some(&airway.entry),
        RouteComponent::Procedure { .. } => None,
    }
}

fn route_component_last_nav_ref(component: &RouteComponent) -> Option<&NavRef> {
    match component {
        RouteComponent::Waypoint { waypoint } => Some(waypoint),
        RouteComponent::Airway { airway } => Some(&airway.exit),
        RouteComponent::Procedure { .. } => None,
    }
}

fn tokenize_flight_plan_entry(input: &str) -> Vec<ParsedInputToken> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, ch) in input.char_indices() {
        if ch.is_whitespace() {
            if let Some(token_start) = start.take() {
                tokens.push(ParsedInputToken {
                    text: input[token_start..index].to_ascii_uppercase(),
                    start: token_start,
                    end: index,
                    terminated: true,
                });
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(token_start) = start {
        tokens.push(ParsedInputToken {
            text: input[token_start..].to_ascii_uppercase(),
            start: token_start,
            end: input.len(),
            terminated: false,
        });
    }
    tokens
}

fn evaluate_flight_plan_entry_tokens(
    store: &NavKvStore,
    tokens: &[ParsedInputToken],
) -> Result<Vec<EvaluatedInputToken>, HadReadError> {
    let mut evaluated = Vec::with_capacity(tokens.len());
    for token in tokens {
        let recognized = recognize_flight_plan_entry_token(store, &token.text)?;
        let token_state = match (&recognized, token.terminated) {
            (Some(_), _) => FlightPlanEntryTokenState::Recognized,
            (None, true) => FlightPlanEntryTokenState::Invalid,
            (None, false) => FlightPlanEntryTokenState::Neutral,
        };
        evaluated.push(EvaluatedInputToken {
            parsed: token.clone(),
            token_state,
            recognized,
        });
    }
    Ok(evaluated)
}

fn recognize_flight_plan_entry_token(
    store: &NavKvStore,
    token: &str,
) -> Result<Option<RecognizedInputToken>, HadReadError> {
    if token.trim().is_empty() {
        return Ok(None);
    }
    if let Some(nav_ref) = resolve_waypoint_identifier_for_ui(store, token)? {
        return Ok(Some(RecognizedInputToken::Waypoint(nav_ref)));
    }
    if let Some(branches) = read_optional::<Vec<AirwayBranch>>(
        store,
        NavKvQuery::AirwayBranches {
            airway_name: token.to_string(),
        },
    )? {
        if !branches.is_empty() {
            return Ok(Some(RecognizedInputToken::Airway {
                airway_name: token.to_string(),
                branches,
            }));
        }
    }
    Ok(None)
}

fn validate_flight_plan_entry(
    plan: &FlightPlan,
    tokens: &[EvaluatedInputToken],
) -> Result<Vec<FlightPlanEntryIssue>, HadReadError> {
    let mut issues = Vec::new();
    let mut current_anchor = trailing_component_anchor(plan);
    let mut current_anchor_token_index = None;
    let mut pending_airway: Option<(usize, NavRef, String, Vec<AirwayBranch>)> = None;

    for (index, token) in tokens.iter().enumerate() {
        let Some(recognized) = &token.recognized else {
            if token.parsed.terminated {
                issues.push(FlightPlanEntryIssue {
                    start: token.parsed.start,
                    end: token.parsed.end,
                    message: format!("unknown route element {}", token.parsed.text),
                });
            }
            break;
        };
        match recognized {
            RecognizedInputToken::Waypoint(nav_ref) => {
                if let Some((airway_index, origin_anchor, airway_name, branches)) =
                    pending_airway.take()
                {
                    if let Err(err) = exact_airway_materialization(
                        &airway_name,
                        &branches,
                        &origin_anchor,
                        nav_ref,
                        plan.route_components.len(),
                    ) {
                        let message = match err {
                            HadReadError::Fatal(message) => message,
                            HadReadError::NeedPages(_) => {
                                "airway lookup needs resources".to_string()
                            }
                        };
                        issues.push(FlightPlanEntryIssue {
                            start: tokens[airway_index].parsed.start,
                            end: token.parsed.end,
                            message,
                        });
                        break;
                    }
                }
                current_anchor = Some(nav_ref.clone());
                current_anchor_token_index = Some(index);
            }
            RecognizedInputToken::Airway {
                airway_name,
                branches,
            } => {
                let Some(origin_anchor) = current_anchor.clone() else {
                    issues.push(FlightPlanEntryIssue {
                        start: token.parsed.start,
                        end: token.parsed.end,
                        message: format!("{airway_name} requires a preceding waypoint"),
                    });
                    break;
                };
                if !airway_contains_nav_ref(airway_name, branches, &origin_anchor) {
                    let start = current_anchor_token_index
                        .and_then(|anchor_index| tokens.get(anchor_index))
                        .map(|anchor_token| anchor_token.parsed.start)
                        .unwrap_or(token.parsed.start);
                    issues.push(FlightPlanEntryIssue {
                        start,
                        end: token.parsed.end,
                        message: format!(
                            "{} not on {}",
                            nav_ref_display_label(&origin_anchor),
                            airway_name
                        ),
                    });
                    break;
                }
                if pending_airway.is_some() {
                    issues.push(FlightPlanEntryIssue {
                        start: token.parsed.start,
                        end: token.parsed.end,
                        message: "airway requires an exit waypoint".to_string(),
                    });
                    break;
                }
                pending_airway =
                    Some((index, origin_anchor, airway_name.clone(), branches.clone()));
            }
        }
    }

    if issues.is_empty() && tokens.iter().all(|token| token.recognized.is_some()) {
        if let Some((airway_index, _, airway_name, _)) = pending_airway {
            if tokens[airway_index].parsed.terminated {
                issues.push(FlightPlanEntryIssue {
                    start: tokens[airway_index].parsed.start,
                    end: tokens[airway_index].parsed.end,
                    message: format!("{airway_name} requires an exit waypoint"),
                });
            }
        }
    }
    Ok(issues)
}

fn append_flight_plan_tokens(
    plan: &FlightPlan,
    tokens: &[EvaluatedInputToken],
) -> Result<FlightPlan, HadReadError> {
    let mut next_plan = plan.clone();
    let mut index = 0usize;
    while index < tokens.len() {
        let Some(recognized) = &tokens[index].recognized else {
            return Err(HadReadError::Fatal(format!(
                "unknown route element {}",
                tokens[index].parsed.text
            )));
        };
        match recognized {
            RecognizedInputToken::Waypoint(nav_ref) => {
                next_plan = append_waypoint_tail(&next_plan, nav_ref.clone())?;
                index += 1;
            }
            RecognizedInputToken::Airway {
                airway_name,
                branches,
            } => {
                let origin_anchor = trailing_component_anchor(&next_plan).ok_or_else(|| {
                    HadReadError::Fatal(format!("{airway_name} requires a preceding waypoint"))
                })?;
                let next_token = tokens.get(index + 1).ok_or_else(|| {
                    HadReadError::Fatal(format!("{airway_name} requires an exit waypoint"))
                })?;
                let destination = match &next_token.recognized {
                    Some(RecognizedInputToken::Waypoint(nav_ref)) => nav_ref.clone(),
                    Some(RecognizedInputToken::Airway { .. }) => {
                        return Err(HadReadError::Fatal(format!(
                            "{airway_name} requires an exit waypoint"
                        )))
                    }
                    None => {
                        return Err(HadReadError::Fatal(format!(
                            "unknown route element {}",
                            next_token.parsed.text
                        )))
                    }
                };
                let materialized = exact_airway_materialization(
                    airway_name,
                    branches,
                    &origin_anchor,
                    &destination,
                    next_plan.route_components.len(),
                )?;
                next_plan = append_airway_tail(&next_plan, materialized)?;
                index += 2;
            }
        }
    }
    let normalized = next_plan.normalized();
    if normalized.resolved_legs.is_empty() && normalized.route_components.len() > 1 {
        return Err(HadReadError::Fatal(
            "flight plan append requires at least one flyable leg".to_string(),
        ));
    }
    Ok(normalized)
}

fn append_waypoint_tail(plan: &FlightPlan, waypoint: NavRef) -> Result<FlightPlan, HadReadError> {
    if plan.route_components.is_empty() {
        let mut next_plan = plan.clone();
        next_plan
            .route_components
            .push(RouteComponent::Waypoint { waypoint });
        next_plan.resolved_legs.clear();
        return Ok(next_plan.normalized());
    }
    insert_waypoint(plan, plan.route_components.len() - 1, false, waypoint).map_err(Into::into)
}

fn append_airway_tail(
    plan: &FlightPlan,
    materialized: ExactAirwayMaterialization,
) -> Result<FlightPlan, HadReadError> {
    let plan = plan.clone();
    let start_component_index = plan
        .route_components
        .len()
        .checked_sub(1)
        .ok_or_else(|| HadReadError::Fatal("cannot append airway to empty plan".to_string()))?;
    if let Some(RouteComponent::Airway { airway }) =
        plan.route_components.get(start_component_index)
    {
        if airway.exit != materialized.airway.entry {
            return Err(HadReadError::Fatal(format!(
                "{} cannot start after {}",
                materialized.airway.name,
                nav_ref_display_label(&airway.exit)
            )));
        }
        return insert_airway_after_airway(
            &plan,
            start_component_index,
            materialized.airway,
            materialized.resolved_legs,
        )
        .map_err(Into::into);
    }
    insert_airway_after_waypoint(
        &plan,
        start_component_index,
        materialized.airway,
        materialized.resolved_legs,
    )
    .map_err(Into::into)
}

fn exact_airway_materialization(
    airway_name: &str,
    branches: &[AirwayBranch],
    origin_anchor: &NavRef,
    destination_anchor: &NavRef,
    component_index: usize,
) -> Result<ExactAirwayMaterialization, HadReadError> {
    for branch in branches
        .iter()
        .filter(|branch| branch.display_name.trim() == airway_name.trim())
    {
        let Some(entry_index) = branch
            .points
            .iter()
            .position(|point| point.nav_ref == *origin_anchor)
        else {
            continue;
        };
        let Some(exit_index) = branch
            .points
            .iter()
            .position(|point| point.nav_ref == *destination_anchor)
        else {
            continue;
        };
        let entry = AirwayEntryCandidate {
            airway_name: airway_name.to_string(),
            branch_key: branch.branch_key.clone(),
            branch_point_index: entry_index,
            sequence: branch.points[entry_index].sequence,
            nav_ref: branch.points[entry_index].nav_ref.clone(),
            distance_from_anchor_nm: 0.0,
            previous_nav_ref: entry_index
                .checked_sub(1)
                .and_then(|idx| branch.points.get(idx))
                .map(|point| point.nav_ref.clone()),
            next_nav_ref: branch
                .points
                .get(entry_index + 1)
                .map(|point| point.nav_ref.clone()),
        };
        let exit = AirwayExitCandidate {
            airway_name: airway_name.to_string(),
            branch_key: branch.branch_key.clone(),
            branch_point_index: exit_index,
            sequence: branch.points[exit_index].sequence,
            nav_ref: branch.points[exit_index].nav_ref.clone(),
            leg_offset_from_entry: exit_index as isize - entry_index as isize,
            is_entry: exit_index == entry_index,
            distance_from_target_nm: Some(0.0),
        };
        let (airway, resolved_legs) =
            materialize_airway_from_branches(component_index, &entry, &exit, branches)?;
        return Ok(ExactAirwayMaterialization {
            airway,
            resolved_legs,
        });
    }
    let matching_branches = branches
        .iter()
        .filter(|branch| branch.display_name.trim() == airway_name.trim())
        .collect::<Vec<_>>();
    let origin_on_airway = matching_branches.iter().any(|branch| {
        branch
            .points
            .iter()
            .any(|point| point.nav_ref == *origin_anchor)
    });
    let destination_on_airway = matching_branches.iter().any(|branch| {
        branch
            .points
            .iter()
            .any(|point| point.nav_ref == *destination_anchor)
    });
    if !origin_on_airway {
        return Err(HadReadError::Fatal(format!(
            "{} not on {}",
            nav_ref_display_label(origin_anchor),
            airway_name
        )));
    }
    if !destination_on_airway {
        return Err(HadReadError::Fatal(format!(
            "{} not on {}",
            nav_ref_display_label(destination_anchor),
            airway_name
        )));
    }
    Err(HadReadError::Fatal(format!(
        "{} and {} are not on the same {} branch",
        nav_ref_display_label(origin_anchor),
        nav_ref_display_label(destination_anchor),
        airway_name
    )))
}

fn airway_contains_nav_ref(airway_name: &str, branches: &[AirwayBranch], nav_ref: &NavRef) -> bool {
    branches
        .iter()
        .filter(|branch| branch.display_name.trim() == airway_name.trim())
        .any(|branch| branch.points.iter().any(|point| point.nav_ref == *nav_ref))
}

fn trailing_component_anchor(plan: &FlightPlan) -> Option<NavRef> {
    match plan.route_components.last()? {
        RouteComponent::Waypoint { waypoint } => Some(waypoint.clone()),
        RouteComponent::Airway { airway } => Some(airway.exit.clone()),
        RouteComponent::Procedure { .. } => None,
    }
}

fn nav_ref_display_label(nav_ref: &NavRef) -> String {
    match nav_ref {
        NavRef::Airport(value) | NavRef::Navaid(value) | NavRef::Fix(value) => value.clone(),
        NavRef::ArincNavaid { identifier, .. } | NavRef::TerminalNavaid { identifier, .. } => {
            identifier.clone()
        }
        NavRef::LatLon(value) => format!("{:.3},{:.3}", value.lat, value.lon),
        NavRef::Spot(value) => format!("SPOT {:.3},{:.3}", value.lat, value.lon),
    }
}

pub(crate) fn describe_plate_loads(
    store: &NavKvStore,
    plan: &FlightPlan,
    plate_id: &str,
) -> Result<ProcedureLoadMenu, HadReadError> {
    let Some(rows) = read_optional::<Vec<CifpTppMatchRow>>(
        store,
        NavKvQuery::PlateProcedureCandidates {
            plate_id: plate_id.to_string(),
        },
    )?
    else {
        return Ok(crate::empty_procedure_load_menu());
    };
    let mut grouped = HashMap::<String, Vec<CifpTppMatchRow>>::new();
    for row in rows {
        grouped
            .entry(format!("{}:{}", row.airport_id, row.cifp_id))
            .or_default()
            .push(row);
    }
    let mut candidates = Vec::new();
    for match_rows in grouped.into_values() {
        let Some(preferred) = crate::select_preferred_cifp_tpp_match(match_rows.clone()) else {
            continue;
        };
        let options = describe_procedure_options_from_geometry_keys(
            store,
            &preferred.airport_id,
            &preferred.cifp_id,
            preferred.procedure_kind.clone(),
        )?;
        if options.valid_choices.is_empty() {
            continue;
        }
        candidates.push(PlateProcedureLoadCandidateInput {
            airport_id: preferred.airport_id,
            cifp_id: preferred.cifp_id,
            match_rows,
            options,
        });
    }
    describe_plate_procedure_load_menu(plan, candidates).map_err(Into::into)
}

impl From<serde_json::Error> for HadReadError {
    fn from(err: serde_json::Error) -> Self {
        Self::Fatal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{planning::NavElementUiView, AirportId, NavKvRoot, ProcedureKind, SequencingMode};
    use app_fixtures::load_fixture_nav_kv_pages;

    fn test_aircraft_definitions() -> BTreeMap<String, product_contracts::AircraftDefinition> {
        [
            include_str!(
                "../../../../../product/preprocessor/preprocessor-cli/resources/aircraft/cessna-172-generic.json"
            ),
            include_str!(
                "../../../../../product/preprocessor/preprocessor-cli/resources/aircraft/piper-pa46-310p.json"
            ),
        ]
        .into_iter()
        .map(|source| {
            let definition: product_contracts::AircraftDefinition =
                serde_json::from_str(source).expect("bundled aircraft definition");
            let hash = definition.content_hash().expect("aircraft definition hash");
            (hash, definition)
        })
        .collect()
    }

    fn test_aircraft_selection(
        lineage_id: &str,
        profile_id: &str,
    ) -> product_contracts::AircraftSelection {
        let (definition_hash, definition) = test_aircraft_definitions()
            .into_iter()
            .find(|(_, definition)| definition.lineage_id == lineage_id)
            .expect("test aircraft lineage");
        assert!(definition.profile(profile_id).is_some());
        product_contracts::AircraftSelection {
            definition_hash,
            profile_id: profile_id.to_string(),
        }
    }

    fn flight_plan_ui_state(
        store: &NavKvStore,
        plan: FlightPlan,
        current_ui_state: FlightPlanUiState,
        computer: crate::FlightDataComputer,
        live_data: FlightPlanLiveData,
    ) -> Result<FlightPlanUiState, HadReadError> {
        Ok(flight_plan_ui_projection(
            store,
            &test_aircraft_definitions(),
            &BTreeMap::new(),
            plan,
            current_ui_state,
            computer,
            live_data,
            PlannerAtmosphereSelection::no_wind(false),
        )?
        .ui_state)
    }

    fn default_flight_plan_ui_state_for_test(
        store: &NavKvStore,
        plan: &FlightPlan,
    ) -> FlightPlanUiState {
        flight_plan_ui_state(
            store,
            plan.clone(),
            crate::project_ui_state(plan),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData::default(),
        )
        .expect("project flight plan UI state")
    }

    fn resource_page_indexes(resources: &[CoreResourceRequest]) -> Vec<u32> {
        resources
            .iter()
            .map(|resource| {
                nav_kv_page_index_from_resource_id(&resource.id)
                    .unwrap_or_else(|| panic!("unexpected resource id: {}", resource.id))
            })
            .collect()
    }

    fn nav_db_open_page_indexes(resources: &[CoreResourceRequest]) -> Vec<u32> {
        resources
            .iter()
            .map(|resource| {
                nav_db_artifact_resource_index(&resource.id)
                    .and_then(|resource| resource.page_index)
                    .unwrap_or_else(|| {
                        panic!("unexpected nav_db open resource id: {}", resource.id)
                    })
            })
            .collect()
    }

    fn nav_kv_pair(key: &str, value: &[u8]) -> had_nav_kv::NavKvPair {
        had_nav_kv::NavKvPair {
            key: key.to_string(),
            value: value.to_vec(),
        }
    }

    #[test]
    fn flyway_family_uses_full_launcher_label() {
        assert!(supported_chart_families().contains(&("flyway", "FLYWAY", "FLYWAY")));
    }

    #[test]
    fn chart_reference_families_follow_map_selector_order() {
        let family_index = br#"[
          {"id":"enr-h","label":"IFR-HIGH"},
          {"id":"enr-l","label":"IFR-LOW"},
          {"id":"flyway","label":"FLYWAY"},
          {"id":"sec","label":"SECTIONAL"},
          {"id":"shaded-relief","label":"SHADED RELIEF"},
          {"id":"tac","label":"TAC"}
        ]"#;
        let (root, pages) = fixture(
            &[("chart-reference/family-index", family_index.as_slice())],
            4096,
        );
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        let state = chart_page_state(
            &store,
            ChartPageQuery {
                plan: &FlightPlan::default(),
                stored_recent_airport_ids: &[],
                plate_target_airport_id: None,
                candidate_airport_id: None,
                selected_reference_family_id: None,
                candidate_chart_id: None,
                suggested_chart_ids: &[],
                notam_display_index: None,
            },
        )
        .expect("derive chart page state");
        let family_ids = state
            .reference_families
            .into_iter()
            .map(|family| family.id)
            .collect::<Vec<_>>();
        let menu_family_ids = state
            .airport_menu_entries
            .into_iter()
            .filter_map(|entry| match entry {
                crate::DerivedChartAirportMenuEntry::Reference { reference } => Some(reference.id),
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected = vec!["sec", "tac", "flyway", "enr-l", "enr-h", "shaded-relief"];

        assert_eq!(family_ids, expected);
        assert_eq!(menu_family_ids, expected);
    }

    #[test]
    fn chart_reference_families_reject_unknown_map_family() {
        let family_index = br#"[{"id":"future-chart","label":"FUTURE CHART"}]"#;
        let (root, pages) = fixture(
            &[("chart-reference/family-index", family_index.as_slice())],
            4096,
        );
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        let error = resolve_chart_reference_families(&store, None)
            .expect_err("unsupported reference family must fail the nav-db contract");

        assert_eq!(
            error,
            HadReadError::Fatal(
                "chart reference family index contains unsupported family future-chart".to_string()
            )
        );
    }

    #[test]
    fn operation_reports_page_faults_instead_of_exposing_query_keys() {
        let (root, _pages) = fixture(&[("vector/manifest", br#"{"layers":[]}"#.as_slice())], 64);
        let store = NavKvStore::new(root);

        match run_had_operation(&store, HadOperation::VectorManifest).unwrap() {
            HadOperationOutcome::NeedResources { resources } => {
                assert_eq!(resource_page_indexes(&resources), vec![0]);
            }
            other => panic!("expected resource fault, got {other:?}"),
        }
    }

    #[test]
    fn operation_decodes_values_after_platform_supplies_pages() {
        let (root, pages) = fixture(&[("vector/manifest", br#"{"layers":[]}"#.as_slice())], 64);
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        assert_eq!(
            run_had_operation(&store, HadOperation::VectorManifest).unwrap(),
            HadOperationOutcome::complete(serde_json::json!({"layers":[]}))
        );
    }

    #[test]
    fn vector_manifest_operation_reads_core_owned_key() {
        let manifest = br#"{"point_layers":{},"airspace":{"reference_tile_min_zoom":0,"reference_tile_max_zoom":12,"label_tile_min_zoom":0,"label_tile_max_zoom":12}}"#;
        let (root, pages) = fixture(&[("vector/manifest", manifest.as_slice())], 256);
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        let outcome = run_had_operation(&store, HadOperation::VectorManifest).unwrap();
        match outcome {
            HadOperationOutcome::Complete { result, .. } => {
                assert_eq!(result["airspace"]["reference_tile_max_zoom"], 12);
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("unexpected resources: {resources:?}")
            }
            HadOperationOutcome::NeedSnapshotResources { .. } => {
                panic!("generic HAD operation requested a session snapshot continuation")
            }
        }
    }

    #[test]
    fn nav_db_open_controller_skips_bad_candidate_and_selects_readable_root() {
        let contract = format!(r#"{{"contract_id":"{REQUIRED_NAV_DB_CONTRACT_ID}"}}"#);
        let (root_bytes, pages) = build_root(&[("contract/nav-db", contract.as_bytes())], 256);
        let mut controller = NavDbOpenController::new_at_epoch_ms(
            vec![
                NavDbArtifactCandidate {
                    package_id: "NAV_DB_BAD".to_string(),
                    filename: "nav_db_bad.zip".to_string(),
                    contract_id: None,
                    cycle: None,
                    cycle_version: None,
                    effective_date: None,
                    expiration_date: None,
                    warning_text: None,
                    root_source: None,
                },
                NavDbArtifactCandidate {
                    package_id: "NAV_DB_GOOD".to_string(),
                    filename: "nav_db_good.zip".to_string(),
                    contract_id: None,
                    cycle: None,
                    cycle_version: None,
                    effective_date: None,
                    expiration_date: None,
                    warning_text: None,
                    root_source: Some(CoreResourceSource::PublicUrl {
                        url: "https://example.test/nav_db/root".to_string(),
                    }),
                },
            ],
            0,
        );

        match controller.step().expect("step bad candidate") {
            HadOperationOutcome::NeedResources { resources } => {
                assert_eq!(resources[0].id, "nav_db/artifact/0/root");
                assert_eq!(
                    resources[0].source,
                    CoreResourceSource::InstalledArtifactMember {
                        filename: "nav_db_bad.zip".to_string(),
                        member_path: "root".to_string(),
                    }
                );
            }
            other => panic!("expected first root resource, got {other:?}"),
        }
        controller
            .ingest_resource("nav_db/artifact/0/root", b"not a nav db root")
            .expect("ingest bad root");
        match controller.step().expect("step good candidate") {
            HadOperationOutcome::NeedResources { resources } => {
                assert_eq!(resources[0].id, "nav_db/artifact/1/root");
                assert_eq!(
                    resources[0].source,
                    CoreResourceSource::PublicUrl {
                        url: "https://example.test/nav_db/root".to_string(),
                    }
                );
            }
            other => panic!("expected second root resource, got {other:?}"),
        }
        controller
            .ingest_resource("nav_db/artifact/1/root", &root_bytes)
            .expect("ingest good root");
        match controller.step().expect("request contract page") {
            HadOperationOutcome::NeedResources { resources } => {
                assert_eq!(resources[0].id, "nav_db/artifact/1/page/0000");
                assert_eq!(
                    resources[0].source,
                    CoreResourceSource::PublicUrl {
                        url: "https://example.test/nav_db/page_0000".to_string(),
                    }
                );
            }
            other => panic!("expected contract page resource, got {other:?}"),
        }
        controller
            .ingest_resource("nav_db/artifact/1/page/0000", &pages[0])
            .expect("ingest contract page");

        match controller.step().expect("complete") {
            HadOperationOutcome::Complete { result, .. } => {
                let result: NavDbOpenResult =
                    serde_json::from_value(result).expect("decode result");
                assert_eq!(result.selected_package_id, "NAV_DB_GOOD");
                assert_eq!(result.statuses.len(), 2);
                assert!(!result.statuses[0].readable);
                assert!(result.statuses[1].readable);
            }
            other => panic!("expected complete open result, got {other:?}"),
        }
        assert!(controller.selected_store().is_some());
    }

    #[test]
    fn nav_db_open_controller_prefetches_root_pages_before_contract_validation() {
        let contract = format!(r#"{{"contract_id":"{REQUIRED_NAV_DB_CONTRACT_ID}"}}"#);
        let mut pairs = vec![
            nav_kv_pair("chart/catalog", b"catalog"),
            nav_kv_pair("contract/nav-db", contract.as_bytes()),
            nav_kv_pair("vector/manifest", b"{}"),
            nav_kv_pair("weather/metar-important-stations", b"KAAA\nKBBB\n"),
        ];
        for index in 0..24 {
            pairs.push(nav_kv_pair(
                &format!("package/by-id/package-{index:02}"),
                format!("package payload {index:02}").as_bytes(),
            ));
        }
        let built = had_nav_kv::build_nav_kv_sorted(pairs, 128).expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        assert!(
            root.prefetch_pages().len() > 1,
            "test fixture should exercise batched prefetch, got {:?}",
            root.prefetch_pages()
        );
        let mut controller = NavDbOpenController::new_at_epoch_ms(
            vec![NavDbArtifactCandidate {
                package_id: "NAV_DB_GOOD".to_string(),
                filename: "nav_db_good.zip".to_string(),
                contract_id: None,
                cycle: None,
                cycle_version: None,
                effective_date: None,
                expiration_date: None,
                warning_text: None,
                root_source: Some(CoreResourceSource::PublicUrl {
                    url: "https://example.test/nav_db/root".to_string(),
                }),
            }],
            0,
        );

        match controller.step().expect("request root") {
            HadOperationOutcome::NeedResources { resources } => {
                assert_eq!(resources[0].id, "nav_db/artifact/0/root");
            }
            other => panic!("expected root resource, got {other:?}"),
        }
        controller
            .ingest_resource("nav_db/artifact/0/root", &built.root_bytes)
            .expect("ingest root");

        match controller.step().expect("request root prefetch pages") {
            HadOperationOutcome::NeedResources { resources } => {
                assert_eq!(nav_db_open_page_indexes(&resources), root.prefetch_pages());
                assert!(
                    resources.len() > 1,
                    "prefetch should batch more than one page"
                );
            }
            other => panic!("expected batched prefetch resources, got {other:?}"),
        }
        for page in root.prefetch_pages() {
            controller
                .ingest_resource(
                    &format!("nav_db/artifact/0/page/{page:04}"),
                    &built.pages[*page as usize],
                )
                .expect("ingest prefetch page");
        }

        match controller.step().expect("complete after prefetch") {
            HadOperationOutcome::Complete { result, .. } => {
                let result: NavDbOpenResult =
                    serde_json::from_value(result).expect("decode result");
                assert_eq!(result.selected_package_id, "NAV_DB_GOOD");
                assert_eq!(result.statuses.len(), 1);
                assert!(result.statuses[0].readable);
            }
            other => panic!("expected complete open result, got {other:?}"),
        }
    }

    #[test]
    fn nav_db_open_controller_prefers_current_candidate_over_future_candidate() {
        let contract = format!(r#"{{"contract_id":"{REQUIRED_NAV_DB_CONTRACT_ID}"}}"#);
        let (root_bytes, pages) = build_root(&[("contract/nav-db", contract.as_bytes())], 256);
        let mut controller = NavDbOpenController::new_at_epoch_ms(
            vec![
                nav_db_candidate_for_selection_test(
                    "NAV_DB_2607",
                    "nav_db_2607.zip",
                    "2026-06-11",
                    "2026-07-09",
                ),
                nav_db_candidate_for_selection_test(
                    "NAV_DB_2606",
                    "nav_db_2606.zip",
                    "2026-05-14",
                    "2026-06-11",
                ),
            ],
            parse_nav_db_timestamp("2026-05-20T12:00:00Z").expect("now"),
        );

        loop {
            match controller.step().expect("controller step") {
                HadOperationOutcome::NeedResources { resources } => {
                    for resource in resources {
                        let bytes =
                            nav_db_selection_test_resource_bytes(&resource, &root_bytes, &pages);
                        controller
                            .ingest_resource(&resource.id, &bytes)
                            .expect("ingest resource");
                    }
                }
                HadOperationOutcome::Complete { result, .. } => {
                    let result: NavDbOpenResult =
                        serde_json::from_value(result).expect("decode result");
                    assert_eq!(result.selected_package_id, "NAV_DB_2606");
                    assert_eq!(result.selected_filename, "nav_db_2606.zip");
                    assert_eq!(result.statuses.len(), 2);
                    assert!(result.statuses.iter().all(|status| status.readable));
                    break;
                }
                HadOperationOutcome::NeedSnapshotResources { .. } => {
                    panic!("nav-db open requested a session snapshot continuation")
                }
            }
        }
    }

    #[test]
    fn nav_db_candidate_selection_rejects_future_only_candidate() {
        let candidate = nav_db_candidate_for_selection_test(
            "NAV_DB_2608",
            "nav_db_2608.zip",
            "2026-07-16T00:00:00Z",
            "2026-08-13T00:00:00Z",
        );
        let now = parse_nav_db_timestamp("2026-07-15T23:59:59Z").expect("now");

        assert!(select_preferred_nav_db_candidate(&[candidate], now).is_none());
    }

    #[test]
    fn nav_db_open_controller_rejects_missing_contract_id() {
        let (root_bytes, pages) = build_root(&[("vector/manifest", b"{}")], 256);
        let mut controller = NavDbOpenController::new_at_epoch_ms(
            vec![NavDbArtifactCandidate {
                package_id: "NAV_DB_OLD".to_string(),
                filename: "nav_db_old.zip".to_string(),
                contract_id: None,
                cycle: None,
                cycle_version: None,
                effective_date: None,
                expiration_date: None,
                warning_text: None,
                root_source: None,
            }],
            0,
        );
        controller
            .ingest_resource("nav_db/artifact/0/root", &root_bytes)
            .expect("ingest root");
        match controller.step().expect("request contract page") {
            HadOperationOutcome::NeedResources { resources } => {
                assert_eq!(resources[0].id, "nav_db/artifact/0/page/0000");
            }
            other => panic!("expected contract page resource, got {other:?}"),
        }
        controller
            .ingest_resource("nav_db/artifact/0/page/0000", &pages[0])
            .expect("ingest page");
        let err = controller
            .step()
            .expect_err("missing contract rejects nav_db");
        assert!(
            err.contains("no readable installed nav-db package"),
            "{err}"
        );
        assert!(err.contains("nav_db_old.zip"), "{err}");
        assert!(err.contains("missing nav-db contract"), "{err}");
        let statuses = controller.statuses();
        assert_eq!(statuses.len(), 1);
        assert!(!statuses[0].readable);
        assert!(
            statuses[0]
                .message
                .as_deref()
                .is_some_and(|message| message.contains("missing nav-db contract")),
            "{statuses:?}"
        );
    }

    #[test]
    fn plate_airport_operation_resolves_chart_ids() {
        let airport = br#"{"id":"KRNT","label":"RENTON MUNI","airport_type":"AIRPORT","package_ids":["NW_TPP_2604"],"chart_ids":["plate:KRNT:APD-WA-AIRPORT DIAGRAM.png"],"charted_procedures":[]}"#;
        let plate = br#"{"id":"plate:KRNT:APD-WA-AIRPORT DIAGRAM.png","airport_id":"KRNT","package_id":"NW_TPP_2604","label":"Airport Diagram","kind":"plate","folder_category":"airport-diagram","asset_path":"plates/RNT/APD-WA-AIRPORT DIAGRAM.png"}"#;
        let (root, pages) = fixture(
            &[
                ("plate/airport/KRNT", airport.as_slice()),
                (
                    "plate/by-id/plate%3AKRNT%3AAPD-WA-AIRPORT%20DIAGRAM.png",
                    plate.as_slice(),
                ),
            ],
            1024,
        );
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        let outcome = run_had_operation(
            &store,
            HadOperation::PlateAirport {
                airport_id: "KRNT".to_string(),
            },
        )
        .expect("resolve plate airport");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete plate airport, got {outcome:?}");
        };
        let airport: crate::DerivedChartAirport =
            serde_json::from_value(result).expect("decode plate airport");
        assert_eq!(airport.id, "KRNT");
        assert_eq!(airport.charts.len(), 1);
        assert_eq!(airport.charts[0].label, "Airport Diagram");
    }

    #[test]
    fn empty_plan_direct_to_projects_active_route_segment() {
        let (root, _pages) = fixture(&[("dummy", b"1".as_slice())], 64);
        let store = NavKvStore::new(root);
        let start = LatLon {
            lat: 47.600,
            lon: -122.300,
        };
        let target = LatLon {
            lat: 47.700,
            lon: -122.100,
        };
        let plan = crate::activate_direct_to(&FlightPlan::empty(), start, NavRef::LatLon(target))
            .expect("activate direct-to");

        let route = project_flight_plan_route(&store, &plan).expect("project direct-to route");

        assert_eq!(route.len(), 1);
        assert_eq!(route[0].id, "direct-to");
        assert_eq!(route[0].status, crate::FlightPlanRouteSegmentStatus::Active);
        assert_eq!(route[0].from, start);
        assert_eq!(route[0].to, target);
    }

    #[test]
    fn map_selector_state_enriches_full_coverage_zoom_from_package_metadata() {
        let chart_catalog = br#"[{
          "id":"sec:nw",
          "label":"Northwest Sectional",
          "region_id":"nw",
          "map_view":{
            "chart_family":"sec",
            "chart_name":"Northwest Sectional",
            "chart_index":0,
            "tile_root":"tiles",
            "tile_url_root":"tiles",
            "tile_path_template":"0/{z}/{x}/{y}.webp",
            "tile_size":512,
            "min_zoom":4.2,
            "max_zoom":12.5,
            "storage_kind":"sectional_package",
            "package_name":"NW_SEC_2604",
            "initial_viewport":{"lat":45.0,"lon":-122.0,"zoom":8.0},
            "levels":[{"zoom":10,"boxes":[{"x_min":1,"x_max":2,"y_tms_min":3,"y_tms_max":4}]}]
          }
        }]"#;
        let package_by_id = br#"{
          "id":"NW_SEC_2604",
          "family_id":"sec",
          "region_id":"nw",
          "relative_path":"nw_sec_2604_hash.zip",
          "effective_date":"2026-05-14",
          "expiration_date":"2026-07-09",
          "metadata":{"full_coverage_zoom":7}
        }"#;
        let (root, pages) = fixture(
            &[
                ("chart/catalog", chart_catalog.as_slice()),
                ("package/by-id/NW_SEC_2604", package_by_id.as_slice()),
            ],
            8192,
        );
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        let state = map_selector_state(&store, Some("sec:nw"), None).expect("map selector state");
        assert_eq!(
            state
                .selected_map
                .as_ref()
                .and_then(|view| view.map_view.full_coverage_zoom),
            Some(7.0)
        );
        assert_eq!(
            state.displayed_maps[0].map_view.full_coverage_zoom,
            Some(7.0)
        );
        assert_eq!(
            state.displayed_maps[0]
                .map_view
                .package_relative_path
                .as_deref(),
            Some("nw_sec_2604_hash.zip")
        );
        assert_eq!(
            state.displayed_maps[0]
                .map_view
                .package_effective_date
                .as_deref(),
            Some("2026-05-14")
        );
        assert_eq!(
            state.displayed_maps[0]
                .map_view
                .package_expiration_date
                .as_deref(),
            Some("2026-07-09")
        );
        assert_eq!(state.displayed_maps[0].map_view.tile_url_root, "tiles");
    }

    fn fixture(entries: &[(&str, &[u8])], page_size: u32) -> (NavKvRoot, Vec<Vec<u8>>) {
        let (root, pages) = build_root(entries, page_size);
        (NavKvRoot::parse(&root).unwrap(), pages)
    }

    fn build_root(entries: &[(&str, &[u8])], page_size: u32) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut value_bytes = Vec::new();
        let mut leaf = Vec::new();
        leaf.extend_from_slice(&1u32.to_le_bytes());
        leaf.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        leaf.extend_from_slice(&u32::MAX.to_le_bytes());
        for (key, value) in entries {
            leaf.extend_from_slice(&(key.len() as u32).to_le_bytes());
            if value.len() > 4096 {
                let offset = value_bytes.len() as u32;
                leaf.extend_from_slice(&0u32.to_le_bytes());
                leaf.extend_from_slice(&offset.to_le_bytes());
                leaf.extend_from_slice(&(value.len() as u32).to_le_bytes());
                value_bytes.extend_from_slice(value);
            } else {
                leaf.extend_from_slice(&1u32.to_le_bytes());
                leaf.extend_from_slice(&(value.len() as u32).to_le_bytes());
                leaf.extend_from_slice(&0u32.to_le_bytes());
            }
            leaf.extend_from_slice(key.as_bytes());
            if value.len() <= 4096 {
                leaf.extend_from_slice(value);
            }
        }
        assert!(leaf.len() <= page_size as usize);
        let mut pages = vec![leaf];
        let value_page_start = pages.len() as u32;
        pages.extend(
            value_bytes
                .chunks(page_size as usize)
                .map(|chunk| chunk.to_vec()),
        );
        let page_count = pages.len() as u32;
        let mut root = vec![0; 64];
        root[..16].copy_from_slice(b"AEROBAGNAVKV0001");
        write_u32(&mut root, 16, 4);
        write_u32(&mut root, 20, entries.len() as u32);
        write_u32(&mut root, 24, page_size);
        write_u32(&mut root, 28, 0);
        write_u32(&mut root, 32, page_count);
        write_u32(&mut root, 36, value_page_start);
        write_u32(&mut root, 40, value_bytes.len() as u32);
        (root, pages)
    }

    fn nav_db_candidate_for_selection_test(
        package_id: &str,
        filename: &str,
        effective_date: &str,
        expiration_date: &str,
    ) -> NavDbArtifactCandidate {
        NavDbArtifactCandidate {
            package_id: package_id.to_string(),
            filename: filename.to_string(),
            contract_id: Some(REQUIRED_NAV_DB_CONTRACT_ID.to_string()),
            cycle: None,
            cycle_version: None,
            effective_date: Some(effective_date.to_string()),
            expiration_date: Some(expiration_date.to_string()),
            warning_text: None,
            root_source: Some(CoreResourceSource::InstalledArtifactMember {
                filename: filename.to_string(),
                member_path: "root".to_string(),
            }),
        }
    }

    fn nav_db_selection_test_resource_bytes(
        resource: &CoreResourceRequest,
        root_bytes: &[u8],
        pages: &[Vec<u8>],
    ) -> Vec<u8> {
        let CoreResourceSource::InstalledArtifactMember { member_path, .. } = &resource.source
        else {
            panic!("unexpected source: {:?}", resource.source);
        };
        if member_path == "root" {
            return root_bytes.to_vec();
        }
        let page = member_path
            .strip_prefix("page_")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or_else(|| panic!("unexpected nav-db member path: {member_path}"));
        pages
            .get(page)
            .unwrap_or_else(|| panic!("missing page {page}"))
            .clone()
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn load_fixture_nav_kv_store() -> NavKvStore {
        let (root_bytes, pages) = load_fixture_nav_kv_pages();
        let root = NavKvRoot::parse(&root_bytes).expect("parse fixture nav_kv root");
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }
        store
    }

    fn test_nav_kv_store(entries: &[(&str, serde_json::Value)]) -> NavKvStore {
        let encoded = entries
            .iter()
            .map(|(key, value)| {
                (
                    *key,
                    serde_json::to_vec(value).expect("encode test nav_kv value"),
                )
            })
            .collect::<Vec<_>>();
        let entry_refs = encoded
            .iter()
            .map(|(key, value)| (*key, value.as_slice()))
            .collect::<Vec<_>>();
        let mut entry_refs = entry_refs;
        entry_refs.sort_by(|left, right| left.0.cmp(right.0));
        let (root, pages) = fixture(&entry_refs, 65536);
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }
        store
    }

    fn three_airport_test_plan() -> FlightPlan {
        crate::build_flight_plan(FlightPlan {
            id: "plan-three-airports".to_string(),
            name: "KAAA KBBB KCCC".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KCCC".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KCCC".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .expect("build plan")
    }

    fn three_airport_nav_store(aaa: LatLon, bbb: LatLon, ccc: LatLon) -> NavKvStore {
        test_nav_kv_store(&[
            ("airport/info/KAAA", airport_info_value("KAAA", aaa, 100.0)),
            ("airport/info/KCCC", airport_info_value("KCCC", ccc, 200.0)),
            (
                "navref/position/airport/KAAA",
                serde_json::json!({"lat": aaa.lat, "lon": aaa.lon}),
            ),
            (
                "navref/position/airport/KBBB",
                serde_json::json!({"lat": bbb.lat, "lon": bbb.lon}),
            ),
            (
                "navref/position/airport/KCCC",
                serde_json::json!({"lat": ccc.lat, "lon": ccc.lon}),
            ),
        ])
    }

    fn row_cell<'a>(
        row: &'a crate::planning::FlightPlanDisplayRowUiView,
        id: &str,
    ) -> &'a crate::FlightDataCell {
        row.data_cells
            .iter()
            .find(|cell| cell.id == id)
            .expect("cell")
    }

    #[derive(Debug, Clone, Copy)]
    struct ConstantForecastAtmosphere;

    impl crate::AtmosphereModel for ConstantForecastAtmosphere {
        fn sample(
            &self,
            _position: LatLon,
            pressure_altitude_ft: f64,
            _epoch_ms: i64,
        ) -> Result<crate::AtmosphereSample, String> {
            Ok(crate::AtmosphereSample {
                wind_east_kt: 50.0,
                wind_north_kt: 0.0,
                temperature_c: 15.0 - pressure_altitude_ft * 0.001_981_2,
            })
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct ForecastUntilAtmosphere {
        valid_through_epoch_ms: i64,
    }

    impl crate::AtmosphereModel for ForecastUntilAtmosphere {
        fn sample(
            &self,
            _position: LatLon,
            pressure_altitude_ft: f64,
            epoch_ms: i64,
        ) -> Result<crate::AtmosphereSample, String> {
            if epoch_ms > self.valid_through_epoch_ms {
                return Err(crate::forecast_atmosphere::TIME_OUTSIDE_FORECAST_COVERAGE.to_string());
            }
            Ok(crate::AtmosphereSample {
                wind_east_kt: 50.0,
                wind_north_kt: 0.0,
                temperature_c: 15.0 - pressure_altitude_ft * 0.001_981_2,
            })
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct ForecastWindowAtmosphere {
        valid_from_epoch_ms: i64,
        valid_through_epoch_ms: i64,
    }

    impl crate::AtmosphereModel for ForecastWindowAtmosphere {
        fn sample(
            &self,
            _position: LatLon,
            pressure_altitude_ft: f64,
            epoch_ms: i64,
        ) -> Result<crate::AtmosphereSample, String> {
            if epoch_ms < self.valid_from_epoch_ms || epoch_ms > self.valid_through_epoch_ms {
                return Err(crate::forecast_atmosphere::TIME_OUTSIDE_FORECAST_COVERAGE.to_string());
            }
            Ok(crate::AtmosphereSample {
                wind_east_kt: 50.0,
                wind_north_kt: 0.0,
                temperature_c: 15.0 - pressure_altitude_ft * 0.001_981_2,
            })
        }
    }

    fn wind_control(state: &FlightPlanUiState) -> &crate::AltitudePlannerControlUiView {
        state
            .altitude_planner
            .controls
            .iter()
            .find(|control| control.id == crate::AltitudePlannerControlId::WindModel)
            .expect("wind-model control")
    }

    fn airport_info_value(id: &str, position: LatLon, elevation_msl_ft: f64) -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "airport_id": id,
            "name": format!("{id} TEST AIRPORT"),
            "latitude": position.lat,
            "longitude": position.lon,
            "time_zone": "UTC",
            "elevation_msl_ft": elevation_msl_ft,
            "traffic_pattern_altitude_msl_ft": null
        })
    }

    #[test]
    fn modeled_estimates_use_vector_bridge_instead_of_chevron_display_path() {
        let row_id = crate::FlightPlanRowId("vector-row".to_string());
        let display_start = LatLon { lat: 0.0, lon: 0.0 };
        let display_end = LatLon { lat: 1.0, lon: 0.5 };
        let estimate_end = LatLon { lat: 0.0, lon: 2.0 };
        let row = crate::flight_plan_materialization::MaterializedFlightPlanRow {
            id: row_id.clone(),
            location: Some(NavRef::LatLon(estimate_end)),
            leg_index: Some(0),
            tone: crate::FlightDataCellTone::Planned,
            arrow: crate::flight_plan_materialization::MaterializedFlightPlanRowArrow::Leg,
            geometry: Some(crate::GuidanceLegGeometry {
                leg_id: "vectors".to_string(),
                from: display_start,
                to: display_end,
                path: vec![display_start, display_end],
            }),
            estimate_geometry: Some(crate::GuidanceLegGeometry {
                leg_id: "vectors".to_string(),
                from: display_start,
                to: estimate_end,
                path: vec![display_start, estimate_end],
            }),
            distance_remaining_nm: Some(crate::great_circle_distance_nm(
                display_start,
                estimate_end,
            )),
            cumulative_distance_remaining_nm: Some(crate::great_circle_distance_nm(
                display_start,
                estimate_end,
            )),
        };
        let materialized = crate::flight_plan_materialization::MaterializedFlightPlan {
            order: vec![row_id.clone()],
            rows: BTreeMap::from([(row_id.clone(), row)]),
            active: None,
            total_distance_remaining_nm: Some(crate::great_circle_distance_nm(
                display_start,
                estimate_end,
            )),
        };

        let route = trajectory_route_legs(&materialized, None);

        assert_eq!(route.len(), 1);
        assert_eq!(route[0].row_id, row_id);
        assert_eq!(route[0].path, vec![display_start, estimate_end]);
    }

    #[test]
    fn missing_selected_definition_falls_back_until_private_definition_arrives() {
        let store = test_nav_kv_store(&[]);
        let definitions = test_aircraft_definitions();
        let selected = test_aircraft_selection("piper-pa46-310p-tsio-520-be", "economy-65");
        let mut before_arrival = definitions.clone();
        before_arrival.remove(&selected.definition_hash);

        let fallback =
            planner_aircraft(&store, Some(&selected), &before_arrival).expect("default fallback");
        assert_eq!(
            fallback.selection,
            product_contracts::default_aircraft_selection()
        );
        assert!(fallback.advisory.is_some());
        assert!(planner_aircraft_selection_cruise_altitude(
            &store,
            &before_arrival,
            &selected,
            Some(12_000),
        )
        .expect("selection availability")
        .is_none());

        let resolved = planner_aircraft(&store, Some(&selected), &definitions)
            .expect("private definition arrival");
        assert_eq!(resolved.selection, selected);
        assert!(resolved.advisory.is_none());
        assert_eq!(
            planner_aircraft_selection_cruise_altitude(
                &store,
                &definitions,
                &selected,
                Some(12_000),
            )
            .expect("selection availability"),
            Some(12_000)
        );
    }

    #[test]
    fn superseded_aircraft_selection_keeps_its_profile_on_the_new_definition() {
        let store = test_nav_kv_store(&[]);
        let definitions = test_aircraft_definitions();
        let (current_hash, definition) = definitions
            .iter()
            .find(|(_, definition)| definition.lineage_id == "piper-pa46-310p-tsio-520-be")
            .expect("PA46 definition");
        let superseded_hash = definition
            .supersedes
            .first()
            .expect("PA46 predecessor hash");
        let requested = product_contracts::AircraftSelection {
            definition_hash: superseded_hash.clone(),
            profile_id: "economy-65".to_string(),
        };

        let resolved =
            planner_aircraft(&store, Some(&requested), &definitions).expect("superseded selection");

        assert_eq!(resolved.selection.definition_hash, *current_hash);
        assert_eq!(resolved.selection.profile_id, requested.profile_id);
        assert!(resolved.advisory.is_none());
    }

    #[test]
    fn ambiguous_aircraft_supersession_is_fatal() {
        let store = test_nav_kv_store(&[]);
        let mut definitions = test_aircraft_definitions();
        let predecessor = "1".repeat(64);
        for suffix in ["a", "b"] {
            let mut definition = definitions
                .values()
                .next()
                .expect("aircraft fixture")
                .clone();
            definition.lineage_id = format!("ambiguous-{suffix}");
            definition.label = format!("AMBIGUOUS {suffix}");
            definition.supersedes = vec![predecessor.clone()];
            let hash = definition.content_hash().expect("aircraft hash");
            definitions.insert(hash, definition);
        }
        let requested = product_contracts::AircraftSelection {
            definition_hash: predecessor,
            profile_id: product_contracts::DEFAULT_AIRCRAFT_PROFILE_ID.to_string(),
        };

        let error = planner_aircraft(&store, Some(&requested), &definitions)
            .expect_err("ambiguous supersession must fail");

        let HadReadError::Fatal(message) = error else {
            panic!("expected fatal ambiguity, got {error:?}");
        };
        assert!(message.contains("superseded ambiguously"));
    }

    #[test]
    fn inactive_airport_plan_uses_selected_aircraft_modeled_time_and_fuel() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.5 };
        let ccc = LatLon { lat: 0.0, lon: 3.0 };
        let store = test_nav_kv_store(&[
            ("airport/info/KAAA", airport_info_value("KAAA", aaa, 100.0)),
            ("airport/info/KCCC", airport_info_value("KCCC", ccc, 200.0)),
            (
                "navref/position/airport/KAAA",
                serde_json::json!({"lat": aaa.lat, "lon": aaa.lon}),
            ),
            (
                "navref/position/airport/KBBB",
                serde_json::json!({"lat": bbb.lat, "lon": bbb.lon}),
            ),
            (
                "navref/position/airport/KCCC",
                serde_json::json!({"lat": ccc.lat, "lon": ccc.lon}),
            ),
        ]);
        let mut plan = three_airport_test_plan();
        plan.aircraft = Some(test_aircraft_selection(
            "piper-pa46-310p-tsio-520-be",
            "economy-65",
        ));
        let ui_state = default_flight_plan_ui_state_for_test(&store, &plan);

        assert_eq!(
            ui_state.altitude_planner.estimate_kind,
            crate::FlightEstimateKind::Modeled
        );
        assert!(ui_state
            .altitude_planner
            .controls
            .iter()
            .any(
                |control| control.id == crate::AltitudePlannerControlId::AircraftProfile
                    && control.label.contains("65% ECONOMY")
            ));
        let aircraft_control = ui_state
            .altitude_planner
            .controls
            .iter()
            .find(|control| control.id == crate::AltitudePlannerControlId::Aircraft)
            .expect("aircraft control");
        assert_eq!(aircraft_control.options.len(), 2);
        let profile_control = ui_state
            .altitude_planner
            .controls
            .iter()
            .find(|control| control.id == crate::AltitudePlannerControlId::AircraftProfile)
            .expect("profile control");
        assert_eq!(profile_control.options.len(), 3);
        assert!(profile_control
            .options
            .iter()
            .all(|option| { planner_aircraft_from_action_uid(&option.action_uid).is_some() }));
        assert_eq!(
            ui_state.altitude_planner.estimate_summary.label,
            "Estimate basis:\nNo wind\n12,000 cruise"
        );

        for label in ["KBBB", "KCCC", "TOTAL"] {
            let row = ui_state
                .display_rows
                .iter()
                .find(|row| row.label == label)
                .expect("modeled row");
            for cell_id in ["waypoint_ete", "fuel"] {
                let cell = row_cell(row, cell_id);
                assert!(cell.value.as_ref().is_some_and(|value| !value.is_empty()));
                assert_eq!(cell.estimate_kind, crate::FlightEstimateKind::Modeled);
            }
        }
        let baseline_total_fuel = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "TOTAL")
            .map(|row| row_cell(row, "fuel").value.clone())
            .expect("baseline total fuel");

        let panel = altitude_comparison_panel(
            &store,
            &test_aircraft_definitions(),
            plan.clone(),
            FlightPlanLiveData {
                now_epoch_ms: Some(12 * 60 * 60 * 1000),
                ..FlightPlanLiveData::default()
            },
            PlannerAtmosphereSelection::no_wind(false),
        )
        .expect("altitude comparison panel");
        assert_eq!(panel.columns, crate::altitude_comparison_columns());
        assert_eq!(panel.rows.len(), 12);
        assert_eq!(
            panel
                .rows
                .first()
                .and_then(|row| row.cells.first())
                .and_then(|cell| cell.value.as_deref()),
            Some("24000")
        );
        assert_eq!(
            panel
                .rows
                .last()
                .and_then(|row| row.cells.first())
                .and_then(|cell| cell.value.as_deref()),
            Some("2000")
        );
        let selected = panel
            .rows
            .iter()
            .find(|row| row.selected)
            .expect("selected altitude");
        assert!(selected.enabled);
        assert_eq!(
            selected.action_uid.as_deref(),
            Some("altitude-planner:select:12000")
        );
        for cell_id in ["cruise_altitude", "waypoint_ete", "fuel", "wind"] {
            let cell = selected
                .cells
                .iter()
                .find(|cell| cell.id == cell_id)
                .expect("comparison cell");
            assert!(cell.value.as_ref().is_some_and(|value| !value.is_empty()));
        }

        let mut higher_plan = plan;
        higher_plan.cruise_altitude_ft = Some(16_000);
        let higher_ui_state = default_flight_plan_ui_state_for_test(&store, &higher_plan);
        assert_eq!(
            higher_ui_state.altitude_planner.estimate_summary.label,
            "Estimate basis:\nNo wind\n16,000 cruise"
        );
        let higher_total = higher_ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "TOTAL")
            .expect("higher-altitude total");
        assert_ne!(row_cell(higher_total, "fuel").value, baseline_total_fuel);
        assert_eq!(
            row_cell(higher_total, "fuel").estimate_kind,
            crate::FlightEstimateKind::Modeled
        );
    }

    #[test]
    fn inactive_plan_ending_at_navaid_uses_cruise_altitude_model_estimates() {
        let airport = LatLon { lat: 0.0, lon: 0.0 };
        let navaid = LatLon { lat: 0.0, lon: 2.0 };
        let store = test_nav_kv_store(&[
            (
                "airport/info/KAAA",
                airport_info_value("KAAA", airport, 100.0),
            ),
            (
                "navref/position/airport/KAAA",
                serde_json::json!({"lat": airport.lat, "lon": airport.lon}),
            ),
            (
                "navref/position/navaid/VVV",
                serde_json::json!({"lat": navaid.lat, "lon": navaid.lon}),
            ),
        ]);
        let plan = crate::build_flight_plan(FlightPlan {
            id: "plan-airport-to-vor".to_string(),
            name: "KAAA VVV".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("VVV".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KAAA".to_string())),
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: Some(8_000),
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .expect("build airport-to-VOR plan");

        let ui_state = flight_plan_ui_state(
            &store,
            plan.clone(),
            crate::project_ui_state(&plan),
            crate::FlightDataComputer::with_fuel_flow_and_clock(
                None,
                None,
                Some(12 * 60 * 60 * 1000),
            ),
            FlightPlanLiveData {
                now_epoch_ms: Some(12 * 60 * 60 * 1000),
                ..FlightPlanLiveData::default()
            },
        )
        .expect("project airport-to-VOR plan");
        let destination = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "VVV")
            .expect("VOR destination row");

        assert_eq!(
            ui_state.altitude_planner.estimate_kind,
            crate::FlightEstimateKind::Modeled
        );
        assert_eq!(
            ui_state.altitude_planner.estimate_summary.label,
            "Estimate basis:\nNo wind\n8,000 cruise"
        );
        for cell_id in ["final_eta", "waypoint_ete", "fuel"] {
            let cell = row_cell(destination, cell_id);
            assert!(
                cell.value.as_ref().is_some_and(|value| !value.is_empty()),
                "{cell_id} should use the aircraft model when the endpoint has no elevation"
            );
            assert_eq!(cell.estimate_kind, crate::FlightEstimateKind::Modeled);
        }
    }

    #[test]
    fn selected_aircraft_profile_changes_time_and_fuel_predictions() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.5 };
        let ccc = LatLon { lat: 0.0, lon: 3.0 };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let definitions = test_aircraft_definitions();
        let project = |profile_id: &str| {
            let mut plan = three_airport_test_plan();
            plan.aircraft = Some(test_aircraft_selection(
                "piper-pa46-310p-tsio-520-be",
                profile_id,
            ));
            let ui_state = default_flight_plan_ui_state_for_test(&store, &plan);
            let total = ui_state
                .display_rows
                .iter()
                .find(|row| row.label == "TOTAL")
                .expect("flight-plan total");
            let flight_plan_values = (
                row_cell(total, "waypoint_ete").value.clone(),
                row_cell(total, "fuel").value.clone(),
            );
            let panel = altitude_comparison_panel(
                &store,
                &definitions,
                plan,
                FlightPlanLiveData {
                    now_epoch_ms: Some(12 * 60 * 60 * 1000),
                    ..FlightPlanLiveData::default()
                },
                PlannerAtmosphereSelection::no_wind(false),
            )
            .expect("altitude comparison panel");
            let selected = panel
                .rows
                .iter()
                .find(|row| row.selected)
                .expect("selected altitude");
            let comparison_value = |cell_id: &str| {
                selected
                    .cells
                    .iter()
                    .find(|cell| cell.id == cell_id)
                    .and_then(|cell| cell.value.clone())
                    .expect("comparison value")
            };
            (
                flight_plan_values,
                (comparison_value("waypoint_ete"), comparison_value("fuel")),
            )
        };

        let high_speed = project("high-speed-75");
        let long_range = project("long-range-55");

        assert_ne!(
            high_speed.0, long_range.0,
            "flight-plan estimate must change"
        );
        assert_ne!(
            high_speed.1, long_range.1,
            "altitude-comparison estimate must change"
        );
    }

    #[test]
    fn out_of_range_selected_altitude_names_profile_and_bounds() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.5 };
        let ccc = LatLon { lat: 0.0, lon: 3.0 };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let mut plan = three_airport_test_plan();
        plan.cruise_altitude_ft = Some(18_000);

        let state = default_flight_plan_ui_state_for_test(&store, &plan);
        let reason = state
            .altitude_planner
            .unavailable_reasons
            .iter()
            .find(|reason| {
                reason.code
                    == crate::AltitudePlannerUnavailableReasonCode::PerformanceRegimeUnavailable
            })
            .expect("performance range reason");

        assert_eq!(
            reason.message,
            "NORMAL CRUISE cruise performance covers 0–12,000 ft; this flight requires 18,000 ft."
        );
    }

    #[test]
    fn installed_forecast_is_a_core_selected_planner_alternative() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.5 };
        let ccc = LatLon { lat: 0.0, lon: 3.0 };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let plan = three_airport_test_plan();
        let project = |atmosphere| {
            flight_plan_ui_projection(
                &store,
                &test_aircraft_definitions(),
                &BTreeMap::new(),
                plan.clone(),
                crate::project_ui_state(&plan),
                crate::FlightDataComputer::default(),
                FlightPlanLiveData::default(),
                atmosphere,
            )
            .expect("project planner")
            .ui_state
        };

        let no_wind = project(PlannerAtmosphereSelection::no_wind(true));
        assert_eq!(wind_control(&no_wind).label, "WIND\nNO WIND");
        assert!(wind_control(&no_wind).enabled);
        assert_eq!(
            wind_control(&no_wind).action_uid.as_deref(),
            Some(SELECT_GFS_WIND_ACTION_UID)
        );

        let gfs = project(PlannerAtmosphereSelection::gfs(
            Some(&ConstantForecastAtmosphere),
            None,
        ));
        assert_eq!(wind_control(&gfs).label, "WIND\nFORECAST");
        assert_eq!(
            wind_control(&gfs).action_uid.as_deref(),
            Some(SELECT_NO_WIND_ACTION_UID)
        );
        let total_ete = |state: &FlightPlanUiState| {
            state
                .display_rows
                .iter()
                .find(|row| row.label == "TOTAL")
                .map(|row| row_cell(row, "waypoint_ete").value.clone())
                .expect("total ETE")
        };
        assert_ne!(total_ete(&gfs), total_ete(&no_wind));
    }

    #[test]
    fn aircraft_library_membership_filters_the_short_menu_but_keeps_the_selection_resolvable() {
        let store = three_airport_nav_store(
            LatLon { lat: 0.0, lon: 0.0 },
            LatLon { lat: 0.0, lon: 1.0 },
            LatLon { lat: 0.0, lon: 2.0 },
        );
        let definitions = test_aircraft_definitions();
        let aircraft = planner_aircraft(&store, None, &definitions).expect("planner aircraft");
        let hidden_hash = definitions
            .keys()
            .find(|hash| **hash != aircraft.selection.definition_hash)
            .expect("alternate aircraft")
            .clone();
        let memberships = BTreeMap::from([(
            hidden_hash.clone(),
            product_contracts::AircraftLibraryMembership { included: false },
        )]);
        let (options, _) = aircraft_control_options(&aircraft, &memberships);
        assert!(options
            .iter()
            .all(|option| !option.action_uid.contains(&hidden_hash)));

        let selected_memberships = BTreeMap::from([(
            aircraft.selection.definition_hash.clone(),
            product_contracts::AircraftLibraryMembership { included: false },
        )]);
        let (options, _) = aircraft_control_options(&aircraft, &selected_memberships);
        assert!(options.iter().any(|option| option.selected));
    }

    #[test]
    fn selected_missing_forecast_visibly_falls_back_to_no_wind_model() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.5 };
        let ccc = LatLon { lat: 0.0, lon: 3.0 };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let plan = three_airport_test_plan();
        let state = flight_plan_ui_projection(
            &store,
            &test_aircraft_definitions(),
            &BTreeMap::new(),
            plan.clone(),
            crate::project_ui_state(&plan),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData::default(),
            PlannerAtmosphereSelection::gfs(None, None),
        )
        .expect("project unavailable forecast")
        .ui_state;

        assert!(!state
            .altitude_planner
            .unavailable_reasons
            .iter()
            .any(|reason| {
                reason.code == crate::AltitudePlannerUnavailableReasonCode::WindModelUnavailable
            }));
        assert_eq!(
            state.altitude_planner.estimate_summary.label,
            "Estimate basis:\nNo-wind fallback\n12,000 cruise"
        );
        assert!(state.altitude_planner.advisories.iter().any(|message| {
            message.contains("forecast product is unavailable")
                && message.contains("showing no-wind/ISA estimates")
        }));
        assert!(wind_control(&state).enabled);
        assert_eq!(
            wind_control(&state).action_uid.as_deref(),
            Some(SELECT_NO_WIND_ACTION_UID)
        );
    }

    #[test]
    fn planned_departure_beyond_forecast_coverage_uses_visible_no_wind_fallback() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.5 };
        let ccc = LatLon { lat: 0.0, lon: 3.0 };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let mut plan = three_airport_test_plan();
        plan.planned_departure_time_epoch_ms = Some(48 * 60 * 60 * 1_000);
        let forecast = ForecastUntilAtmosphere {
            valid_through_epoch_ms: 24 * 60 * 60 * 1_000,
        };
        let (mut manifest, _, _, _) = crate::forecast_atmosphere::tests::test_forecast_payload();
        manifest.valid_times_epoch_ms = vec![0, forecast.valid_through_epoch_ms];

        let state = flight_plan_ui_projection(
            &store,
            &test_aircraft_definitions(),
            &BTreeMap::new(),
            plan.clone(),
            crate::project_ui_state(&plan),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData {
                now_epoch_ms: Some(0),
                time_display_mode: crate::TimeDisplayMode::Utc,
                ..FlightPlanLiveData::default()
            },
            PlannerAtmosphereSelection::gfs(Some(&forecast), Some(&manifest)),
        )
        .expect("project scheduled departure")
        .ui_state;

        assert_eq!(state.altitude_planner.departure.when_value, "2d");
        assert_eq!(
            state.altitude_planner.estimate_summary.label,
            "Estimate basis:\nNo-wind fallback\n12,000 cruise"
        );
        assert!(state.altitude_planner.advisories.is_empty());
        let forecast_status = state
            .altitude_planner
            .forecast
            .expect("coverage fallback status");
        assert!(forecast_status.summary.starts_with(
            "Currently available forecast product valid until 0000Z, flight extends until "
        ));
        assert!(forecast_status.summary.ends_with(". Using NO-WIND model."));

        let panel = altitude_comparison_panel(
            &store,
            &test_aircraft_definitions(),
            plan,
            FlightPlanLiveData {
                now_epoch_ms: Some(0),
                time_display_mode: crate::TimeDisplayMode::Utc,
                ..FlightPlanLiveData::default()
            },
            PlannerAtmosphereSelection::gfs(Some(&forecast), Some(&manifest)),
        )
        .expect("project comparison panel");
        assert!(
            panel.advisories.is_empty(),
            "coverage fallback must not also produce a comparison advisory"
        );
    }

    #[test]
    fn planned_departure_before_forecast_coverage_uses_visible_no_wind_fallback() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.5 };
        let ccc = LatLon { lat: 0.0, lon: 3.0 };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let mut plan = three_airport_test_plan();
        plan.planned_departure_time_epoch_ms = Some(6 * 60 * 60 * 1_000);
        let forecast = ForecastWindowAtmosphere {
            valid_from_epoch_ms: 12 * 60 * 60 * 1_000,
            valid_through_epoch_ms: 24 * 60 * 60 * 1_000,
        };
        let (mut manifest, _, _, _) = crate::forecast_atmosphere::tests::test_forecast_payload();
        manifest.valid_times_epoch_ms = vec![
            forecast.valid_from_epoch_ms,
            forecast.valid_through_epoch_ms,
        ];

        let state = flight_plan_ui_projection(
            &store,
            &test_aircraft_definitions(),
            &BTreeMap::new(),
            plan.clone(),
            crate::project_ui_state(&plan),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData {
                now_epoch_ms: Some(11 * 60 * 60 * 1_000),
                time_display_mode: crate::TimeDisplayMode::Utc,
                ..FlightPlanLiveData::default()
            },
            PlannerAtmosphereSelection::gfs(Some(&forecast), Some(&manifest)),
        )
        .expect("project past scheduled departure")
        .ui_state;

        assert_eq!(state.altitude_planner.departure.when_value, "−5h");
        assert!(state.altitude_planner.departure.when_is_past);
        assert_eq!(
            state
                .altitude_planner
                .forecast
                .expect("coverage fallback status")
                .summary,
            "Currently available forecast product valid from 1200Z, flight starts at 0600Z. Using NO-WIND model."
        );
    }

    #[test]
    fn forecast_provenance_reports_cycle_age_and_valid_window() {
        let (mut manifest, _, _, _) = crate::forecast_atmosphere::tests::test_forecast_payload();
        manifest.model_id = "gfs-0p25".to_string();
        manifest.cycle_time_epoch_ms = 3_600_000;
        manifest.valid_times_epoch_ms = vec![3_600_000, 7_200_000];

        let view = planner_forecast_ui(
            PlannerAtmosphereSelection::gfs(Some(&ConstantForecastAtmosphere), Some(&manifest)),
            5_400_000,
            crate::TimeDisplayMode::Utc,
            chrono_tz::UTC,
        )
        .expect("forecast provenance");

        assert_eq!(
            view.summary,
            "NOAA GFS-0P25 cycle 01/01 01:00Z (30m old); valid 01/01 01:00Z through 01/01 02:00Z."
        );
    }

    #[test]
    fn active_navigation_ignores_stored_departure_and_models_from_now() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.5 };
        let ccc = LatLon { lat: 0.0, lon: 3.0 };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let mut plan = three_airport_test_plan();
        plan.planned_departure_time_epoch_ms = Some(48 * 60 * 60 * 1_000);
        let plan = crate::activate_leg(&plan, 0).expect("activate first leg");
        let forecast = ForecastUntilAtmosphere {
            valid_through_epoch_ms: 24 * 60 * 60 * 1_000,
        };

        let panel = altitude_comparison_panel(
            &store,
            &test_aircraft_definitions(),
            plan,
            FlightPlanLiveData {
                ownship_position: Some(aaa),
                ownship_altitude_ft: Some(100.0),
                now_epoch_ms: Some(0),
                ..FlightPlanLiveData::default()
            },
            PlannerAtmosphereSelection::gfs(Some(&forecast), None),
        )
        .expect("active altitude comparisons");

        assert!(panel.advisories.is_empty());
        assert!(panel.rows.iter().filter(|row| row.enabled).all(|row| {
            row.advisory.is_none()
                && row
                    .cells
                    .iter()
                    .find(|cell| cell.id == "wind")
                    .and_then(|cell| cell.value.as_deref())
                    != Some("·")
        }));
    }

    #[test]
    fn nav_symbol_feature_decodes_current_navref_symbol_records() {
        let store = test_nav_kv_store(&[
            (
                "navref/symbol/airport/KRNT",
                serde_json::json!({
                    "kind": "airport",
                    "label": "KRNT",
                    "symbol_kind": "airport",
                    "style_class": "airport",
                    "towered": true
                }),
            ),
            (
                "navref/symbol/navaid/SEA",
                serde_json::json!({
                    "kind": "vortac",
                    "label": "SEA 116.80",
                    "symbol_kind": "nav",
                    "style_class": "nav"
                }),
            ),
            (
                "navref/symbol/fix/EPH",
                serde_json::json!({
                    "kind": "fix",
                    "label": "EPH",
                    "symbol_kind": "fix",
                    "style_class": "fix"
                }),
            ),
        ]);

        let airport = nav_symbol_feature(&store, &NavRef::Airport("KRNT".to_string()))
            .expect("decode airport symbol")
            .expect("airport symbol present");
        let navaid = nav_symbol_feature(&store, &NavRef::Navaid("SEA".to_string()))
            .expect("decode navaid symbol")
            .expect("navaid symbol present");
        let fix = nav_symbol_feature(&store, &NavRef::Fix("EPH".to_string()))
            .expect("decode fix symbol")
            .expect("fix symbol present");

        assert_eq!(airport.symbol_kind, "airport");
        assert_eq!(navaid.symbol_kind, "nav");
        assert_eq!(fix.symbol_kind, "fix");
    }

    fn procedure_plate_match_value(
        airport_id: &str,
        procedure_id: &str,
        plate_label: &str,
    ) -> serde_json::Value {
        serde_json::json!([{
            "airport_id": airport_id,
            "cifp_id": procedure_id,
            "procedure_kind": "approach",
            "plate_id": format!("Plate:{airport_id}:IAP-{procedure_id}.png"),
            "plate_label": plate_label,
            "package_id": "tpp-test",
            "public": 1,
            "priority": 0,
            "match_kind": "unique",
            "is_primary": 1
        }])
    }

    #[test]
    fn procedure_geometry_materialization_rehydrates_omitted_wire_fields() {
        let key = crate::navkv::procedure_geometry_key(
            "KAAA",
            &ProcedureKind::Approach,
            "RNAV-A",
            None,
            Some("TRANS"),
        );
        let store = test_nav_kv_store(&[
            (
                "plate/cifp/KAAA/RNAV-A",
                procedure_plate_match_value("KAAA", "RNAV-A", "RNAV-A"),
            ),
            (
                &key,
                serde_json::json!({
                    "leg_bundles": [{
                        "role": "common",
                        "from": { "kind": "airport", "value": "KAAA" },
                        "to": { "kind": "fix", "value": "FIXA" },
                        "path_termination": "track_to_fix",
                        "path": { "elements": [] }
                    }],
                    "data_quality": [{
                        "message": "Procedure encoding is suspicious; read plate."
                    }]
                }),
            ),
        ]);

        let options = describe_procedure_options(&store, "KAAA", "RNAV-A", ProcedureKind::Approach)
            .expect("describe key-only procedure choices");
        assert_eq!(
            options.valid_choices,
            vec![crate::ProcedureSpecChoice {
                runway_transition: None,
                enroute_transition: Some("TRANS".to_string()),
                label: "from TRANS".to_string(),
            }]
        );

        let materialized = materialize_procedure(
            &store,
            "KAAA",
            "RNAV-A",
            ProcedureKind::Approach,
            None,
            Some("TRANS"),
            2,
        )
        .expect("materialize keyless waypointless procedure geometry");

        assert_eq!(
            materialized.procedure.airport_id,
            AirportId("KAAA".to_string())
        );
        assert_eq!(materialized.procedure.procedure_id, "RNAV-A");
        assert_eq!(
            materialized.procedure.display_label.as_deref(),
            Some("RNAV-A")
        );
        assert_eq!(
            materialized.procedure.enroute_transition,
            Some("TRANS".to_string())
        );
        assert_eq!(
            materialized.procedure.data_quality,
            vec!["Procedure encoding is suspicious; read plate.".to_string()]
        );
        assert_eq!(materialized.resolved_legs.len(), 1);
        assert_eq!(
            materialized.resolved_legs[0].id,
            "procedure-KAAA-APPROACH-RNAV-A-_-TRANS-0"
        );
        assert_eq!(
            materialized.resolved_legs[0].source,
            ResolvedLegSource::RouteComponent { component_index: 2 }
        );
    }

    #[test]
    fn procedure_geometry_materialization_expands_segment_refs() {
        let segment_ref = "0123456789abcdef";
        let geometry_key = crate::navkv::procedure_geometry_key(
            "KAAA",
            &ProcedureKind::Approach,
            "RNAV-A",
            None,
            Some("TRANS"),
        );
        let segment_key = pgt::procedure_geometry_segment_navdb_key(segment_ref);
        let store = test_nav_kv_store(&[
            (
                "plate/cifp/KAAA/RNAV-A",
                procedure_plate_match_value("KAAA", "RNAV-A", "RNAV-A"),
            ),
            (
                &geometry_key,
                serde_json::json!({
                    "components": [{
                        "kind": "segment_ref",
                        "segment_ref": segment_ref
                    }]
                }),
            ),
            (
                &segment_key,
                serde_json::json!({
                    "leg_bundles": [{
                        "role": "common",
                        "from": { "kind": "airport", "value": "KAAA" },
                        "to": { "kind": "fix", "value": "FIXA" },
                        "path_termination": "track_to_fix",
                        "path": { "elements": [] }
                    }]
                }),
            ),
        ]);

        let materialized = materialize_procedure(
            &store,
            "KAAA",
            "RNAV-A",
            ProcedureKind::Approach,
            None,
            Some("TRANS"),
            2,
        )
        .expect("materialize split procedure geometry");

        assert_eq!(materialized.resolved_legs.len(), 1);
        assert_eq!(
            materialized.resolved_legs[0].id,
            "procedure-KAAA-APPROACH-RNAV-A-_-TRANS-0"
        );
    }

    #[test]
    fn flight_plan_ui_state_projects_magnetic_course_columns() {
        let store = test_nav_kv_store(&[
            ("magvar/48/-111", serde_json::json!(14.0)),
            ("magvar/48/-110", serde_json::json!(14.0)),
            ("magvar/49/-111", serde_json::json!(14.0)),
            ("magvar/49/-110", serde_json::json!(14.0)),
            (
                "navref/position/airport/KAAA",
                serde_json::json!({"lat": 48.0, "lon": -110.0}),
            ),
            (
                "navref/position/airport/KBBB",
                serde_json::json!({"lat": 48.0, "lon": -111.0}),
            ),
        ]);
        let plan = crate::build_flight_plan(FlightPlan {
            id: "plan-magvar".to_string(),
            name: "KAAA KBBB".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KBBB".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .expect("build plan");
        let true_course = project_flight_plan_route(&store, &plan).unwrap()[0].course_deg;
        let ui_state = flight_plan_ui_state(
            &store,
            plan.clone(),
            crate::planning::project_ui_state(&plan),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData::default(),
        )
        .expect("project flight plan ui state");
        let row_course = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "KBBB")
            .and_then(|row| {
                row.data_cells
                    .iter()
                    .find(|cell| cell.id == "desired_track")
                    .and_then(|cell| cell.value.as_deref())
            })
            .expect("destination row course");

        assert!((true_course - 270.3).abs() < 0.5, "{true_course}");
        assert_eq!(
            row_course,
            crate::flight_data::format_course_degrees(true_course - 14.0)
        );
    }

    #[test]
    fn flight_plan_ui_state_projects_live_remaining_distance_and_summary() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.0 };
        let ccc = LatLon { lat: 0.0, lon: 2.0 };
        let ddd = LatLon { lat: 0.0, lon: 3.0 };
        let ownship = LatLon { lat: 0.0, lon: 1.5 };
        let store = test_nav_kv_store(&[
            (
                "navref/position/airport/KAAA",
                serde_json::json!({"lat": aaa.lat, "lon": aaa.lon}),
            ),
            (
                "navref/position/airport/KBBB",
                serde_json::json!({"lat": bbb.lat, "lon": bbb.lon}),
            ),
            (
                "navref/position/airport/KCCC",
                serde_json::json!({"lat": ccc.lat, "lon": ccc.lon}),
            ),
            (
                "navref/position/airport/KDDD",
                serde_json::json!({"lat": ddd.lat, "lon": ddd.lon}),
            ),
        ]);
        let plan = crate::build_flight_plan(FlightPlan {
            id: "plan-live-distance".to_string(),
            name: "KAAA KBBB KCCC KDDD".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KCCC".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KDDD".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KDDD".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .expect("build plan");
        let plan = crate::activate_leg(&plan, 1).expect("activate KBBB-KCCC leg");
        let now_epoch_ms = 12 * 60 * 60 * 1000;
        let computer = crate::FlightDataComputer::new(Some(120.0));
        let ui_state = flight_plan_ui_state(
            &store,
            plan.clone(),
            crate::planning::project_ui_state(&plan),
            computer,
            FlightPlanLiveData {
                ownship_position: Some(ownship),
                ownship_altitude_ft: Some(4_000.0),
                now_epoch_ms: Some(now_epoch_ms),
                ..FlightPlanLiveData::default()
            },
        )
        .expect("project live flight plan ui state");

        let bbb_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "KBBB")
            .expect("KBBB row");
        let ccc_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "KCCC")
            .expect("KCCC row");
        let ddd_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "KDDD")
            .expect("KDDD row");
        let summary_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.row_kind == crate::FlightPlanDisplayRowKind::Summary)
            .expect("summary row");
        let live_distance_nm = crate::great_circle_distance_nm(ownship, ccc);
        let remaining_planned_nm = crate::great_circle_distance_nm(ccc, ddd);
        let live_total_nm = live_distance_nm + remaining_planned_nm;

        assert_eq!(
            row_cell(bbb_row, "waypoint_distance").tone,
            crate::FlightDataCellTone::Passed
        );
        assert_eq!(
            row_cell(bbb_row, "waypoint_ete").value.as_deref(),
            None,
            "rows before the active guidance leg should not contribute to remaining ETE"
        );
        assert_eq!(
            row_cell(ccc_row, "waypoint_distance").value.as_deref(),
            Some(crate::flight_data::format_nm(live_distance_nm).as_str())
        );
        assert_eq!(
            row_cell(ccc_row, "waypoint_distance").tone,
            crate::FlightDataCellTone::Active
        );
        assert_eq!(
            row_cell(ccc_row, "final_eta").value,
            computer.format_eta_at(live_distance_nm, now_epoch_ms)
        );
        assert_eq!(
            row_cell(ddd_row, "waypoint_distance").tone,
            crate::FlightDataCellTone::Planned
        );
        assert_eq!(
            row_cell(ddd_row, "waypoint_distance").value.as_deref(),
            Some(crate::flight_data::format_nm(remaining_planned_nm).as_str())
        );
        assert_eq!(
            row_cell(summary_row, "waypoint_distance").value.as_deref(),
            Some(crate::flight_data::format_nm(live_total_nm).as_str())
        );
        assert_eq!(
            row_cell(summary_row, "waypoint_ete").value.as_deref(),
            row_cell(ddd_row, "waypoint_ete").value.as_deref()
        );
        assert_eq!(
            row_cell(summary_row, "waypoint_distance").tone,
            crate::FlightDataCellTone::Active
        );
        assert!(
            row_cell(summary_row, "fuel")
                .value
                .as_ref()
                .is_some_and(|value| !value.is_empty()),
            "live-GS estimates must use the selected aircraft profile's fuel flow"
        );
        assert_eq!(
            row_cell(summary_row, "fuel").estimate_kind,
            crate::FlightEstimateKind::Basic
        );
    }

    #[test]
    fn flight_plan_ui_state_keeps_static_distance_when_active_but_ownship_missing() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.0 };
        let ccc = LatLon { lat: 0.0, lon: 2.0 };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let plan =
            crate::activate_leg(&three_airport_test_plan(), 1).expect("activate KBBB-KCCC leg");
        let ui_state = flight_plan_ui_state(
            &store,
            plan.clone(),
            crate::planning::project_ui_state(&plan),
            crate::FlightDataComputer::with_fuel_flow_and_clock(
                None,
                None,
                Some(12 * 60 * 60 * 1000),
            ),
            FlightPlanLiveData {
                ownship_position: None,
                ownship_altitude_ft: None,
                now_epoch_ms: Some(12 * 60 * 60 * 1000),
                ..FlightPlanLiveData::default()
            },
        )
        .expect("project active flight plan without ownship");

        let aaa_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "KAAA")
            .expect("KAAA row");
        let bbb_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "KBBB")
            .expect("KBBB row");
        let ccc_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "KCCC")
            .expect("KCCC row");
        let summary_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.row_kind == crate::FlightPlanDisplayRowKind::Summary)
            .expect("summary row");
        let static_bbb_ccc_nm = crate::great_circle_distance_nm(bbb, ccc);
        assert_eq!(
            row_cell(aaa_row, "waypoint_distance").value.as_deref(),
            None,
            "the first plan row has no preceding segment"
        );
        assert_eq!(
            row_cell(bbb_row, "waypoint_distance").tone,
            crate::FlightDataCellTone::Passed,
            "navigation progress remains known when live position is unavailable"
        );
        assert_eq!(
            row_cell(ccc_row, "waypoint_distance").value.as_deref(),
            Some(crate::flight_data::format_nm(static_bbb_ccc_nm).as_str()),
            "active guidance without ownship must keep the active row's static leg distance"
        );
        assert_eq!(
            row_cell(summary_row, "waypoint_distance").value.as_deref(),
            Some(crate::flight_data::format_nm(static_bbb_ccc_nm).as_str()),
            "the active row falls back to its fixed geometry without restoring passed legs"
        );
        assert_eq!(
            row_cell(summary_row, "waypoint_distance").tone,
            crate::FlightDataCellTone::Active
        );
        for cell_id in ["final_eta", "waypoint_ete", "fuel"] {
            let cell = row_cell(summary_row, cell_id);
            assert!(
                cell.value.as_ref().is_some_and(|value| !value.is_empty()),
                "{cell_id} should use the aircraft model while ownship is unavailable"
            );
            assert_eq!(cell.estimate_kind, crate::FlightEstimateKind::Modeled);
        }
        assert!(ui_state
            .altitude_planner
            .unavailable_reasons
            .iter()
            .any(|reason| reason.code
                == crate::AltitudePlannerUnavailableReasonCode::OwnshipAltitudeUnavailable));
    }

    #[test]
    fn active_plan_with_zero_groundspeed_uses_aircraft_model() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.0 };
        let ccc = LatLon { lat: 0.0, lon: 2.0 };
        let ownship = LatLon { lat: 0.0, lon: 1.5 };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let plan =
            crate::activate_leg(&three_airport_test_plan(), 1).expect("activate KBBB-KCCC leg");
        let ui_state = flight_plan_ui_state(
            &store,
            plan.clone(),
            crate::planning::project_ui_state(&plan),
            crate::FlightDataComputer::with_fuel_flow_and_clock(
                Some(0.0),
                None,
                Some(12 * 60 * 60 * 1000),
            ),
            FlightPlanLiveData {
                ownship_position: Some(ownship),
                ownship_altitude_ft: Some(4_000.0),
                now_epoch_ms: Some(12 * 60 * 60 * 1000),
                ..FlightPlanLiveData::default()
            },
        )
        .expect("project active flight plan at zero groundspeed");
        let summary = ui_state
            .display_rows
            .iter()
            .find(|row| row.row_kind == crate::FlightPlanDisplayRowKind::Summary)
            .expect("summary row");

        assert_eq!(
            ui_state.altitude_planner.estimate_kind,
            crate::FlightEstimateKind::Modeled
        );
        for cell_id in ["final_eta", "waypoint_ete", "fuel"] {
            let cell = row_cell(summary, cell_id);
            assert!(cell.value.as_ref().is_some_and(|value| !value.is_empty()));
            assert_eq!(cell.estimate_kind, crate::FlightEstimateKind::Modeled);
        }
    }

    #[test]
    fn active_altitude_comparison_starts_at_ownship_and_ignores_passed_legs() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.0 };
        let ccc = LatLon { lat: 0.0, lon: 2.0 };
        let ownship = LatLon { lat: 0.0, lon: 1.5 };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let inactive_plan = three_airport_test_plan();
        let active_plan = crate::activate_leg(&inactive_plan, 1).expect("activate KBBB-KCCC leg");
        let selected_fuel = |panel: &crate::AltitudeComparisonPanelUiView| {
            panel
                .rows
                .iter()
                .find(|row| row.selected)
                .and_then(|row| row.cells.iter().find(|cell| cell.id == "fuel"))
                .and_then(|cell| cell.value.as_deref())
                .expect("selected fuel")
                .parse::<f64>()
                .expect("numeric fuel")
        };
        let inactive = altitude_comparison_panel(
            &store,
            &test_aircraft_definitions(),
            inactive_plan,
            FlightPlanLiveData::default(),
            PlannerAtmosphereSelection::no_wind(false),
        )
        .expect("inactive comparison");
        let active = altitude_comparison_panel(
            &store,
            &test_aircraft_definitions(),
            active_plan,
            FlightPlanLiveData {
                ownship_position: Some(ownship),
                ownship_altitude_ft: Some(4_000.0),
                now_epoch_ms: Some(12 * 60 * 60 * 1000),
                ..FlightPlanLiveData::default()
            },
            PlannerAtmosphereSelection::no_wind(false),
        )
        .expect("active comparison");

        assert!(selected_fuel(&active) < selected_fuel(&inactive) * 0.5);
        assert!(active.rows.iter().all(|row| row.enabled));
    }

    #[test]
    fn flight_plan_ui_state_keeps_static_distance_when_ownship_present_but_no_active_leg() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.0 };
        let ccc = LatLon { lat: 0.0, lon: 2.0 };
        let ownship = LatLon {
            lat: 10.0,
            lon: 20.0,
        };
        let store = three_airport_nav_store(aaa, bbb, ccc);
        let plan = three_airport_test_plan();
        let ui_state = flight_plan_ui_state(
            &store,
            plan.clone(),
            crate::planning::project_ui_state(&plan),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData {
                ownship_position: Some(ownship),
                ownship_altitude_ft: Some(4_000.0),
                now_epoch_ms: Some(12 * 60 * 60 * 1000),
                ..FlightPlanLiveData::default()
            },
        )
        .expect("project inactive flight plan with ownship");

        let aaa_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "KAAA")
            .expect("KAAA row");
        let bbb_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "KBBB")
            .expect("KBBB row");
        let ccc_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.label == "KCCC")
            .expect("KCCC row");
        let summary_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.row_kind == crate::FlightPlanDisplayRowKind::Summary)
            .expect("summary row");
        let static_bbb_ccc_nm = crate::great_circle_distance_nm(bbb, ccc);
        let static_total_nm = crate::great_circle_distance_nm(aaa, bbb) + static_bbb_ccc_nm;
        let ownship_to_ccc_nm = crate::great_circle_distance_nm(ownship, ccc);
        assert_ne!(
            crate::flight_data::format_nm(static_bbb_ccc_nm),
            crate::flight_data::format_nm(ownship_to_ccc_nm),
            "test fixture must distinguish static and ownship-derived distances"
        );

        assert_eq!(
            row_cell(aaa_row, "waypoint_distance").value.as_deref(),
            None,
            "the first plan row has no preceding segment"
        );
        assert_eq!(
            row_cell(bbb_row, "waypoint_distance").tone,
            crate::FlightDataCellTone::Planned
        );
        assert_eq!(
            row_cell(ccc_row, "waypoint_distance").value.as_deref(),
            Some(crate::flight_data::format_nm(static_bbb_ccc_nm).as_str()),
            "ownship without active guidance must keep the row's static leg distance"
        );
        assert_eq!(
            row_cell(summary_row, "waypoint_distance").value.as_deref(),
            Some(crate::flight_data::format_nm(static_total_nm).as_str()),
            "ownship without active guidance must keep the static total distance"
        );
        assert_eq!(
            row_cell(summary_row, "waypoint_distance").tone,
            crate::FlightDataCellTone::Planned
        );
    }

    #[test]
    fn flight_plan_ui_state_projects_live_distance_for_off_plan_direct_to_spot() {
        let store = test_nav_kv_store(&[]);
        let ownship = LatLon {
            lat: 47.60,
            lon: -122.30,
        };
        let target = LatLon {
            lat: 47.67,
            lon: -122.12,
        };
        let plan = crate::activate_direct_to(&FlightPlan::empty(), ownship, NavRef::LatLon(target))
            .expect("activate off-plan direct-to");
        let ui_state = flight_plan_ui_state(
            &store,
            plan.clone(),
            crate::planning::project_ui_state(&plan),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData {
                ownship_position: Some(ownship),
                ownship_altitude_ft: Some(4_000.0),
                now_epoch_ms: Some(12 * 60 * 60 * 1000),
                ..FlightPlanLiveData::default()
            },
        )
        .expect("project direct-to flight plan ui state");
        let direct_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.synthetic_direct_to)
            .expect("synthetic direct-to row");
        let distance_cell = direct_row
            .data_cells
            .iter()
            .find(|cell| cell.id == "waypoint_distance")
            .expect("distance cell");
        let expected_distance =
            crate::flight_data::format_nm(crate::great_circle_distance_nm(ownship, target));

        assert_eq!(direct_row.nav_ref, Some(NavRef::LatLon(target)));
        assert_eq!(
            distance_cell.value.as_deref(),
            Some(expected_distance.as_str())
        );
        assert_eq!(distance_cell.tone, crate::FlightDataCellTone::Active);
    }

    #[test]
    fn flight_plan_ui_state_projects_static_total_summary_without_active_leg() {
        let aaa = LatLon { lat: 0.0, lon: 0.0 };
        let bbb = LatLon { lat: 0.0, lon: 1.0 };
        let ccc = LatLon { lat: 0.0, lon: 2.0 };
        let store = test_nav_kv_store(&[
            (
                "navref/position/airport/KAAA",
                serde_json::json!({"lat": aaa.lat, "lon": aaa.lon}),
            ),
            (
                "navref/position/airport/KBBB",
                serde_json::json!({"lat": bbb.lat, "lon": bbb.lon}),
            ),
            (
                "navref/position/airport/KCCC",
                serde_json::json!({"lat": ccc.lat, "lon": ccc.lon}),
            ),
        ]);
        let plan = crate::build_flight_plan(FlightPlan {
            id: "plan-static-total".to_string(),
            name: "KAAA KBBB KCCC".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KCCC".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KCCC".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .expect("build plan");
        let ui_state = flight_plan_ui_state(
            &store,
            plan.clone(),
            crate::planning::project_ui_state(&plan),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData::default(),
        )
        .expect("project static flight plan ui state");
        let total_distance_nm =
            crate::great_circle_distance_nm(aaa, bbb) + crate::great_circle_distance_nm(bbb, ccc);
        let summary_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.row_kind == crate::FlightPlanDisplayRowKind::Summary)
            .expect("summary row");
        let distance_cell = summary_row
            .data_cells
            .iter()
            .find(|cell| cell.id == "waypoint_distance")
            .expect("summary distance cell");

        assert_eq!(summary_row.label, "TOTAL");
        assert_eq!(
            distance_cell.value.as_deref(),
            Some(crate::flight_data::format_nm(total_distance_nm).as_str())
        );
    }

    #[test]
    fn flight_plan_ui_state_batches_enrichment_page_faults() {
        let large_symbol = format!(
            r#"{{
                "kind":"airport",
                "label":"{}",
                "symbol_kind":"airport",
                "style_class":"airport",
                "towered":false,
                "fuel_available":false,
                "runway_length_ratio":0.0,
                "longest_runway_heading_true_deg":null
            }}"#,
            "x".repeat(5000)
        )
        .into_bytes();
        let large_magvar = format!("14.0{}", " ".repeat(4996)).into_bytes();
        let mut entries = vec![
            ("magvar/48/-111", large_magvar.as_slice()),
            (
                "navref/position/airport/KAAA",
                br#"{"lat":48.0,"lon":-110.0}"#.as_slice(),
            ),
            (
                "navref/position/airport/KBBB",
                br#"{"lat":48.0,"lon":-111.0}"#.as_slice(),
            ),
            ("navref/symbol/airport/KAAA", large_symbol.as_slice()),
            ("navref/symbol/airport/KBBB", large_symbol.as_slice()),
        ];
        entries.sort_by(|left, right| left.0.cmp(right.0));
        let (root, pages) = fixture(&entries, 8192);
        let mut store = NavKvStore::new(root);
        store.insert_page(0, pages[0].clone());
        let plan = crate::build_flight_plan(FlightPlan {
            id: "plan-batched-page-faults".to_string(),
            name: "KAAA KBBB".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KBBB".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .expect("build plan");

        let missing_pages = match flight_plan_ui_state(
            &store,
            plan.clone(),
            crate::planning::project_ui_state(&plan),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData::default(),
        ) {
            Ok(_) => panic!("expected batched page fault"),
            Err(HadReadError::NeedPages(pages)) => {
                assert!(
                    pages.len() > 1,
                    "expected batched enrichment pages, got {pages:?}"
                );
                pages
            }
            Err(HadReadError::Fatal(message)) => panic!("unexpected fatal error: {message}"),
        };
        for page in missing_pages {
            store.insert_page(page, pages[page as usize].clone());
        }
        flight_plan_ui_state(
            &store,
            plan.clone(),
            crate::planning::project_ui_state(&plan),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData::default(),
        )
        .expect("project flight plan ui state after batch");
    }

    #[test]
    fn project_flight_plan_route_batches_position_page_faults() {
        let large_position = |lat: f64, lon: f64| {
            format!(
                r#"{{"lat":{lat},"lon":{lon},"padding":"{}"}}"#,
                "x".repeat(5000)
            )
            .into_bytes()
        };
        let aaa = large_position(48.0, -110.0);
        let bbb = large_position(48.0, -111.0);
        let ccc = large_position(48.5, -111.5);
        let mut entries = vec![
            ("navref/position/airport/KAAA", aaa.as_slice()),
            ("navref/position/airport/KBBB", bbb.as_slice()),
            ("navref/position/airport/KCCC", ccc.as_slice()),
        ];
        entries.sort_by(|left, right| left.0.cmp(right.0));
        let (root, pages) = fixture(&entries, 8192);
        let mut store = NavKvStore::new(root);
        store.insert_page(0, pages[0].clone());
        let plan = crate::build_flight_plan(FlightPlan {
            id: "plan-route-page-faults".to_string(),
            name: "KAAA KBBB KCCC".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KAAA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KBBB".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KCCC".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KAAA".to_string())),
            destination: Some(AirportId("KCCC".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        })
        .expect("build plan");

        let missing_pages = match project_flight_plan_route(&store, &plan) {
            Ok(_) => panic!("expected batched page fault"),
            Err(HadReadError::NeedPages(pages)) => {
                assert!(pages.len() > 1, "expected batched pages, got {pages:?}");
                pages
            }
            Err(HadReadError::Fatal(message)) => panic!("unexpected fatal error: {message}"),
        };
        for page in missing_pages {
            store.insert_page(page, pages[page as usize].clone());
        }
        let route = project_flight_plan_route(&store, &plan).expect("project route after batch");
        assert_eq!(route.len(), 2);
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn generated_nav_kv_projects_kpae_vor_a_inserted_plan_ui_state() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "krnt-sea-pae".to_string(),
            name: "KRNT SEA PAE".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("SEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: Some(crate::GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KPAE".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let built = materialize_procedure(
            &store,
            "KPAE",
            "VOR-A",
            ProcedureKind::Approach,
            None,
            Some("ECEPO"),
            2,
        )
        .expect("materialize KPAE VOR-A ECEPO through generated nav_kv");
        let mutation = crate::insert_procedure_materialized(&plan, 1, 2, built)
            .expect("insert KPAE VOR-A ECEPO");
        let route = project_flight_plan_route(&store, &mutation).expect("project route");
        assert!(
            route.iter().any(|segment| matches!(
                segment.geometry,
                crate::GuidanceRouteGeometry::Arc { .. }
            )),
            "procedure route should preserve fine-grained arc geometry"
        );
        assert!(
            route.len() > mutation.resolved_legs.len(),
            "procedure route should split coarse legs into drawable fine-grained segments"
        );

        let ui_state = flight_plan_ui_projection(
            &store,
            &test_aircraft_definitions(),
            &BTreeMap::new(),
            mutation.clone(),
            crate::project_ui_state(&mutation),
            crate::FlightDataComputer::default(),
            FlightPlanLiveData::default(),
            PlannerAtmosphereSelection::no_wind(false),
        )
        .expect("project flight plan ui state")
        .ui_state;
        assert!(
            mutation.route_components.iter().any(|component| {
                matches!(
                    component,
                    RouteComponent::Procedure { procedure }
                        if procedure.procedure_id == "VOR-A"
                )
            }),
            "expected inserted procedure component in authoritative plan"
        );
        for (label, expected_leg_index) in [
            ("ECEPO", 1),
            ("YAVUR", 2),
            ("ZELIG", 3),
            ("XUKRE", 4),
            ("ECEPO", 5),
        ] {
            let row = ui_state
                .display_rows
                .iter()
                .filter(|row| row.depth == 1 && row.label == label)
                .find(|row| row.leg_index == Some(expected_leg_index))
                .unwrap_or_else(|| panic!("expected procedure row for {label}"));
            assert_eq!(
                row.leg_index,
                Some(expected_leg_index),
                "procedure waypoint row {label} should activate the guidance leg ending there"
            );
            assert!(
                crate::planning::flight_plan_row_actions(row).any(|action| {
                    action.id == crate::FlightPlanRowActionId::ActivateLeg && action.enabled
                }),
                "procedure waypoint row {label} should expose enabled Activate Leg"
            );
            assert!(
                        crate::planning::flight_plan_row_actions(row).all(|action| {
                            action.id != crate::FlightPlanRowActionId::WaypointInfo
                                && action.id != crate::FlightPlanRowActionId::Plates
                        }),
                        "procedure waypoint row {label} should not expose generic waypoint info or plates actions"
                    );
        }
        let hold_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.depth == 1 && row.label == "HOLD")
            .expect("expected procedure hold row");
        assert_eq!(
            hold_row.leg_index,
            Some(5),
            "hold row should activate the guidance leg carrying the hold geometry"
        );
        assert!(
            crate::planning::flight_plan_row_actions(hold_row).any(|action| {
                action.id == crate::FlightPlanRowActionId::ActivateLeg && action.enabled
            }),
            "hold row should expose enabled Activate Leg"
        );

        let activated =
            crate::activate_leg(&mutation, 5).expect("activate XUKRE -> ECEPO guidance leg");
        let activated_ui = crate::project_ui_state(&activated);
        let active_ecepo_row = activated_ui
            .display_rows
            .iter()
            .filter(|row| row.depth == 1 && row.label == "ECEPO")
            .find(|row| row.leg_index == Some(5))
            .expect("active ECEPO row");
        assert!(
            crate::planning::flight_plan_row_actions(active_ecepo_row).any(|action| {
                action.id == crate::FlightPlanRowActionId::ActivateLeg && !action.enabled
            }),
            "already-active guidance leg row should disable Activate Leg"
        );
        let active_hold_row = activated_ui
            .display_rows
            .iter()
            .find(|row| row.depth == 1 && row.label == "HOLD")
            .expect("active plan hold row");
        assert!(
            crate::planning::flight_plan_row_actions(active_hold_row).any(|action| {
                action.id == crate::FlightPlanRowActionId::ActivateLeg && action.enabled
            }),
            "hold row should still activate the hold detail when the inbound leg is active"
        );
        let airport_row = activated_ui
            .display_rows
            .iter()
            .find(|row| row.depth == 0 && row.label == "KPAE")
            .expect("destination airport row");
        assert_eq!(
                    airport_row.leg_index, None,
                    "destination airport after a terminal-hold procedure should not bridge to the procedure's last guidance leg"
                );

        let activated_route =
            project_flight_plan_route(&store, &activated).expect("project active route");
        let active_segment_index = activated_route
            .iter()
            .position(|segment| segment.status == crate::FlightPlanRouteSegmentStatus::Active)
            .expect("active guidance leg should have a CDI-active path element");
        let active_leg_id = activated_route[active_segment_index].leg_id.clone();
        let active_leg_segments = activated_route
            .iter()
            .filter(|segment| segment.leg_id == active_leg_id)
            .collect::<Vec<_>>();
        assert!(
            active_leg_segments
                .iter()
                .filter(|segment| { segment.status == crate::FlightPlanRouteSegmentStatus::Active })
                .count()
                == 1,
            "XUKRE -> ECEPO should have exactly one CDI-active path element"
        );
        let first_hold_segment_index = active_leg_segments
            .iter()
            .position(|segment| segment.status == crate::FlightPlanRouteSegmentStatus::Remaining)
            .expect("XUKRE -> ECEPO should include inactive hold path elements");
        assert!(
            active_leg_segments
                .iter()
                .take(first_hold_segment_index)
                .filter(|segment| {
                    segment.status == crate::FlightPlanRouteSegmentStatus::ActiveLegRemaining
                })
                .count()
                > 0,
            "XUKRE -> ECEPO should light remaining non-hold path elements"
        );
        assert!(
            active_leg_segments
                .iter()
                .skip(first_hold_segment_index)
                .all(|segment| {
                    segment.status == crate::FlightPlanRouteSegmentStatus::Remaining
                }),
            "hold path elements should not light up until the hold row is activated"
        );

        let hold_detail = crate::terminal_hold_start_detail_index_for_leg(&activated, 5)
            .expect("hold detail start");
        let hold_activated = crate::activate_leg_at_detail_index(&activated, 5, hold_detail)
            .expect("activate hold detail");
        let hold_activated_ui = crate::project_ui_state(&hold_activated);
        let guidance = hold_activated_ui.guidance.as_ref().expect("guidance");
        let active_from_row = hold_activated_ui
            .display_rows
            .iter()
            .find(|row| guidance.active_from_row_uid.as_ref() == Some(&row.uid))
            .expect("hold active from row");
        let active_to_row = hold_activated_ui
            .display_rows
            .iter()
            .find(|row| guidance.active_to_row_uid.as_ref() == Some(&row.uid))
            .expect("hold active to row");
        assert_eq!(active_from_row.label, "ECEPO");
        assert_eq!(active_to_row.label, "HOLD");

        let hold_activated_route =
            project_flight_plan_route(&store, &hold_activated).expect("project hold route");
        let hold_segments = hold_activated_route
            .iter()
            .filter(|segment| segment.leg_id == active_leg_id)
            .collect::<Vec<_>>();
        let first_hold_active_index = hold_segments
            .iter()
            .position(|segment| segment.status == crate::FlightPlanRouteSegmentStatus::Active)
            .expect("activating HOLD should light the racetrack");
        assert!(
            hold_segments
                .iter()
                .take(first_hold_active_index)
                .all(|segment| segment.status == crate::FlightPlanRouteSegmentStatus::Completed),
            "activating HOLD should mark the inbound elements complete"
        );

        let zelig_activated =
            crate::activate_leg(&mutation, 4).expect("activate ZELIG -> XUKRE guidance leg");
        let zelig_route =
            project_flight_plan_route(&store, &zelig_activated).expect("project ZELIG route");
        let zelig_segments = zelig_route
            .iter()
            .filter(|segment| segment.status == crate::FlightPlanRouteSegmentStatus::Active)
            .collect::<Vec<_>>();
        assert_eq!(
            zelig_segments.len(),
            1,
            "ZELIG -> XUKRE should have one active path element"
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn fixture_nav_db_plate_procedure_candidate_keys_round_trip_through_core_encoder() {
        let store = load_fixture_nav_kv_store();
        let prefix = "plate/procedure-candidates/";
        let keys = store.keys_with_prefix(prefix);
        assert!(
            !keys.is_empty(),
            "current nav-db has no {prefix} records; plate-page LOAD APPCH cannot be validated"
        );

        for key in keys {
            let bytes = match store
                .get_bytes(&key)
                .unwrap_or_else(|err| panic!("read {key}: {err}"))
            {
                NavKvLookup::Hit(bytes) => bytes,
                other => panic!("expected loaded value for enumerated key {key}, got {other:?}"),
            };
            let rows: Vec<CifpTppMatchRow> =
                serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("decode {key}: {err}"));
            assert!(!rows.is_empty(), "empty candidate row list at {key}");
            for row in rows {
                let generated =
                    crate::nav_kv_key_for_query(&NavKvQuery::PlateProcedureCandidates {
                        plate_id: row.plate_id.clone(),
                    })
                    .expect("plate procedure candidate query key");
                assert_eq!(
                    generated, key,
                    "core must use the same HAD component escaping as preproc for plate_id {:?}",
                    row.plate_id
                );
            }
        }
    }

    #[test]
    fn flight_plan_ui_state_enrichment_preserves_live_guidance_nav_element() {
        let store = test_nav_kv_store(&[
            (
                "navref/position/airport/KPAO",
                serde_json::json!({"lat": 37.461, "lon": -122.115}),
            ),
            (
                "navref/position/fix/VPDUB",
                serde_json::json!({"lat": 38.0, "lon": -122.0}),
            ),
            (
                "navref/position/airport/KVCB",
                serde_json::json!({"lat": 38.377, "lon": -121.962}),
            ),
            (
                "navref/position/airport/KWLW",
                serde_json::json!({"lat": 39.516, "lon": -122.218}),
            ),
        ]);
        let plan = FlightPlan {
            id: "kpao-vpdub-vcb-wlw".to_string(),
            name: "KPAO VPDUB KVCB KWLW".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAO".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Fix("VPDUB".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KVCB".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KWLW".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: Some(crate::GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KPAO".to_string())),
            destination: Some(AirportId("KWLW".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let mut current_ui_state =
            crate::project_ui_state(&crate::build_flight_plan(plan.clone()).expect("build plan"));
        current_ui_state
            .guidance
            .as_mut()
            .expect("guidance")
            .nav_element = NavElementUiView {
            active_leg_summary: "LIVE".to_string(),
            cdi_indicator_dots: Some(2.5),
            cdi_offscale_readout: Some("R".to_string()),
        };

        let ui_state = flight_plan_ui_state(
            &store,
            plan,
            current_ui_state,
            crate::FlightDataComputer::default(),
            FlightPlanLiveData::default(),
        )
        .expect("project enriched flight plan ui state");

        let nav_element = &ui_state.guidance.as_ref().expect("guidance").nav_element;
        assert_eq!(nav_element.active_leg_summary, "LIVE");
        assert_eq!(nav_element.cdi_indicator_dots, Some(2.5));
        assert_eq!(nav_element.cdi_offscale_readout.as_deref(), Some("R"));
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn generated_nav_kv_session_select_chart_keeps_kpae_vor_a() {
        let store = load_fixture_nav_kv_store();
        let kpae = match run_had_operation(
            &store,
            HadOperation::PlateAirport {
                airport_id: "KPAE".to_string(),
            },
        )
        .expect("load generated KPAE plate airport")
        {
            HadOperationOutcome::Complete { result, .. } => {
                serde_json::from_value::<crate::DerivedChartAirport>(result)
                    .expect("decode generated KPAE plate airport")
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!(
                    "expected complete KPAE plate airport, got missing resources: {resources:?}"
                );
            }
            HadOperationOutcome::NeedSnapshotResources { .. } => {
                panic!("generic HAD operation requested a session snapshot continuation")
            }
        };
        let vor_a = kpae
            .charts
            .iter()
            .find(|chart| chart.label == "VOR-A")
            .unwrap_or_else(|| {
                panic!(
                    "expected KPAE VOR-A chart in generated catalog: {:?}",
                    kpae.charts
                )
            })
            .clone();
        assert_eq!(vor_a.label, "VOR-A");
        let _chart_catalog = crate::DerivedChartCatalog {
            airports: vec![kpae],
        };
        let plan = FlightPlan {
            id: "krnt-sea-pae".to_string(),
            name: "KRNT SEA KPAE".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Navaid("SEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: Some(crate::GuidanceState {
                active_leg_index: 0,
                active_detail_index: Some(0),
                display_split_leg_id: None,
                sequencing_mode: SequencingMode::FollowPlan,
                direct_to: None,
                suspend_reason: None,
            }),
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KPAE".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let init = crate::create_ui_session(plan, &["KPAE".to_string()], Some("KPAE"), None)
            .expect("create ui session");

        let outcome = crate::select_chart_in_session(init.handle, &vor_a.id)
            .expect("select KPAE VOR-A chart in session");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("chart selection unexpectedly needed resources: {outcome:?}");
        };
        let snapshot: crate::session::UiSessionSnapshot =
            serde_json::from_value(result).expect("decode session snapshot");

        assert_eq!(snapshot.chart_page_state.selected_airport_id, "KPAE");
        assert_eq!(snapshot.chart_page_state.selected_chart_id, vor_a.id);
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn generated_nav_kv_chart_catalog_lists_multiple_regions() {
        let store = load_fixture_nav_kv_store();
        let catalog = chart_catalog(&store).expect("load generated chart catalog");

        let regions = catalog
            .iter()
            .map(|view| view.region_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let ids = catalog
            .iter()
            .map(|view| view.id.as_str())
            .collect::<Vec<_>>();

        println!("generated chart catalog regions: {regions:?}");
        println!("generated chart catalog ids: {ids:?}");

        assert!(
            regions.len() > 1,
            "expected multi-region chart catalog, got {regions:?}"
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn generated_nav_kv_default_map_selector_state_displays_all_tac_and_sec_regions() {
        let store = load_fixture_nav_kv_store();
        let state =
            map_selector_state(&store, None, None).expect("load generated map selector state");

        let displayed_ids = state
            .displayed_maps
            .iter()
            .map(|view| view.id.as_str())
            .collect::<Vec<_>>();
        let displayed_regions = state
            .displayed_maps
            .iter()
            .map(|view| view.region_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();

        println!("default selected map id: {}", state.selected_map_id);
        println!("default displayed map ids: {displayed_ids:?}");
        println!("default displayed regions: {displayed_regions:?}");

        assert_eq!(state.selected_map_id, "tac:nw");
        assert!(displayed_ids.contains(&"sec:sc"));
        assert!(displayed_ids.contains(&"tac:sc"));
        let sec_sc = state
            .displayed_maps
            .iter()
            .find(|view| view.id == "sec:sc")
            .expect("missing sec:sc map");
        assert!(!sec_sc.map_view.tile_url_root.starts_with("/packages/"));
        assert!(sec_sc
            .map_view
            .package_relative_path
            .as_deref()
            .is_some_and(|path| path.starts_with("sec_sc_") && path.ends_with(".zip")));
        let tac_sc = state
            .displayed_maps
            .iter()
            .find(|view| view.id == "tac:sc")
            .expect("missing tac:sc map");
        assert!(!tac_sc.map_view.tile_url_root.starts_with("/packages/"));
        assert!(state.geometry.polygon_sets.is_empty());
        assert!(
            displayed_regions.len() > 1,
            "expected multi-region displayed maps, got {displayed_regions:?}"
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn generated_nav_kv_south_central_maps_cover_kmsy_tile() {
        let store = load_fixture_nav_kv_store();
        let state =
            map_selector_state(&store, None, None).expect("load generated map selector state");

        fn lat_lon_to_tile_tms(lat: f64, lon: f64, zoom: f64) -> (i64, i64) {
            let world_size = 256.0f64;
            let max_latitude = 85.05112878f64;
            let clamped_lat = lat.max(-max_latitude).min(max_latitude);
            let world_x = ((lon + 180.0) / 360.0) * world_size;
            let world_y = ((1.0 - clamped_lat.to_radians().tan().asinh() / std::f64::consts::PI)
                / 2.0)
                * world_size;
            let scale = 2.0f64.powf(zoom);
            let x_xyz = (world_x * scale / world_size).floor() as i64;
            let y_xyz = (world_y * scale / world_size).floor() as i64;
            let y_tms = ((2.0f64.powf(zoom) as i64) - 1) - y_xyz;
            (x_xyz, y_tms)
        }

        let kmsy_lat = 29.993389f64;
        let kmsy_lon = -90.258028f64;
        for map_id in ["sec:sc", "tac:sc"] {
            let view = state
                .displayed_maps
                .iter()
                .find(|view| view.id == map_id)
                .unwrap_or_else(|| panic!("missing displayed map {map_id}"));
            let level = view
                .map_view
                .levels
                .iter()
                .find(|level| level.zoom == 10)
                .unwrap_or_else(|| {
                    panic!(
                        "missing zoom-10 level for {map_id}: {:?}",
                        view.map_view.levels
                    )
                });
            let (x, y_tms) = lat_lon_to_tile_tms(kmsy_lat, kmsy_lon, level.zoom as f64);
            println!(
                "{map_id} zoom {} tile x={} y_tms={} boxes {:?}",
                level.zoom, x, y_tms, level.boxes
            );
            assert!(
                level.boxes.iter().any(|bbox| {
                    x >= bbox.x_min
                        && x <= bbox.x_max
                        && y_tms >= bbox.y_tms_min
                        && y_tms <= bbox.y_tms_max
                }),
                "{map_id} does not cover KMSY at zoom {}",
                level.zoom
            );
        }
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn generated_nav_kv_materialize_procedure_kpae_vor_a_from_ecepo_succeeds() {
        let store = load_fixture_nav_kv_store();

        let materialized = materialize_procedure(
            &store,
            "KPAE",
            "VOR-A",
            ProcedureKind::Approach,
            None,
            Some("ECEPO"),
            2,
        )
        .expect("materialize KPAE VOR-A ECEPO through generated nav_kv");

        assert_eq!(materialized.procedure.procedure_id, "VOR-A");
        assert_eq!(
            materialized.procedure.enroute_transition.as_deref(),
            Some("ECEPO")
        );
        assert!(!materialized.resolved_legs.is_empty());
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn generated_nav_kv_flagged_procedure_quality_reaches_session_caution() {
        let store = load_fixture_nav_kv_store();
        // This is a plumbing test: it proves that a data-quality annotation produced by
        // procedure geometry generation survives materialization and reaches the core
        // data-status/caution UI when the procedure is displayed in the flight plan.
        // KGRK VOR-A DARTE is only a currently flagged exemplar. If that procedure is
        // corrected later, update this test to another known flagged procedure rather
        // than preserving this procedure's warning.
        let airport_id = "KGRK";
        let procedure_id = "VOR-A";
        let transition = "DARTE";
        let expected_message = "Procedure encoding requires a PI/course reversal from an excessive inbound turn; borrowed a later same-fix hold to define protected-side reversal geometry.";

        let materialized = materialize_procedure(
            &store,
            airport_id,
            procedure_id,
            ProcedureKind::Approach,
            None,
            Some(transition),
            0,
        )
        .expect("materialize currently flagged procedure");
        assert!(
            materialized
                .procedure
                .data_quality
                .iter()
                .any(|message| message == expected_message),
            "the chosen exemplar is no longer flagged with the expected message; choose another known flagged procedure if the plumbing is still required"
        );

        let plan = FlightPlan {
            id: "procedure-quality-current-navdb".to_string(),
            name: "KGRK".to_string(),
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Airport(airport_id.to_string()),
            }],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: Some(AirportId(airport_id.to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let init =
            crate::create_ui_session(plan, &[airport_id.to_string()], Some(airport_id), None)
                .expect("create ui session");
        crate::attach_nav_kv_store_to_session(init.handle, 1, &store).expect("attach nav kv");
        let row_uid = init
            .snapshot
            .app_ui_state
            .active_plan
            .as_ref()
            .expect("active plan")
            .display_rows
            .iter()
            .find(|row| row.label == airport_id)
            .expect("airport row")
            .uid
            .clone();

        let outcome = crate::session::select_procedure_at_flight_plan_row_in_session(
            init.handle,
            row_uid,
            airport_id.to_string(),
            procedure_id.to_string(),
            ProcedureKind::Approach,
            None,
            Some(transition.to_string()),
        )
        .expect("load flagged procedure into session");

        let HadOperationOutcome::Complete { .. } = outcome else {
            panic!("expected complete outcome, got missing resources: {outcome:?}");
        };
        let outcome = crate::get_session_snapshot(init.handle).expect("session snapshot");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("session snapshot unexpectedly needed resources: {outcome:?}");
        };
        let snapshot: crate::session::UiSessionSnapshot =
            serde_json::from_value(result).expect("decode session snapshot");
        let warning = snapshot
            .data_status_state
            .boxes
            .iter()
            .find(|box_| box_.id.starts_with("procedure_geometry:"))
            .expect("procedure geometry caution");
        assert_eq!(warning.label, "PROC");
        assert_eq!(warning.value.as_deref(), Some(procedure_id));
        assert!(warning.drives_caution);
        assert!(
            warning.detail.contains(airport_id)
                && warning.detail.contains(procedure_id)
                && warning.detail.contains(transition)
                && warning.detail.contains(expected_message),
            "procedure caution detail should describe the surfaced geometry warning: {warning:?}"
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn generated_nav_kv_lists_khvr_approaches_from_geometry_keys() {
        let store = load_fixture_nav_kv_store();

        let outcome = run_had_operation(
            &store,
            HadOperation::ListProcedures {
                airport_id: "KHVR".to_string(),
                procedure_kind: ProcedureKind::Approach,
            },
        )
        .expect("list KHVR approaches through generated nav_kv");

        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete outcome, got missing resources: {outcome:?}");
        };
        let procedures = serde_json::from_value::<Vec<ProcedureSummary>>(result)
            .expect("decode KHVR approach summaries");
        let procedure_ids = procedures
            .iter()
            .map(|procedure| procedure.procedure_id.as_str())
            .collect::<Vec<_>>();
        assert!(
            procedure_ids.contains(&"R08"),
            "KHVR geometry procedures should include R08: {procedure_ids:?}"
        );
        assert!(
            procedure_ids.contains(&"R26"),
            "KHVR geometry procedures should include R26: {procedure_ids:?}"
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn generated_nav_kv_labels_ksea_h34lz_from_plate_match() {
        let store = load_fixture_nav_kv_store();

        let outcome = run_had_operation(
            &store,
            HadOperation::ListProcedures {
                airport_id: "KSEA".to_string(),
                procedure_kind: ProcedureKind::Approach,
            },
        )
        .expect("list KSEA approaches through generated nav_kv");

        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete outcome, got missing resources: {outcome:?}");
        };
        let procedures = serde_json::from_value::<Vec<ProcedureSummary>>(result)
            .expect("decode KSEA approach summaries");
        let h34lz = procedures
            .iter()
            .find(|procedure| procedure.procedure_id == "H34LZ")
            .unwrap_or_else(|| panic!("KSEA approaches should include H34LZ: {procedures:?}"));

        assert_eq!(h34lz.display_label, "RNAV (RNP) Z 34L");
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn generated_nav_kv_materializes_ksea_i34r_jipox_with_base_label() {
        let store = load_fixture_nav_kv_store();

        let options = describe_procedure_options(&store, "KSEA", "I34R", ProcedureKind::Approach)
            .expect("describe KSEA I34R choices");
        assert!(
            options.valid_choices.contains(&crate::ProcedureSpecChoice {
                runway_transition: None,
                enroute_transition: Some("JIPOX".to_string()),
                label: "from JIPOX".to_string(),
            }),
            "KSEA I34R should offer JIPOX: {:?}",
            options.valid_choices
        );

        let materialized = materialize_procedure(
            &store,
            "KSEA",
            "I34R",
            ProcedureKind::Approach,
            None,
            Some("JIPOX"),
            1,
        )
        .expect("materialize KSEA I34R JIPOX");
        assert_eq!(
            materialized.procedure.display_label.as_deref(),
            Some("ILS or LOC 34R")
        );
        assert_eq!(
            materialized.procedure.enroute_transition.as_deref(),
            Some("JIPOX")
        );
    }

    #[test]
    fn list_approaches_uses_plate_label_from_cifp_match() {
        let (root, pages) = fixture(
            &[
                (
                    "plate/cifp/KSEA/H34LZ",
                    br#"[{
                        "airport_id": "KSEA",
                        "cifp_id": "H34LZ",
                        "procedure_kind": "approach",
                        "plate_id": "Plate:KSEA:IAP-RNAV-RNP-Z-34L.png",
                        "plate_label": "RNAV (RNP) Z 34L",
                        "package_id": "tpp-nw",
                        "public": 1,
                        "priority": 0,
                        "match_kind": "unique",
                        "is_primary": 1
                    }]"#
                    .as_slice(),
                ),
                (
                    "procedure/geometry/KSEA/APPROACH/H34LZ/_/_",
                    br#"{}"#.as_slice(),
                ),
            ],
            4096,
        );
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        let outcome = run_had_operation(
            &store,
            HadOperation::ListProcedures {
                airport_id: "KSEA".to_string(),
                procedure_kind: ProcedureKind::Approach,
            },
        )
        .expect("list KSEA approaches through nav_kv");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete outcome, got missing resources: {outcome:?}");
        };
        let procedures =
            serde_json::from_value::<Vec<ProcedureSummary>>(result).expect("decode procedures");

        assert_eq!(procedures.len(), 1);
        assert_eq!(procedures[0].procedure_id, "H34LZ");
        assert_eq!(procedures[0].display_label, "RNAV (RNP) Z 34L");
        assert_eq!(procedures[0].accent_category, "approach");
    }

    #[test]
    fn sid_match_drives_pilot_label_and_plate_departure_menu() {
        let plate_id = "plate:KSEA:DP-WA-BANGR NINE (RNAV).png";
        let match_rows = serde_json::json!([{
            "airport_id": "KSEA",
            "cifp_id": "BANGR9",
            "procedure_kind": "sid",
            "plate_id": plate_id,
            "plate_label": "BANGR NINE (RNAV)",
            "package_id": "tpp-nw",
            "public": 1,
            "priority": 0,
            "match_kind": "unique",
            "is_primary": 1
        }]);
        let store = test_nav_kv_store(&[
            ("plate/cifp/KSEA/BANGR9", match_rows.clone()),
            (
                "plate/procedure-candidates/plate%3AKSEA%3ADP-WA-BANGR%20NINE%20%28RNAV%29.png",
                match_rows,
            ),
            (
                "procedure/geometry/KSEA/SID/BANGR9/RW16L/BANGR",
                serde_json::json!({}),
            ),
        ]);

        let outcome = run_had_operation(
            &store,
            HadOperation::ListProcedures {
                airport_id: "KSEA".to_string(),
                procedure_kind: ProcedureKind::Sid,
            },
        )
        .expect("list KSEA departures");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete departure list");
        };
        let procedures =
            serde_json::from_value::<Vec<ProcedureSummary>>(result).expect("decode departures");
        assert_eq!(procedures[0].procedure_id, "BANGR9");
        assert_eq!(procedures[0].display_label, "BANGR NINE (RNAV)");
        assert_eq!(procedures[0].accent_category, "departure");

        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KSEA".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
            ],
            ..FlightPlan::default()
        };
        let menu = describe_plate_loads(&store, &plan, plate_id).expect("describe SID plate");
        assert_eq!(menu.procedure_kind, Some(ProcedureKind::Sid));
        assert_eq!(menu.launcher_label, "LOAD\nDEP");
        assert_eq!(menu.options[0].label, "via RW16L to BANGR");
    }

    #[test]
    fn star_match_drives_pilot_label_and_plate_arrival_menu() {
        let plate_id = "plate:KRNT:STAR-WA-GLASR THREE.png";
        let match_rows = serde_json::json!([{
            "airport_id": "KRNT",
            "cifp_id": "GLASR3",
            "procedure_kind": "star",
            "plate_id": plate_id,
            "plate_label": "GLASR THREE",
            "package_id": "tpp-nw",
            "public": 1,
            "priority": 0,
            "match_kind": "unique",
            "is_primary": 1
        }]);
        let store = test_nav_kv_store(&[
            ("plate/cifp/KRNT/GLASR3", match_rows.clone()),
            (
                "plate/procedure-candidates/plate%3AKRNT%3ASTAR-WA-GLASR%20THREE.png",
                match_rows,
            ),
            (
                "procedure/geometry/KRNT/STAR/GLASR3/RW16/GLASR",
                serde_json::json!({}),
            ),
        ]);

        let outcome = run_had_operation(
            &store,
            HadOperation::ListProcedures {
                airport_id: "KRNT".to_string(),
                procedure_kind: ProcedureKind::Star,
            },
        )
        .expect("list KRNT arrivals");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete arrival list");
        };
        let procedures =
            serde_json::from_value::<Vec<ProcedureSummary>>(result).expect("decode arrivals");
        assert_eq!(procedures[0].procedure_id, "GLASR3");
        assert_eq!(procedures[0].display_label, "GLASR THREE");
        assert_eq!(procedures[0].accent_category, "star");
        assert!(procedures[0].enabled);
        assert_eq!(procedures[0].disabled_reason, None);

        let plan = FlightPlan {
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KRNT".to_string()),
                },
            ],
            ..FlightPlan::default()
        };
        let menu = describe_plate_loads(&store, &plan, plate_id).expect("describe STAR plate");
        assert_eq!(menu.procedure_kind, Some(ProcedureKind::Star));
        assert_eq!(menu.launcher_label, "LOAD\nARR");
        assert_eq!(menu.options[0].label, "from GLASR to RW16");
    }

    #[test]
    fn charted_procedure_without_geometry_is_listed_but_disabled() {
        let airport = serde_json::json!({
            "id": "KSEA",
            "label": "Seattle-Tacoma International",
            "package_ids": ["tpp-nw"],
            "chart_ids": ["plate:KSEA:STR-WA-GLASR THREE.png"],
            "charted_procedures": [{
                "procedure_id": "GLASR3",
                "display_label": "GLASR THREE",
                "kind": "star",
                "plate_id": "plate:KSEA:STR-WA-GLASR THREE.png"
            }]
        });
        let store = test_nav_kv_store(&[("plate/airport/KSEA", airport)]);

        let outcome = run_had_operation(
            &store,
            HadOperation::ListProcedures {
                airport_id: "KSEA".to_string(),
                procedure_kind: ProcedureKind::Star,
            },
        )
        .expect("list KSEA arrivals");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete arrival list");
        };
        let procedures =
            serde_json::from_value::<Vec<ProcedureSummary>>(result).expect("decode arrivals");

        assert_eq!(procedures.len(), 1);
        assert_eq!(procedures[0].procedure_id, "GLASR3");
        assert_eq!(procedures[0].display_label, "GLASR THREE");
        assert!(!procedures[0].enabled);
        assert_eq!(
            procedures[0].disabled_reason.as_deref(),
            Some("No geometry; use Plates page.")
        );
    }

    #[test]
    fn list_approaches_disambiguates_procedures_sharing_a_plate_label() {
        let (root, pages) = fixture(
            &[
                (
                    "plate/cifp/KAMA/I04",
                    br#"[{
                        "airport_id": "KAMA",
                        "cifp_id": "I04",
                        "procedure_kind": "approach",
                        "plate_id": "plate:KAMA:IAP-TX-ILS OR LOC RWY 04.png",
                        "plate_label": "ILS or LOC 04",
                        "package_id": "tpp-sc",
                        "public": 1,
                        "priority": 0,
                        "match_kind": "unique",
                        "is_primary": 1
                    }]"#
                    .as_slice(),
                ),
                (
                    "plate/cifp/KAMA/L04",
                    br#"[{
                        "airport_id": "KAMA",
                        "cifp_id": "L04",
                        "procedure_kind": "approach",
                        "plate_id": "plate:KAMA:IAP-TX-ILS OR LOC RWY 04.png",
                        "plate_label": "ILS or LOC 04",
                        "package_id": "tpp-sc",
                        "public": 1,
                        "priority": 0,
                        "match_kind": "unique",
                        "is_primary": 1
                    }]"#
                    .as_slice(),
                ),
                (
                    "procedure/geometry/KAMA/APPROACH/I04/_/_",
                    br#"{}"#.as_slice(),
                ),
                (
                    "procedure/geometry/KAMA/APPROACH/L04/_/_",
                    br#"{}"#.as_slice(),
                ),
            ],
            4096,
        );
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        let outcome = run_had_operation(
            &store,
            HadOperation::ListProcedures {
                airport_id: "KAMA".to_string(),
                procedure_kind: ProcedureKind::Approach,
            },
        )
        .expect("list KAMA approaches through nav_kv");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete outcome, got missing resources: {outcome:?}");
        };
        let procedures =
            serde_json::from_value::<Vec<ProcedureSummary>>(result).expect("decode procedures");
        let labels = procedures
            .iter()
            .map(|procedure| procedure.display_label.as_str())
            .collect::<Vec<_>>();

        assert_eq!(labels, vec!["ILS 04", "LOC 04"]);
    }

    #[test]
    fn list_approaches_requires_plate_label() {
        let (root, pages) = fixture(
            &[(
                "procedure/geometry/KSEA/APPROACH/H34LZ/_/_",
                br#"{}"#.as_slice(),
            )],
            4096,
        );
        let mut store = NavKvStore::new(root);
        for (index, page) in pages.into_iter().enumerate() {
            store.insert_page(index as u32, page);
        }

        let err = run_had_operation(
            &store,
            HadOperation::ListProcedures {
                airport_id: "KSEA".to_string(),
                procedure_kind: ProcedureKind::Approach,
            },
        )
        .expect_err("approach list should require plate/cifp label data");
        assert!(
            err.message
                .contains("approach KSEA H34LZ has no preferred plate label"),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn fixture_nav_kv_resolves_waypoint_identifier() {
        let store = load_fixture_nav_kv_store();
        let outcome = run_had_operation(
            &store,
            HadOperation::ResolveWaypointIdentifier {
                identifier: "OLM".to_string(),
            },
        )
        .expect("resolve OLM through fixture nav_kv");
        match outcome {
            HadOperationOutcome::Complete { result, .. } => {
                let nav_ref =
                    serde_json::from_value::<Option<NavRef>>(result).expect("decode nav ref");
                assert_eq!(nav_ref, Some(NavRef::Navaid("OLM".to_string())));
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
            HadOperationOutcome::NeedSnapshotResources { .. } => {
                panic!("generic HAD operation requested a session snapshot continuation")
            }
        }
    }

    #[test]
    fn fixture_nav_kv_suggests_waypoint_identifiers_near_view_center() {
        let kr_bucket = vec![
            WaypointSearchRecord {
                identifier: "KRNT".to_string(),
                kind: "airport".to_string(),
                display_name: "Renton Municipal\nRenton, WA".to_string(),
                lat: 47.493,
                lon: -122.216,
                matched_term: "KRNT".to_string(),
                match_kind: WaypointSearchMatchKind::Identifier,
            },
            WaypointSearchRecord {
                identifier: "KRDD".to_string(),
                kind: "airport".to_string(),
                display_name: "Redding Regional\nRedding, CA".to_string(),
                lat: 40.509,
                lon: -122.293,
                matched_term: "KRDD".to_string(),
                match_kind: WaypointSearchMatchKind::Identifier,
            },
        ];
        let store = test_nav_kv_store(&[(
            "waypoint/search-prefix/KR",
            serde_json::to_value(kr_bucket).expect("encode waypoint search fixture"),
        )]);
        let outcome = run_had_operation(
            &store,
            HadOperation::SuggestWaypointIdentifiersNear {
                anchor: LatLon {
                    lat: 47.493,
                    lon: -122.216,
                },
                query: "KR".to_string(),
                limit: 5,
            },
        )
        .expect("suggest waypoint identifiers near view center");
        match outcome {
            HadOperationOutcome::Complete { result, .. } => {
                let suggestions =
                    serde_json::from_value::<Vec<WaypointIdentifierSuggestion>>(result)
                        .expect("decode waypoint suggestions");
                assert!(!suggestions.is_empty());
                assert!(suggestions.len() <= 5);
                assert!(suggestions
                    .iter()
                    .all(|suggestion| suggestion.identifier.starts_with("KR")));
                assert!(suggestions.windows(2).all(|pair| {
                    pair[0].distance_from_anchor_nm <= pair[1].distance_from_anchor_nm
                }));
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
            HadOperationOutcome::NeedSnapshotResources { .. } => {
                panic!("generic HAD operation requested a session snapshot continuation")
            }
        }
    }

    #[test]
    fn missing_waypoint_search_suggestions_are_empty() {
        let store = test_nav_kv_store(&[]);
        let suggestions =
            suggest_waypoint_identifier_candidates(&store, "K", 5, LatLon { lat: 0.0, lon: 0.0 })
                .expect("missing search term should not be fatal");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn decimal_coordinates_resolve_and_suggest_spot_without_navdb() {
        let store = test_nav_kv_store(&[]);
        let expected = LatLon {
            lat: 47.3,
            lon: -122.9,
        };

        assert_eq!(
            resolve_waypoint_identifier_for_ui(&store, " 47.3, -122.9 ")
                .expect("resolve decimal coordinates"),
            Some(NavRef::Spot(expected))
        );

        let suggestions = suggest_waypoint_identifier_candidates(
            &store,
            "47.3,-122.9",
            8,
            LatLon {
                lat: 47.0,
                lon: -123.0,
            },
        )
        .expect("suggest decimal coordinates");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].identifier, "SPOT");
        assert_eq!(
            suggestions[0].nav_ref,
            app_ui_contracts::nav_query::WaypointSuggestionNavRef::Spot {
                position: app_ui_contracts::nav_query::WaypointSuggestionPosition {
                    lat: expected.lat,
                    lon: expected.lon,
                },
            }
        );
        assert_eq!(suggestions[0].kind, "spot");
        assert_eq!(suggestions[0].display_name, "47.3000,-122.9000");
        assert!(suggestions[0].symbol_feature.is_none());
    }

    #[test]
    fn decimal_coordinate_parser_rejects_invalid_or_out_of_range_values() {
        for value in [
            "",
            "47.3",
            "47.3,-122.9,1",
            "north,west",
            "90.1,0",
            "-90.1,0",
            "0,180.1",
            "0,-180.1",
            "NaN,0",
            "0,inf",
        ] {
            assert_eq!(parse_spot_coordinates(value), None, "{value}");
        }
    }

    #[test]
    fn append_route_entry_accepts_decimal_coordinate_spot() {
        let store = test_nav_kv_store(&[]);
        let plan = FlightPlan::empty();
        let input = "47.3,-122.9";

        let preview = preview_flight_plan_entry(&store, &plan, input).expect("preview SPOT");
        assert!(preview.can_commit);
        assert_eq!(preview.tokens.len(), 1);
        assert_eq!(
            preview.tokens[0].state,
            FlightPlanEntryTokenState::Recognized
        );

        let mutation = append_flight_plan_entry(&store, &plan, input).expect("append SPOT");
        assert_eq!(
            mutation.route_components,
            vec![RouteComponent::Waypoint {
                waypoint: NavRef::Spot(LatLon {
                    lat: 47.3,
                    lon: -122.9,
                }),
            }]
        );
    }

    #[test]
    fn waypoint_search_suggestions_fall_back_to_shortest_emitted_bucket() {
        let kr_bucket = vec![
            WaypointSearchRecord {
                identifier: "KRNT".to_string(),
                kind: "airport".to_string(),
                display_name: "Renton Municipal\nRenton, WA".to_string(),
                lat: 47.493,
                lon: -122.216,
                matched_term: "KRNT".to_string(),
                match_kind: WaypointSearchMatchKind::Identifier,
            },
            WaypointSearchRecord {
                identifier: "KRDD".to_string(),
                kind: "airport".to_string(),
                display_name: "Redding Regional\nRedding, CA".to_string(),
                lat: 40.509,
                lon: -122.293,
                matched_term: "KRDD".to_string(),
                match_kind: WaypointSearchMatchKind::Identifier,
            },
        ];
        let store = test_nav_kv_store(&[(
            "waypoint/search-prefix/KR",
            serde_json::to_value(kr_bucket).expect("encode waypoint search bucket"),
        )]);
        let suggestions = suggest_waypoint_identifier_candidates(
            &store,
            "KRNT",
            5,
            LatLon {
                lat: 47.493,
                lon: -122.216,
            },
        )
        .expect("suggest from ancestor prefix bucket");
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].identifier, "KRNT");
    }

    #[test]
    fn waypoint_search_ignores_broad_terms_and_uses_discriminating_terms() {
        let die_bucket = vec![WaypointSearchRecord {
            identifier: "KSAN".to_string(),
            kind: "airport".to_string(),
            display_name: "San Diego Intl\nSan Diego, CA".to_string(),
            lat: 32.7336,
            lon: -117.1897,
            matched_term: "DIEGO".to_string(),
            match_kind: WaypointSearchMatchKind::AirportName,
        }];
        let store = test_nav_kv_store(&[(
            "waypoint/search-prefix/DIE",
            serde_json::to_value(die_bucket).expect("encode DIE search bucket"),
        )]);

        let suggestions = suggest_waypoint_identifier_candidates(
            &store,
            "SAN DIEG",
            5,
            LatLon {
                lat: 32.7336,
                lon: -117.1897,
            },
        )
        .expect("search by broad and discriminating terms");

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].identifier, "KSAN");
    }

    #[test]
    fn waypoint_search_intersects_name_terms() {
        let kpae_paine = WaypointSearchRecord {
            identifier: "KPAE".to_string(),
            kind: "airport".to_string(),
            display_name: "Seattle Paine Fld Intl\nEverett, WA".to_string(),
            lat: 47.904,
            lon: -122.281,
            matched_term: "PAINE".to_string(),
            match_kind: WaypointSearchMatchKind::AirportName,
        };
        let other_paine = WaypointSearchRecord {
            identifier: "PAIN".to_string(),
            kind: "fix".to_string(),
            display_name: "PAIN".to_string(),
            lat: 40.0,
            lon: -120.0,
            matched_term: "PAIN".to_string(),
            match_kind: WaypointSearchMatchKind::Identifier,
        };
        let kpae_everett = WaypointSearchRecord {
            matched_term: "EVERETT".to_string(),
            ..kpae_paine.clone()
        };
        let store = test_nav_kv_store(&[
            (
                "waypoint/search-prefix/PAI",
                serde_json::to_value(vec![kpae_paine, other_paine])
                    .expect("encode PAI search bucket"),
            ),
            (
                "waypoint/search-prefix/EVE",
                serde_json::to_value(vec![kpae_everett]).expect("encode EVE search bucket"),
            ),
        ]);

        let suggestions = suggest_waypoint_identifier_candidates(
            &store,
            "PAINE EVERETT",
            5,
            LatLon {
                lat: 47.904,
                lon: -122.281,
            },
        )
        .expect("intersect airport name terms");

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].identifier, "KPAE");
    }

    #[test]
    fn waypoint_search_ranks_identifier_matches_before_name_matches() {
        let bucket = vec![
            WaypointSearchRecord {
                identifier: "KPAE".to_string(),
                kind: "airport".to_string(),
                display_name: "Seattle Paine Fld Intl\nEverett, WA".to_string(),
                lat: 47.904,
                lon: -122.281,
                matched_term: "PAINE".to_string(),
                match_kind: WaypointSearchMatchKind::AirportName,
            },
            WaypointSearchRecord {
                identifier: "PAICE".to_string(),
                kind: "fix".to_string(),
                display_name: "PAICE".to_string(),
                lat: 31.1485,
                lon: -94.7165,
                matched_term: "PAICE".to_string(),
                match_kind: WaypointSearchMatchKind::Identifier,
            },
        ];
        let store = test_nav_kv_store(&[(
            "waypoint/search-prefix/PAI",
            serde_json::to_value(bucket).expect("encode PAI search bucket"),
        )]);

        let suggestions = suggest_waypoint_identifier_candidates(
            &store,
            "PAI",
            5,
            LatLon {
                lat: 47.904,
                lon: -122.281,
            },
        )
        .expect("rank mixed search matches");

        assert_eq!(
            suggestions
                .iter()
                .map(|suggestion| suggestion.identifier.as_str())
                .collect::<Vec<_>>(),
            vec!["PAICE", "KPAE"]
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn fixture_nav_kv_suggests_airways_near_krnt() {
        let store = load_fixture_nav_kv_store();
        let outcome = run_had_operation(
            &store,
            HadOperation::SuggestAirwaysNearAnchor {
                anchor: NavRef::Airport("KRNT".to_string()),
                limit: 5,
            },
        )
        .expect("suggest nearby airways through fixture nav_kv");
        match outcome {
            HadOperationOutcome::Complete { result, .. } => {
                let suggestions = serde_json::from_value::<Vec<AirwaySuggestion>>(result)
                    .expect("decode airway suggestions");
                assert!(!suggestions.is_empty());
                assert!(suggestions.windows(2).all(|pair| {
                    pair[0].distance_from_anchor_nm <= pair[1].distance_from_anchor_nm
                }));
                assert!(suggestions.iter().all(|suggestion| {
                    suggestion.distance_from_anchor_nm.is_finite()
                        && !suggestion.airway_name.trim().is_empty()
                }));
            }
            HadOperationOutcome::NeedResources { resources } => {
                panic!("expected complete outcome, got missing resources: {resources:?}");
            }
            HadOperationOutcome::NeedSnapshotResources { .. } => {
                panic!("generic HAD operation requested a session snapshot continuation")
            }
        }
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn fixture_nav_kv_prepares_and_materializes_v2_between_krnt_and_kuao() {
        let store = load_fixture_nav_kv_store();
        let presentation = prepare_airway_presentation_for_anchors(
            &store,
            "V2",
            &NavRef::Airport("KRNT".to_string()),
            Some(&NavRef::Airport("KUAO".to_string())),
        )
        .expect("prepare airway presentation through fixture nav_kv");

        let entry_point_uid = presentation
            .points
            .iter()
            .find(|point| point.nav_ref == NavRef::Navaid("SEA".to_string()))
            .expect("presentation should include SEA")
            .uid
            .clone();
        let exit_point_uid = presentation
            .points
            .iter()
            .find(|point| point.nav_ref == NavRef::Fix("VAMPS".to_string()))
            .expect("presentation should include VAMPS")
            .uid
            .clone();

        let materialized = materialize_airway_presentation_selection(
            &store,
            0,
            AirwayPresentationSelection {
                airway_name: presentation.airway_name,
                branch_key: presentation.branch_key,
                entry_point_uid,
                exit_point_uid,
            },
        )
        .expect("materialize airway presentation selection through fixture nav_kv");

        assert_eq!(materialized.airway.name, "V2");
        assert_eq!(materialized.airway.entry, NavRef::Navaid("SEA".to_string()));
        assert_eq!(materialized.airway.exit, NavRef::Fix("VAMPS".to_string()));
        assert!(!materialized.resolved_legs.is_empty());
        assert_eq!(
            materialized.resolved_legs.first().unwrap().from,
            materialized.airway.entry
        );
        assert_eq!(
            materialized.resolved_legs.last().unwrap().to,
            materialized.airway.exit
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn fixture_nav_kv_previews_append_entry_with_relationship_issue() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "krnt".to_string(),
            name: "KRNT".to_string(),
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KRNT".to_string()),
            }],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KRNT".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let preview = preview_flight_plan_entry(&store, &plan, "BTG V112 VAMPS")
            .expect("preview route entry");

        assert!(!preview.can_commit);
        assert_eq!(preview.tokens.len(), 3);
        assert_eq!(
            preview.tokens[0].state,
            FlightPlanEntryTokenState::Recognized
        );
        assert_eq!(
            preview.tokens[1].state,
            FlightPlanEntryTokenState::Recognized
        );
        assert_eq!(
            preview.tokens[2].state,
            FlightPlanEntryTokenState::Recognized
        );
        assert_eq!(preview.issues.len(), 1);
        assert!(preview.issues[0].message.contains("V112"));
        assert_eq!(preview.issues[0].start, 4);
        assert_eq!(preview.issues[0].end, 14);
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn fixture_nav_kv_appends_waypoint_airway_waypoint_sequence() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "krnt".to_string(),
            name: "KRNT".to_string(),
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Airport("KRNT".to_string()),
            }],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KRNT".to_string())),
            destination: Some(AirportId("KRNT".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation = append_flight_plan_entry(&store, &plan, "SEA V2 VAMPS KUAO")
            .expect("append route entry");

        assert_eq!(mutation.route_components.len(), 3);
        assert!(matches!(
            mutation.route_components[0],
            RouteComponent::Waypoint { .. }
        ));
        assert!(matches!(
            mutation.route_components[1],
            RouteComponent::Airway { ref airway } if airway.name == "V2"
        ));
        assert!(matches!(
            mutation.route_components[2],
            RouteComponent::Waypoint { waypoint: NavRef::Airport(ref id) } if id == "KUAO"
        ));
        assert!(!mutation.route_components.iter().any(|component| {
            matches!(component, RouteComponent::Waypoint { waypoint: NavRef::Fix(id) } if id == "VAMPS")
        }));
    }

    #[test]
    fn exact_airway_materialization_reports_missing_entry_waypoint() {
        let branches = vec![AirwayBranch {
            display_name: "V495".to_string(),
            branch_key: String::new(),
            points: vec![
                crate::navdb_types::AirwayFixPoint {
                    airway_name: "V495".to_string(),
                    sequence: 10,
                    position: LatLon { lat: 0.0, lon: 0.0 },
                    nav_ref: NavRef::Navaid("SEA".to_string()),
                },
                crate::navdb_types::AirwayFixPoint {
                    airway_name: "V495".to_string(),
                    sequence: 20,
                    position: LatLon { lat: 1.0, lon: 1.0 },
                    nav_ref: NavRef::Fix("NEXT".to_string()),
                },
            ],
        }];

        let err = exact_airway_materialization(
            "V495",
            &branches,
            &NavRef::Navaid("PAE".to_string()),
            &NavRef::Navaid("SEA".to_string()),
            0,
        )
        .expect_err("PAE is not on V495");

        assert!(matches!(err, HadReadError::Fatal(message) if message == "PAE not on V495"));
    }

    #[test]
    fn route_entry_validation_reports_missing_airway_entry_before_missing_exit() {
        let tokens = vec![
            EvaluatedInputToken {
                parsed: ParsedInputToken {
                    text: "PAE".to_string(),
                    start: 5,
                    end: 8,
                    terminated: true,
                },
                token_state: FlightPlanEntryTokenState::Recognized,
                recognized: Some(RecognizedInputToken::Waypoint(NavRef::Navaid(
                    "PAE".to_string(),
                ))),
            },
            EvaluatedInputToken {
                parsed: ParsedInputToken {
                    text: "V495".to_string(),
                    start: 9,
                    end: 13,
                    terminated: true,
                },
                token_state: FlightPlanEntryTokenState::Recognized,
                recognized: Some(RecognizedInputToken::Airway {
                    airway_name: "V495".to_string(),
                    branches: vec![AirwayBranch {
                        display_name: "V495".to_string(),
                        branch_key: String::new(),
                        points: vec![crate::navdb_types::AirwayFixPoint {
                            airway_name: "V495".to_string(),
                            sequence: 10,
                            position: LatLon { lat: 0.0, lon: 0.0 },
                            nav_ref: NavRef::Navaid("SEA".to_string()),
                        }],
                    }],
                }),
            },
        ];

        let plan = FlightPlan {
            id: "route".to_string(),
            name: "Route".to_string(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let issues = validate_flight_plan_entry(&plan, &tokens).expect("validate route");

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].start, 5);
        assert_eq!(issues[0].end, 13);
        assert_eq!(issues[0].message, "PAE not on V495");
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn route_entry_appends_chained_airways_with_shared_waypoint() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "route".to_string(),
            name: "Route".to_string(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let input = "KPAE PAE V23 SEA V495 VAUGN EUG";
        let preview = preview_flight_plan_entry(&store, &plan, input).expect("preview");
        assert!(preview.can_commit, "{preview:#?}");
        let mutation = append_flight_plan_entry(&store, &plan, input).expect("append route");

        assert!(mutation.route_components.iter().any(|component| {
            matches!(component, RouteComponent::Airway { airway } if airway.name == "V23")
        }));
        assert!(mutation.route_components.iter().any(|component| {
            matches!(component, RouteComponent::Airway { airway } if airway.name == "V495")
        }));
        assert!(!mutation.route_components.iter().any(|component| {
            matches!(component, RouteComponent::Waypoint { waypoint: NavRef::Navaid(id) } if id == "SEA")
        }));
        assert!(!mutation.route_components.iter().any(|component| {
            matches!(component, RouteComponent::Waypoint { waypoint: NavRef::Fix(id) } if id == "VAUGN")
        }));
        let ui_state = default_flight_plan_ui_state_for_test(&store, &mutation);
        let airway_child_labels = ui_state
            .display_rows
            .iter()
            .filter(|row| {
                row.component_kind == Some(crate::RouteComponentViewKind::Airway) && row.depth > 0
            })
            .map(|row| row.label.as_str())
            .collect::<Vec<_>>();
        assert!(
            airway_child_labels.starts_with(&["PAE", "SEA"]),
            "{airway_child_labels:?}"
        );
        assert!(
            airway_child_labels.ends_with(&["VAUGN"]),
            "{airway_child_labels:?}"
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn route_entry_materializes_airway_exit_before_final_airport() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "route".to_string(),
            name: "Route".to_string(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let input = "KPAE PAE V23 SEA V495 VAUGN KEUG";
        let preview = preview_flight_plan_entry(&store, &plan, input).expect("preview");
        assert!(preview.can_commit, "{preview:#?}");
        let mutation = append_flight_plan_entry(&store, &plan, input).expect("append route");

        let route = &mutation.route_components;
        assert!(route.windows(2).any(|window| {
            matches!(
                (&window[0], &window[1]),
                (
                    RouteComponent::Airway {
                        airway: AirwaySegment { exit, .. }
                    },
                    RouteComponent::Waypoint {
                        waypoint: NavRef::Airport(airport)
                    },
                ) if exit == &NavRef::Fix("VAUGN".to_string()) && airport == "KEUG"
            )
        }));
        assert!(!route.iter().any(|component| {
            matches!(component, RouteComponent::Waypoint { waypoint: NavRef::Fix(id) } if id == "VAUGN")
        }));

        let ui_state = default_flight_plan_ui_state_for_test(&store, &mutation);
        let keug_row = ui_state
            .display_rows
            .iter()
            .find(|row| row.depth == 0 && row.label == "KEUG")
            .expect("KEUG row");
        assert!(
            keug_row.symbol_feature.is_some(),
            "airport row should keep its symbol"
        );
        assert!(
            crate::planning::flight_plan_row_actions(keug_row)
                .any(|action| action.id == FlightPlanRowActionId::SelectApproach && action.enabled),
            "final airport after airway exit should offer Select Approach"
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn chained_airway_handoff_row_can_activate_inbound_leg() {
        let store = load_fixture_nav_kv_store();
        let mutation = append_route_entry_for_chained_airways();
        let sea_leg_index = mutation
            .resolved_legs
            .iter()
            .position(|leg| {
                leg.from == NavRef::Navaid("PAE".to_string())
                    && leg.to == NavRef::Navaid("SEA".to_string())
            })
            .expect("PAE to SEA leg");
        let ui_state = default_flight_plan_ui_state_for_test(&store, &mutation);
        let sea_row = ui_state
            .display_rows
            .iter()
            .find(|row| {
                row.depth == 1
                    && row.component_kind == Some(crate::RouteComponentViewKind::Airway)
                    && row.label == "SEA"
            })
            .expect("visible SEA airway child row");

        assert_eq!(sea_row.leg_index, Some(sea_leg_index));
        assert!(
            crate::planning::flight_plan_row_actions(sea_row)
                .any(|action| action.id == FlightPlanRowActionId::ActivateLeg && action.enabled),
            "SEA airway child row should activate PAE -> SEA"
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn chained_airway_handoff_row_distance_uses_inbound_leg() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "route".to_string(),
            name: "Route".to_string(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        let mutation = append_flight_plan_entry(&store, &plan, "KPAE PAE V23 SEA V2 ELN KYKM")
            .expect("append chained airway route");
        let sea_leg = mutation
            .resolved_legs
            .iter()
            .find(|leg| {
                leg.from == NavRef::Navaid("PAE".to_string())
                    && leg.to == NavRef::Navaid("SEA".to_string())
            })
            .expect("PAE to SEA leg");
        let ui_state = default_flight_plan_ui_state_for_test(&store, &mutation);
        let sea_row = ui_state
            .display_rows
            .iter()
            .find(|row| {
                row.depth == 1
                    && row.component_kind == Some(crate::RouteComponentViewKind::Airway)
                    && row.label == "SEA"
            })
            .expect("visible SEA airway child row");
        let distance_cell = sea_row
            .data_cells
            .iter()
            .find(|cell| cell.id == "waypoint_distance")
            .expect("SEA distance cell");
        let from = nav_ref_position(&store, &sea_leg.from, None).expect("PAE position");
        let to = nav_ref_position(&store, &sea_leg.to, None).expect("SEA position");
        let expected_distance =
            crate::flight_data::format_nm(crate::great_circle_distance_nm(from, to));

        assert_eq!(
            distance_cell.value.as_deref(),
            Some(expected_distance.as_str())
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn activate_next_leg_from_first_leg_keeps_chained_airway_arrow_visible() {
        let mutation = append_route_entry_for_chained_airways();
        let first_leg_index = mutation
            .resolved_legs
            .iter()
            .position(|leg| {
                leg.from == NavRef::Airport("KPAE".to_string())
                    && leg.to == NavRef::Navaid("PAE".to_string())
            })
            .expect("KPAE to PAE leg");
        let activated = crate::activate_leg(&mutation, first_leg_index).expect("activate leg");
        let next = crate::activate_next_leg(&activated).expect("activate next leg");
        let ui = crate::project_ui_state(&next);
        let guidance = ui.guidance.as_ref().expect("guidance");

        assert_eq!(
            guidance.active_leg,
            Some(crate::PlanLeg {
                from: NavRef::Navaid("PAE".to_string()),
                to: NavRef::Navaid("SEA".to_string()),
                airway: None,
            })
        );
        assert!(
            guidance.active_from_row_uid.is_some(),
            "active from row should remain visible"
        );
        assert!(
            guidance.active_to_row_uid.is_some(),
            "active to row should remain visible"
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn sequence_active_leg_walks_chained_airway_route_to_last_leg() {
        let mutation = append_route_entry_for_chained_airways();
        let mut plan = crate::activate_leg(&mutation, 0).expect("activate first leg");
        let expected = mutation
            .resolved_legs
            .iter()
            .map(|leg| (leg.from.clone(), leg.to.clone()))
            .collect::<Vec<_>>();

        for (index, (from, to)) in expected.iter().enumerate() {
            let active = crate::active_guidance_leg(&plan).expect("active leg");
            assert_eq!(
                (active.from.clone(), active.to.clone()),
                (from.clone(), to.clone()),
                "active leg {index}"
            );
            let ui = crate::project_ui_state(&plan);
            let guidance = ui.guidance.as_ref().expect("guidance");
            assert!(
                guidance.active_to_row_uid.is_some(),
                "active to row should be visible for leg {index}: {from:?} -> {to:?}"
            );
            if index + 1 < expected.len() {
                plan = crate::sequence_active_leg(&plan).expect("sequence active leg");
            }
        }

        let final_active = crate::active_guidance_leg(&plan).expect("final active leg");
        let (last_from, last_to) = expected.last().expect("last leg");
        assert_eq!(
            (final_active.from, final_active.to),
            (last_from.clone(), last_to.clone())
        );
        let suspended = crate::sequence_active_leg(&plan).expect("sequence at route end");
        assert_eq!(
            suspended
                .guidance
                .as_ref()
                .expect("route-end guidance")
                .sequencing_mode,
            crate::SequencingMode::Suspended
        );
    }

    fn append_route_entry_for_chained_airways() -> FlightPlan {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "route".to_string(),
            name: "Route".to_string(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };
        append_flight_plan_entry(&store, &plan, "KPAE PAE V23 SEA V495 VAUGN KEUG")
            .expect("append chained airway route")
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn append_flight_plan_entry_allows_empty_plan_to_gain_single_waypoint() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "empty".to_string(),
            name: "Empty".to_string(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation = append_flight_plan_entry(&store, &plan, "KPAE")
            .expect("append route entry to empty plan");

        assert_eq!(mutation.route_components.len(), 1);
        assert!(mutation.resolved_legs.is_empty());
        assert!(matches!(
            mutation.route_components[0],
            RouteComponent::Waypoint { waypoint: NavRef::Airport(ref id) } if id == "KPAE"
        ));
    }

    #[test]
    fn waypoint_identifier_ui_rule_rejects_short_airport_aliases() {
        let store = test_nav_kv_store(&[(
            "navref/symbol/navaid/SEA",
            serde_json::json!({
                "kind": "vortac",
                "label": "SEA 116.80",
                "symbol_kind": "nav",
                "style_class": "nav"
            }),
        )]);

        assert!(!waypoint_identifier_is_canonical_for_ui(
            "RNT",
            &NavRef::Airport("KRNT".to_string())
        ));
        assert!(waypoint_identifier_is_canonical_for_ui(
            "KRNT",
            &NavRef::Airport("KRNT".to_string())
        ));
        assert!(waypoint_identifier_is_canonical_for_ui(
            "SEA",
            &NavRef::Navaid("SEA".to_string())
        ));
        assert!(waypoint_identifier_nav_ref_is_acceptable_for_ui(
            &store,
            &NavRef::Navaid("SEA".to_string())
        )
        .expect("VOR-family symbol is acceptable"));
        assert!(!waypoint_identifier_nav_ref_is_acceptable_for_ui(
            &store,
            &NavRef::Navaid("RNT".to_string())
        )
        .expect("navaid without a symbol is rejected"));
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn insert_waypoint_best_position_uses_minimum_added_route_length() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "west-coast".to_string(),
            name: "West Coast".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KUAO".to_string()),
                },
            ],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: Some(AirportId("KPAE".to_string())),
            destination: Some(AirportId("KUAO".to_string())),
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation =
            insert_waypoint_best_position(&store, &plan, NavRef::Navaid("SEA".to_string()))
                .expect("insert waypoint");

        assert_eq!(mutation.route_components.len(), 3);
        assert!(matches!(
            mutation.route_components[1],
            RouteComponent::Waypoint { waypoint: NavRef::Navaid(ref id) } if id == "SEA"
        ));
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn insert_waypoint_best_position_accepts_spot_waypoint() {
        let store = load_fixture_nav_kv_store();
        let spot = NavRef::Spot(LatLon {
            lat: 47.5,
            lon: -122.2,
        });
        let plan = FlightPlan {
            id: "spot".to_string(),
            name: "Spot".to_string(),
            route_components: Vec::new(),
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let mutation =
            insert_waypoint_best_position(&store, &plan, spot.clone()).expect("insert spot");

        assert_eq!(
            mutation.route_components,
            vec![RouteComponent::Waypoint { waypoint: spot }]
        );
    }

    #[test]
    #[ignore = "requires the external NAVDB fixture"]
    fn insert_waypoint_best_position_rejects_existing_waypoint() {
        let store = load_fixture_nav_kv_store();
        let plan = FlightPlan {
            id: "duplicate".to_string(),
            name: "Duplicate".to_string(),
            route_components: vec![RouteComponent::Waypoint {
                waypoint: NavRef::Navaid("SEA".to_string()),
            }],
            route_component_uids: Vec::new(),
            route_component_uid_counter: 0,
            resolved_legs: Vec::new(),
            guidance: None,
            departure: None,
            destination: None,
            alternate: None,
            aircraft: None,
            cruise_altitude_ft: None,
            planned_departure_time_epoch_ms: None,
            notes: None,
            updated_at_epoch_ms: 0,
            version: 1,
        };

        let err = insert_waypoint_best_position(&store, &plan, NavRef::Navaid("SEA".to_string()))
            .expect_err("duplicate insert should fail");

        assert!(
            matches!(err, HadReadError::Fatal(message) if message.contains("already in the flight plan"))
        );
    }

    #[test]
    fn insert_waypoint_best_position_skips_boundaries_inside_attached_procedures() {
        let lgd = LatLon {
            lat: 45.2902,
            lon: -118.0071,
        };
        let laced = LatLon {
            lat: 46.6933,
            lon: -120.5408,
        };
        let pae = LatLon {
            lat: 47.9063,
            lon: -122.2816,
        };
        let store = test_nav_kv_store(&[
            (
                "navref/position/airport/KLGD",
                serde_json::json!({"lat": lgd.lat, "lon": lgd.lon}),
            ),
            (
                "navref/position/fix/LACED",
                serde_json::json!({"lat": laced.lat, "lon": laced.lon}),
            ),
            (
                "navref/position/airport/KPAE",
                serde_json::json!({"lat": pae.lat, "lon": pae.lon}),
            ),
        ]);
        let procedure = |kind, airport_id: &str, procedure_id: &str| RouteComponent::Procedure {
            procedure: ProcedureSegment {
                airport_id: AirportId(airport_id.to_string()),
                procedure_id: procedure_id.to_string(),
                display_label: None,
                kind,
                runway_transition: None,
                enroute_transition: None,
                terminal_discontinuity: None,
                data_quality: Vec::new(),
            },
        };
        let plan = FlightPlan {
            id: "lgd-chins".to_string(),
            name: "KLGD LGD1 CHINS5 KPAE".to_string(),
            route_components: vec![
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KLGD".to_string()),
                },
                procedure(ProcedureKind::Sid, "KLGD", "LGD1"),
                procedure(ProcedureKind::Star, "KPAE", "CHINS5"),
                RouteComponent::Waypoint {
                    waypoint: NavRef::Airport("KPAE".to_string()),
                },
            ],
            departure: Some(AirportId("KLGD".to_string())),
            destination: Some(AirportId("KPAE".to_string())),
            ..FlightPlan::default()
        }
        .normalized();

        let inserted =
            insert_waypoint_best_position(&store, &plan, NavRef::Fix("LACED".to_string()))
                .expect("insert LACED between the SID and STAR");

        assert!(matches!(
            inserted.route_components.get(2),
            Some(RouteComponent::Waypoint {
                waypoint: NavRef::Fix(id)
            }) if id == "LACED"
        ));
    }
}
