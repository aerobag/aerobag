// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use product_contracts::publication::{bundle::v2 as bundle_v2, current::v1 as current_v1};
use product_contracts::versioned_json;

pub const CHART_HIGH_RESOLUTION_PRODUCT_ID: &str = "chart-high-resolution";
const WIDE_COVERAGE_REGION_ID: &str = "wide";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum OfflinePackageSelection {
    Unselected,
    Pause,
    Play,
}

impl Default for OfflinePackageSelection {
    fn default() -> Self {
        Self::Play
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagePreferences {
    pub regions: BTreeMap<String, OfflinePackageSelection>,
    pub products: BTreeMap<String, OfflinePackageSelection>,
}

/// Core's package-planning projection of a publication package artifact.
///
/// External JSON must first be decoded through `product_contracts::publication`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePackageArtifact {
    pub id: String,
    pub family_id: String,
    pub contract_id: String,
    pub region_id: Option<String>,
    pub filename: String,
    pub relative_path: String,
    pub cycle: Option<String>,
    pub cycle_version: Option<String>,
    pub checksum_sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub effective_date: Option<String>,
    pub expiration_date: Option<String>,
    #[serde(default)]
    pub warning_text: Option<String>,
    pub metadata: Option<BundlePackageMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePackageMetadata {
    #[serde(default)]
    pub chart_package_tier: Option<product_contracts::ChartPackageTier>,
    pub full_coverage_zoom: Option<u32>,
    pub wide_angle_region_id: Option<String>,
    pub wide_angle_max_zoom: Option<u32>,
    pub wide_angle: Option<bool>,
    pub min_source_zoom: Option<u32>,
    pub max_source_zoom: Option<u32>,
    pub tile_count: Option<u64>,
}

pub(crate) fn required_package_contract_id(family_id: &str) -> Option<&'static str> {
    product_contracts::contract_id_for_family(family_id)
}

pub(crate) fn package_contract_is_supported(pkg: &BundlePackageArtifact) -> bool {
    required_package_contract_id(&pkg.family_id) == Some(pkg.contract_id.as_str())
}

pub fn current_artifacts_manifest_is_supported(manifest: &CurrentArtifactsManifest) -> bool {
    !manifest.contracts.is_empty()
        && manifest.contracts.iter().all(|(family_id, contract_id)| {
            required_package_contract_id(family_id) == Some(contract_id.as_str())
        })
}

/// Core's package-planning projection of a publication bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub packages: Vec<BundlePackageArtifact>,
}

