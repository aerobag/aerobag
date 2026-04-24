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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest {
    pub packages: Vec<BundlePackageArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledArtifact {
    pub artifact_id: String,
    pub size_bytes: Option<u64>,
    pub checksum_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManagementInput {
    pub now_epoch_ms: i64,
    pub preferences: OfflinePackagePreferences,
    pub bundle: BundleManifest,
    pub installed: Vec<InstalledArtifact>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OfflinePackagesEvent {
    CycleRegion { id: String },
    CycleProduct { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesUiRow {
    pub id: String,
    pub selection: OfflinePackageSelection,
    pub fetch_count: usize,
    pub gc_count: usize,
    pub pause_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflinePackagesUiState {
    pub summary_text: String,
    pub regions: Vec<OfflinePackagesUiRow>,
    pub products: Vec<OfflinePackagesUiRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesInitInput {
    pub state: Option<OfflinePackagesState>,
    pub region_ids: Vec<String>,
    pub product_ids: Vec<String>,
    pub now_epoch_ms: i64,
    pub bundle: BundleManifest,
    pub installed: Vec<InstalledArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesReduceInput {
    pub state: OfflinePackagesState,
    pub event: OfflinePackagesEvent,
    pub region_ids: Vec<String>,
    pub product_ids: Vec<String>,
    pub now_epoch_ms: i64,
    pub bundle: BundleManifest,
    pub installed: Vec<InstalledArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfflinePackagesReduceResult {
    pub state: OfflinePackagesState,
    pub ui_state: OfflinePackagesUiState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AvailablePackageArtifact {
    artifact_id: String,
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

pub fn initialize_offline_packages(input: &OfflinePackagesInitInput) -> OfflinePackagesReduceResult {
    let state = OfflinePackagesState {
        preferences: normalize_preferences(
            input.state.as_ref().map(|state| &state.preferences),
            &input.region_ids,
            &input.product_ids,
        ),
    };
    OfflinePackagesReduceResult {
        ui_state: project_offline_packages_ui_state(
            &state,
            &input.region_ids,
            &input.product_ids,
            input.now_epoch_ms,
            &input.bundle,
            &input.installed,
        ),
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
    };

    match &input.event {
        OfflinePackagesEvent::CycleRegion { id } => {
            cycle_selection(&mut state.preferences.regions, id);
        }
        OfflinePackagesEvent::CycleProduct { id } => {
            cycle_selection(&mut state.preferences.products, id);
        }
    }

    state.preferences = normalize_preferences(
        Some(&state.preferences),
        &input.region_ids,
        &input.product_ids,
    );

    OfflinePackagesReduceResult {
        ui_state: project_offline_packages_ui_state(
            &state,
            &input.region_ids,
            &input.product_ids,
            input.now_epoch_ms,
            &input.bundle,
            &input.installed,
        ),
        state,
    }
}

pub fn plan_offline_packages(input: &PackageManagementInput) -> PackageManagementPlan {
    let available_artifacts: Vec<AvailablePackageArtifact> = input
        .bundle
        .packages
        .iter()
        .filter_map(bundle_package_to_artifact)
        .collect();

    let installed: BTreeSet<String> = input
        .installed
        .iter()
        .map(|artifact| artifact.artifact_id.clone())
        .collect();
    let available: BTreeSet<String> = available_artifacts
        .iter()
        .map(|artifact| artifact.artifact_id.clone())
        .collect();
    let mut fetch = BTreeSet::new();
    let mut retain_installed = BTreeSet::new();
    let mut protected_by_pause = BTreeSet::new();
    let mut slots_with_current_installed = BTreeSet::new();
    let mut stale_selected_installed = Vec::new();

    for artifact in &available_artifacts {
        match artifact_policy(input, artifact) {
            ArtifactPolicy::Desired => {
                if installed.contains(&artifact.artifact_id) {
                    retain_installed.insert(artifact.artifact_id.clone());
                    slots_with_current_installed.insert(artifact_slot(artifact));
                } else {
                    fetch.insert(artifact.artifact_id.clone());
                }
            }
            ArtifactPolicy::ProtectedByPause => {
                if installed.contains(&artifact.artifact_id) {
                    retain_installed.insert(artifact.artifact_id.clone());
                    protected_by_pause.insert(artifact.artifact_id.clone());
                }
            }
            ArtifactPolicy::NotSelected => {
                if installed.contains(&artifact.artifact_id)
                    && is_expired(input.now_epoch_ms, artifact)
                    && selected_state(input, artifact) == OfflinePackageSelection::Play
                {
                    stale_selected_installed.push(artifact);
                }
            }
        }
    }

    for artifact in stale_selected_installed {
        if !slots_with_current_installed.contains(&artifact_slot(artifact)) {
            retain_installed.insert(artifact.artifact_id.clone());
        }
    }

    let mut gc = BTreeSet::new();
    for artifact_id in &installed {
        if retain_installed.contains(artifact_id) || fetch.contains(artifact_id) {
            continue;
        }
        gc.insert(artifact_id.clone());
    }

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
    bundle: &BundleManifest,
    installed: &[InstalledArtifact],
) -> OfflinePackagesUiState {
    let plan = plan_offline_packages(&PackageManagementInput {
        now_epoch_ms,
        preferences: state.preferences.clone(),
        bundle: bundle.clone(),
        installed: installed.to_vec(),
    });
    let counts = plan_counts_by_dimension(bundle, &plan);

    OfflinePackagesUiState {
        summary_text: format!(
            "{} regions playing, {} products playing",
            state.preferences.regions.values().filter(|&&s| s == OfflinePackageSelection::Play).count(),
            state.preferences.products.values().filter(|&&s| s == OfflinePackageSelection::Play).count(),
        ),
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
                fetch_count: counts.regions.get(id).map_or(0, |counts| counts.fetch_count),
                gc_count: counts.regions.get(id).map_or(0, |counts| counts.gc_count),
                pause_count: counts.regions.get(id).map_or(0, |counts| counts.pause_count),
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
                fetch_count: counts.products.get(id).map_or(0, |counts| counts.fetch_count),
                gc_count: counts.products.get(id).map_or(0, |counts| counts.gc_count),
                pause_count: counts.products.get(id).map_or(0, |counts| counts.pause_count),
            })
            .collect(),
    }
}

#[derive(Default)]
struct DimensionCounts {
    fetch_count: usize,
    gc_count: usize,
    pause_count: usize,
}

#[derive(Default)]
struct PlanCountsByDimension {
    regions: BTreeMap<String, DimensionCounts>,
    products: BTreeMap<String, DimensionCounts>,
}

fn plan_counts_by_dimension(bundle: &BundleManifest, plan: &PackageManagementPlan) -> PlanCountsByDimension {
    let packages_by_id: BTreeMap<&str, &BundlePackageArtifact> =
        bundle.packages.iter().map(|pkg| (pkg.id.as_str(), pkg)).collect();
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
        }
        mutate(counts.products.entry(pkg.family_id.clone()).or_default());
    }

    for artifact_id in &plan.fetch {
        apply(&mut counts, &packages_by_id, artifact_id, |counts| counts.fetch_count += 1);
    }
    for artifact_id in &plan.gc {
        apply(&mut counts, &packages_by_id, artifact_id, |counts| counts.gc_count += 1);
    }
    for artifact_id in &plan.protected_by_pause {
        apply(&mut counts, &packages_by_id, artifact_id, |counts| counts.pause_count += 1);
    }

    counts
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

fn cycle_selection(
    selections: &mut BTreeMap<String, OfflinePackageSelection>,
    id: &str,
) {
    let next = match selections.get(id).copied().unwrap_or(OfflinePackageSelection::Play) {
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
            product_id: pkg.family_id.clone(),
            region_id: pkg.region_id.clone(),
            effective_at_epoch_ms: pkg.effective_date.as_deref().and_then(ymd_date_to_epoch_ms),
            expires_at_epoch_ms: pkg.expiration_date.as_deref().and_then(ymd_date_to_epoch_ms),
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

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) as i64
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
        }
    }

    fn installed(id: &str) -> InstalledArtifact {
        InstalledArtifact {
            artifact_id: id.to_string(),
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
                    pkg("NW_SEC_2603", "sec", Some("nw"), Some("2026-03-19"), Some("1970-01-01")),
                    pkg("NW_SEC_2604", "sec", Some("nw"), Some("2026-04-16"), Some("2099-01-01")),
                ],
            },
            installed: vec![installed("NW_SEC_2603")],
        };

        let plan = plan_offline_packages(&input);

        assert_eq!(plan.fetch, vec!["NW_SEC_2604"]);
        assert_eq!(plan.retain_installed, vec!["NW_SEC_2603"]);
        assert!(plan.gc.is_empty());
    }

    #[test]
    fn selected_expired_package_can_be_collected_after_replacement_is_installed() {
        let input = PackageManagementInput {
            now_epoch_ms: 200,
            preferences: default_offline_package_preferences(["nw"], ["sec"]),
            bundle: BundleManifest {
                packages: vec![
                    pkg("NW_SEC_2603", "sec", Some("nw"), Some("2026-03-19"), Some("1970-01-01")),
                    pkg("NW_SEC_2604", "sec", Some("nw"), Some("2026-04-16"), Some("2099-01-01")),
                ],
            },
            installed: vec![installed("NW_SEC_2603"), installed("NW_SEC_2604")],
        };

        let plan = plan_offline_packages(&input);

        assert!(plan.fetch.is_empty());
        assert_eq!(plan.retain_installed, vec!["NW_SEC_2604"]);
        assert_eq!(plan.gc, vec!["NW_SEC_2603"]);
    }

    #[test]
    fn multiple_not_yet_expired_cycles_in_one_selected_slot_are_all_desired() {
        let manifest = BundleManifest {
            packages: vec![
                pkg("NW_SEC_2603", "sec", Some("nw"), Some("2026-03-19"), Some("2099-01-01")),
                pkg("NW_SEC_2604", "sec", Some("nw"), Some("2099-04-16"), Some("2099-05-14")),
            ],
        };
        let preferences = default_offline_package_preferences(["nw"], ["sec"]);

        let missing_plan = plan_offline_packages(&PackageManagementInput {
            now_epoch_ms: 200,
            preferences: preferences.clone(),
            bundle: manifest.clone(),
            installed: vec![],
        });

        assert_eq!(missing_plan.fetch, vec!["NW_SEC_2603", "NW_SEC_2604"]);
        assert!(missing_plan.retain_installed.is_empty());
        assert!(missing_plan.gc.is_empty());

        let installed_plan = plan_offline_packages(&PackageManagementInput {
            now_epoch_ms: 200,
            preferences,
            bundle: manifest,
            installed: vec![installed("NW_SEC_2603"), installed("NW_SEC_2604")],
        });

        assert!(installed_plan.fetch.is_empty());
        assert_eq!(
            installed_plan.retain_installed,
            vec!["NW_SEC_2603", "NW_SEC_2604"]
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
                    pkg("NW_SEC_2604", "sec", Some("nw"), Some("2026-04-16"), Some("2099-01-01")),
                    pkg("NW_SEC_2603", "sec", Some("nw"), Some("2026-03-19"), Some("1970-01-01")),
                ],
            },
            installed: vec![installed("NW_SEC_2603")],
        };

        let plan = plan_offline_packages(&input);

        assert!(plan.fetch.is_empty());
        assert_eq!(plan.retain_installed, vec!["NW_SEC_2603"]);
        assert!(plan.gc.is_empty());
        assert_eq!(plan.protected_by_pause, vec!["NW_SEC_2603"]);
    }

    #[test]
    fn reducer_cycles_region_in_core() {
        let init = initialize_offline_packages(&OfflinePackagesInitInput {
            state: None,
            region_ids: vec!["nw".to_string()],
            product_ids: vec!["sec".to_string()],
            now_epoch_ms: 200,
            bundle: BundleManifest {
                packages: vec![pkg("NW_SEC_2604", "sec", Some("nw"), None, Some("2099-01-01"))],
            },
            installed: vec![],
        });

        assert_eq!(init.ui_state.regions[0].selection, OfflinePackageSelection::Play);
        assert_eq!(init.ui_state.summary_text, "1 regions playing, 1 products playing");

        let paused = reduce_offline_packages(&OfflinePackagesReduceInput {
            state: init.state,
            event: OfflinePackagesEvent::CycleRegion { id: "nw".to_string() },
            region_ids: vec!["nw".to_string()],
            product_ids: vec!["sec".to_string()],
            now_epoch_ms: 200,
            bundle: BundleManifest {
                packages: vec![pkg("NW_SEC_2604", "sec", Some("nw"), None, Some("2099-01-01"))],
            },
            installed: vec![],
        });

        assert_eq!(paused.ui_state.regions[0].selection, OfflinePackageSelection::Pause);
        assert_eq!(paused.ui_state.summary_text, "0 regions playing, 1 products playing");
    }
}
