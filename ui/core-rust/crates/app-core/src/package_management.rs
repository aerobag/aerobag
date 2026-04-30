use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePackageArtifact {
    pub id: String,
    pub family_id: String,
    pub region_id: Option<String>,
    pub filename: String,
    pub relative_path: String,
    pub cycle: Option<String>,
    pub cycle_version: Option<String>,
    pub checksum_sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub effective_date: Option<String>,
    pub expiration_date: Option<String>,
    pub metadata: Option<BundlePackageMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePackageMetadata {
    pub full_coverage_zoom: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub packages: Vec<BundlePackageArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentArtifactsManifest {
    pub schema_version: Option<u32>,
    pub as_of_date: Option<String>,
    pub as_of_utc: Option<String>,
    pub bundles: Vec<CurrentArtifactsBundleRef>,
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
pub struct InstalledArtifact {
    pub artifact_id: String,
    pub filename: String,
    pub size_bytes: Option<u64>,
    pub checksum_sha256: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesUiRow {
    pub id: String,
    pub selection: OfflinePackageSelection,
    pub fetch_count: usize,
    pub gc_count: usize,
    pub pause_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesClockOption {
    pub id: String,
    pub label: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagesUiState {
    pub summary_text: String,
    pub clock_label: String,
    pub clock_options: Vec<OfflinePackagesClockOption>,
    pub core_products: Vec<OfflinePackagesUiRow>,
    pub regions: Vec<OfflinePackagesUiRow>,
    pub products: Vec<OfflinePackagesUiRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesInitInput {
    pub state: Option<OfflinePackagesState>,
    pub region_ids: Vec<String>,
    pub product_ids: Vec<String>,
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
    pub region_ids: Vec<String>,
    pub product_ids: Vec<String>,
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
    pub library_loading: bool,
    pub library_error_message: Option<String>,
    pub sync_in_flight: bool,
    pub sync_message: Option<String>,
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
    InstalledArtifactHealthObserved {
        unreadable_installed_filename_messages: BTreeMap<String, String>,
    },
    PackagesEvent {
        event: OfflinePackagesEvent,
    },
    SyncRequested,
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
        plan: PackageManagementPlan,
        bundle: BundleManifest,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagesControllerUiState {
    pub planner_ui_state: Option<OfflinePackagesUiState>,
    pub library_loaded: bool,
    pub library_loading: bool,
    pub library_error_message: Option<String>,
    pub sync_in_flight: bool,
    pub sync_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesControllerInput {
    pub state: Option<OfflinePackagesControllerState>,
    pub package_source_base_url: String,
    pub discovery_filenames: Vec<String>,
    pub region_ids: Vec<String>,
    pub product_ids: Vec<String>,
    pub now_epoch_ms: i64,
    pub installed: Vec<InstalledArtifact>,
    pub event: OfflinePackagesControllerEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesControllerResult {
    pub state: OfflinePackagesControllerState,
    pub ui_state: OfflinePackagesControllerUiState,
    pub command: Option<OfflinePackagesControllerCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AvailablePackageArtifact {
    artifact_id: String,
    filename: String,
    product_id: String,
    region_id: Option<String>,
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
            .map(|id| (id.into(), OfflinePackageSelection::Play))
            .collect(),
    }
}

pub fn initialize_offline_packages(
    input: &OfflinePackagesInitInput,
) -> OfflinePackagesReduceResult {
    let state = OfflinePackagesState {
        preferences: normalize_preferences(
            input.state.as_ref().map(|state| &state.preferences),
            &input.region_ids,
            &input.product_ids,
        ),
        now_override_epoch_ms: input
            .state
            .as_ref()
            .and_then(|state| state.now_override_epoch_ms),
    };
    let effective_now_epoch_ms = effective_now_epoch_ms(&state, input.now_epoch_ms);
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
            &input.region_ids,
            &input.product_ids,
            effective_now_epoch_ms,
            &input.discovery_manifests,
            &input.bundle_manifests_by_filename,
            &input.installed,
            &input.forced_gc_installed_filenames,
            &input.suppressed_fetch_filenames,
        ),
        effective_now_epoch_ms,
        plan,
        bundle,
        state,
    }
}

pub fn reduce_offline_packages(input: &OfflinePackagesReduceInput) -> OfflinePackagesReduceResult {
    let mut state = OfflinePackagesState {
        preferences: normalize_preferences(
            Some(&input.state.preferences),
            &input.region_ids,
            &input.product_ids,
        ),
        now_override_epoch_ms: input.state.now_override_epoch_ms,
    };

    match &input.event {
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

    state.preferences = normalize_preferences(
        Some(&state.preferences),
        &input.region_ids,
        &input.product_ids,
    );
    let effective_now_epoch_ms = effective_now_epoch_ms(&state, input.now_epoch_ms);
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
            &input.region_ids,
            &input.product_ids,
            effective_now_epoch_ms,
            &input.discovery_manifests,
            &input.bundle_manifests_by_filename,
            &input.installed,
            &input.forced_gc_installed_filenames,
            &input.suppressed_fetch_filenames,
        ),
        effective_now_epoch_ms,
        plan,
        bundle,
        state,
    }
}

pub fn reduce_offline_packages_controller(
    input: &OfflinePackagesControllerInput,
) -> OfflinePackagesControllerResult {
    let mut state = input.state.clone().unwrap_or_default();
    let package_source_base_url = input
        .package_source_base_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    let mut command = None;

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
        }
        OfflinePackagesControllerEvent::LibraryRefreshFailed { message } => {
            state.library_loading = false;
            state.library_error_message = Some(message.clone());
        }
        OfflinePackagesControllerEvent::InstalledArtifactHealthObserved {
            unreadable_installed_filename_messages,
        } => {
            state.tombstoned_installed_filename_messages =
                unreadable_installed_filename_messages.clone();
        }
        OfflinePackagesControllerEvent::PackagesEvent { event } => {
            let Some(library_cache) = state.library_cache.as_ref() else {
                state.library_error_message =
                    Some("offline packages library is not loaded".to_string());
                return OfflinePackagesControllerResult {
                    ui_state: project_offline_packages_controller_ui_state(
                        &state,
                        &package_source_base_url,
                        None,
                    ),
                    state,
                    command,
                };
            };
            let reduced = reduce_offline_packages(&OfflinePackagesReduceInput {
                state: state.packages_state.clone().unwrap_or_default(),
                event: event.clone(),
                region_ids: input.region_ids.clone(),
                product_ids: input.product_ids.clone(),
                now_epoch_ms: input.now_epoch_ms,
                discovery_manifests: library_cache.discovery_manifests.clone(),
                bundle_manifests_by_filename: library_cache.bundle_manifests_by_filename.clone(),
                installed: effective_installed_artifacts(&state, &input.installed),
                forced_gc_installed_filenames: forced_gc_installed_filenames(
                    &state,
                    &input.installed,
                ),
                suppressed_fetch_filenames: state
                    .suppressed_fetch_filename_messages
                    .keys()
                    .cloned()
                    .collect(),
            });
            state.packages_state = Some(reduced.state.clone());
            return OfflinePackagesControllerResult {
                ui_state: project_offline_packages_controller_ui_state(
                    &state,
                    &package_source_base_url,
                    Some(reduced.ui_state),
                ),
                state,
                command,
            };
        }
        OfflinePackagesControllerEvent::SyncRequested => {
            let Some(library_cache) = state.library_cache.as_ref() else {
                state.library_error_message =
                    Some("offline packages library is not loaded".to_string());
                return OfflinePackagesControllerResult {
                    ui_state: project_offline_packages_controller_ui_state(
                        &state,
                        &package_source_base_url,
                        None,
                    ),
                    state,
                    command,
                };
            };
            let current = initialize_offline_packages(&OfflinePackagesInitInput {
                state: state.packages_state.clone(),
                region_ids: input.region_ids.clone(),
                product_ids: input.product_ids.clone(),
                now_epoch_ms: input.now_epoch_ms,
                discovery_manifests: library_cache.discovery_manifests.clone(),
                bundle_manifests_by_filename: library_cache.bundle_manifests_by_filename.clone(),
                installed: effective_installed_artifacts(&state, &input.installed),
                forced_gc_installed_filenames: forced_gc_installed_filenames(
                    &state,
                    &input.installed,
                ),
                suppressed_fetch_filenames: state
                    .suppressed_fetch_filename_messages
                    .keys()
                    .cloned()
                    .collect(),
            });
            state.packages_state = Some(current.state.clone());
            state.sync_in_flight = true;
            command = Some(OfflinePackagesControllerCommand::Sync {
                package_source_base_url: package_source_base_url.clone(),
                plan: current.plan,
                bundle: current.bundle,
            });
            return OfflinePackagesControllerResult {
                ui_state: project_offline_packages_controller_ui_state(
                    &state,
                    &package_source_base_url,
                    Some(current.ui_state),
                ),
                state,
                command,
            };
        }
        OfflinePackagesControllerEvent::SyncFinished { summary } => {
            state.sync_in_flight = false;
            state.sync_message = Some(format_offline_packages_sync_summary(summary));
            state
                .suppressed_fetch_filename_messages
                .extend(summary.remote_poisoned_filename_messages.clone());
            state
                .tombstoned_installed_filename_messages
                .extend(summary.remote_poisoned_filename_messages.clone());
        }
    }

    let effective_installed = effective_installed_artifacts(&state, &input.installed);
    let forced_gc_installed_filenames = forced_gc_installed_filenames(&state, &input.installed);
    let planner_ui_state = replan_controller_ui_state(
        &mut state,
        &package_source_base_url,
        &input.region_ids,
        &input.product_ids,
        input.now_epoch_ms,
        &effective_installed,
        &forced_gc_installed_filenames,
    );
    OfflinePackagesControllerResult {
        ui_state: project_offline_packages_controller_ui_state(
            &state,
            &package_source_base_url,
            planner_ui_state,
        ),
        state,
        command,
    }
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
    region_ids: &[String],
    product_ids: &[String],
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
        region_ids: region_ids.to_vec(),
        product_ids: product_ids.to_vec(),
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
    state.packages_state = Some(reduced.state);
    Some(reduced.ui_state)
}

fn project_offline_packages_controller_ui_state(
    state: &OfflinePackagesControllerState,
    package_source_base_url: &str,
    planner_ui_state: Option<OfflinePackagesUiState>,
) -> OfflinePackagesControllerUiState {
    OfflinePackagesControllerUiState {
        planner_ui_state,
        library_loaded: state.library_cache.as_ref().is_some_and(|cache| {
            cache.package_source_base_url == package_source_base_url
                && !cache.discovery_manifests.is_empty()
        }),
        library_loading: state.library_loading,
        library_error_message: state.library_error_message.clone(),
        sync_in_flight: state.sync_in_flight,
        sync_message: state.sync_message.clone(),
    }
}

fn format_offline_packages_sync_summary(summary: &OfflinePackagesSyncSummary) -> String {
    let base = format!(
        "SYNC fetched {}, GC {}",
        summary.fetched_count, summary.gc_count
    );
    if summary.warnings.is_empty() {
        return base;
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
    format!(
        "{base}. WARN {}: {}",
        summary.warnings.len(),
        parts.join(" | ")
    )
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
    let mut fetch = BTreeSet::new();
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
                    fetch.insert(artifact.artifact_id.clone());
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
            || (fetch.contains(&installed.artifact_id) && !forced_gc_filenames.contains(filename))
        {
            continue;
        }
        gc.insert(filename.clone());
    }
    gc.extend(forced_gc_filenames);

    PackageManagementPlan {
        fetch: fetch.into_iter().collect(),
        retain_installed: retain_installed.into_iter().collect(),
        gc: gc.into_iter().collect(),
        protected_by_pause: protected_by_pause.into_iter().collect(),
    }
}

fn project_offline_packages_ui_state(
    state: &OfflinePackagesState,
    region_ids: &[String],
    product_ids: &[String],
    now_epoch_ms: i64,
    discovery_manifests: &[CurrentArtifactsManifest],
    bundle_manifests_by_filename: &BTreeMap<String, BundleManifest>,
    installed: &[InstalledArtifact],
    forced_gc_installed_filenames: &[String],
    suppressed_fetch_filenames: &[String],
) -> OfflinePackagesUiState {
    let bundle = resolve_cycle_bundle_manifest(
        discovery_manifests,
        bundle_manifests_by_filename,
        now_epoch_ms,
    );
    let plan = plan_offline_packages(&PackageManagementInput {
        now_epoch_ms,
        preferences: state.preferences.clone(),
        bundle,
        installed: installed.to_vec(),
        forced_gc_installed_filenames: forced_gc_installed_filenames.to_vec(),
        suppressed_fetch_filenames: suppressed_fetch_filenames.to_vec(),
    });
    let counts = plan_counts_by_dimension(
        &plan,
        discovery_manifests,
        bundle_manifests_by_filename,
        now_epoch_ms,
        installed,
    );

    OfflinePackagesUiState {
        summary_text: format!(
            "{} regions playing, {} products playing",
            state
                .preferences
                .regions
                .values()
                .filter(|&&s| s == OfflinePackageSelection::Play)
                .count(),
            state
                .preferences
                .products
                .values()
                .filter(|&&s| s == OfflinePackageSelection::Play)
                .count(),
        ),
        clock_label: clock_label(now_epoch_ms, state.now_override_epoch_ms),
        clock_options: clock_options(discovery_manifests, state.now_override_epoch_ms),
        core_products: {
            let active_bundle = resolve_cycle_bundle_manifest(
                discovery_manifests,
                bundle_manifests_by_filename,
                now_epoch_ms,
            );
            let mut ids = BTreeSet::new();
            for pkg in &active_bundle.packages {
                if pkg.region_id.is_none()
                    && !product_ids
                        .iter()
                        .any(|product_id| product_id == &pkg.family_id)
                {
                    ids.insert(pkg.family_id.clone());
                }
            }
            ids.extend(counts.core_products.keys().cloned());
            ids.into_iter()
                .map(|id| {
                    let counts = counts.core_products.get(&id);
                    OfflinePackagesUiRow {
                        id,
                        selection: OfflinePackageSelection::Play,
                        fetch_count: counts.map_or(0, |counts| counts.fetch_count),
                        gc_count: counts.map_or(0, |counts| counts.gc_count),
                        pause_count: counts.map_or(0, |counts| counts.pause_count),
                    }
                })
                .collect()
        },
        regions: region_ids
            .iter()
            .map(|id| OfflinePackagesUiRow {
                id: id.clone(),
                selection: state
                    .preferences
                    .regions
                    .get(id)
                    .copied()
                    .unwrap_or(OfflinePackageSelection::Play),
                fetch_count: counts
                    .regions
                    .get(id)
                    .map_or(0, |counts| counts.fetch_count),
                gc_count: counts.regions.get(id).map_or(0, |counts| counts.gc_count),
                pause_count: counts
                    .regions
                    .get(id)
                    .map_or(0, |counts| counts.pause_count),
            })
            .collect(),
        products: product_ids
            .iter()
            .map(|id| OfflinePackagesUiRow {
                id: id.clone(),
                selection: state
                    .preferences
                    .products
                    .get(id)
                    .copied()
                    .unwrap_or(OfflinePackageSelection::Play),
                fetch_count: counts
                    .products
                    .get(id)
                    .map_or(0, |counts| counts.fetch_count),
                gc_count: counts.products.get(id).map_or(0, |counts| counts.gc_count),
                pause_count: counts
                    .products
                    .get(id)
                    .map_or(0, |counts| counts.pause_count),
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
struct DimensionCounts {
    fetch_count: usize,
    gc_count: usize,
    pause_count: usize,
}

#[derive(Default)]
struct PlanCountsByDimension {
    core_products: BTreeMap<String, DimensionCounts>,
    regions: BTreeMap<String, DimensionCounts>,
    products: BTreeMap<String, DimensionCounts>,
}

fn plan_counts_by_dimension_from_packages(
    plan: &PackageManagementPlan,
    packages_by_id: &BTreeMap<&str, &BundlePackageArtifact>,
    installed_by_filename: &BTreeMap<&str, &InstalledArtifact>,
) -> PlanCountsByDimension {
    let mut counts = PlanCountsByDimension::default();

    fn apply(
        counts: &mut PlanCountsByDimension,
        packages_by_id: &BTreeMap<&str, &BundlePackageArtifact>,
        artifact_id: &str,
        mutate: impl Fn(&mut DimensionCounts),
    ) {
        let Some(pkg) = packages_by_id.get(artifact_id) else {
            return;
        };
        if let Some(region_id) = &pkg.region_id {
            mutate(counts.regions.entry(region_id.clone()).or_default());
        } else {
            mutate(
                counts
                    .core_products
                    .entry(pkg.family_id.clone())
                    .or_default(),
            );
        }
        mutate(counts.products.entry(pkg.family_id.clone()).or_default());
    }

    for artifact_id in &plan.fetch {
        apply(&mut counts, &packages_by_id, artifact_id, |counts| {
            counts.fetch_count += 1
        });
    }
    for filename in &plan.gc {
        let Some(installed) = installed_by_filename.get(filename.as_str()) else {
            continue;
        };
        apply(
            &mut counts,
            &packages_by_id,
            &installed.artifact_id,
            |counts| counts.gc_count += 1,
        );
    }
    for filename in &plan.protected_by_pause {
        let Some(installed) = installed_by_filename.get(filename.as_str()) else {
            continue;
        };
        apply(
            &mut counts,
            &packages_by_id,
            &installed.artifact_id,
            |counts| counts.pause_count += 1,
        );
    }

    counts
}

fn plan_counts_by_dimension(
    plan: &PackageManagementPlan,
    discovery_manifests: &[CurrentArtifactsManifest],
    bundle_manifests_by_filename: &BTreeMap<String, BundleManifest>,
    now_epoch_ms: i64,
    installed: &[InstalledArtifact],
) -> PlanCountsByDimension {
    let _ = resolve_cycle_bundle_manifest(
        discovery_manifests,
        bundle_manifests_by_filename,
        now_epoch_ms,
    );
    let packages_by_id: BTreeMap<&str, &BundlePackageArtifact> = bundle_manifests_by_filename
        .values()
        .flat_map(|bundle| bundle.packages.iter())
        .map(|pkg| (pkg.id.as_str(), pkg))
        .collect();
    let installed_by_filename: BTreeMap<&str, &InstalledArtifact> = installed
        .iter()
        .map(|artifact| (artifact.filename.as_str(), artifact))
        .collect();
    plan_counts_by_dimension_from_packages(plan, &packages_by_id, &installed_by_filename)
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
                        .unwrap_or(OfflinePackageSelection::Play),
                )
            })
            .collect(),
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
    match pkg.family_id.as_str() {
        "sec" | "tac" | "shaded-relief" | "enr-l" | "enr-h" | "tpp" | "csup" | "nav-db"
        | "vectors" | "geo" | "terrain" => Some(AvailablePackageArtifact {
            artifact_id: pkg.id.clone(),
            filename: pkg.filename.clone(),
            product_id: pkg.family_id.clone(),
            region_id: pkg.region_id.clone(),
            effective_at_epoch_ms: pkg.effective_date.as_deref().and_then(ymd_date_to_epoch_ms),
            expires_at_epoch_ms: pkg
                .expiration_date
                .as_deref()
                .and_then(ymd_date_to_epoch_ms),
        }),
        _ => None,
    }
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

fn artifact_slot(artifact: &AvailablePackageArtifact) -> (String, Option<String>) {
    (artifact.product_id.clone(), artifact.region_id.clone())
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

    match (region, product) {
        (OfflinePackageSelection::Unselected, _) | (_, OfflinePackageSelection::Unselected) => {
            OfflinePackageSelection::Unselected
        }
        (OfflinePackageSelection::Pause, _) | (_, OfflinePackageSelection::Pause) => {
            OfflinePackageSelection::Pause
        }
        (OfflinePackageSelection::Play, OfflinePackageSelection::Play) => {
            OfflinePackageSelection::Play
        }
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
            region_id: region.map(str::to_string),
            filename: format!("{id}.zip"),
            relative_path: format!("{id}.zip"),
            cycle: Some("2604".to_string()),
            cycle_version: Some("01".to_string()),
            checksum_sha256: None,
            size_bytes: None,
            effective_date: effective.map(str::to_string),
            expiration_date: expires.map(str::to_string),
            metadata: None,
        }
    }

    fn installed(id: &str) -> InstalledArtifact {
        InstalledArtifact {
            artifact_id: id.to_string(),
            filename: format!("{id}.zip"),
            size_bytes: None,
            checksum_sha256: None,
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
    fn forced_gc_bad_navdbs_show_fetch_and_gc_in_core_row() {
        let discovery = CurrentArtifactsManifest {
            schema_version: Some(1),
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
        };
        let bundle_2603 = BundleManifest {
            packages: vec![BundlePackageArtifact {
                id: "NAV_DB_2603_01".to_string(),
                family_id: "nav-db".to_string(),
                region_id: None,
                filename: "nav_db_2603.zip".to_string(),
                relative_path: "nav_db_2603.zip".to_string(),
                cycle: Some("2603".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: None,
                size_bytes: None,
                effective_date: Some("2026-03-20".to_string()),
                expiration_date: Some("2026-04-16".to_string()),
                metadata: None,
            }],
        };
        let bundle_2604 = BundleManifest {
            packages: vec![BundlePackageArtifact {
                id: "NAV_DB_2604_01".to_string(),
                family_id: "nav-db".to_string(),
                region_id: None,
                filename: "nav_db_2604.zip".to_string(),
                relative_path: "nav_db_2604.zip".to_string(),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: None,
                size_bytes: None,
                effective_date: Some("2026-04-16".to_string()),
                expiration_date: Some("2026-05-14".to_string()),
                metadata: None,
            }],
        };

        let result = reduce_offline_packages_controller(&OfflinePackagesControllerInput {
            state: Some(OfflinePackagesControllerState {
                packages_state: Some(OfflinePackagesState {
                    preferences: default_offline_package_preferences(
                        Vec::<String>::new(),
                        ["nav-db"],
                    ),
                    now_override_epoch_ms: Some(1_774_401_600_000),
                }),
                library_cache: Some(OfflinePackagesLibraryCache {
                    package_source_base_url: "http://example.test".to_string(),
                    fetched_at_epoch_ms: 1_774_401_600_000,
                    discovery_manifests: vec![discovery],
                    bundle_manifests_by_filename: BTreeMap::from([
                        ("bundle_cycle_2603.json".to_string(), bundle_2603),
                        ("bundle_cycle_2604.json".to_string(), bundle_2604),
                    ]),
                }),
                tombstoned_installed_filename_messages: BTreeMap::new(),
                suppressed_fetch_filename_messages: BTreeMap::new(),
                library_loading: false,
                library_error_message: None,
                sync_in_flight: false,
                sync_message: None,
            }),
            package_source_base_url: "http://example.test".to_string(),
            discovery_filenames: vec![],
            region_ids: vec![],
            product_ids: vec!["nav-db".to_string()],
            now_epoch_ms: 1_774_401_600_000,
            installed: vec![installed("NAV_DB_2603_01"), installed("NAV_DB_2604_01")],
            event: OfflinePackagesControllerEvent::InstalledArtifactHealthObserved {
                unreadable_installed_filename_messages: BTreeMap::from([
                    ("NAV_DB_2603_01.zip".to_string(), "bad".to_string()),
                    ("NAV_DB_2604_01.zip".to_string(), "bad".to_string()),
                ]),
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
        assert_eq!(navdb.fetch_count, 1);
        assert_eq!(navdb.gc_count, 2);
    }

    #[test]
    fn remote_poisoned_filename_is_suppressed_from_refetch() {
        let discovery = CurrentArtifactsManifest {
            schema_version: Some(1),
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
        };
        let bundle_2604 = BundleManifest {
            packages: vec![BundlePackageArtifact {
                id: "NAV_DB_2604_01".to_string(),
                family_id: "nav-db".to_string(),
                region_id: None,
                filename: "nav_db_2604_01_good.zip".to_string(),
                relative_path: "nav_db_2604_01_good.zip".to_string(),
                cycle: Some("2604".to_string()),
                cycle_version: Some("01".to_string()),
                checksum_sha256: None,
                size_bytes: None,
                effective_date: Some("2026-04-16".to_string()),
                expiration_date: Some("2026-05-14".to_string()),
                metadata: None,
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
                library_loading: false,
                library_error_message: None,
                sync_in_flight: true,
                sync_message: None,
            }),
            package_source_base_url: "http://example.test".to_string(),
            discovery_filenames: vec![],
            region_ids: vec![],
            product_ids: vec!["nav-db".to_string()],
            now_epoch_ms: 1_777_120_000_000,
            installed: vec![InstalledArtifact {
                artifact_id: "NAV_DB_2604_01".to_string(),
                filename: "nav_db_2604_01_good.zip".to_string(),
                size_bytes: None,
                checksum_sha256: None,
            }],
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
    fn reducer_cycles_region_in_core() {
        let discovery = CurrentArtifactsManifest {
            schema_version: Some(1),
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
            region_ids: vec!["nw".to_string()],
            product_ids: vec!["sec".to_string()],
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
        assert_eq!(
            init.ui_state.summary_text,
            "1 regions playing, 1 products playing"
        );

        let paused = reduce_offline_packages(&OfflinePackagesReduceInput {
            state: init.state,
            event: OfflinePackagesEvent::CycleRegion {
                id: "nw".to_string(),
            },
            region_ids: vec!["nw".to_string()],
            product_ids: vec!["sec".to_string()],
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
        assert_eq!(
            paused.ui_state.summary_text,
            "0 regions playing, 1 products playing"
        );
    }

    #[test]
    fn discovery_selects_latest_manifest_not_after_now_and_merges_cycle_bundles() {
        let early = CurrentArtifactsManifest {
            schema_version: Some(1),
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
        };
        let overlap = CurrentArtifactsManifest {
            schema_version: Some(1),
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
}
