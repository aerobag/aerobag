use std::collections::BTreeMap;

use app_core::{
    default_offline_package_preferences, BundleManifest, BundlePackageArtifact, InstalledArtifact,
    OfflinePackagePreferences, OfflinePackageSelection, PackageManagementInput,
    plan_offline_packages,
};

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