/// Core's discovery projection after strict versioned wire decoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentArtifactsManifest {
    pub schema_version: Option<u32>,
    pub contracts: BTreeMap<String, String>,
    pub artifact_roots: CurrentArtifactsArtifactRoots,
    pub as_of_date: Option<String>,
    pub as_of_utc: Option<String>,
    pub bundles: Vec<CurrentArtifactsBundleRef>,
    #[serde(default)]
    pub startup_prefetch: Option<CurrentStartupPrefetchManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentArtifactsArtifactRoots {
    pub packaged: String,
    pub unpacked: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentArtifactsBundleRef {
    pub filename: String,
    pub relative_path: String,
    pub id: String,
    pub bundle_type: String,
    pub cycle: Option<String>,
    pub cycle_version: Option<String>,
    pub start_valid: Option<String>,
    pub end_valid: Option<String>,
    pub checksum_sha256: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentStartupPrefetchManifest {
    pub schema_version: u32,
    pub cycle_resources: Vec<CurrentStartupPrefetchCycleResources>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentStartupPrefetchCycleResources {
    pub bundle_id: String,
    pub cycle: String,
    pub cycle_version: String,
    pub start_valid: String,
    pub end_valid: String,
    pub resources: Vec<CurrentStartupPrefetchResource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentStartupPrefetchResource {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentArtifactsBundleRequest {
    pub filename: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentArtifactsDiscoveryPlan {
    pub discovery_jsons: Vec<String>,
    pub bundle_requests: Vec<CurrentArtifactsBundleRequest>,
}

pub fn decode_current_artifacts_manifest_list(
    payload: &str,
) -> Result<Vec<CurrentArtifactsManifest>, String> {
    versioned_json::decode_offered_list::<current_v1::CurrentArtifactsManifest>(
        "current_artifacts.json",
        payload.as_bytes(),
        current_v1::SCHEMA_VERSION,
    )
    .map_err(|error| error.to_string())?
    .into_iter()
    .map(CurrentArtifactsManifest::try_from)
    .collect()
}

pub fn decode_current_artifacts_manifest(
    payload: &str,
) -> Result<CurrentArtifactsManifest, String> {
    let manifest = versioned_json::decode_exact::<current_v1::CurrentArtifactsManifest>(
        "current_artifacts.json manifest",
        payload.as_bytes(),
        current_v1::SCHEMA_VERSION,
    )
    .map_err(|error| error.to_string())?;
    CurrentArtifactsManifest::try_from(manifest)
}

pub fn decode_bundle_manifest(payload: &str) -> Result<BundleManifest, String> {
    let manifest = versioned_json::decode_exact::<bundle_v2::BundleManifest>(
        "bundle manifest",
        payload.as_bytes(),
        bundle_v2::SCHEMA_VERSION,
    )
    .map_err(|error| error.to_string())?;
    BundleManifest::try_from(manifest)
}

impl TryFrom<bundle_v2::BundlePackageArtifact> for BundlePackageArtifact {
    type Error = String;

    fn try_from(wire: bundle_v2::BundlePackageArtifact) -> Result<Self, Self::Error> {
        let metadata = if wire.metadata.is_empty() {
            None
        } else {
            Some(
                serde_json::from_value::<BundlePackageMetadata>(serde_json::Value::Object(
                    wire.metadata.into_iter().collect(),
                ))
                .map_err(|error| {
                    format!("package {} has invalid typed metadata: {error}", wire.id)
                })?,
            )
        };
        Ok(Self {
            id: wire.id,
            family_id: wire.family_id,
            contract_id: wire.contract_id,
            region_id: wire.region_id,
            filename: wire.filename,
            relative_path: wire.relative_path,
            cycle: wire.cycle,
            cycle_version: wire.cycle_version,
            checksum_sha256: Some(wire.checksum_sha256),
            size_bytes: Some(wire.size_bytes),
            effective_date: wire.effective_date,
            expiration_date: wire.expiration_date,
            warning_text: wire.warning_text,
            metadata,
        })
    }
}

impl TryFrom<bundle_v2::BundleManifest> for BundleManifest {
    type Error = String;

    fn try_from(wire: bundle_v2::BundleManifest) -> Result<Self, Self::Error> {
        if wire.schema_version != bundle_v2::SCHEMA_VERSION {
            return Err(format!(
                "bundle manifest schema {} reached v2 conversion",
                wire.schema_version
            ));
        }
        Ok(Self {
            packages: wire
                .packages
                .into_iter()
                .map(BundlePackageArtifact::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl TryFrom<current_v1::CurrentArtifactsManifest> for CurrentArtifactsManifest {
    type Error = String;

    fn try_from(wire: current_v1::CurrentArtifactsManifest) -> Result<Self, Self::Error> {
        if wire.schema_version != current_v1::SCHEMA_VERSION {
            return Err(format!(
                "current_artifacts.json schema {} reached v1 conversion",
                wire.schema_version
            ));
        }
        Ok(Self {
            schema_version: Some(wire.schema_version),
            contracts: wire.contracts,
            artifact_roots: CurrentArtifactsArtifactRoots {
                packaged: wire.artifact_roots.packaged,
                unpacked: wire.artifact_roots.unpacked,
            },
            as_of_date: Some(wire.as_of_date),
            as_of_utc: Some(wire.as_of_utc),
            bundles: wire
                .bundles
                .into_iter()
                .map(|entry| CurrentArtifactsBundleRef {
                    filename: entry.filename,
                    relative_path: entry.relative_path,
                    id: entry.id,
                    bundle_type: entry.bundle_type,
                    cycle: Some(entry.cycle),
                    cycle_version: Some(entry.cycle_version),
                    start_valid: Some(entry.start_valid),
                    end_valid: Some(entry.end_valid),
                    checksum_sha256: Some(entry.checksum_sha256),
                    size_bytes: Some(entry.size_bytes),
                })
                .collect(),
            startup_prefetch: wire.startup_prefetch.map(|prefetch| {
                CurrentStartupPrefetchManifest {
                    schema_version: prefetch.schema_version,
                    cycle_resources: prefetch
                        .cycle_resources
                        .into_iter()
                        .map(|cycle| CurrentStartupPrefetchCycleResources {
                            bundle_id: cycle.bundle_id,
                            cycle: cycle.cycle,
                            cycle_version: cycle.cycle_version,
                            start_valid: cycle.start_valid,
                            end_valid: cycle.end_valid,
                            resources: cycle
                                .resources
                                .into_iter()
                                .map(|resource| CurrentStartupPrefetchResource {
                                    url: resource.url,
                                })
                                .collect(),
                        })
                        .collect(),
                }
            }),
        })
    }
}

pub fn select_supported_current_artifacts_manifests(
    manifests: Vec<CurrentArtifactsManifest>,
) -> Result<Vec<CurrentArtifactsManifest>, String> {
    let selected = manifests
        .iter()
        .filter(|manifest| current_artifacts_manifest_is_supported(manifest))
        .cloned()
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(current_artifacts_contract_mismatch_message(&manifests));
    }
    Ok(selected)
}

fn current_artifacts_contract_mismatch_message(manifests: &[CurrentArtifactsManifest]) -> String {
    let offered_by_family = manifests
        .iter()
        .flat_map(|manifest| manifest.contracts.iter())
        .fold(
            BTreeMap::<String, BTreeSet<String>>::new(),
            |mut offered, (family_id, contract_id)| {
                offered
                    .entry(family_id.clone())
                    .or_default()
                    .insert(contract_id.clone());
                offered
            },
        );
    if offered_by_family.is_empty() {
        return "current_artifacts.json has no manifest supported by this app: artifacts declare no product contracts".to_string();
    }

    let mut mismatches = Vec::new();
    for contract in product_contracts::PRODUCT_CONTRACTS {
        let Some(offered) = offered_by_family.get(contract.family_id) else {
            continue;
        };
        if offered.contains(contract.contract_id) {
            continue;
        }
        mismatches.push((
            contract.family_id.to_string(),
            contract.contract_id.to_string(),
            offered.iter().cloned().collect::<Vec<_>>().join("/"),
        ));
    }

    if mismatches.is_empty() {
        let unknown = offered_by_family
            .iter()
            .filter(|(family_id, _)| required_package_contract_id(family_id).is_none())
            .map(|(family_id, offered)| {
                format!(
                    "{}={}",
                    family_id,
                    offered.iter().cloned().collect::<Vec<_>>().join("/")
                )
            })
            .take(4)
            .collect::<Vec<_>>();
        let more = offered_by_family
            .keys()
            .filter(|family_id| required_package_contract_id(family_id).is_none())
            .count()
            .saturating_sub(unknown.len());
        let more_suffix = if more == 0 {
            String::new()
        } else {
            format!("; {more} more unsupported contract families")
        };
        return format!(
            "current_artifacts.json has no manifest supported by this app: artifacts declare unsupported contract families {}{}",
            unknown.join(", "),
            more_suffix
        );
    }

    let visible = mismatches.iter().take(4).collect::<Vec<_>>();
    let required = visible
        .iter()
        .map(|(family_id, required, _)| format!("{family_id}={required}"))
        .collect::<Vec<_>>()
        .join(", ");
    let offered = visible
        .iter()
        .map(|(family_id, _, offered)| format!("{family_id}={offered}"))
        .collect::<Vec<_>>()
        .join(", ");
    let more = mismatches.len().saturating_sub(visible.len());
    let more_suffix = if more == 0 {
        String::new()
    } else {
        format!("; {more} more contract mismatches")
    };
    format!(
        "current_artifacts.json has no manifest supported by this app: app requires {required}; artifacts offer {offered}{more_suffix}"
    )
}

pub fn plan_current_artifacts_discovery(
    publication_root_url: &str,
    current_artifacts_json: &str,
) -> Result<CurrentArtifactsDiscoveryPlan, String> {
    let selected = select_supported_current_artifacts_manifests(
        decode_current_artifacts_manifest_list(current_artifacts_json)?,
    )?;
    let discovery_jsons = selected
        .iter()
        .map(|manifest| serde_json::to_string(manifest).map_err(|err| err.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut seen_filenames = BTreeSet::new();
    let mut bundle_requests = Vec::new();
    for manifest in &selected {
        let packaged_root = join_package_url([
            publication_root_url,
            manifest.artifact_roots.packaged.as_str(),
        ]);
        for bundle in &manifest.bundles {
            if !seen_filenames.insert(bundle.filename.clone()) {
                continue;
            }
            let relative_path = if bundle.relative_path.trim().is_empty() {
                bundle.filename.as_str()
            } else {
                bundle.relative_path.as_str()
            };
            bundle_requests.push(CurrentArtifactsBundleRequest {
                filename: bundle.filename.clone(),
                url: join_package_url([packaged_root.as_str(), relative_path]),
            });
        }
    }
    Ok(CurrentArtifactsDiscoveryPlan {
        discovery_jsons,
        bundle_requests,
    })
}

fn join_package_url<const N: usize>(parts: [&str; N]) -> String {
    parts
        .into_iter()
        .enumerate()
        .map(|(index, part)| {
            if index == 0 {
                part.trim().trim_end_matches('/').to_string()
            } else {
                part.trim().trim_matches('/').to_string()
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledArtifact {
    pub artifact_id: String,
    pub filename: String,
    pub size_bytes: Option<u64>,
    pub checksum_sha256: Option<String>,
    #[serde(default)]
    pub family_id: Option<String>,
    #[serde(default)]
    pub region_id: Option<String>,
    #[serde(default)]
    pub chart_package_tier: Option<product_contracts::ChartPackageTier>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledArtifactMetadataUpdate {
    pub artifact_id: String,
    pub filename: String,
    pub family_id: String,
    pub region_id: Option<String>,
    pub chart_package_tier: Option<product_contracts::ChartPackageTier>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManagementInput {
    pub now_epoch_ms: i64,
    pub preferences: OfflinePackagePreferences,
    pub bundle: BundleManifest,
    pub installed: Vec<InstalledArtifact>,
    #[serde(default)]
    pub forced_gc_installed_filenames: Vec<String>,
    #[serde(default)]
    pub suppressed_fetch_filenames: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PackageManagementPlan {
    pub fetch: Vec<String>,
    pub retain_installed: Vec<String>,
    pub gc: Vec<String>,
    pub protected_by_pause: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagesState {
    pub preferences: OfflinePackagePreferences,
    pub now_override_epoch_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OfflinePackagesEvent {
    CycleRegion { id: String },
    CycleProduct { id: String },
    UseSystemClock,
    SetClockOverride { epoch_ms: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagesUiRow {
    pub id: String,
    #[serde(default)]
    pub label: String,
    pub selection: OfflinePackageSelection,
    #[serde(default)]
    pub selection_event: Option<OfflinePackagesEvent>,
    #[serde(default)]
    pub help_text: Option<String>,
    pub fetch_count: usize,
    pub gc_count: usize,
    pub pause_count: usize,
    #[serde(default)]
    pub plan_entries: Vec<OfflinePackagesUiPlanEntry>,
    #[serde(default)]
    pub installed_size_label: String,
    #[serde(default)]
    pub planned_change_label: String,
    #[serde(default)]
    pub planned_total_size_label: String,
    #[serde(default)]
    pub planned_size_change_visible: bool,
    #[serde(default)]
    pub sync_progress_per_mille: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum OfflinePackagesUiPlanAction {
    Delete,
    Keep,
    Pause,
    Fetch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesUiPlanEntry {
    pub action: OfflinePackagesUiPlanAction,
    pub count: usize,
    pub cycles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesClockOption {
    pub id: String,
    pub label: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagesUiState {
    pub clock_label: String,
    pub clock_options: Vec<OfflinePackagesClockOption>,
    pub all_packages: OfflinePackagesUiRow,
    pub core_products: Vec<OfflinePackagesUiRow>,
    pub zoom_levels: Vec<OfflinePackagesUiRow>,
    pub regions: Vec<OfflinePackagesUiRow>,
    pub products: Vec<OfflinePackagesUiRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesInitInput {
    pub state: Option<OfflinePackagesState>,
    pub now_epoch_ms: i64,
    pub discovery_manifests: Vec<CurrentArtifactsManifest>,
    pub bundle_manifests_by_filename: BTreeMap<String, BundleManifest>,
    pub installed: Vec<InstalledArtifact>,
    #[serde(default)]
    pub forced_gc_installed_filenames: Vec<String>,
    #[serde(default)]
    pub suppressed_fetch_filenames: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesReduceInput {
    pub state: OfflinePackagesState,
    pub event: OfflinePackagesEvent,
    pub now_epoch_ms: i64,
    pub discovery_manifests: Vec<CurrentArtifactsManifest>,
    pub bundle_manifests_by_filename: BTreeMap<String, BundleManifest>,
    pub installed: Vec<InstalledArtifact>,
    #[serde(default)]
    pub forced_gc_installed_filenames: Vec<String>,
    #[serde(default)]
    pub suppressed_fetch_filenames: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesReduceResult {
    pub state: OfflinePackagesState,
    pub ui_state: OfflinePackagesUiState,
    pub effective_now_epoch_ms: i64,
    pub plan: PackageManagementPlan,
    pub bundle: BundleManifest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesWarning {
    pub artifact_id: String,
    pub family_id: Option<String>,
    pub region_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesSyncSummary {
    pub fetched_count: usize,
    pub gc_count: usize,
    pub warnings: Vec<OfflinePackagesWarning>,
    #[serde(default)]
    pub remote_poisoned_filename_messages: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagesLibraryCache {
    pub package_source_base_url: String,
    pub fetched_at_epoch_ms: i64,
    pub discovery_manifests: Vec<CurrentArtifactsManifest>,
    pub bundle_manifests_by_filename: BTreeMap<String, BundleManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagesControllerState {
    pub packages_state: Option<OfflinePackagesState>,
    pub library_cache: Option<OfflinePackagesLibraryCache>,
    pub tombstoned_installed_filename_messages: BTreeMap<String, String>,
    pub suppressed_fetch_filename_messages: BTreeMap<String, String>,
    #[serde(default)]
    pub sync_progress: Option<OfflinePackagesSyncProgress>,
    pub library_loading: bool,
    pub library_error_message: Option<String>,
    #[serde(default)]
    pub sync_after_library_refresh: bool,
    pub sync_in_flight: bool,
    pub sync_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagesSyncProgress {
    #[serde(default)]
    pub planned_fetch_artifact_ids: BTreeSet<String>,
    #[serde(default)]
    pub completed_fetch_artifact_ids: BTreeSet<String>,
    #[serde(default)]
    pub active_fetch_bytes_by_artifact_id: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OfflinePackagesControllerEvent {
    EnsureLibrary,
    RefreshLibraryRequested,
    LibraryRefreshSucceeded {
        fetched_at_epoch_ms: i64,
        discovery_manifests: Vec<CurrentArtifactsManifest>,
        bundle_manifests_by_filename: BTreeMap<String, BundleManifest>,
    },
    LibraryRefreshFailed {
        message: String,
    },
    PackagesEvent {
        event: OfflinePackagesEvent,
    },
    ApplySynchronizedPreferences {
        preferences: OfflinePackagePreferences,
    },
    SyncRequested,
    SyncProgressObserved {
        progress: OfflinePackagesSyncProgress,
    },
    SyncFinished {
        summary: OfflinePackagesSyncSummary,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OfflinePackagesControllerCommand {
    RefreshLibrary {
        package_source_base_url: String,
        discovery_filenames: Vec<String>,
    },
    Sync {
        package_source_base_url: String,
        packaged_artifact_root: String,
        plan: PackageManagementPlan,
        bundle: BundleManifest,
        max_parallel_fetches: usize,
    },
}

pub const OFFLINE_PACKAGES_MAX_PARALLEL_FETCHES: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagesControllerUiState {
    pub planner_ui_state: Option<OfflinePackagesUiState>,
    pub library_loaded: bool,
    pub library_loading: bool,
    pub library_error_message: Option<String>,
    pub library_status_message: Option<String>,
    pub sync_in_flight: bool,
    pub sync_message: Option<String>,
    pub storage_capacity_label: Option<String>,
    pub package_source_editable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_source_edit_disabled_reason: Option<String>,
    pub refresh_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_disabled_reason: Option<String>,
    pub refresh_cancel_enabled: bool,
    pub sync_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_disabled_reason: Option<String>,
    pub sync_cancel_enabled: bool,
    pub planner_interactions_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planner_interactions_disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesControllerInput {
    pub state: Option<OfflinePackagesControllerState>,
    pub package_source_base_url: String,
    pub discovery_filenames: Vec<String>,
    pub now_epoch_ms: i64,
    pub installed: Vec<InstalledArtifact>,
    #[serde(default)]
    pub storage: Option<OfflinePackagesStorageInfo>,
    pub event: OfflinePackagesControllerEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesStorageInfo {
    pub available_bytes: u64,
    #[serde(default)]
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesControllerResult {
    pub state: OfflinePackagesControllerState,
    pub ui_state: OfflinePackagesControllerUiState,
    pub command: Option<OfflinePackagesControllerCommand>,
    pub preferences_for_cloud: Option<OfflinePackagePreferences>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AvailablePackageArtifact {
    artifact_id: String,
    filename: String,
    product_id: String,
    region_id: Option<String>,
    chart_package_tier: Option<product_contracts::ChartPackageTier>,
    effective_at_epoch_ms: Option<i64>,
    expires_at_epoch_ms: Option<i64>,
}

pub fn default_offline_package_preferences<I, J>(
    region_ids: I,
    product_ids: J,
) -> OfflinePackagePreferences
where
    I: IntoIterator,
    I::Item: Into<String>,
    J: IntoIterator,
    J::Item: Into<String>,
{
    OfflinePackagePreferences {
        regions: region_ids
            .into_iter()
            .map(|id| (id.into(), OfflinePackageSelection::Play))
            .collect(),
        products: product_ids
            .into_iter()
            .map(|id| {
                let id = id.into();
                let selection = default_product_selection(&id);
                (id, selection)
            })
            .collect(),
    }
}

pub fn initialize_offline_packages(
    input: &OfflinePackagesInitInput,
) -> OfflinePackagesReduceResult {
    let prior_state = input.state.clone().unwrap_or_default();
    let effective_now_epoch_ms = effective_now_epoch_ms(&prior_state, input.now_epoch_ms);
    let (region_ids, product_ids) = offline_package_catalog_dimensions(
        &input.discovery_manifests,
        &input.bundle_manifests_by_filename,
        effective_now_epoch_ms,
        None,
    );
    let state = OfflinePackagesState {
        preferences: normalize_preferences(
            input.state.as_ref().map(|state| &state.preferences),
            &region_ids,
            &product_ids,
        ),
        now_override_epoch_ms: prior_state.now_override_epoch_ms,
    };
    let bundle = resolve_cycle_bundle_manifest(
        &input.discovery_manifests,
        &input.bundle_manifests_by_filename,
        effective_now_epoch_ms,
    );
    let plan = plan_offline_packages(&PackageManagementInput {
        now_epoch_ms: effective_now_epoch_ms,
        preferences: state.preferences.clone(),
        bundle: bundle.clone(),
        installed: input.installed.clone(),
        forced_gc_installed_filenames: input.forced_gc_installed_filenames.clone(),
        suppressed_fetch_filenames: input.suppressed_fetch_filenames.clone(),
    });
    OfflinePackagesReduceResult {
        ui_state: project_offline_packages_ui_state(
            &state,
            effective_now_epoch_ms,
            &input.discovery_manifests,
            &input.bundle_manifests_by_filename,
            &input.installed,
            &input.forced_gc_installed_filenames,
            &input.suppressed_fetch_filenames,
            None,
        ),
        effective_now_epoch_ms,
        plan,
        bundle,
        state,
    }
}

pub fn reduce_offline_packages(input: &OfflinePackagesReduceInput) -> OfflinePackagesReduceResult {
    reduce_offline_packages_from_catalog(
        input.state.clone(),
        &input.event,
        input.now_epoch_ms,
        &input.discovery_manifests,
        &input.bundle_manifests_by_filename,
        &input.installed,
        &input.forced_gc_installed_filenames,
        &input.suppressed_fetch_filenames,
    )
}

#[allow(clippy::too_many_arguments)]
fn reduce_offline_packages_from_catalog(
    prior_state: OfflinePackagesState,
    event: &OfflinePackagesEvent,
    now_epoch_ms: i64,
    discovery_manifests: &[CurrentArtifactsManifest],
    bundle_manifests_by_filename: &BTreeMap<String, BundleManifest>,
    installed: &[InstalledArtifact],
    forced_gc_installed_filenames: &[String],
    suppressed_fetch_filenames: &[String],
) -> OfflinePackagesReduceResult {
    let initial_now_epoch_ms = effective_now_epoch_ms(&prior_state, now_epoch_ms);
    let (initial_region_ids, initial_product_ids) = offline_package_catalog_dimensions(
        discovery_manifests,
        bundle_manifests_by_filename,
        initial_now_epoch_ms,
        None,
    );
    let mut state = OfflinePackagesState {
        preferences: normalize_preferences(
            Some(&prior_state.preferences),
            &initial_region_ids,
            &initial_product_ids,
        ),
        now_override_epoch_ms: prior_state.now_override_epoch_ms,
    };

    match event {
        OfflinePackagesEvent::CycleRegion { id } => {
            cycle_selection(&mut state.preferences.regions, id);
        }
        OfflinePackagesEvent::CycleProduct { id } => {
            cycle_selection(&mut state.preferences.products, id);
        }
        OfflinePackagesEvent::UseSystemClock => {
            state.now_override_epoch_ms = None;
        }
        OfflinePackagesEvent::SetClockOverride { epoch_ms } => {
            state.now_override_epoch_ms = Some(*epoch_ms);
        }
    }

    let effective_now_epoch_ms = effective_now_epoch_ms(&state, now_epoch_ms);
    let (region_ids, product_ids) = offline_package_catalog_dimensions(
        discovery_manifests,
        bundle_manifests_by_filename,
        effective_now_epoch_ms,
        None,
    );
    state.preferences = normalize_preferences(Some(&state.preferences), &region_ids, &product_ids);
    let bundle = resolve_cycle_bundle_manifest(
        discovery_manifests,
        bundle_manifests_by_filename,
        effective_now_epoch_ms,
    );
    let plan_input = PackageManagementInput {
        now_epoch_ms: effective_now_epoch_ms,
        preferences: state.preferences.clone(),
        bundle,
        installed: installed.to_vec(),
        forced_gc_installed_filenames: forced_gc_installed_filenames.to_vec(),
        suppressed_fetch_filenames: suppressed_fetch_filenames.to_vec(),
    };
    let plan = plan_offline_packages(&plan_input);
    let ui_state = project_offline_packages_ui_state_with_plan(
        &state,
        effective_now_epoch_ms,
        bundle_manifests_by_filename,
        discovery_manifests,
        None,
        &plan_input,
        &plan,
    );
    let bundle = plan_input.bundle;

    OfflinePackagesReduceResult {
        ui_state,
        effective_now_epoch_ms,
        plan,
        bundle,
        state,
    }
}

pub fn reduce_offline_packages_controller(
    input: &OfflinePackagesControllerInput,
) -> OfflinePackagesControllerResult {
    reduce_offline_packages_controller_owned(input.clone())
}

pub fn reduce_offline_packages_controller_owned(
    mut input: OfflinePackagesControllerInput,
) -> OfflinePackagesControllerResult {
    let mut state = input.state.take().unwrap_or_default();
    let package_source_base_url = input
        .package_source_base_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    let mut command = None;
    let mut preferences_for_cloud = None;

    match &input.event {
        OfflinePackagesControllerEvent::EnsureLibrary => {
            if library_refresh_needed(
                state.library_cache.as_ref(),
                &package_source_base_url,
                input.now_epoch_ms,
            ) {
                state.library_loading = true;
                state.library_error_message = None;
                command = Some(OfflinePackagesControllerCommand::RefreshLibrary {
                    package_source_base_url: package_source_base_url.clone(),
                    discovery_filenames: input.discovery_filenames.clone(),
                });
            }
        }
        OfflinePackagesControllerEvent::RefreshLibraryRequested => {
            state.library_loading = true;
            state.library_error_message = None;
            command = Some(OfflinePackagesControllerCommand::RefreshLibrary {
                package_source_base_url: package_source_base_url.clone(),
                discovery_filenames: input.discovery_filenames.clone(),
            });
        }
        OfflinePackagesControllerEvent::LibraryRefreshSucceeded {
            fetched_at_epoch_ms,
            discovery_manifests,
            bundle_manifests_by_filename,
        } => {
            state.library_cache = Some(OfflinePackagesLibraryCache {
                package_source_base_url: package_source_base_url.clone(),
                fetched_at_epoch_ms: *fetched_at_epoch_ms,
                discovery_manifests: discovery_manifests.clone(),
                bundle_manifests_by_filename: bundle_manifests_by_filename.clone(),
            });
            state.library_loading = false;
            state.library_error_message = None;
            if state.sync_after_library_refresh {
                state.sync_after_library_refresh = false;
                return start_offline_packages_sync(state, &package_source_base_url, &input);
            }
        }
        OfflinePackagesControllerEvent::LibraryRefreshFailed { message } => {
            state.library_loading = false;
            state.library_error_message = Some(message.clone());
            state.sync_after_library_refresh = false;
        }
        OfflinePackagesControllerEvent::PackagesEvent { event } => {
            if state.library_cache.is_none() {
                state.library_error_message =
                    Some("offline packages library is not loaded".to_string());
                return OfflinePackagesControllerResult {
                    ui_state: project_offline_packages_controller_ui_state(
                        &state,
                        &package_source_base_url,
                        None,
                        input.storage.as_ref(),
                    ),
                    state,
                    command,
                    preferences_for_cloud: None,
                };
            }
            let installed = effective_installed_artifacts(&state, &input.installed);
            let forced_gc_installed_filenames =
                forced_gc_installed_filenames(&state, &input.installed);
            let suppressed_fetch_filenames = state
                .suppressed_fetch_filename_messages
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            let packages_state = state.packages_state.take().unwrap_or_default();
            let library_cache = state
                .library_cache
                .as_ref()
                .expect("library cache checked above");
            let reduced = reduce_offline_packages_from_catalog(
                packages_state,
                event,
                input.now_epoch_ms,
                &library_cache.discovery_manifests,
                &library_cache.bundle_manifests_by_filename,
                &installed,
                &forced_gc_installed_filenames,
                &suppressed_fetch_filenames,
            );
            let preferences_for_cloud = reduced.state.preferences.clone();
            state.packages_state = Some(reduced.state);
            return OfflinePackagesControllerResult {
                ui_state: project_offline_packages_controller_ui_state(
                    &state,
                    &package_source_base_url,
                    Some(reduced.ui_state),
                    input.storage.as_ref(),
                ),
                state,
                command,
                preferences_for_cloud: Some(preferences_for_cloud),
            };
        }
        OfflinePackagesControllerEvent::ApplySynchronizedPreferences { preferences } => {
            let packages_state = state.packages_state.get_or_insert_with(Default::default);
            packages_state
                .preferences
                .regions
                .extend(preferences.regions.clone());
            packages_state
                .preferences
                .products
                .extend(preferences.products.clone());
            if packages_state.preferences != *preferences {
                preferences_for_cloud = Some(packages_state.preferences.clone());
            }
        }
        OfflinePackagesControllerEvent::SyncRequested => {
            if state.library_cache.is_none() {
                state.library_error_message =
                    Some("offline packages library is not loaded".to_string());
            } else {
                state.sync_after_library_refresh = true;
                state.library_loading = true;
                state.library_error_message = None;
                command = Some(OfflinePackagesControllerCommand::RefreshLibrary {
                    package_source_base_url: package_source_base_url.clone(),
                    discovery_filenames: input.discovery_filenames.clone(),
                });
            }
        }
        OfflinePackagesControllerEvent::SyncProgressObserved { progress } => {
            state.sync_in_flight = true;
            let planned_fetch_artifact_ids = progress
                .planned_fetch_artifact_ids
                .is_empty()
                .then(|| {
                    state
                        .sync_progress
                        .as_ref()
                        .map(|progress| progress.planned_fetch_artifact_ids.clone())
                        .unwrap_or_default()
                })
                .filter(|planned| !planned.is_empty())
                .unwrap_or_else(|| progress.planned_fetch_artifact_ids.clone());
            state.sync_progress = Some(OfflinePackagesSyncProgress {
                planned_fetch_artifact_ids,
                ..progress.clone()
            });
        }
        OfflinePackagesControllerEvent::SyncFinished { summary } => {
            state.sync_after_library_refresh = false;
            state.sync_in_flight = false;
            state.sync_progress = None;
            state.sync_message = format_offline_packages_sync_summary(summary);
            state
                .suppressed_fetch_filename_messages
                .extend(summary.remote_poisoned_filename_messages.clone());
            state
                .tombstoned_installed_filename_messages
                .extend(summary.remote_poisoned_filename_messages.clone());
        }
    }

    let forced_gc_installed_filenames = forced_gc_installed_filenames(&state, &input.installed);
    let planner_ui_state = replan_controller_ui_state(
        &mut state,
        &package_source_base_url,
        input.now_epoch_ms,
        &input.installed,
        &forced_gc_installed_filenames,
    );
    OfflinePackagesControllerResult {
        ui_state: project_offline_packages_controller_ui_state(
            &state,
            &package_source_base_url,
            planner_ui_state,
            input.storage.as_ref(),
        ),
        state,
        command,
        preferences_for_cloud,
    }
}

fn start_offline_packages_sync(
    mut state: OfflinePackagesControllerState,
    package_source_base_url: &str,
    input: &OfflinePackagesControllerInput,
) -> OfflinePackagesControllerResult {
    if state.library_error_message.is_some() {
        let forced_gc_installed_filenames = forced_gc_installed_filenames(&state, &input.installed);
        let planner_ui_state = replan_controller_ui_state(
            &mut state,
            package_source_base_url,
            input.now_epoch_ms,
            &input.installed,
            &forced_gc_installed_filenames,
        );
        return OfflinePackagesControllerResult {
            ui_state: project_offline_packages_controller_ui_state(
                &state,
                package_source_base_url,
                planner_ui_state,
                input.storage.as_ref(),
            ),
            state,
            command: None,
            preferences_for_cloud: None,
        };
    }
    let Some(library_cache) = state.library_cache.as_ref() else {
        state.library_error_message = Some("offline packages library is not loaded".to_string());
        return OfflinePackagesControllerResult {
            ui_state: project_offline_packages_controller_ui_state(
                &state,
                package_source_base_url,
                None,
                input.storage.as_ref(),
            ),
            state,
            command: None,
            preferences_for_cloud: None,
        };
    };
    let Some(packaged_artifact_root) = packaged_artifact_root(&library_cache.discovery_manifests)
    else {
        state.library_error_message =
            Some("offline packages library has no packaged artifact root".to_string());
        return OfflinePackagesControllerResult {
            ui_state: project_offline_packages_controller_ui_state(
                &state,
                package_source_base_url,
                None,
                input.storage.as_ref(),
            ),
            state,
            command: None,
            preferences_for_cloud: None,
        };
    };
    let current = initialize_offline_packages(&OfflinePackagesInitInput {
        state: state.packages_state.clone(),
        now_epoch_ms: input.now_epoch_ms,
        discovery_manifests: library_cache.discovery_manifests.clone(),
        bundle_manifests_by_filename: library_cache.bundle_manifests_by_filename.clone(),
        installed: effective_installed_artifacts(&state, &input.installed),
        forced_gc_installed_filenames: forced_gc_installed_filenames(&state, &input.installed),
        suppressed_fetch_filenames: state
            .suppressed_fetch_filename_messages
            .keys()
            .cloned()
            .collect(),
    });
    state.packages_state = Some(current.state.clone());
    state.sync_in_flight = true;
    state.sync_progress = Some(OfflinePackagesSyncProgress {
        planned_fetch_artifact_ids: current.plan.fetch.iter().cloned().collect(),
        ..OfflinePackagesSyncProgress::default()
    });
    let command = Some(OfflinePackagesControllerCommand::Sync {
        package_source_base_url: package_source_base_url.to_string(),
        packaged_artifact_root,
        plan: current.plan,
        bundle: current.bundle,
        max_parallel_fetches: OFFLINE_PACKAGES_MAX_PARALLEL_FETCHES,
    });
    OfflinePackagesControllerResult {
        ui_state: project_offline_packages_controller_ui_state(
            &state,
            package_source_base_url,
            Some(current.ui_state),
            input.storage.as_ref(),
        ),
        state,
        command,
        preferences_for_cloud: None,
    }
}

fn packaged_artifact_root(discovery_manifests: &[CurrentArtifactsManifest]) -> Option<String> {
    discovery_manifests
        .first()
        .map(|manifest| manifest.artifact_roots.packaged.trim().to_string())
        .filter(|root| !root.is_empty())
}

fn library_refresh_needed(
    cache: Option<&OfflinePackagesLibraryCache>,
    package_source_base_url: &str,
    now_epoch_ms: i64,
) -> bool {
    let Some(cache) = cache else {
        return true;
    };
    if cache.discovery_manifests.is_empty() {
        return true;
    }
    if cache.package_source_base_url != package_source_base_url {
        return true;
    }
    now_epoch_ms - cache.fetched_at_epoch_ms > 60 * 60 * 1000
}

fn effective_installed_artifacts(
    state: &OfflinePackagesControllerState,
    installed: &[InstalledArtifact],
) -> Vec<InstalledArtifact> {
    installed
        .iter()
        .filter(|artifact| {
            !state
                .tombstoned_installed_filename_messages
                .contains_key(&artifact.filename)
        })
        .cloned()
        .collect()
}

fn forced_gc_installed_filenames(
    state: &OfflinePackagesControllerState,
    installed: &[InstalledArtifact],
) -> Vec<String> {
    installed
        .iter()
        .filter(|artifact| {
            state
                .tombstoned_installed_filename_messages
                .contains_key(&artifact.filename)
        })
        .map(|artifact| artifact.filename.clone())
        .collect()
}

fn replan_controller_ui_state(
    state: &mut OfflinePackagesControllerState,
    package_source_base_url: &str,
    now_epoch_ms: i64,
    installed: &[InstalledArtifact],
    forced_gc_installed_filenames: &[String],
) -> Option<OfflinePackagesUiState> {
    let library_cache = state.library_cache.as_ref()?;
    if library_cache.package_source_base_url != package_source_base_url {
        return None;
    }
    let reduced = initialize_offline_packages(&OfflinePackagesInitInput {
        state: state.packages_state.clone(),
        now_epoch_ms,
        discovery_manifests: library_cache.discovery_manifests.clone(),
        bundle_manifests_by_filename: library_cache.bundle_manifests_by_filename.clone(),
        installed: installed.to_vec(),
        forced_gc_installed_filenames: forced_gc_installed_filenames.to_vec(),
        suppressed_fetch_filenames: state
            .suppressed_fetch_filename_messages
            .keys()
            .cloned()
            .collect(),
    });
    let ui_state = project_offline_packages_ui_state(
        &reduced.state,
        reduced.effective_now_epoch_ms,
        &library_cache.discovery_manifests,
        &library_cache.bundle_manifests_by_filename,
        installed,
        forced_gc_installed_filenames,
        &state
            .suppressed_fetch_filename_messages
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        state.sync_progress.as_ref(),
    );
    state.packages_state = Some(reduced.state);
    Some(ui_state)
}

fn project_offline_packages_controller_ui_state(
    state: &OfflinePackagesControllerState,
    package_source_base_url: &str,
    planner_ui_state: Option<OfflinePackagesUiState>,
    storage: Option<&OfflinePackagesStorageInfo>,
) -> OfflinePackagesControllerUiState {
    let library_loaded = state.library_cache.as_ref().is_some_and(|cache| {
        cache.package_source_base_url == package_source_base_url
            && !cache.discovery_manifests.is_empty()
    });
    let operation_in_flight = state.library_loading || state.sync_in_flight;
    let package_source_editable = !operation_in_flight;
    let refresh_enabled = !operation_in_flight;
    let sync_enabled =
        library_loaded && !operation_in_flight && state.library_error_message.is_none();
    let planner_interactions_enabled = !state.sync_in_flight;
    OfflinePackagesControllerUiState {
        planner_ui_state,
        library_loaded,
        library_loading: state.library_loading,
        library_error_message: state.library_error_message.clone(),
        library_status_message: format_offline_package_library_status_message(
            state,
            package_source_base_url,
        ),
        sync_in_flight: state.sync_in_flight,
        sync_message: state.sync_message.clone(),
        storage_capacity_label: format_offline_package_storage_capacity_label(storage),
        package_source_editable,
        package_source_edit_disabled_reason: (!package_source_editable).then(|| {
            "Wait for the current package operation to finish before changing the source."
                .to_string()
        }),
        refresh_enabled,
        refresh_disabled_reason: (!refresh_enabled).then(|| {
            if state.library_loading {
                "Refresh is already running.".to_string()
            } else {
                "Wait for sync to finish before refreshing.".to_string()
            }
        }),
        refresh_cancel_enabled: state.library_loading,
        sync_enabled,
        sync_disabled_reason: (!sync_enabled).then(|| {
            if state.sync_in_flight {
                "Sync is already running.".to_string()
            } else if state.library_loading {
                "Wait for refresh to finish before syncing.".to_string()
            } else if !library_loaded {
                "Refresh the package catalog before syncing.".to_string()
            } else if state.library_error_message.is_some() {
                "Refresh the package catalog successfully before syncing.".to_string()
            } else {
                "Sync is not available.".to_string()
            }
        }),
        sync_cancel_enabled: state.sync_in_flight,
        planner_interactions_enabled,
        planner_interactions_disabled_reason: (!planner_interactions_enabled)
            .then(|| "Wait for sync to finish before changing package selections.".to_string()),
    }
}

fn format_offline_package_library_status_message(
    state: &OfflinePackagesControllerState,
    package_source_base_url: &str,
) -> Option<String> {
    let error = state.library_error_message.as_ref()?;
    match state.library_cache.as_ref() {
        Some(cache) if cache.package_source_base_url == package_source_base_url
            && !cache.discovery_manifests.is_empty() =>
        {
            Some(format!(
                "Using cached package catalog from {}; refresh failed: {error}",
                format_epoch_ms_utc(cache.fetched_at_epoch_ms)
            ))
        }
        Some(cache) if cache.package_source_base_url != package_source_base_url => Some(format!(
            "Cached package catalog is for {}, but current package source is {}; refresh failed: {error}",
            cache.package_source_base_url, package_source_base_url
        )),
        Some(_) => Some(format!(
            "Cached package catalog is incomplete, so installed packages cannot be grouped; refresh failed: {error}"
        )),
        None => Some(format!(
            "No compatible package catalog is loaded, so installed packages cannot be grouped: {error}"
        )),
    }
}

fn format_offline_package_storage_capacity_label(
    storage: Option<&OfflinePackagesStorageInfo>,
) -> Option<String> {
    let storage = storage?;
    let available = format_package_size_label(storage.available_bytes);
    match storage.total_bytes.filter(|bytes| *bytes > 0) {
        Some(total_bytes) => Some(format!(
            "STORAGE {available} FREE / {} TOTAL",
            format_package_size_label(total_bytes)
        )),
        None => Some(format!("STORAGE {available} FREE")),
    }
}

fn format_offline_packages_sync_summary(summary: &OfflinePackagesSyncSummary) -> Option<String> {
    if summary.warnings.is_empty() {
        return None;
    }
    let core_warnings: Vec<_> = summary
        .warnings
        .iter()
        .filter(|warning| warning.region_id.is_none())
        .collect();
    let visible_warnings: Vec<_> = summary
        .warnings
        .iter()
        .filter(|warning| warning.region_id.is_some())
        .collect();
    let mut parts = Vec::new();
    if !visible_warnings.is_empty() {
        let details = visible_warnings
            .iter()
            .take(2)
            .map(|warning| format!("{}: {}", warning.artifact_id, warning.message))
            .collect::<Vec<_>>()
            .join(" | ");
        let more = visible_warnings
            .len()
            .saturating_sub(2)
            .checked_sub(0)
            .filter(|count| *count > 0)
            .map(|count| format!(" (+{} more)", count))
            .unwrap_or_default();
        parts.push(format!("{details}{more}"));
    }
    if !core_warnings.is_empty() {
        let core_ids = core_warnings
            .iter()
            .take(2)
            .map(|warning| warning.artifact_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let more = core_warnings
            .len()
            .saturating_sub(2)
            .checked_sub(0)
            .filter(|count| *count > 0)
            .map(|count| format!(" (+{} more)", count))
            .unwrap_or_default();
        parts.push(format!("core packages: {core_ids}{more}"));
    }
    Some(format!(
        "WARN {}: {}",
        summary.warnings.len(),
        parts.join(" | ")
    ))
}

pub fn plan_offline_packages(input: &PackageManagementInput) -> PackageManagementPlan {
    let available_artifacts: Vec<AvailablePackageArtifact> = input
        .bundle
        .packages
        .iter()
        .filter_map(bundle_package_to_artifact)
        .collect();

    let installed_by_filename: BTreeMap<String, &InstalledArtifact> = input
        .installed
        .iter()
        .map(|artifact| (artifact.filename.clone(), artifact))
        .collect();
    let forced_gc_filenames: BTreeSet<String> = input
        .forced_gc_installed_filenames
        .iter()
        .cloned()
        .collect();
    let effective_installed_by_filename: BTreeMap<String, &InstalledArtifact> =
        installed_by_filename
            .iter()
            .filter(|(filename, _)| !forced_gc_filenames.contains(*filename))
            .map(|(filename, artifact)| (filename.clone(), *artifact))
            .collect();
    let suppressed_fetch_filenames: BTreeSet<String> =
        input.suppressed_fetch_filenames.iter().cloned().collect();
    let effective_installed_filenames_for_artifact_id = |artifact_id: &str| -> Vec<String> {
        effective_installed_by_filename
            .values()
            .filter(|installed| installed.artifact_id == artifact_id)
            .map(|installed| installed.filename.clone())
            .collect()
    };
    let mut fetch = Vec::new();
    let mut fetch_set = BTreeSet::new();
    let mut retain_installed = BTreeSet::new();
    let mut protected_by_pause = BTreeSet::new();
    let mut slots_with_current_installed = BTreeSet::new();
    let mut stale_selected_installed = Vec::new();

    for artifact in &available_artifacts {
        let matching_installed_filenames =
            effective_installed_filenames_for_artifact_id(&artifact.artifact_id);
        let desired_filename_installed = matching_installed_filenames
            .iter()
            .any(|filename| filename == &artifact.filename);
        match artifact_policy(input, artifact) {
            ArtifactPolicy::Desired => {
                if desired_filename_installed {
                    retain_installed.insert(artifact.filename.clone());
                    slots_with_current_installed.insert(artifact_slot(artifact));
                } else if suppressed_fetch_filenames.contains(&artifact.filename) {
                    // Known-bad immutable remote artifact for this app run; do not requeue fetch.
                } else {
                    push_fetch_artifact(&mut fetch, &mut fetch_set, artifact);
                    retain_installed.extend(matching_installed_filenames);
                }
            }
            ArtifactPolicy::ProtectedByPause => {
                if desired_filename_installed {
                    retain_installed.insert(artifact.filename.clone());
                    protected_by_pause.insert(artifact.filename.clone());
                }
            }
            ArtifactPolicy::NotSelected => {
                if !matching_installed_filenames.is_empty()
                    && is_expired(input.now_epoch_ms, artifact)
                    && selected_state(input, artifact) == OfflinePackageSelection::Play
                {
                    stale_selected_installed.extend(
                        matching_installed_filenames
                            .into_iter()
                            .map(|filename| (artifact, filename)),
                    );
                }
            }
        }
    }

    for (artifact, filename) in stale_selected_installed {
        if !slots_with_current_installed.contains(&artifact_slot(artifact)) {
            retain_installed.insert(filename);
        }
    }

    let mut gc = BTreeSet::new();
    for (filename, installed) in &installed_by_filename {
        if retain_installed.contains(filename)
            || (fetch_set.contains(&installed.artifact_id)
                && !forced_gc_filenames.contains(filename))
        {
            continue;
        }
        gc.insert(filename.clone());
    }
    gc.extend(forced_gc_filenames);

    PackageManagementPlan {
        fetch,
        retain_installed: retain_installed.into_iter().collect(),
        gc: gc.into_iter().collect(),
        protected_by_pause: protected_by_pause.into_iter().collect(),
    }
}

fn push_fetch_artifact(
    fetch: &mut Vec<String>,
    fetch_set: &mut BTreeSet<String>,
    artifact: &AvailablePackageArtifact,
) {
    if !fetch_set.insert(artifact.artifact_id.clone()) {
        return;
    }
    let key = fetch_sort_key(artifact);
    let insert_at = fetch
        .binary_search_by(|existing_id| fetch_sort_key_for_id(existing_id).cmp(&key))
        .unwrap_or_else(|index| index);
    fetch.insert(insert_at, artifact.artifact_id.clone());
}

fn fetch_sort_key(artifact: &AvailablePackageArtifact) -> (u8, &str) {
    (
        fetch_product_priority(&artifact.product_id),
        artifact.artifact_id.as_str(),
    )
}

fn fetch_sort_key_for_id(artifact_id: &str) -> (u8, &str) {
    (
        fetch_product_priority_from_artifact_id(artifact_id),
        artifact_id,
    )
}

fn fetch_product_priority(product_id: &str) -> u8 {
    match product_id {
        "nav-db" => 0,
        "geo" => 1,
        _ => 2,
    }
}

fn fetch_product_priority_from_artifact_id(artifact_id: &str) -> u8 {
    let normalized = artifact_id.to_ascii_lowercase().replace('_', "-");
    if normalized.starts_with("nav-db-") {
        0
    } else if normalized.starts_with("geo-") {
        1
    } else {
        2
    }
}

fn project_offline_packages_ui_state(
    state: &OfflinePackagesState,
    now_epoch_ms: i64,
    discovery_manifests: &[CurrentArtifactsManifest],
    bundle_manifests_by_filename: &BTreeMap<String, BundleManifest>,
    installed: &[InstalledArtifact],
    forced_gc_installed_filenames: &[String],
    suppressed_fetch_filenames: &[String],
    sync_progress: Option<&OfflinePackagesSyncProgress>,
) -> OfflinePackagesUiState {
    let active_bundle = resolve_cycle_bundle_manifest(
        discovery_manifests,
        bundle_manifests_by_filename,
        now_epoch_ms,
    );
    let plan_input = PackageManagementInput {
        now_epoch_ms,
        preferences: state.preferences.clone(),
        bundle: active_bundle,
        installed: installed.to_vec(),
        forced_gc_installed_filenames: forced_gc_installed_filenames.to_vec(),
        suppressed_fetch_filenames: suppressed_fetch_filenames.to_vec(),
    };
    let plan = plan_offline_packages(&plan_input);
    project_offline_packages_ui_state_with_plan(
        state,
        now_epoch_ms,
        bundle_manifests_by_filename,
        discovery_manifests,
        sync_progress,
        &plan_input,
        &plan,
    )
}

#[allow(clippy::too_many_arguments)]
fn project_offline_packages_ui_state_with_plan(
    state: &OfflinePackagesState,
    now_epoch_ms: i64,
    bundle_manifests_by_filename: &BTreeMap<String, BundleManifest>,
    discovery_manifests: &[CurrentArtifactsManifest],
    sync_progress: Option<&OfflinePackagesSyncProgress>,
    plan_input: &PackageManagementInput,
    plan: &PackageManagementPlan,
) -> OfflinePackagesUiState {
    let rows = plan_rows_by_dimension(
        plan_input,
        plan,
        bundle_manifests_by_filename,
        sync_progress,
    );
    let (region_ids, product_ids) = offline_package_catalog_dimensions(
        discovery_manifests,
        bundle_manifests_by_filename,
        now_epoch_ms,
        Some(&rows),
    );
    let active_bundle = &plan_input.bundle;

    OfflinePackagesUiState {
        clock_label: clock_label(now_epoch_ms, state.now_override_epoch_ms),
        clock_options: clock_options(discovery_manifests, state.now_override_epoch_ms),
        all_packages: offline_packages_ui_row(
            "all-packages".to_string(),
            "All packages".to_string(),
            OfflinePackageSelection::Play,
            None,
            None,
            Some(&rows.all_packages),
        ),
        core_products: {
            let mut ids = BTreeSet::new();
            for pkg in &active_bundle.packages {
                if pkg.region_id.is_none() {
                    ids.insert(pkg.family_id.clone());
                }
            }
            ids.extend(rows.core_products.keys().cloned());
            ids.into_iter()
                .map(|id| {
                    let row = rows.core_products.get(&id);
                    offline_packages_ui_row(
                        id.clone(),
                        offline_core_product_label(&id),
                        OfflinePackageSelection::Play,
                        None,
                        offline_core_product_help_text(&id),
                        row,
                    )
                })
                .collect()
        },
        zoom_levels: {
            let mut zoom_levels = Vec::new();
            if active_bundle
                .packages
                .iter()
                .any(|pkg| pkg.region_id.as_deref() == Some(WIDE_COVERAGE_REGION_ID))
                || rows.zoom_levels.contains_key(WIDE_COVERAGE_REGION_ID)
            {
                let details = rows.zoom_levels.get(WIDE_COVERAGE_REGION_ID);
                zoom_levels.push(offline_packages_ui_row(
                    WIDE_COVERAGE_REGION_ID.to_string(),
                    "Wide all-region coverage".to_string(),
                    automatic_row_selection(details),
                    None,
                    Some(
                        "Automatically included for each selected product when any region is selected."
                            .to_string(),
                    ),
                    details,
                ));
            }
            if product_ids
                .iter()
                .any(|id| id == CHART_HIGH_RESOLUTION_PRODUCT_ID)
                || rows
                    .zoom_levels
                    .contains_key(CHART_HIGH_RESOLUTION_PRODUCT_ID)
            {
                zoom_levels.push(offline_packages_ui_row(
                    CHART_HIGH_RESOLUTION_PRODUCT_ID.to_string(),
                    "High resolution charts".to_string(),
                    state
                        .preferences
                        .products
                        .get(CHART_HIGH_RESOLUTION_PRODUCT_ID)
                        .copied()
                        .unwrap_or_else(|| {
                            default_product_selection(CHART_HIGH_RESOLUTION_PRODUCT_ID)
                        }),
                    Some(OfflinePackagesEvent::CycleProduct {
                        id: CHART_HIGH_RESOLUTION_PRODUCT_ID.to_string(),
                    }),
                    Some(
                        "Adds one more layer of tiles at the cost of downloading more data."
                            .to_string(),
                    ),
                    rows.zoom_levels.get(CHART_HIGH_RESOLUTION_PRODUCT_ID),
                ));
            }
            zoom_levels
        },
        regions: region_ids
            .iter()
            .map(|id| {
                offline_packages_ui_row(
                    id.clone(),
                    offline_region_label(id),
                    state
                        .preferences
                        .regions
                        .get(id)
                        .copied()
                        .unwrap_or(OfflinePackageSelection::Play),
                    Some(OfflinePackagesEvent::CycleRegion { id: id.clone() }),
                    Some(offline_region_help_text(id)),
                    rows.regions.get(id),
                )
            })
            .collect(),
        products: product_ids
            .iter()
            .filter(|id| id.as_str() != CHART_HIGH_RESOLUTION_PRODUCT_ID)
            .map(|id| {
                offline_packages_ui_row(
                    id.clone(),
                    offline_product_label(id),
                    state
                        .preferences
                        .products
                        .get(id)
                        .copied()
                        .unwrap_or_else(|| default_product_selection(id)),
                    Some(OfflinePackagesEvent::CycleProduct { id: id.clone() }),
                    Some(offline_product_help_text(id)),
                    rows.products.get(id),
                )
            })
            .collect(),
    }
}

fn effective_now_epoch_ms(state: &OfflinePackagesState, fallback_now_epoch_ms: i64) -> i64 {
    state.now_override_epoch_ms.unwrap_or(fallback_now_epoch_ms)
}

fn clock_label(now_epoch_ms: i64, override_epoch_ms: Option<i64>) -> String {
    match override_epoch_ms {
        Some(epoch_ms) => format!("CLOCK {}", format_epoch_ms_utc(epoch_ms)),
        None => format!("CLOCK NOW ({})", format_epoch_ms_utc(now_epoch_ms)),
    }
}

fn offline_package_catalog_dimensions(
    discovery_manifests: &[CurrentArtifactsManifest],
    bundle_manifests_by_filename: &BTreeMap<String, BundleManifest>,
    now_epoch_ms: i64,
    rows: Option<&PlanRowsByDimension>,
) -> (Vec<String>, Vec<String>) {
    let active_bundle = resolve_cycle_bundle_manifest(
        discovery_manifests,
        bundle_manifests_by_filename,
        now_epoch_ms,
    );
    let mut region_ids = BTreeSet::new();
    let mut product_ids = BTreeSet::new();

    for bundle in bundle_manifests_by_filename
        .values()
        .chain(std::iter::once(&active_bundle))
    {
        for pkg in &bundle.packages {
            if let Some(region_id) = pkg.region_id.as_deref() {
                if region_id != WIDE_COVERAGE_REGION_ID {
                    region_ids.insert(region_id.to_string());
                }
                product_ids.insert(pkg.family_id.clone());
                if chart_package_is_detail(pkg) {
                    product_ids.insert(CHART_HIGH_RESOLUTION_PRODUCT_ID.to_string());
                }
            }
        }
    }

    if let Some(rows) = rows {
        region_ids.extend(rows.regions.keys().cloned());
        product_ids.extend(rows.products.keys().cloned());
    }

    let mut region_ids: Vec<String> = region_ids.into_iter().collect();
    region_ids.sort_by_key(|id| (offline_region_sort_order(id), id.clone()));
    let mut product_ids: Vec<String> = product_ids.into_iter().collect();
    product_ids.sort_by_key(|id| (offline_product_sort_order(id), id.clone()));
    (region_ids, product_ids)
}

fn offline_region_label(id: &str) -> String {
    match id {
        "ak" => "Alaska",
        "ec" => "East Central",
        "nc" => "North Central",
        "ne" => "Northeast",
        "nw" => "Northwest",
        "pac" => "Pacific",
        "sc" => "South Central",
        "se" => "Southeast",
        "sw" => "Southwest",
        "world" => "World",
        other => return fallback_dimension_label(other),
    }
    .to_string()
}

fn offline_region_sort_order(id: &str) -> usize {
    match id {
        "ak" => 0,
        "ec" => 1,
        "nc" => 2,
        "ne" => 3,
        "nw" => 4,
        "pac" => 5,
        "sc" => 6,
        "se" => 7,
        "sw" => 8,
        "world" => 9,
        _ => usize::MAX,
    }
}

fn offline_product_label(id: &str) -> String {
    match id {
        "sec" => "Sectional",
        "tac" => "TAC",
        "shaded-relief" => "Shaded Relief",
        "terrain" => "Terrain",
        "enr-l" => "IFR-L",
        "enr-h" => "IFR-H",
        "tpp" => "TPP",
        "csup" => "CSUP",
        CHART_HIGH_RESOLUTION_PRODUCT_ID => "High Resolution",
        other => return fallback_dimension_label(other),
    }
    .to_string()
}

fn offline_region_help_text(id: &str) -> String {
    format!(
        "Select {} region products. Enable Offline Regions to display coverage areas.",
        offline_region_label(id)
    )
}

fn offline_product_help_text(id: &str) -> String {
    match id {
        "sec" => "VFR Sectional Aeronautical Charts.".to_string(),
        "tac" => "VFR Terminal Area Charts for major metropolitan areas.".to_string(),
        "shaded-relief" => {
            "Uncluttered base maps useful for vector-only and track-up displays.".to_string()
        }
        "terrain" => "Terrain data, used for the terrain warning layer, SPOT altitude measurements, and ownship AGL."
            .to_string(),
        "enr-l" => "Low-altitude IFR enroute charts.".to_string(),
        "enr-h" => "High-altitude IFR enroute charts.".to_string(),
        "tpp" =>
            "Approach plates, airport diagrams, departure and arrival procedures, and terminal minima."
                .to_string(),
        "csup" => "Chart Supplement airport and facility directory pages.".to_string(),
        _ => format!("{} packages.", offline_product_label(id)),
    }
}

fn offline_product_sort_order(id: &str) -> usize {
    match id {
        "sec" => 0,
        "tac" => 1,
        "shaded-relief" => 2,
        "terrain" => 3,
        "enr-l" => 4,
        "enr-h" => 5,
        "tpp" => 6,
        "csup" => 7,
        CHART_HIGH_RESOLUTION_PRODUCT_ID => 8,
        _ => usize::MAX,
    }
}

fn offline_core_product_label(id: &str) -> String {
    match id {
        "nav-db" => "NAV DB",
        "world-basemap" => "WORLD BASEMAP",
        "vectors" => "VECTORS",
        "geo" => "GEO",
        "terrain" => "TERRAIN",
        other => return fallback_dimension_label(other),
    }
    .to_string()
}

fn offline_core_product_help_text(id: &str) -> Option<String> {
    match id {
        "nav-db" => Some(
            "Required navigation data for airports, fixes, airways, and procedures.".to_string(),
        ),
        "world-basemap" => Some("Lightweight base map for the entire globe.".to_string()),
        _ => None,
    }
}

fn fallback_dimension_label(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join(" ")
}

fn clock_options(
    discovery_manifests: &[CurrentArtifactsManifest],
    override_epoch_ms: Option<i64>,
) -> Vec<OfflinePackagesClockOption> {
    let mut options = vec![OfflinePackagesClockOption {
        id: "system".to_string(),
        label: "NOW".to_string(),
        active: override_epoch_ms.is_none(),
    }];
    let mut seen = BTreeSet::new();
    for manifest in discovery_manifests {
        let Some(as_of_utc) = manifest.as_of_utc.as_deref() else {
            continue;
        };
        let Some(epoch_ms) = as_of_utc_to_epoch_ms(as_of_utc) else {
            continue;
        };
        if !seen.insert(epoch_ms) {
            continue;
        }
        options.push(OfflinePackagesClockOption {
            id: epoch_ms.to_string(),
            label: manifest
                .as_of_date
                .clone()
                .unwrap_or_else(|| format_epoch_ms_utc(epoch_ms)),
            active: override_epoch_ms == Some(epoch_ms),
        });
    }
    options.sort_by(|a, b| {
        if a.id == "system" {
            return std::cmp::Ordering::Less;
        }
        if b.id == "system" {
            return std::cmp::Ordering::Greater;
        }
        a.id.cmp(&b.id)
    });
    if options.iter().all(|option| !option.active) {
        if let Some(system) = options.first_mut() {
            system.active = override_epoch_ms.is_none();
        }
    }
    options
}

#[derive(Default)]
struct DimensionPlanDetails {
    fetch_count: usize,
    gc_count: usize,
    pause_count: usize,
    installed_size_bytes: u64,
    planned_download_bytes: u64,
    planned_gc_bytes: u64,
    sync_loaded_bytes: u64,
    sync_total_bytes: u64,
    plan_groups: BTreeMap<OfflinePackagesUiPlanAction, PlanEntryAccumulator>,
}

#[derive(Default)]
struct PlanEntryAccumulator {
    count: usize,
    cycles: BTreeSet<String>,
}

#[derive(Default)]
struct PlanRowsByDimension {
    all_packages: DimensionPlanDetails,
    core_products: BTreeMap<String, DimensionPlanDetails>,
    zoom_levels: BTreeMap<String, DimensionPlanDetails>,
    regions: BTreeMap<String, DimensionPlanDetails>,
    products: BTreeMap<String, DimensionPlanDetails>,
}

fn offline_packages_ui_row(
    id: String,
    label: String,
    selection: OfflinePackageSelection,
    selection_event: Option<OfflinePackagesEvent>,
    help_text: Option<String>,
    details: Option<&DimensionPlanDetails>,
) -> OfflinePackagesUiRow {
    let plan_entries = details.map_or_else(Vec::new, |details| {
        [
            OfflinePackagesUiPlanAction::Delete,
            OfflinePackagesUiPlanAction::Keep,
            OfflinePackagesUiPlanAction::Pause,
            OfflinePackagesUiPlanAction::Fetch,
        ]
        .into_iter()
        .filter_map(|action| {
            let group = details.plan_groups.get(&action)?;
            if group.count == 0 {
                return None;
            }
            Some(OfflinePackagesUiPlanEntry {
                action,
                count: group.count,
                cycles: group.cycles.iter().cloned().collect(),
            })
        })
        .collect()
    });
    let installed_size_bytes = details.map_or(0, |details| details.installed_size_bytes);
    let planned_download_bytes = details.map_or(0, |details| details.planned_download_bytes);
    let planned_gc_bytes = details.map_or(0, |details| details.planned_gc_bytes);
    let planned_delta_bytes = planned_download_bytes as i128 - planned_gc_bytes as i128;
    let planned_total_size_bytes = installed_size_bytes
        .saturating_add(planned_download_bytes)
        .saturating_sub(planned_gc_bytes);
    let sync_progress_per_mille = details.and_then(|details| {
        if details.sync_total_bytes == 0 {
            None
        } else {
            Some(
                ((details.sync_loaded_bytes.min(details.sync_total_bytes) * 1000)
                    / details.sync_total_bytes) as u16,
            )
        }
    });
    OfflinePackagesUiRow {
        id,
        label,
        selection,
        selection_event,
        help_text,
        fetch_count: details.map_or(0, |details| details.fetch_count),
        gc_count: details.map_or(0, |details| details.gc_count),
        pause_count: details.map_or(0, |details| details.pause_count),
        plan_entries,
        installed_size_label: format_package_size_label(installed_size_bytes),
        planned_change_label: format_package_change_label(
            planned_download_bytes,
            planned_delta_bytes,
        ),
        planned_total_size_label: format_package_size_label(planned_total_size_bytes),
        planned_size_change_visible: planned_download_bytes != 0 || planned_delta_bytes != 0,
        sync_progress_per_mille,
    }
}

fn automatic_row_selection(details: Option<&DimensionPlanDetails>) -> OfflinePackageSelection {
    let Some(details) = details else {
        return OfflinePackageSelection::Unselected;
    };
    if details.plan_groups.keys().any(|action| {
        matches!(
            action,
            OfflinePackagesUiPlanAction::Keep | OfflinePackagesUiPlanAction::Fetch
        )
    }) {
        OfflinePackageSelection::Play
    } else if details.pause_count > 0 {
        OfflinePackageSelection::Pause
    } else {
        OfflinePackageSelection::Unselected
    }
}

fn plan_rows_by_dimension(
    input: &PackageManagementInput,
    plan: &PackageManagementPlan,
    bundle_manifests_by_filename: &BTreeMap<String, BundleManifest>,
    sync_progress: Option<&OfflinePackagesSyncProgress>,
) -> PlanRowsByDimension {
    let mut rows = PlanRowsByDimension::default();
    let packages_by_id: BTreeMap<&str, &BundlePackageArtifact> = bundle_manifests_by_filename
        .values()
        .flat_map(|bundle| bundle.packages.iter())
        .chain(input.bundle.packages.iter())
        .map(|pkg| (pkg.id.as_str(), pkg))
        .collect();
    let active_packages_by_id: BTreeMap<&str, &BundlePackageArtifact> = input
        .bundle
        .packages
        .iter()
        .map(|pkg| (pkg.id.as_str(), pkg))
        .collect();
    let installed_by_filename: BTreeMap<&str, &InstalledArtifact> = input
        .installed
        .iter()
        .map(|artifact| (artifact.filename.as_str(), artifact))
        .collect();
    let mut active_package_by_filename: BTreeMap<&str, &BundlePackageArtifact> = BTreeMap::new();
    for pkg in &input.bundle.packages {
        active_package_by_filename.insert(pkg.filename.as_str(), pkg);
    }

    for installed in &input.installed {
        let pkg = packages_by_id.get(installed.artifact_id.as_str()).copied();
        apply_installed_size(&mut rows, pkg, installed);
    }

    for artifact_id in &plan.fetch {
        let Some(pkg) = active_packages_by_id.get(artifact_id.as_str()).copied() else {
            continue;
        };
        apply_plan_action(&mut rows, pkg, OfflinePackagesUiPlanAction::Fetch, 1);
        let size = package_size_bytes(pkg, None);
        apply_size(&mut rows, pkg, |details| {
            details.fetch_count += 1;
            details.planned_download_bytes = details.planned_download_bytes.saturating_add(size);
        });
    }

    apply_sync_progress(&mut rows, &packages_by_id, plan, sync_progress);

    for filename in &plan.gc {
        let Some(installed) = installed_by_filename.get(filename.as_str()).copied() else {
            continue;
        };
        let pkg = packages_by_id.get(installed.artifact_id.as_str()).copied();
        let size = package_size_bytes_opt(pkg, Some(installed));
        apply_installed_action(
            &mut rows,
            pkg,
            installed,
            OfflinePackagesUiPlanAction::Delete,
        );
        apply_size_opt(&mut rows, pkg, installed, |details| {
            details.gc_count += 1;
            details.planned_gc_bytes = details.planned_gc_bytes.saturating_add(size);
        });
    }

    let protected_by_pause: BTreeSet<&str> =
        plan.protected_by_pause.iter().map(String::as_str).collect();
    for filename in &plan.retain_installed {
        if protected_by_pause.contains(filename.as_str()) {
            continue;
        }
        let Some(installed) = installed_by_filename.get(filename.as_str()).copied() else {
            continue;
        };
        let pkg = packages_by_id.get(installed.artifact_id.as_str()).copied();
        apply_installed_action(&mut rows, pkg, installed, OfflinePackagesUiPlanAction::Keep);
    }

    for filename in &plan.protected_by_pause {
        let Some(installed) = installed_by_filename.get(filename.as_str()).copied() else {
            continue;
        };
        let pkg = packages_by_id.get(installed.artifact_id.as_str()).copied();
        apply_installed_action(
            &mut rows,
            pkg,
            installed,
            OfflinePackagesUiPlanAction::Pause,
        );
        apply_size_opt(&mut rows, pkg, installed, |details| {
            details.pause_count += 1
        });
    }

    for pkg in &input.bundle.packages {
        let Some(artifact) = bundle_package_to_artifact(pkg) else {
            continue;
        };
        if artifact_policy(input, &artifact) != ArtifactPolicy::ProtectedByPause {
            continue;
        }
        if active_package_by_filename.contains_key(pkg.filename.as_str())
            && input
                .installed
                .iter()
                .any(|installed| installed.filename == pkg.filename)
        {
            continue;
        }
        apply_plan_action(&mut rows, pkg, OfflinePackagesUiPlanAction::Pause, 1);
        apply_size(&mut rows, pkg, |details| details.pause_count += 1);
    }

    rows
}

pub fn installed_artifact_metadata_updates(
    bundle_manifests_by_filename: &BTreeMap<String, BundleManifest>,
    installed: &[InstalledArtifact],
) -> Vec<InstalledArtifactMetadataUpdate> {
    let packages_by_id: BTreeMap<&str, &BundlePackageArtifact> = bundle_manifests_by_filename
        .values()
        .flat_map(|bundle| bundle.packages.iter())
        .map(|pkg| (pkg.id.as_str(), pkg))
        .collect();
    let mut updates = installed
        .iter()
        .filter_map(|artifact| {
            let pkg = packages_by_id.get(artifact.artifact_id.as_str()).copied()?;
            let chart_package_tier = pkg
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.chart_package_tier);
            if artifact.family_id.as_deref() == Some(pkg.family_id.as_str())
                && artifact.region_id == pkg.region_id
                && artifact.chart_package_tier == chart_package_tier
            {
                return None;
            }
            Some(InstalledArtifactMetadataUpdate {
                artifact_id: artifact.artifact_id.clone(),
                filename: artifact.filename.clone(),
                family_id: pkg.family_id.clone(),
                region_id: pkg.region_id.clone(),
                chart_package_tier,
            })
        })
        .collect::<Vec<_>>();
    updates.sort_by(|left, right| {
        left.artifact_id
            .cmp(&right.artifact_id)
            .then_with(|| left.filename.cmp(&right.filename))
    });
    updates
}

fn apply_sync_progress(
    rows: &mut PlanRowsByDimension,
    packages_by_id: &BTreeMap<&str, &BundlePackageArtifact>,
    plan: &PackageManagementPlan,
    sync_progress: Option<&OfflinePackagesSyncProgress>,
) {
    let Some(progress) = sync_progress else {
        return;
    };
    let planned_fetch_artifact_ids: Vec<&str> = if progress.planned_fetch_artifact_ids.is_empty() {
        plan.fetch.iter().map(String::as_str).collect()
    } else {
        progress
            .planned_fetch_artifact_ids
            .iter()
            .map(String::as_str)
            .collect()
    };
    for artifact_id in planned_fetch_artifact_ids {
        let Some(pkg) = packages_by_id.get(artifact_id).copied() else {
            continue;
        };
        let size = package_size_bytes(pkg, None);
        let loaded = sync_progress_loaded_bytes(Some(progress), artifact_id, size);
        apply_size(rows, pkg, |details| {
            details.sync_total_bytes = details.sync_total_bytes.saturating_add(size);
            details.sync_loaded_bytes = details.sync_loaded_bytes.saturating_add(loaded);
        });
    }
}

fn apply_installed_size(
    rows: &mut PlanRowsByDimension,
    pkg: Option<&BundlePackageArtifact>,
    installed: &InstalledArtifact,
) {
    let size = package_size_bytes_opt(pkg, Some(installed));
    apply_size_opt(rows, pkg, installed, |details| {
        details.installed_size_bytes = details.installed_size_bytes.saturating_add(size);
    });
}

fn apply_installed_action(
    rows: &mut PlanRowsByDimension,
    pkg: Option<&BundlePackageArtifact>,
    installed: &InstalledArtifact,
    action: OfflinePackagesUiPlanAction,
) {
    let cycle = pkg
        .and_then(|pkg| pkg.cycle.clone())
        .unwrap_or_else(|| cycle_from_artifact_id(&installed.artifact_id));
    apply_to_package_dimensions_opt(rows, pkg, installed, |details| {
        add_plan_group(details, action, cycle.clone());
    });
}

fn apply_plan_action(
    rows: &mut PlanRowsByDimension,
    pkg: &BundlePackageArtifact,
    action: OfflinePackagesUiPlanAction,
    count: usize,
) {
    let cycle = package_cycle_label(pkg);
    apply_size(rows, pkg, |details| {
        let group = details.plan_groups.entry(action).or_default();
        group.count += count;
        group.cycles.insert(cycle.clone());
    });
}

fn add_plan_group(
    details: &mut DimensionPlanDetails,
    action: OfflinePackagesUiPlanAction,
    cycle: String,
) {
    let group = details.plan_groups.entry(action).or_default();
    group.count += 1;
    group.cycles.insert(cycle);
}

fn apply_size(
    rows: &mut PlanRowsByDimension,
    pkg: &BundlePackageArtifact,
    mutate: impl Fn(&mut DimensionPlanDetails),
) {
    apply_to_package_dimensions(rows, pkg, mutate);
}

fn apply_size_opt(
    rows: &mut PlanRowsByDimension,
    pkg: Option<&BundlePackageArtifact>,
    installed: &InstalledArtifact,
    mutate: impl Fn(&mut DimensionPlanDetails),
) {
    apply_to_package_dimensions_opt(rows, pkg, installed, mutate);
}

fn apply_to_package_dimensions(
    rows: &mut PlanRowsByDimension,
    pkg: &BundlePackageArtifact,
    mutate: impl Fn(&mut DimensionPlanDetails),
) {
    apply_to_package_grouping_dimensions(
        rows,
        &pkg.family_id,
        pkg.region_id.as_deref(),
        pkg.metadata
            .as_ref()
            .and_then(|metadata| metadata.chart_package_tier),
        mutate,
    );
}

fn apply_to_package_grouping_dimensions(
    rows: &mut PlanRowsByDimension,
    family_id: &str,
    region_id: Option<&str>,
    chart_package_tier: Option<product_contracts::ChartPackageTier>,
    mutate: impl Fn(&mut DimensionPlanDetails),
) {
    mutate(&mut rows.all_packages);
    if let Some(region_id) = region_id {
        if region_id == WIDE_COVERAGE_REGION_ID {
            mutate(
                rows.zoom_levels
                    .entry(WIDE_COVERAGE_REGION_ID.to_string())
                    .or_default(),
            );
        } else {
            mutate(rows.regions.entry(region_id.to_string()).or_default());
        }
        mutate(rows.products.entry(family_id.to_string()).or_default());
    } else {
        mutate(rows.core_products.entry(family_id.to_string()).or_default());
    }
    if chart_package_tier == Some(product_contracts::ChartPackageTier::Detail) {
        mutate(
            rows.zoom_levels
                .entry(CHART_HIGH_RESOLUTION_PRODUCT_ID.to_string())
                .or_default(),
        );
    }
}

fn apply_to_package_dimensions_opt(
    rows: &mut PlanRowsByDimension,
    pkg: Option<&BundlePackageArtifact>,
    installed: &InstalledArtifact,
    mutate: impl Fn(&mut DimensionPlanDetails),
) {
    if let Some(pkg) = pkg {
        apply_to_package_dimensions(rows, pkg, mutate);
        return;
    }
    if let Some(family_id) = installed.family_id.as_deref() {
        apply_to_package_grouping_dimensions(
            rows,
            family_id,
            installed.region_id.as_deref(),
            installed.chart_package_tier,
            mutate,
        );
        return;
    }
    mutate(&mut rows.all_packages);
    mutate(
        rows.products
            .entry(installed.artifact_id.clone())
            .or_default(),
    );
}

fn sync_progress_loaded_bytes(
    progress: Option<&OfflinePackagesSyncProgress>,
    artifact_id: &str,
    size: u64,
) -> u64 {
    let Some(progress) = progress else {
        return 0;
    };
    if progress.completed_fetch_artifact_ids.contains(artifact_id) {
        return size;
    }
    progress
        .active_fetch_bytes_by_artifact_id
        .get(artifact_id)
        .copied()
        .unwrap_or(0)
        .min(size)
}

fn package_size_bytes(pkg: &BundlePackageArtifact, installed: Option<&InstalledArtifact>) -> u64 {
    installed
        .and_then(|installed| installed.size_bytes)
        .or(pkg.size_bytes)
        .unwrap_or(0)
}

fn package_size_bytes_opt(
    pkg: Option<&BundlePackageArtifact>,
    installed: Option<&InstalledArtifact>,
) -> u64 {
    match pkg {
        Some(pkg) => package_size_bytes(pkg, installed),
        None => installed
            .and_then(|installed| installed.size_bytes)
            .unwrap_or(0),
    }
}

fn package_cycle_label(pkg: &BundlePackageArtifact) -> String {
    pkg.cycle.clone().unwrap_or_else(|| "static".to_string())
}

fn cycle_from_artifact_id(artifact_id: &str) -> String {
    artifact_id
        .split('_')
        .find(|part| part.len() == 4 && part.chars().all(|ch| ch.is_ascii_digit()))
        .unwrap_or("static")
        .to_string()
}

fn format_package_size_label(bytes: u64) -> String {
    if bytes == 0 {
        return "0M".to_string();
    }
    let abs_bytes = bytes as f64;
    let (value, suffix) = if bytes >= 1_000_000_000 {
        (abs_bytes / 1_000_000_000.0, "G")
    } else {
        (abs_bytes / 1_000_000.0, "M")
    };
    let value = round_to_sigfigs(value, 2);
    format!(
        "{value:.precision$}{suffix}",
        precision = size_label_precision(value)
    )
}

fn format_signed_package_size_label(bytes: i128) -> String {
    let sign = if bytes >= 0 { "+" } else { "-" };
    let magnitude = bytes.unsigned_abs().min(u64::MAX as u128) as u64;
    format!("{sign}{}", format_package_size_label(magnitude))
}

fn format_package_change_label(planned_download_bytes: u64, planned_delta_bytes: i128) -> String {
    let delta = format_signed_package_size_label(planned_delta_bytes);
    if planned_download_bytes == 0 {
        delta
    } else {
        format!(
            "⤓{} {delta}",
            format_package_size_label(planned_download_bytes)
        )
    }
}

fn size_label_precision(value: f64) -> usize {
    if value >= 10.0 {
        0
    } else if value >= 1.0 {
        1
    } else {
        2
    }
}

fn round_to_sigfigs(value: f64, sigfigs: i32) -> f64 {
    if value == 0.0 {
        return 0.0;
    }
    let scale = 10_f64.powi(sigfigs - 1 - value.abs().log10().floor() as i32);
    (value * scale).round() / scale
}

fn normalize_preferences(
    source: Option<&OfflinePackagePreferences>,
    region_ids: &[String],
    product_ids: &[String],
) -> OfflinePackagePreferences {
    let source = source.cloned().unwrap_or_default();
    OfflinePackagePreferences {
        regions: region_ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    source
                        .regions
                        .get(id)
                        .copied()
                        .unwrap_or(OfflinePackageSelection::Play),
                )
            })
            .collect(),
        products: product_ids
            .iter()
            .map(|id| {
                let migrated = if id == "tpp" || id == "csup" {
                    source.products.get("plates").copied()
                } else {
                    None
                };
                (
                    id.clone(),
                    source
                        .products
                        .get(id)
                        .copied()
                        .or(migrated)
                        .unwrap_or_else(|| default_product_selection(id)),
                )
            })
            .collect(),
    }
}

fn default_product_selection(id: &str) -> OfflinePackageSelection {
    if id == CHART_HIGH_RESOLUTION_PRODUCT_ID {
        OfflinePackageSelection::Unselected
    } else {
        OfflinePackageSelection::Play
    }
}

fn resolve_cycle_bundle_manifest(
    discovery_manifests: &[CurrentArtifactsManifest],
    bundle_manifests_by_filename: &BTreeMap<String, BundleManifest>,
    now_epoch_ms: i64,
) -> BundleManifest {
    let discovery = discovery_manifests
        .iter()
        .filter_map(|manifest| {
            Some((
                as_of_utc_to_epoch_ms(manifest.as_of_utc.as_deref()?)?,
                manifest,
            ))
        })
        .filter(|(as_of_epoch_ms, _)| *as_of_epoch_ms <= now_epoch_ms)
        .max_by_key(|(as_of_epoch_ms, _)| *as_of_epoch_ms)
        .map(|(_, manifest)| manifest)
        .or_else(|| {
            discovery_manifests
                .iter()
                .max_by_key(|manifest| manifest.as_of_utc.as_deref())
        })
        .unwrap_or_else(|| panic!("no discovery manifests available for offline packages"));

    let mut merged_packages = Vec::new();
    let mut seen_ids = BTreeSet::new();
    for bundle_ref in discovery
        .bundles
        .iter()
        .filter(|bundle_ref| bundle_ref.bundle_type == "cycle")
    {
        let bundle = bundle_manifests_by_filename
            .get(&bundle_ref.filename)
            .unwrap_or_else(|| panic!("missing bundle manifest {}", bundle_ref.filename));
        for pkg in &bundle.packages {
            if seen_ids.insert(pkg.id.clone()) {
                merged_packages.push(pkg.clone());
            }
        }
    }
    BundleManifest {
        packages: merged_packages,
    }
}

fn cycle_selection(selections: &mut BTreeMap<String, OfflinePackageSelection>, id: &str) {
    let next = match selections
        .get(id)
        .copied()
        .unwrap_or(OfflinePackageSelection::Play)
    {
        OfflinePackageSelection::Play => OfflinePackageSelection::Pause,
        OfflinePackageSelection::Pause => OfflinePackageSelection::Unselected,
        OfflinePackageSelection::Unselected => OfflinePackageSelection::Play,
    };
    selections.insert(id.to_string(), next);
}

fn bundle_package_to_artifact(pkg: &BundlePackageArtifact) -> Option<AvailablePackageArtifact> {
    if !package_contract_is_supported(pkg) {
        return None;
    }
    match pkg.family_id.as_str() {
        "sec" | "tac" | "shaded-relief" | "enr-l" | "enr-h" | "tpp" | "csup" | "nav-db" | "geo"
        | "terrain" | "world-basemap" => Some(AvailablePackageArtifact {
            artifact_id: pkg.id.clone(),
            filename: pkg.filename.clone(),
            product_id: pkg.family_id.clone(),
            region_id: pkg.region_id.clone(),
            chart_package_tier: pkg
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.chart_package_tier),
            effective_at_epoch_ms: pkg.effective_date.as_deref().and_then(ymd_date_to_epoch_ms),
            expires_at_epoch_ms: pkg
                .expiration_date
                .as_deref()
                .and_then(ymd_date_to_epoch_ms),
        }),
        _ => None,
    }
}

fn chart_package_is_detail(pkg: &BundlePackageArtifact) -> bool {
    pkg.metadata
        .as_ref()
        .and_then(|metadata| metadata.chart_package_tier)
        == Some(product_contracts::ChartPackageTier::Detail)
}

fn ymd_date_to_epoch_ms(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400_000)
}

fn as_of_utc_to_epoch_ms(value: &str) -> Option<i64> {
    let trimmed = value.strip_suffix('Z')?;
    let (date, time) = trimmed.split_once('T')?;
    let base = ymd_date_to_epoch_ms(date)?;
    let mut parts = time.split(':');
    let hour = parts.next()?.parse::<i64>().ok()?;
    let minute = parts.next()?.parse::<i64>().ok()?;
    let second = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(base + (((hour * 60 + minute) * 60 + second) * 1000))
}

fn format_epoch_ms_utc(epoch_ms: i64) -> String {
    let seconds = epoch_ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}Z")
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as i64
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = (yoe as i32) + (era as i32) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    (year, month as u32, day as u32)
}

fn artifact_slot(
    artifact: &AvailablePackageArtifact,
) -> (
    String,
    Option<String>,
    Option<product_contracts::ChartPackageTier>,
) {
    (
        artifact.product_id.clone(),
        artifact.region_id.clone(),
        artifact.chart_package_tier,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactPolicy {
    Desired,
    ProtectedByPause,
    NotSelected,
}

fn artifact_policy(
    input: &PackageManagementInput,
    artifact: &AvailablePackageArtifact,
) -> ArtifactPolicy {
    if is_expired(input.now_epoch_ms, artifact) {
        return match selected_state(input, artifact) {
            OfflinePackageSelection::Pause => ArtifactPolicy::ProtectedByPause,
            OfflinePackageSelection::Play | OfflinePackageSelection::Unselected => {
                ArtifactPolicy::NotSelected
            }
        };
    }

    match selected_state(input, artifact) {
        OfflinePackageSelection::Play => ArtifactPolicy::Desired,
        OfflinePackageSelection::Pause => ArtifactPolicy::ProtectedByPause,
        OfflinePackageSelection::Unselected => ArtifactPolicy::NotSelected,
    }
}

fn selected_state(
    input: &PackageManagementInput,
    artifact: &AvailablePackageArtifact,
) -> OfflinePackageSelection {
    if artifact.region_id.as_deref() == Some(WIDE_COVERAGE_REGION_ID) {
        return wide_angle_selected_state(input, artifact);
    }
    if artifact.region_id.is_none() {
        return input
            .preferences
            .products
            .get(&artifact.product_id)
            .copied()
            .unwrap_or(OfflinePackageSelection::Play);
    }

    let product = input
        .preferences
        .products
        .get(&artifact.product_id)
        .copied()
        .unwrap_or(OfflinePackageSelection::Unselected);
    let region = artifact
        .region_id
        .as_ref()
        .and_then(|region_id| input.preferences.regions.get(region_id))
        .copied()
        .unwrap_or(OfflinePackageSelection::Unselected);
    let high_resolution =
        if artifact.chart_package_tier == Some(product_contracts::ChartPackageTier::Detail) {
            input
                .preferences
                .products
                .get(CHART_HIGH_RESOLUTION_PRODUCT_ID)
                .copied()
                .unwrap_or(OfflinePackageSelection::Unselected)
        } else {
            OfflinePackageSelection::Play
        };

    if [region, product, high_resolution].contains(&OfflinePackageSelection::Unselected) {
        OfflinePackageSelection::Unselected
    } else if [region, product, high_resolution].contains(&OfflinePackageSelection::Pause) {
        OfflinePackageSelection::Pause
    } else {
        OfflinePackageSelection::Play
    }
}

fn wide_angle_selected_state(
    input: &PackageManagementInput,
    artifact: &AvailablePackageArtifact,
) -> OfflinePackageSelection {
    let product = input
        .preferences
        .products
        .get(&artifact.product_id)
        .copied()
        .unwrap_or(OfflinePackageSelection::Unselected);
    if product == OfflinePackageSelection::Unselected {
        return OfflinePackageSelection::Unselected;
    }

    let mut saw_region = false;
    let mut saw_play = false;
    let mut saw_pause = false;
    for (region_id, selection) in &input.preferences.regions {
        if artifact.region_id.as_deref() == Some(region_id.as_str()) {
            continue;
        }
        saw_region = true;
        match selection {
            OfflinePackageSelection::Play => saw_play = true,
            OfflinePackageSelection::Pause => saw_pause = true,
            OfflinePackageSelection::Unselected => {}
        }
    }
    if !saw_region {
        return OfflinePackageSelection::Unselected;
    }
    match product {
        OfflinePackageSelection::Pause => OfflinePackageSelection::Pause,
        OfflinePackageSelection::Play if saw_play => OfflinePackageSelection::Play,
        OfflinePackageSelection::Play if saw_pause => OfflinePackageSelection::Pause,
        _ => OfflinePackageSelection::Unselected,
    }
}

fn is_expired(now_epoch_ms: i64, artifact: &AvailablePackageArtifact) -> bool {
    artifact
        .expires_at_epoch_ms
        .is_some_and(|expires| expires <= now_epoch_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(
        id: &str,
        product: &str,
        region: Option<&str>,
        effective: Option<&str>,
        expires: Option<&str>,
    ) -> BundlePackageArtifact {
        BundlePackageArtifact {
            id: id.to_string(),
            family_id: product.to_string(),
            contract_id: required_package_contract_id(product)
                .unwrap_or("UNSUPPORTED")
                .to_string(),
            region_id: region.map(str::to_string),
            filename: format!("{id}.zip"),
            relative_path: format!("{id}.zip"),
            cycle: Some("2604".to_string()),
            cycle_version: Some("01".to_string()),
            checksum_sha256: None,
            size_bytes: None,
            effective_date: effective.map(str::to_string),
            expiration_date: expires.map(str::to_string),
            warning_text: None,
            metadata: (product == "nav-db").then(nav_db_metadata),
        }
    }

    fn nav_db_metadata() -> BundlePackageMetadata {
        BundlePackageMetadata {
            chart_package_tier: None,
            full_coverage_zoom: None,
            wide_angle_region_id: None,
            wide_angle_max_zoom: None,
            wide_angle: None,
            min_source_zoom: None,
            max_source_zoom: None,
            tile_count: None,
        }
    }

    fn wide_pkg(
        id: &str,
        product: &str,
        region: &str,
        expires: Option<&str>,
    ) -> BundlePackageArtifact {
        let mut pkg = pkg(id, product, Some(region), None, expires);
        pkg.metadata = Some(BundlePackageMetadata {
            chart_package_tier: None,
            full_coverage_zoom: None,
            wide_angle_region_id: Some(region.to_string()),
            wide_angle_max_zoom: Some(7),
            wide_angle: Some(true),
            min_source_zoom: None,
            max_source_zoom: Some(7),
            tile_count: Some(100),
        });
        pkg
    }

    fn detail_pkg(id: &str, product: &str, region: &str) -> BundlePackageArtifact {
        let mut pkg = pkg(id, product, Some(region), None, Some("2099-01-01"));
        pkg.metadata = Some(BundlePackageMetadata {
            chart_package_tier: Some(product_contracts::ChartPackageTier::Detail),
            full_coverage_zoom: None,
            wide_angle_region_id: None,
            wide_angle_max_zoom: None,
            wide_angle: Some(false),
            min_source_zoom: Some(12),
            max_source_zoom: Some(12),
            tile_count: Some(100),
        });
        pkg
    }

    fn installed(id: &str) -> InstalledArtifact {
        InstalledArtifact {
            artifact_id: id.to_string(),
            filename: format!("{id}.zip"),
            size_bytes: None,
            checksum_sha256: None,
            family_id: None,
            region_id: None,
            chart_package_tier: None,
        }
    }

    fn with_cycle_and_size(
        mut pkg: BundlePackageArtifact,
        cycle: &str,
        size_bytes: u64,
    ) -> BundlePackageArtifact {
        pkg.cycle = Some(cycle.to_string());
        pkg.size_bytes = Some(size_bytes);
        pkg
    }

    fn installed_with_size(id: &str, size_bytes: u64) -> InstalledArtifact {
        InstalledArtifact {
            artifact_id: id.to_string(),
            filename: format!("{id}.zip"),
            size_bytes: Some(size_bytes),
            checksum_sha256: None,
            family_id: None,
            region_id: None,
            chart_package_tier: None,
        }
    }

    fn installed_with_filename(id: &str, filename: &str) -> InstalledArtifact {
        InstalledArtifact {
            artifact_id: id.to_string(),
            filename: filename.to_string(),
            size_bytes: None,
            checksum_sha256: None,
            family_id: None,
            region_id: None,
            chart_package_tier: None,
        }
    }

    fn installed_with_grouping(
        id: &str,
        size_bytes: u64,
        family_id: &str,
        region_id: Option<&str>,
    ) -> InstalledArtifact {
        InstalledArtifact {
            artifact_id: id.to_string(),
            filename: format!("{id}.zip"),
            size_bytes: Some(size_bytes),
            checksum_sha256: None,
            family_id: Some(family_id.to_string()),
            region_id: region_id.map(str::to_string),
            chart_package_tier: None,
        }
    }

    #[test]
    fn selected_expired_package_is_retained_until_replacement_is_installed() {
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(["nw"], ["sec"]),
            bundle: BundleManifest {
                packages: vec![
                    pkg(
                        "NW_SEC_2603",
                        "sec",
                        Some("nw"),
                        Some("2026-03-19"),
                        Some("1970-01-01"),
                    ),
                    pkg(
                        "NW_SEC_2604",
                        "sec",
                        Some("nw"),
                        Some("2026-04-16"),
                        Some("2099-01-01"),
                    ),
                ],
            },
            installed: vec![installed("NW_SEC_2603")],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };

        let plan = plan_offline_packages(&input);

        assert_eq!(plan.fetch, vec!["NW_SEC_2604"]);
        assert_eq!(plan.retain_installed, vec!["NW_SEC_2603.zip"]);
        assert!(plan.gc.is_empty());
    }

    #[test]
    fn selected_package_with_stale_filename_is_visible_until_replacement_is_installed() {
        let current = with_cycle_and_size(
            pkg(
                "NW_SEC_2603",
                "sec",
                Some("nw"),
                Some("2026-03-19"),
                Some("2099-01-01"),
            ),
            "2603",
            1_000,
        );
        let stale_filename = "sec_nw_2603_01_old.zip";
        let current_filename = current.filename.clone();
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(["nw"], ["sec"]),
            bundle: BundleManifest {
                packages: vec![current],
            },
            installed: vec![installed_with_filename("NW_SEC_2603", stale_filename)],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };

        let plan = plan_offline_packages(&input);

        assert_eq!(plan.fetch, vec!["NW_SEC_2603"]);
        assert_eq!(plan.retain_installed, vec![stale_filename]);
        assert!(plan.gc.is_empty());

        let rows = plan_rows_by_dimension(&input, &plan, &BTreeMap::new(), None);
        let row = offline_packages_ui_row(
            "nw".to_string(),
            "Northwest".to_string(),
            OfflinePackageSelection::Play,
            None,
            None,
            rows.regions.get("nw"),
        );
        assert_eq!(
            row.plan_entries,
            vec![
                OfflinePackagesUiPlanEntry {
                    action: OfflinePackagesUiPlanAction::Keep,
                    count: 1,
                    cycles: vec!["2603".to_string()],
                },
                OfflinePackagesUiPlanEntry {
                    action: OfflinePackagesUiPlanAction::Fetch,
                    count: 1,
                    cycles: vec!["2603".to_string()],
                },
            ]
        );
        assert!(!plan.retain_installed.contains(&current_filename));
    }

    #[test]
    fn sync_fetch_plan_prioritizes_core_artifacts() {
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(["nw"], ["sec", "tac"]),
            bundle: BundleManifest {
                packages: vec![
                    pkg("NW_SEC_2603", "sec", Some("nw"), None, Some("2099-01-01")),
                    pkg("NW_TAC_2603", "tac", Some("nw"), None, Some("2099-01-01")),
                    pkg("GEO_STATIC", "geo", None, None, Some("2099-01-01")),
                    pkg("NAV_DB_2604", "nav-db", None, None, Some("2099-01-01")),
                ],
            },
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };

        let plan = plan_offline_packages(&input);

        assert_eq!(
            plan.fetch,
            vec!["NAV_DB_2604", "GEO_STATIC", "NW_SEC_2603", "NW_TAC_2603",]
        );
    }

    #[test]
    fn world_basemap_is_a_core_product() {
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(["nw"], ["sec"]),
            bundle: BundleManifest {
                packages: vec![pkg(
                    "WORLD_BASEMAP",
                    "world-basemap",
                    None,
                    None,
                    Some("2099-01-01"),
                )],
            },
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };

        let plan = plan_offline_packages(&input);

        assert_eq!(plan.fetch, vec!["WORLD_BASEMAP"]);
        let rows = plan_rows_by_dimension(&input, &plan, &BTreeMap::new(), None);
        let row = offline_packages_ui_row(
            "world-basemap".to_string(),
            "WORLD BASEMAP".to_string(),
            OfflinePackageSelection::Play,
            None,
            None,
            rows.core_products.get("world-basemap"),
        );
        assert_eq!(row.fetch_count, 1);
    }

    #[test]
    fn selected_region_fetches_family_wide_angle_package() {
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(["pac"], ["tac"]),
            bundle: BundleManifest {
                packages: vec![wide_pkg("TAC_WIDE_2604", "tac", "wide", Some("2099-01-01"))],
            },
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };

        let plan = plan_offline_packages(&input);

        assert_eq!(plan.fetch, vec!["TAC_WIDE_2604"]);
    }

    #[test]
    fn wide_region_policy_includes_terrain_without_optional_chart_metadata() {
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(["nw"], ["terrain"]),
            bundle: BundleManifest {
                packages: vec![pkg(
                    "TERRAIN_WIDE",
                    "terrain",
                    Some(WIDE_COVERAGE_REGION_ID),
                    None,
                    Some("2099-01-01"),
                )],
            },
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };

        let plan = plan_offline_packages(&input);

        assert_eq!(plan.fetch, vec!["TERRAIN_WIDE"]);
        let rows = plan_rows_by_dimension(&input, &plan, &BTreeMap::new(), None);
        assert_eq!(rows.zoom_levels[WIDE_COVERAGE_REGION_ID].fetch_count, 1);
        assert!(!rows.regions.contains_key(WIDE_COVERAGE_REGION_ID));
    }

    #[test]
    fn family_wide_angle_package_is_not_selected_without_a_region() {
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(Vec::<String>::new(), ["tac"]),
            bundle: BundleManifest {
                packages: vec![wide_pkg("TAC_WIDE_2604", "tac", "wide", Some("2099-01-01"))],
            },
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };

        let plan = plan_offline_packages(&input);

        assert!(plan.fetch.is_empty());
    }

    #[test]
    fn obsolete_standalone_vectors_package_is_ignored_by_sync_plan() {
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(
                Vec::<String>::new(),
                Vec::<String>::new(),
            ),
            bundle: BundleManifest {
                packages: vec![
                    pkg(
                        "VECTORS_DATA_2604",
                        "vectors",
                        None,
                        None,
                        Some("2099-01-01"),
                    ),
                    pkg("NAV_DB_2604", "nav-db", None, None, Some("2099-01-01")),
                ],
            },
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };

        let plan = plan_offline_packages(&input);

        assert_eq!(plan.fetch, vec!["NAV_DB_2604"]);
    }

    #[test]
    fn selected_expired_package_can_be_collected_after_replacement_is_installed() {
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(["nw"], ["sec"]),
            bundle: BundleManifest {
                packages: vec![
                    pkg(
                        "NW_SEC_2603",
                        "sec",
                        Some("nw"),
                        Some("2026-03-19"),
                        Some("1970-01-01"),
                    ),
                    pkg(
                        "NW_SEC_2604",
                        "sec",
                        Some("nw"),
                        Some("2026-04-16"),
                        Some("2099-01-01"),
                    ),
                ],
            },
            installed: vec![installed("NW_SEC_2603"), installed("NW_SEC_2604")],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };

        let plan = plan_offline_packages(&input);

        assert!(plan.fetch.is_empty());
        assert_eq!(plan.retain_installed, vec!["NW_SEC_2604.zip"]);
        assert_eq!(plan.gc, vec!["NW_SEC_2603.zip"]);
    }

    #[test]
    fn multiple_not_yet_expired_cycles_in_one_selected_slot_are_all_desired() {
        let manifest = BundleManifest {
            packages: vec![
                pkg(
                    "NW_SEC_2603",
                    "sec",
                    Some("nw"),
                    Some("2026-03-19"),
                    Some("2099-01-01"),
                ),
                pkg(
                    "NW_SEC_2604",
                    "sec",
                    Some("nw"),
                    Some("2099-04-16"),
                    Some("2099-05-14"),
                ),
            ],
        };
        let preferences = default_offline_package_preferences(["nw"], ["sec"]);

        let missing_plan = plan_offline_packages(&PackageManagementInput {
            now_epoch_ms: 200,
            preferences: preferences.clone(),
            bundle: manifest.clone(),
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        });

        assert_eq!(missing_plan.fetch, vec!["NW_SEC_2603", "NW_SEC_2604"]);
        assert!(missing_plan.retain_installed.is_empty());
        assert!(missing_plan.gc.is_empty());

        let installed_plan = plan_offline_packages(&PackageManagementInput {
            now_epoch_ms: 200,
            preferences,
            bundle: manifest,
            installed: vec![installed("NW_SEC_2603"), installed("NW_SEC_2604")],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        });

        assert!(installed_plan.fetch.is_empty());
        assert_eq!(
            installed_plan.retain_installed,
            vec!["NW_SEC_2603.zip", "NW_SEC_2604.zip"]
        );
        assert!(installed_plan.gc.is_empty());
    }

    #[test]
    fn pause_suppresses_fetch_and_protects_installed_artifacts() {
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: OfflinePackagePreferences {
                regions: BTreeMap::from([("nw".to_string(), OfflinePackageSelection::Pause)]),
                products: BTreeMap::from([("sec".to_string(), OfflinePackageSelection::Play)]),
            },
            bundle: BundleManifest {
                packages: vec![
                    pkg(
                        "NW_SEC_2604",
                        "sec",
                        Some("nw"),
                        Some("2026-04-16"),
                        Some("2099-01-01"),
                    ),
                    pkg(
                        "NW_SEC_2603",
                        "sec",
                        Some("nw"),
                        Some("2026-03-19"),
                        Some("1970-01-01"),
                    ),
                ],
            },
            installed: vec![installed("NW_SEC_2603")],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };

        let plan = plan_offline_packages(&input);

        assert!(plan.fetch.is_empty());
        assert_eq!(plan.retain_installed, vec!["NW_SEC_2603.zip"]);
        assert!(plan.gc.is_empty());
        assert_eq!(plan.protected_by_pause, vec!["NW_SEC_2603.zip"]);
    }

    #[test]
    fn high_resolution_is_opt_in_and_controls_only_regional_detail_packages() {
        let base = pkg("NW_TAC_BASE", "tac", Some("nw"), None, Some("2099-01-01"));
        let detail = detail_pkg("NW_TAC_DETAIL", "tac", "nw");
        let bundle = BundleManifest {
            packages: vec![base.clone(), detail.clone()],
        };
        let normalized = normalize_preferences(
            None,
            &["nw".to_string()],
            &[
                "tac".to_string(),
                CHART_HIGH_RESOLUTION_PRODUCT_ID.to_string(),
            ],
        );
        assert_eq!(
            normalized.products[CHART_HIGH_RESOLUTION_PRODUCT_ID],
            OfflinePackageSelection::Unselected
        );

        let mut preferences =
            default_offline_package_preferences(["nw"], ["tac", CHART_HIGH_RESOLUTION_PRODUCT_ID]);
        preferences.products.insert(
            CHART_HIGH_RESOLUTION_PRODUCT_ID.to_string(),
            OfflinePackageSelection::Play,
        );
        let play_input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: preferences.clone(),
            bundle: bundle.clone(),
            installed: Vec::new(),
            forced_gc_installed_filenames: Vec::new(),
            suppressed_fetch_filenames: Vec::new(),
        };
        let play = plan_offline_packages(&play_input);
        assert_eq!(play.fetch, vec!["NW_TAC_BASE", "NW_TAC_DETAIL"]);
        let rows = plan_rows_by_dimension(&play_input, &play, &BTreeMap::new(), None);
        assert_eq!(
            rows.zoom_levels[CHART_HIGH_RESOLUTION_PRODUCT_ID].fetch_count,
            1
        );

        preferences.products.insert(
            CHART_HIGH_RESOLUTION_PRODUCT_ID.to_string(),
            OfflinePackageSelection::Pause,
        );
        let pause = plan_offline_packages(&PackageManagementInput {
            now_epoch_ms: 200,
            preferences: preferences.clone(),
            bundle: bundle.clone(),
            installed: vec![installed("NW_TAC_BASE"), installed("NW_TAC_DETAIL")],
            forced_gc_installed_filenames: Vec::new(),
            suppressed_fetch_filenames: Vec::new(),
        });
        assert!(pause.fetch.is_empty());
        assert_eq!(
            pause.retain_installed,
            vec!["NW_TAC_BASE.zip", "NW_TAC_DETAIL.zip"]
        );
        assert_eq!(pause.protected_by_pause, vec!["NW_TAC_DETAIL.zip"]);

        preferences.products.insert(
            CHART_HIGH_RESOLUTION_PRODUCT_ID.to_string(),
            OfflinePackageSelection::Unselected,
        );
        let deleted = plan_offline_packages(&PackageManagementInput {
            now_epoch_ms: 200,
            preferences,
            bundle,
            installed: vec![installed("NW_TAC_BASE"), installed("NW_TAC_DETAIL")],
            forced_gc_installed_filenames: Vec::new(),
            suppressed_fetch_filenames: Vec::new(),
        });
        assert_eq!(deleted.retain_installed, vec!["NW_TAC_BASE.zip"]);
        assert_eq!(deleted.gc, vec!["NW_TAC_DETAIL.zip"]);
    }

    #[test]
    fn offline_package_ui_owns_sections_actions_and_help_in_core() {
        let discovery = discovery_manifest_with_nav_contract(crate::REQUIRED_NAV_DB_CONTRACT_ID);
        let mut shaded = pkg(
            "NW_SHADED",
            "shaded-relief",
            Some("nw"),
            None,
            Some("2099-01-01"),
        );
        shaded.metadata = Some(BundlePackageMetadata {
            chart_package_tier: Some(product_contracts::ChartPackageTier::Regional),
            full_coverage_zoom: None,
            wide_angle_region_id: Some(WIDE_COVERAGE_REGION_ID.to_string()),
            wide_angle_max_zoom: Some(7),
            wide_angle: Some(false),
            min_source_zoom: Some(8),
            max_source_zoom: Some(11),
            tile_count: Some(100),
        });
        let bundles = BTreeMap::from([(
            "bundle_cycle_2605.json".to_string(),
            BundleManifest {
                packages: vec![
                    pkg("NAV_DB", "nav-db", None, None, Some("2099-01-01")),
                    pkg(
                        "WORLD_BASEMAP",
                        "world-basemap",
                        None,
                        None,
                        Some("2099-01-01"),
                    ),
                    pkg("NW_SEC", "sec", Some("nw"), None, Some("2099-01-01")),
                    detail_pkg("NW_SEC_DETAIL", "sec", "nw"),
                    shaded,
                    pkg(
                        "TERRAIN_WIDE",
                        "terrain",
                        Some(WIDE_COVERAGE_REGION_ID),
                        None,
                        Some("2099-01-01"),
                    ),
                ],
            },
        )]);

        let result = initialize_offline_packages(&OfflinePackagesInitInput {
            state: None,
            now_epoch_ms: 200,
            discovery_manifests: vec![discovery],
            bundle_manifests_by_filename: bundles,
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        });

        assert!(!result
            .state
            .preferences
            .regions
            .contains_key(WIDE_COVERAGE_REGION_ID));
        assert_eq!(
            result
                .ui_state
                .core_products
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["nav-db", "world-basemap"]
        );
        assert!(result
            .ui_state
            .core_products
            .iter()
            .all(|row| row.selection_event.is_none()));
        assert_eq!(
            result
                .ui_state
                .products
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["sec", "shaded-relief", "terrain"]
        );
        assert_eq!(
            result
                .ui_state
                .regions
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec!["nw"]
        );
        assert_eq!(
            result.ui_state.regions[0].help_text.as_deref(),
            Some(
                "Select Northwest region products. Enable Offline Regions to display coverage areas."
            )
        );
        assert_eq!(
            result
                .ui_state
                .zoom_levels
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Wide all-region coverage", "High resolution charts"]
        );
        assert!(result.ui_state.zoom_levels[0].selection_event.is_none());
        assert_eq!(
            result.ui_state.zoom_levels[1].selection_event,
            Some(OfflinePackagesEvent::CycleProduct {
                id: CHART_HIGH_RESOLUTION_PRODUCT_ID.to_string(),
            })
        );
        assert_eq!(
            result
                .ui_state
                .products
                .iter()
                .find(|row| row.id == "shaded-relief")
                .and_then(|row| row.help_text.as_deref()),
            Some("Uncluttered base maps useful for vector-only and track-up displays.")
        );
        assert!(result.ui_state.products.iter().all(|row| row
            .help_text
            .as_deref()
            .is_some_and(|text| !text.is_empty())));
        assert_eq!(
            result
                .ui_state
                .products
                .iter()
                .find(|row| row.id == "terrain")
                .and_then(|row| row.help_text.as_deref()),
            Some(
                "Terrain data, used for the terrain warning layer, SPOT altitude measurements, and ownship AGL."
            )
        );
    }

    #[test]
    fn offline_package_ui_rows_project_actions_cycles_and_sizes() {
        let old_sec = with_cycle_and_size(
            pkg(
                "NW_SEC_2603",
                "sec",
                Some("nw"),
                Some("2026-03-19"),
                Some("2026-04-16"),
            ),
            "2603",
            3_000,
        );
        let current_sec = with_cycle_and_size(
            pkg(
                "NW_SEC_2604",
                "sec",
                Some("nw"),
                Some("2026-04-16"),
                Some("2099-01-01"),
            ),
            "2604",
            1_000,
        );
        let next_sec = with_cycle_and_size(
            pkg(
                "NW_SEC_2605",
                "sec",
                Some("nw"),
                Some("2026-05-14"),
                Some("2099-01-01"),
            ),
            "2605",
            2_000,
        );
        let paused_tac = with_cycle_and_size(
            pkg(
                "NW_TAC_2605",
                "tac",
                Some("nw"),
                Some("2026-05-14"),
                Some("2099-01-01"),
            ),
            "2605",
            4_000,
        );
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: OfflinePackagePreferences {
                regions: BTreeMap::from([("nw".to_string(), OfflinePackageSelection::Play)]),
                products: BTreeMap::from([
                    ("sec".to_string(), OfflinePackageSelection::Play),
                    ("tac".to_string(), OfflinePackageSelection::Pause),
                ]),
            },
            bundle: BundleManifest {
                packages: vec![current_sec.clone(), next_sec.clone(), paused_tac.clone()],
            },
            installed: vec![
                installed_with_size("NW_SEC_2603", 3_100),
                installed_with_size("NW_SEC_2604", 1_100),
            ],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };
        let plan = plan_offline_packages(&input);
        let rows = plan_rows_by_dimension(
            &input,
            &plan,
            &BTreeMap::from([(
                "bundle_cycle_2603.json".to_string(),
                BundleManifest {
                    packages: vec![
                        old_sec,
                        current_sec.clone(),
                        next_sec.clone(),
                        paused_tac.clone(),
                    ],
                },
            )]),
            Some(&OfflinePackagesSyncProgress {
                planned_fetch_artifact_ids: BTreeSet::from(["NW_SEC_2605".to_string()]),
                completed_fetch_artifact_ids: BTreeSet::new(),
                active_fetch_bytes_by_artifact_id: BTreeMap::from([(
                    "NW_SEC_2605".to_string(),
                    1_000,
                )]),
            }),
        );
        let nw = offline_packages_ui_row(
            "nw".to_string(),
            "Northwest".to_string(),
            OfflinePackageSelection::Play,
            None,
            None,
            rows.regions.get("nw"),
        );
        let all = offline_packages_ui_row(
            "all-packages".to_string(),
            "All packages".to_string(),
            OfflinePackageSelection::Play,
            None,
            None,
            Some(&rows.all_packages),
        );

        assert_eq!(nw.installed_size_label, "0.00M");
        assert_eq!(nw.planned_change_label, "⤓0.00M -0.00M");
        assert_eq!(nw.planned_total_size_label, "0.00M");
        assert!(nw.planned_size_change_visible);
        assert_eq!(nw.sync_progress_per_mille, Some(500));
        assert_eq!(all.sync_progress_per_mille, Some(500));

        let mut replanned_input = input.clone();
        replanned_input
            .installed
            .push(installed_with_size("NW_SEC_2605", 2_000));
        let replanned = plan_offline_packages(&replanned_input);
        let replanned_rows = plan_rows_by_dimension(
            &replanned_input,
            &replanned,
            &BTreeMap::new(),
            Some(&OfflinePackagesSyncProgress {
                planned_fetch_artifact_ids: BTreeSet::from(["NW_SEC_2605".to_string()]),
                completed_fetch_artifact_ids: BTreeSet::new(),
                active_fetch_bytes_by_artifact_id: BTreeMap::from([(
                    "NW_SEC_2605".to_string(),
                    1_000,
                )]),
            }),
        );
        assert_eq!(
            offline_packages_ui_row(
                "all-packages".to_string(),
                "All packages".to_string(),
                OfflinePackageSelection::Play,
                None,
                None,
                Some(&replanned_rows.all_packages),
            )
            .sync_progress_per_mille,
            Some(500)
        );
        assert_eq!(
            nw.plan_entries,
            vec![
                OfflinePackagesUiPlanEntry {
                    action: OfflinePackagesUiPlanAction::Delete,
                    count: 1,
                    cycles: vec!["2603".to_string()],
                },
                OfflinePackagesUiPlanEntry {
                    action: OfflinePackagesUiPlanAction::Keep,
                    count: 1,
                    cycles: vec!["2604".to_string()],
                },
                OfflinePackagesUiPlanEntry {
                    action: OfflinePackagesUiPlanAction::Pause,
                    count: 1,
                    cycles: vec!["2605".to_string()],
                },
                OfflinePackagesUiPlanEntry {
                    action: OfflinePackagesUiPlanAction::Fetch,
                    count: 1,
                    cycles: vec!["2605".to_string()],
                },
            ]
        );
    }

    #[test]
    fn obsolete_installed_artifacts_keep_their_persisted_package_dimensions() {
        let current_tpp = with_cycle_and_size(
            pkg(
                "NW_TPP_TPP1_2608",
                "tpp",
                Some("nw"),
                Some("2026-08-06"),
                Some("2099-01-01"),
            ),
            "2608",
            180_000_000,
        );
        let current_nav = with_cycle_and_size(
            pkg(
                "NAV_DB_NAV19_2608_01",
                "nav-db",
                None,
                Some("2026-08-06"),
                Some("2099-01-01"),
            ),
            "2608",
            20_000_000,
        );
        let mut old_detail =
            installed_with_grouping("NW_SEC_DETAIL_SEC1_2606", 10_000_000, "sec", Some("nw"));
        old_detail.chart_package_tier = Some(product_contracts::ChartPackageTier::Detail);
        let old_tpp = installed_with_grouping("NW_TPP_TPP1_2607", 200_000_000, "tpp", Some("nw"));
        let old_nav = installed_with_grouping("NAV_DB_NAV18_2608_01", 18_000_000, "nav-db", None);
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(["nw"], ["sec", "tpp", "nav-db"]),
            bundle: BundleManifest {
                packages: vec![current_tpp, current_nav],
            },
            installed: vec![old_tpp.clone(), old_nav.clone(), old_detail.clone()],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };
        let plan = PackageManagementPlan {
            fetch: vec![
                "NW_TPP_TPP1_2608".to_string(),
                "NAV_DB_NAV19_2608_01".to_string(),
            ],
            gc: vec![old_tpp.filename, old_nav.filename, old_detail.filename],
            ..PackageManagementPlan::default()
        };

        let rows = plan_rows_by_dimension(&input, &plan, &BTreeMap::new(), None);

        assert_eq!(rows.regions["nw"].gc_count, 2);
        assert_eq!(rows.products["tpp"].gc_count, 1);
        assert_eq!(rows.core_products["nav-db"].gc_count, 1);
        assert_eq!(
            rows.zoom_levels[CHART_HIGH_RESOLUTION_PRODUCT_ID].gc_count,
            1
        );
        assert!(!rows.products.contains_key("NW_TPP_TPP1_2607"));
        assert!(!rows.products.contains_key("NAV_DB_NAV18_2608_01"));
    }

    #[test]
    fn loaded_manifests_backfill_grouping_for_legacy_installed_sidecars() {
        let bundles = BTreeMap::from([(
            "bundle_cycle_2608.json".to_string(),
            BundleManifest {
                packages: vec![
                    detail_pkg("NW_SEC_SEC1_2607", "sec", "nw"),
                    pkg(
                        "NAV_DB_NAV19_2608_01",
                        "nav-db",
                        None,
                        None,
                        Some("2099-01-01"),
                    ),
                ],
            },
        )]);
        let installed = vec![
            installed("NW_SEC_SEC1_2607"),
            installed("NAV_DB_NAV19_2608_01"),
        ];

        let updates = installed_artifact_metadata_updates(&bundles, &installed);

        assert_eq!(
            updates,
            vec![
                InstalledArtifactMetadataUpdate {
                    artifact_id: "NAV_DB_NAV19_2608_01".to_string(),
                    filename: "NAV_DB_NAV19_2608_01.zip".to_string(),
                    family_id: "nav-db".to_string(),
                    region_id: None,
                    chart_package_tier: None,
                },
                InstalledArtifactMetadataUpdate {
                    artifact_id: "NW_SEC_SEC1_2607".to_string(),
                    filename: "NW_SEC_SEC1_2607.zip".to_string(),
                    family_id: "sec".to_string(),
                    region_id: Some("nw".to_string()),
                    chart_package_tier: Some(product_contracts::ChartPackageTier::Detail),
                },
            ]
        );
    }

    #[test]
    fn package_change_label_separates_download_bytes_from_net_storage_change() {
        let row = offline_packages_ui_row(
            "all-packages".to_string(),
            "All packages".to_string(),
            OfflinePackageSelection::Play,
            None,
            None,
            Some(&DimensionPlanDetails {
                planned_download_bytes: 180_000_000,
                planned_gc_bytes: 218_000_000,
                ..DimensionPlanDetails::default()
            }),
        );

        assert_eq!(row.planned_change_label, "⤓180M -38M");
    }

    #[test]
    fn package_size_labels_are_two_significant_digits_in_m_or_g() {
        assert_eq!(format_package_size_label(458_000_000), "460M");
        assert_eq!(format_package_size_label(3_640_000_000), "3.6G");
        assert_eq!(format_package_size_label(42_400_000), "42M");
        assert_eq!(format_package_size_label(4_240_000), "4.2M");
        assert_eq!(format_package_size_label(424_000), "0.42M");
        assert_eq!(format_signed_package_size_label(-458_000_000), "-460M");
        assert!(
            !offline_packages_ui_row(
                "nw".to_string(),
                "Northwest".to_string(),
                OfflinePackageSelection::Play,
                None,
                None,
                None,
            )
            .planned_size_change_visible
        );
    }

    #[test]
    fn sync_progress_sums_parallel_active_fetches_by_row() {
        let nw_sec = with_cycle_and_size(
            pkg(
                "NW_SEC_2605",
                "sec",
                Some("nw"),
                Some("2026-05-14"),
                Some("2099-01-01"),
            ),
            "2605",
            2_000,
        );
        let nw_tpp = with_cycle_and_size(
            pkg(
                "NW_TPP_2605",
                "tpp",
                Some("nw"),
                Some("2026-05-14"),
                Some("2099-01-01"),
            ),
            "2605",
            8_000,
        );
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: OfflinePackagePreferences {
                regions: BTreeMap::from([("nw".to_string(), OfflinePackageSelection::Play)]),
                products: BTreeMap::from([
                    ("sec".to_string(), OfflinePackageSelection::Play),
                    ("tpp".to_string(), OfflinePackageSelection::Play),
                ]),
            },
            bundle: BundleManifest {
                packages: vec![nw_sec, nw_tpp],
            },
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        };
        let plan = plan_offline_packages(&input);
        let rows = plan_rows_by_dimension(
            &input,
            &plan,
            &BTreeMap::new(),
            Some(&OfflinePackagesSyncProgress {
                planned_fetch_artifact_ids: BTreeSet::from([
                    "NW_SEC_2605".to_string(),
                    "NW_TPP_2605".to_string(),
                ]),
                completed_fetch_artifact_ids: BTreeSet::new(),
                active_fetch_bytes_by_artifact_id: BTreeMap::from([
                    ("NW_SEC_2605".to_string(), 1_000),
                    ("NW_TPP_2605".to_string(), 4_000),
                ]),
            }),
        );

        assert_eq!(
            offline_packages_ui_row(
                "nw".to_string(),
                "Northwest".to_string(),
                OfflinePackageSelection::Play,
                None,
                None,
                rows.regions.get("nw"),
            )
            .sync_progress_per_mille,
            Some(500)
        );
    }

    fn test_artifact_roots() -> CurrentArtifactsArtifactRoots {
        CurrentArtifactsArtifactRoots {
            packaged: "published_packaged/".to_string(),
            unpacked: "published_unpacked/".to_string(),
        }
    }

    fn test_contracts() -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                "nav-db".to_string(),
                crate::REQUIRED_NAV_DB_CONTRACT_ID.to_string(),
            ),
            (
                "sec".to_string(),
                product_contracts::SEC_CONTRACT_ID.to_string(),
            ),
        ])
    }

    fn discovery_manifest_with_nav_contract(contract_id: &str) -> CurrentArtifactsManifest {
        CurrentArtifactsManifest {
            schema_version: Some(1),
            contracts: BTreeMap::from([("nav-db".to_string(), contract_id.to_string())]),
            artifact_roots: test_artifact_roots(),
            as_of_date: Some("2026-05-20".to_string()),
            as_of_utc: Some("2026-05-20T12:00:00Z".to_string()),
            bundles: vec![CurrentArtifactsBundleRef {
                filename: "bundle_cycle_2605.json".to_string(),
                relative_path: "bundle_cycle_2605.json".to_string(),
                id: "cycle-2605".to_string(),
                bundle_type: "cycle".to_string(),
                cycle: Some("2605".to_string()),
                cycle_version: Some("01".to_string()),
                start_valid: Some("2026-05-20T00:00:00Z".to_string()),
                end_valid: Some("2026-06-17T00:00:00Z".to_string()),
                checksum_sha256: Some("test-bundle-sha256".to_string()),
                size_bytes: Some(1234),
            }],
            startup_prefetch: None,
        }
    }

    fn controller_test_catalog(
        packaged_root: &str,
    ) -> (CurrentArtifactsManifest, BTreeMap<String, BundleManifest>) {
        let bundle_filename = "bundle_cycle_2605.json".to_string();
        let discovery = CurrentArtifactsManifest {
            schema_version: Some(1),
            contracts: test_contracts(),
            artifact_roots: CurrentArtifactsArtifactRoots {
                packaged: packaged_root.to_string(),
                unpacked: "unpacked/".to_string(),
            },
            as_of_date: Some("2026-05-20".to_string()),
            as_of_utc: Some("2026-05-20T12:00:00Z".to_string()),
            bundles: vec![CurrentArtifactsBundleRef {
                filename: bundle_filename.clone(),
                relative_path: bundle_filename.clone(),
                id: "cycle-2605".to_string(),
                bundle_type: "cycle".to_string(),
                cycle: Some("2605".to_string()),
                cycle_version: Some("01".to_string()),
                start_valid: None,
                end_valid: None,
                checksum_sha256: None,
                size_bytes: None,
            }],
            startup_prefetch: None,
        };
        let bundles = BTreeMap::from([(
            bundle_filename,
            BundleManifest {
                packages: vec![pkg(
                    "NAV_DB_2605_01",
                    "nav-db",
                    None,
                    None,
                    Some("2099-01-01"),
                )],
            },
        )]);
        (discovery, bundles)
    }

    #[test]
    fn explicit_sync_refreshes_catalog_before_emitting_transfer_command() {
        let (old_discovery, old_bundles) = controller_test_catalog("old-packaged/");
        let initial_state = OfflinePackagesControllerState {
            packages_state: Some(OfflinePackagesState {
                preferences: default_offline_package_preferences(Vec::<String>::new(), ["nav-db"]),
                now_override_epoch_ms: None,
            }),
            library_cache: Some(OfflinePackagesLibraryCache {
                package_source_base_url: "https://example.test/packages".to_string(),
                fetched_at_epoch_ms: 1_778_025_600_000,
                discovery_manifests: vec![old_discovery],
                bundle_manifests_by_filename: old_bundles,
            }),
            ..Default::default()
        };
        let requested = reduce_offline_packages_controller(&OfflinePackagesControllerInput {
            state: Some(initial_state),
            package_source_base_url: "https://example.test/packages".to_string(),
            discovery_filenames: vec![],
            now_epoch_ms: 1_778_025_600_000,
            installed: vec![],
            storage: None,
            event: OfflinePackagesControllerEvent::SyncRequested,
        });

        assert!(requested.state.sync_after_library_refresh);
        assert!(matches!(
            requested.command,
            Some(OfflinePackagesControllerCommand::RefreshLibrary { .. })
        ));

        let (fresh_discovery, fresh_bundles) = controller_test_catalog("fresh-packaged/");
        let refreshed = reduce_offline_packages_controller(&OfflinePackagesControllerInput {
            state: Some(requested.state),
            package_source_base_url: "https://example.test/packages".to_string(),
            discovery_filenames: vec![],
            now_epoch_ms: 1_778_025_600_000,
            installed: vec![],
            storage: None,
            event: OfflinePackagesControllerEvent::LibraryRefreshSucceeded {
                fetched_at_epoch_ms: 1_778_025_600_001,
                discovery_manifests: vec![fresh_discovery],
                bundle_manifests_by_filename: fresh_bundles,
            },
        });

        let Some(OfflinePackagesControllerCommand::Sync {
            packaged_artifact_root,
            plan,
            ..
        }) = refreshed.command
        else {
            panic!("fresh catalog should produce a transfer command");
        };
        assert_eq!(packaged_artifact_root, "fresh-packaged/");
        assert_eq!(plan.fetch, vec!["NAV_DB_2605_01"]);
        assert!(refreshed.state.sync_in_flight);
        assert!(!refreshed.state.sync_after_library_refresh);
    }

    #[test]
    fn durable_progress_reestablishes_sync_after_controller_recreation() {
        let result = reduce_offline_packages_controller(&OfflinePackagesControllerInput {
            state: Some(OfflinePackagesControllerState::default()),
            package_source_base_url: "https://example.test/packages".to_string(),
            discovery_filenames: vec![],
            now_epoch_ms: 1_778_025_600_000,
            installed: vec![],
            storage: None,
            event: OfflinePackagesControllerEvent::SyncProgressObserved {
                progress: OfflinePackagesSyncProgress {
                    planned_fetch_artifact_ids: BTreeSet::from(["NW_SEC_2605".to_string()]),
                    completed_fetch_artifact_ids: BTreeSet::new(),
                    active_fetch_bytes_by_artifact_id: BTreeMap::from([(
                        "NW_SEC_2605".to_string(),
                        1_000,
                    )]),
                },
            },
        });

        assert!(result.state.sync_in_flight);
        assert_eq!(
            result
                .state
                .sync_progress
                .as_ref()
                .unwrap()
                .active_fetch_bytes_by_artifact_id
                .get("NW_SEC_2605"),
            Some(&1_000)
        );
    }

    #[test]
    fn current_artifacts_root_requires_list_json() {
        let object_json = serde_json::to_string(&discovery_manifest_with_nav_contract(
            crate::REQUIRED_NAV_DB_CONTRACT_ID,
        ))
        .unwrap();

        assert!(decode_current_artifacts_manifest_list(&object_json).is_err());
    }

    #[test]
    fn current_artifacts_discovery_plan_selects_supported_contracts() {
        let unsupported = discovery_manifest_with_nav_contract("NAV999");
        let supported = discovery_manifest_with_nav_contract(crate::REQUIRED_NAV_DB_CONTRACT_ID);
        let list_json = serde_json::to_string(&vec![unsupported, supported]).unwrap();

        let plan =
            plan_current_artifacts_discovery("https://example.test/packages", &list_json).unwrap();

        assert_eq!(plan.discovery_jsons.len(), 1);
        assert_eq!(
            plan.bundle_requests,
            vec![CurrentArtifactsBundleRequest {
                filename: "bundle_cycle_2605.json".to_string(),
                url: "https://example.test/packages/published_packaged/bundle_cycle_2605.json"
                    .to_string(),
            }]
        );
        let selected: CurrentArtifactsManifest =
            serde_json::from_str(&plan.discovery_jsons[0]).unwrap();
        assert_eq!(
            selected.contracts.get("nav-db").map(String::as_str),
            Some(crate::REQUIRED_NAV_DB_CONTRACT_ID)
        );
    }

    #[test]
    fn current_artifacts_discovery_plan_reports_contract_mismatch() {
        let unsupported = discovery_manifest_with_nav_contract("NAVBOGUS");
        let list_json = serde_json::to_string(&vec![unsupported]).unwrap();

        let error = plan_current_artifacts_discovery("https://example.test/packages", &list_json)
            .unwrap_err();

        assert!(error.contains("app requires nav-db="));
        assert!(error.contains(crate::REQUIRED_NAV_DB_CONTRACT_ID));
        assert!(error.contains("artifacts offer nav-db=NAVBOGUS"));
    }

    #[test]
    fn remote_poisoned_filename_is_suppressed_from_refetch() {
        let discovery = CurrentArtifactsManifest {
            schema_version: Some(1),
            contracts: test_contracts(),
            artifact_roots: test_artifact_roots(),
            as_of_date: Some("2026-04-25".to_string()),
            as_of_utc: Some("2026-04-25T12:00:00Z".to_string()),
            bundles: vec![CurrentArtifactsBundleRef {
                filename: "bundle_cycle_2604.json".to_string(),
                relative_path: "bundle_cycle_2604.json".to_string(),
                id: "cycle-2604".to_string(),
                bundle_type: "cycle".to_string(),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                start_valid: None,
                end_valid: None,
                checksum_sha256: None,
                size_bytes: None,
            }],
            startup_prefetch: None,
        };
        let bundle_2604 = BundleManifest {
            packages: vec![BundlePackageArtifact {
                id: "NAV_DB_2604_01".to_string(),
                family_id: "nav-db".to_string(),
                contract_id: crate::REQUIRED_NAV_DB_CONTRACT_ID.to_string(),
                region_id: None,
                filename: "nav_db_2604_01_good.zip".to_string(),
                relative_path: "nav_db_2604_01_good.zip".to_string(),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: None,
                size_bytes: None,
                effective_date: Some("2026-04-16".to_string()),
                expiration_date: Some("2026-05-14".to_string()),
                warning_text: None,
                metadata: Some(nav_db_metadata()),
            }],
        };
        let result = reduce_offline_packages_controller(&OfflinePackagesControllerInput {
            state: Some(OfflinePackagesControllerState {
                packages_state: Some(OfflinePackagesState {
                    preferences: default_offline_package_preferences(
                        Vec::<String>::new(),
                        ["nav-db"],
                    ),
                    now_override_epoch_ms: Some(1_777_120_000_000),
                }),
                library_cache: Some(OfflinePackagesLibraryCache {
                    package_source_base_url: "http://example.test".to_string(),
                    fetched_at_epoch_ms: 1_777_120_000_000,
                    discovery_manifests: vec![discovery],
                    bundle_manifests_by_filename: BTreeMap::from([(
                        "bundle_cycle_2604.json".to_string(),
                        bundle_2604,
                    )]),
                }),
                tombstoned_installed_filename_messages: BTreeMap::new(),
                suppressed_fetch_filename_messages: BTreeMap::new(),
                sync_progress: None,
                library_loading: false,
                library_error_message: None,
                sync_after_library_refresh: false,
                sync_in_flight: true,
                sync_message: None,
            }),
            package_source_base_url: "http://example.test".to_string(),
            discovery_filenames: vec![],
            now_epoch_ms: 1_777_120_000_000,
            installed: vec![InstalledArtifact {
                artifact_id: "NAV_DB_2604_01".to_string(),
                filename: "nav_db_2604_01_good.zip".to_string(),
                size_bytes: None,
                checksum_sha256: None,
                family_id: None,
                region_id: None,
                chart_package_tier: None,
            }],
            storage: None,
            event: OfflinePackagesControllerEvent::SyncFinished {
                summary: OfflinePackagesSyncSummary {
                    fetched_count: 1,
                    gc_count: 0,
                    warnings: vec![],
                    remote_poisoned_filename_messages: BTreeMap::from([(
                        "nav_db_2604_01_good.zip".to_string(),
                        "bad remote artifact".to_string(),
                    )]),
                },
            },
        });

        let navdb = result
            .ui_state
            .planner_ui_state
            .unwrap()
            .core_products
            .into_iter()
            .find(|row| row.id == "nav-db")
            .unwrap();
        assert_eq!(navdb.fetch_count, 0);
        assert_eq!(navdb.gc_count, 1);
        assert!(result
            .state
            .suppressed_fetch_filename_messages
            .contains_key("nav_db_2604_01_good.zip"));
    }

    #[test]
    fn offline_packages_controller_reports_storage_capacity_from_platform_facts() {
        let result = reduce_offline_packages_controller(&OfflinePackagesControllerInput {
            state: Some(OfflinePackagesControllerState::default()),
            package_source_base_url: "http://example.test".to_string(),
            discovery_filenames: vec![],
            now_epoch_ms: 1_777_120_000_000,
            installed: vec![],
            storage: Some(OfflinePackagesStorageInfo {
                available_bytes: 12_345_678_901,
                total_bytes: Some(64_000_000_000),
            }),
            event: OfflinePackagesControllerEvent::EnsureLibrary,
        });

        assert_eq!(
            result.ui_state.storage_capacity_label.as_deref(),
            Some("STORAGE 12G FREE / 64G TOTAL")
        );
    }

    #[test]
    fn offline_packages_controller_uses_cached_catalog_for_display_after_refresh_failure() {
        let discovery = CurrentArtifactsManifest {
            schema_version: Some(1),
            contracts: test_contracts(),
            artifact_roots: test_artifact_roots(),
            as_of_date: Some("2026-05-20".to_string()),
            as_of_utc: Some("2026-05-20T12:00:00Z".to_string()),
            bundles: vec![CurrentArtifactsBundleRef {
                filename: "bundle_cycle_2605.json".to_string(),
                relative_path: "bundle_cycle_2605.json".to_string(),
                id: "cycle-2605".to_string(),
                bundle_type: "cycle".to_string(),
                cycle: Some("2605".to_string()),
                cycle_version: Some("01".to_string()),
                start_valid: None,
                end_valid: None,
                checksum_sha256: None,
                size_bytes: None,
            }],
            startup_prefetch: None,
        };
        let bundle = BundleManifest {
            packages: vec![with_cycle_and_size(
                pkg(
                    "NW_SEC_2605",
                    "sec",
                    Some("nw"),
                    Some("2026-05-20"),
                    Some("2026-06-17"),
                ),
                "2605",
                52_000_000,
            )],
        };
        let result = reduce_offline_packages_controller(&OfflinePackagesControllerInput {
            state: Some(OfflinePackagesControllerState {
                packages_state: Some(OfflinePackagesState {
                    preferences: default_offline_package_preferences(["nw"], ["sec"]),
                    now_override_epoch_ms: Some(1_778_025_600_000),
                }),
                library_cache: Some(OfflinePackagesLibraryCache {
                    package_source_base_url: "http://example.test".to_string(),
                    fetched_at_epoch_ms: 1_778_025_600_000,
                    discovery_manifests: vec![discovery],
                    bundle_manifests_by_filename: BTreeMap::from([(
                        "bundle_cycle_2605.json".to_string(),
                        bundle,
                    )]),
                }),
                tombstoned_installed_filename_messages: BTreeMap::new(),
                suppressed_fetch_filename_messages: BTreeMap::new(),
                sync_progress: None,
                library_loading: true,
                library_error_message: None,
                sync_after_library_refresh: false,
                sync_in_flight: false,
                sync_message: None,
            }),
            package_source_base_url: "http://example.test".to_string(),
            discovery_filenames: vec![],
            now_epoch_ms: 1_778_025_600_000,
            installed: vec![installed_with_size("NW_SEC_2605", 52_000_000)],
            storage: None,
            event: OfflinePackagesControllerEvent::LibraryRefreshFailed {
                message: format!(
                    "app requires nav-db={}; artifacts offer nav-db=NAVBOGUS",
                    crate::REQUIRED_NAV_DB_CONTRACT_ID
                ),
            },
        });

        let ui_state = result.ui_state.planner_ui_state.unwrap();
        assert_eq!(ui_state.all_packages.installed_size_label, "52M");
        assert_eq!(ui_state.regions[0].installed_size_label, "52M");
        assert!(!result.ui_state.sync_enabled);
        assert!(result
            .ui_state
            .library_status_message
            .as_deref()
            .unwrap()
            .contains("Using cached package catalog"));
    }

    #[test]
    fn offline_packages_controller_reports_source_mismatched_cached_catalog_without_table() {
        let discovery = CurrentArtifactsManifest {
            schema_version: Some(1),
            contracts: test_contracts(),
            artifact_roots: test_artifact_roots(),
            as_of_date: Some("2026-05-20".to_string()),
            as_of_utc: Some("2026-05-20T12:00:00Z".to_string()),
            bundles: vec![CurrentArtifactsBundleRef {
                filename: "bundle_cycle_2605.json".to_string(),
                relative_path: "bundle_cycle_2605.json".to_string(),
                id: "cycle-2605".to_string(),
                bundle_type: "cycle".to_string(),
                cycle: Some("2605".to_string()),
                cycle_version: Some("01".to_string()),
                start_valid: None,
                end_valid: None,
                checksum_sha256: None,
                size_bytes: None,
            }],
            startup_prefetch: None,
        };
        let result = reduce_offline_packages_controller(&OfflinePackagesControllerInput {
            state: Some(OfflinePackagesControllerState {
                packages_state: Some(OfflinePackagesState {
                    preferences: default_offline_package_preferences(["nw"], ["sec"]),
                    now_override_epoch_ms: Some(1_778_025_600_000),
                }),
                library_cache: Some(OfflinePackagesLibraryCache {
                    package_source_base_url: "http://old-source.test".to_string(),
                    fetched_at_epoch_ms: 1_778_025_600_000,
                    discovery_manifests: vec![discovery],
                    bundle_manifests_by_filename: BTreeMap::new(),
                }),
                tombstoned_installed_filename_messages: BTreeMap::new(),
                suppressed_fetch_filename_messages: BTreeMap::new(),
                sync_progress: None,
                library_loading: true,
                library_error_message: None,
                sync_after_library_refresh: false,
                sync_in_flight: false,
                sync_message: None,
            }),
            package_source_base_url: "http://current-source.test".to_string(),
            discovery_filenames: vec![],
            now_epoch_ms: 1_778_025_600_000,
            installed: vec![installed_with_size("NW_SEC_2605", 52_000_000)],
            storage: None,
            event: OfflinePackagesControllerEvent::LibraryRefreshFailed {
                message: "Failed to connect to current-source.test".to_string(),
            },
        });

        assert!(result.ui_state.planner_ui_state.is_none());
        assert!(!result.ui_state.library_loaded);
        let message = result
            .ui_state
            .library_status_message
            .as_deref()
            .expect("library status message");
        assert!(message.contains("Cached package catalog is for http://old-source.test"));
        assert!(message.contains("current package source is http://current-source.test"));
        assert!(
            !message.contains("Using cached package catalog"),
            "source-mismatched cache must not claim it is being used"
        );
    }

    #[test]
    fn reducer_cycles_region_in_core() {
        let discovery = CurrentArtifactsManifest {
            schema_version: Some(1),
            contracts: test_contracts(),
            artifact_roots: test_artifact_roots(),
            as_of_date: Some("2026-04-15".to_string()),
            as_of_utc: Some("2026-04-15T12:00:00Z".to_string()),
            bundles: vec![CurrentArtifactsBundleRef {
                filename: "bundle_cycle_2604.json".to_string(),
                relative_path: "bundle_cycle_2604.json".to_string(),
                id: "cycle-2604".to_string(),
                bundle_type: "cycle".to_string(),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                start_valid: None,
                end_valid: None,
                checksum_sha256: None,
                size_bytes: None,
            }],
            startup_prefetch: None,
        };
        let bundles = BTreeMap::from([(
            "bundle_cycle_2604.json".to_string(),
            BundleManifest {
                packages: vec![pkg(
                    "NW_SEC_2604",
                    "sec",
                    Some("nw"),
                    None,
                    Some("2099-01-01"),
                )],
            },
        )]);
        let init = initialize_offline_packages(&OfflinePackagesInitInput {
            state: None,
            now_epoch_ms: 200,
            discovery_manifests: vec![discovery.clone()],
            bundle_manifests_by_filename: bundles.clone(),
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        });

        assert_eq!(
            init.ui_state.regions[0].selection,
            OfflinePackageSelection::Play
        );
        let paused = reduce_offline_packages(&OfflinePackagesReduceInput {
            state: init.state,
            event: OfflinePackagesEvent::CycleRegion {
                id: "nw".to_string(),
            },
            now_epoch_ms: 200,
            discovery_manifests: vec![discovery],
            bundle_manifests_by_filename: bundles,
            installed: vec![],
            forced_gc_installed_filenames: vec![],
            suppressed_fetch_filenames: vec![],
        });

        assert_eq!(
            paused.ui_state.regions[0].selection,
            OfflinePackageSelection::Pause
        );
    }

    #[test]
    fn discovery_selects_latest_manifest_not_after_now_and_merges_cycle_bundles() {
        let early = CurrentArtifactsManifest {
            schema_version: Some(1),
            contracts: test_contracts(),
            artifact_roots: test_artifact_roots(),
            as_of_date: Some("2026-03-25".to_string()),
            as_of_utc: Some("2026-03-25T12:00:00Z".to_string()),
            bundles: vec![CurrentArtifactsBundleRef {
                filename: "bundle_cycle_2603.json".to_string(),
                relative_path: "bundle_cycle_2603.json".to_string(),
                id: "cycle-2603".to_string(),
                bundle_type: "cycle".to_string(),
                cycle: Some("2603".to_string()),
                cycle_version: Some("01".to_string()),
                start_valid: None,
                end_valid: None,
                checksum_sha256: None,
                size_bytes: None,
            }],
            startup_prefetch: None,
        };
        let overlap = CurrentArtifactsManifest {
            schema_version: Some(1),
            contracts: test_contracts(),
            artifact_roots: test_artifact_roots(),
            as_of_date: Some("2026-04-15".to_string()),
            as_of_utc: Some("2026-04-15T12:00:00Z".to_string()),
            bundles: vec![
                CurrentArtifactsBundleRef {
                    filename: "bundle_cycle_2603.json".to_string(),
                    relative_path: "bundle_cycle_2603.json".to_string(),
                    id: "cycle-2603".to_string(),
                    bundle_type: "cycle".to_string(),
                    cycle: Some("2603".to_string()),
                    cycle_version: Some("01".to_string()),
                    start_valid: None,
                    end_valid: None,
                    checksum_sha256: None,
                    size_bytes: None,
                },
                CurrentArtifactsBundleRef {
                    filename: "bundle_cycle_2604.json".to_string(),
                    relative_path: "bundle_cycle_2604.json".to_string(),
                    id: "cycle-2604".to_string(),
                    bundle_type: "cycle".to_string(),
                    cycle: Some("2604".to_string()),
                    cycle_version: Some("01".to_string()),
                    start_valid: None,
                    end_valid: None,
                    checksum_sha256: None,
                    size_bytes: None,
                },
            ],
            startup_prefetch: None,
        };
        let bundles = BTreeMap::from([
            (
                "bundle_cycle_2603.json".to_string(),
                BundleManifest {
                    packages: vec![pkg(
                        "NW_SEC_2603",
                        "sec",
                        Some("nw"),
                        Some("2026-03-19"),
                        Some("2026-04-16"),
                    )],
                },
            ),
            (
                "bundle_cycle_2604.json".to_string(),
                BundleManifest {
                    packages: vec![pkg(
                        "NW_SEC_2604",
                        "sec",
                        Some("nw"),
                        Some("2026-04-16"),
                        Some("2026-05-14"),
                    )],
                },
            ),
        ]);

        let before_rollover = resolve_cycle_bundle_manifest(
            &[early.clone(), overlap.clone()],
            &bundles,
            as_of_utc_to_epoch_ms("2026-04-15T18:00:00Z").unwrap(),
        );
        assert_eq!(
            before_rollover
                .packages
                .iter()
                .map(|pkg| pkg.id.as_str())
                .collect::<Vec<_>>(),
            vec!["NW_SEC_2603", "NW_SEC_2604"]
        );

        let before_overlap = resolve_cycle_bundle_manifest(
            &[early, overlap],
            &bundles,
            as_of_utc_to_epoch_ms("2026-04-01T00:00:00Z").unwrap(),
        );
        assert_eq!(
            before_overlap
                .packages
                .iter()
                .map(|pkg| pkg.id.as_str())
                .collect::<Vec<_>>(),
            vec!["NW_SEC_2603"]
        );
    }

    #[test]
    fn synchronized_preferences_merge_without_starting_package_sync() {
        let local = OfflinePackagePreferences {
            regions: BTreeMap::from([("nw".to_string(), OfflinePackageSelection::Play)]),
            products: BTreeMap::from([("terrain".to_string(), OfflinePackageSelection::Pause)]),
        };
        let synchronized = OfflinePackagePreferences {
            regions: BTreeMap::from([("nw".to_string(), OfflinePackageSelection::Unselected)]),
            products: BTreeMap::new(),
        };
        let result = reduce_offline_packages_controller(&OfflinePackagesControllerInput {
            state: Some(OfflinePackagesControllerState {
                packages_state: Some(OfflinePackagesState {
                    preferences: local,
                    now_override_epoch_ms: None,
                }),
                ..OfflinePackagesControllerState::default()
            }),
            package_source_base_url: "https://example.test/packages".to_string(),
            discovery_filenames: Vec::new(),
            now_epoch_ms: 100,
            installed: Vec::new(),
            storage: None,
            event: OfflinePackagesControllerEvent::ApplySynchronizedPreferences {
                preferences: synchronized,
            },
        });

        let merged = result.preferences_for_cloud.unwrap();
        assert_eq!(merged.regions["nw"], OfflinePackageSelection::Unselected);
        assert_eq!(merged.products["terrain"], OfflinePackageSelection::Pause);
        assert!(result.command.is_none());

        let second = reduce_offline_packages_controller(&OfflinePackagesControllerInput {
            state: Some(result.state),
            package_source_base_url: "https://example.test/packages".to_string(),
            discovery_filenames: Vec::new(),
            now_epoch_ms: 101,
            installed: Vec::new(),
            storage: None,
            event: OfflinePackagesControllerEvent::ApplySynchronizedPreferences {
                preferences: merged,
            },
        });
        assert!(second.preferences_for_cloud.is_none());
        assert!(second.command.is_none());
    }
}
