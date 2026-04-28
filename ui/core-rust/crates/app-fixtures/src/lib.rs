use app_core::{GeometryBundle, PolygonRecord};
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn sample_geometry() -> GeometryBundle {
    GeometryBundle {
        schema_version: 1,
        polygons: vec![PolygonRecord {
            id: "sec:boston".to_string(),
            points: vec![[-72.0, 43.0], [-72.0, 41.0], [-69.0, 41.0], [-69.0, 43.0]],
        }],
    }
}

pub fn sample_geometry_json() -> String {
    serde_json::to_string(&sample_geometry()).expect("sample geometry should serialize")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .expect("resolve repository root")
}

pub fn fixture_snapshot_root() -> PathBuf {
    std::env::var_os("AEROBAG_FIXTURE_SNAPSHOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/root/aerobag-artifacts-snapshot"))
}

pub fn fixture_published_unpacked_root() -> PathBuf {
    if let Some(value) = std::env::var_os("AEROBAG_FIXTURE_UNPACKED_ROOT") {
        let path = PathBuf::from(value);
        assert!(
            path.is_dir(),
            "AEROBAG_FIXTURE_UNPACKED_ROOT does not name a directory: {}",
            path.display()
        );
        return path;
    }
    let root = fixture_snapshot_root().join("published-unpacked");
    assert!(
        root.is_dir(),
        "fixture published-unpacked root missing: {}",
        root.display()
    );
    root
}

pub fn fixture_published_packaged_root() -> PathBuf {
    let root = fixture_snapshot_root().join("published-packaged");
    assert!(
        root.is_dir(),
        "fixture published-packaged root missing: {}",
        root.display()
    );
    root
}

pub fn fixture_nav_db_package_zip_path() -> PathBuf {
    if let Some(value) = std::env::var_os("AEROBAG_FIXTURE_NAV_DB_PACKAGE") {
        let path = PathBuf::from(value);
        assert!(
            path.is_file(),
            "AEROBAG_FIXTURE_NAV_DB_PACKAGE does not name a file: {}",
            path.display()
        );
        return path;
    }
    let root = fixture_published_packaged_root();
    let mut matches = fs::read_dir(&root)
        .unwrap_or_else(|err| panic!("read {}: {err}", root.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("nav_db_") && name.ends_with(".zip"))
        })
        .collect::<Vec<_>>();
    matches.sort();
    assert!(
        !matches.is_empty(),
        "expected at least one nav_db zip under {}, found 0",
        root.display(),
    );
    matches.pop().expect("nav_db match after non-empty check")
}

pub fn load_fixture_nav_kv_pages() -> (Vec<u8>, Vec<Vec<u8>>) {
    let zip_path = fixture_nav_db_package_zip_path();
    let file =
        fs::File::open(&zip_path).unwrap_or_else(|err| panic!("open {}: {err}", zip_path.display()));
    let mut archive = zip::ZipArchive::new(file)
        .unwrap_or_else(|err| panic!("parse {} as zip: {err}", zip_path.display()));

    let mut has_root = false;
    for index in 0..archive.len() {
        let name = archive
            .by_index(index)
            .unwrap_or_else(|err| panic!("read zip entry {index} from {}: {err}", zip_path.display()))
            .name()
            .to_string();
        if name == "root" {
            has_root = true;
            break;
        }
    }
    if !has_root {
        panic!(
            "nav_db package {} does not contain a root nav_kv entry",
            zip_path.display()
        );
    }
    let root_name = "root";

    let mut root_bytes = Vec::new();
    archive
        .by_name(root_name)
        .unwrap_or_else(|err| panic!("open {} in {}: {err}", root_name, zip_path.display()))
        .read_to_end(&mut root_bytes)
        .unwrap_or_else(|err| panic!("read {} in {}: {err}", root_name, zip_path.display()));

    let root = app_core::NavKvRoot::parse(&root_bytes)
        .unwrap_or_else(|err| panic!("parse {} in {}: {err}", root_name, zip_path.display()));
    let page_count = ((root.value_bytes_len() + root.page_size() - 1) / root.page_size()) as usize;
    let mut pages = Vec::with_capacity(page_count);
    for page_index in 0..page_count {
        let entry_name = format!("values_{page_index:04}");
        let mut page_bytes = Vec::new();
        archive
            .by_name(&entry_name)
            .unwrap_or_else(|err| {
                panic!(
                    "open {} in {}: {err}",
                    entry_name,
                    zip_path.display()
                )
            })
            .read_to_end(&mut page_bytes)
            .unwrap_or_else(|err| {
                panic!(
                    "read {} in {}: {err}",
                    entry_name,
                    zip_path.display()
                )
            });
        pages.push(page_bytes);
    }
    (root_bytes, pages)
}

pub fn generated_static_vectors_root() -> PathBuf {
    let ui_dir = repo_root().join("ui");
    let target_root_raw =
        fs::read_to_string(ui_dir.join("target-root.txt")).expect("read ui/target-root.txt");
    let target_root = repo_root()
        .join(target_root_raw.trim())
        .canonicalize()
        .expect("resolve ui target root");
    let path = target_root.join("web/generated-static/vectors");
    assert!(
        path.is_dir(),
        "generated vector fixture root missing: {}",
        path.display()
    );
    path
}

pub fn fixture_vector_tile_root(layer: &str, z: u8) -> PathBuf {
    if let Some(value) = std::env::var_os("AEROBAG_FIXTURE_VECTOR_ROOT") {
        let path = PathBuf::from(value);
        assert!(
            path.is_dir(),
            "AEROBAG_FIXTURE_VECTOR_ROOT does not name a directory: {}",
            path.display()
        );
        return path;
    }
    let path = generated_static_vectors_root().join(format!("points/{layer}/{z}"));
    assert!(
        path.is_dir(),
        "generated vector tile fixture root missing: {}",
        path.display()
    );
    path
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanningScenario {
    pub id: &'static str,
    pub title: &'static str,
    pub tags: &'static [&'static str],
    pub summary: &'static str,
    pub expected_behavior: &'static str,
}

pub fn planning_bootstrap_scenarios() -> Vec<PlanningScenario> {
    vec![
        PlanningScenario {
            id: "airway_insert_between_existing_fixes",
            title: "Insert airway between existing fixes",
            tags: &["airway", "editing", "resolution"],
            summary: "A plan already contains the airway entry and exit fixes, and airway insertion should expand the published intermediate fixes while preserving the airway as a route component.",
            expected_behavior: "The resolved leg sequence should include the airway interior fixes in published order, and deleting the airway later should remove the airway component cleanly instead of leaving orphaned metadata.",
        },
        PlanningScenario {
            id: "airway_branch_selection_uses_internal_unique_key",
            title: "Airway branch selection pins one internal airway key",
            tags: &["airway", "resolution", "editing"],
            summary: "A displayed airway name such as V16 corresponds to multiple internal bead strings. After the user or resolver selects the entry waypoint, the app should bind the route to one internal branch identity such as V16-A while continuing to display V16.",
            expected_behavior: "Entry search may inspect every branch that displays as V16, but once one branch is selected, only that internal airway key should be used for exit selection, route expansion, and later edits.",
        },
        PlanningScenario {
            id: "delete_waypoint_inside_airway_requires_clean_decomposition",
            title: "Delete interior waypoint from airway",
            tags: &["airway", "editing"],
            summary: "A pilot removes a waypoint that lies inside an airway-derived span.",
            expected_behavior: "The result should either remove or split the airway component into meaningful remaining structure. It must not leave a broken airway marker that no longer corresponds to a valid database concept.",
        },
        PlanningScenario {
            id: "select_sid_with_transition_preserves_procedure_identity",
            title: "Select SID with runway and transition",
            tags: &["procedure", "sid", "resolution"],
            summary: "The departure airport receives a SID selection that includes runway choice and exit transition.",
            expected_behavior: "The stored route should preserve the SID identity and selected transition while the resolved leg sequence expands into flyable legs with provenance back to that procedure selection.",
        },
        PlanningScenario {
            id: "remove_single_leg_from_procedure_requires_flatten_or_whole_remove",
            title: "Delete one leg from procedure",
            tags: &["procedure", "editing"],
            summary: "A pilot attempts to remove a single leg that is interior to a published procedure.",
            expected_behavior: "The system should either remove the whole procedure, trim only at a legal boundary, or explicitly flatten to standalone legs before the edit. Silent corruption of the procedure is not acceptable.",
        },
        PlanningScenario {
            id: "direct_to_fix_ahead_in_active_plan",
            title: "Direct-to fix ahead in active plan",
            tags: &["direct_to", "sequencing"],
            summary: "The active route contains a future fix and the pilot activates direct-to that fix.",
            expected_behavior: "Direct-to should be represented as active guidance state layered on top of the stored route. Canceling direct-to should allow a sensible resume path without having destroyed the underlying plan.",
        },
        PlanningScenario {
            id: "direct_to_off_plan_fix_preserves_underlying_route",
            title: "Direct-to off-plan fix",
            tags: &["direct_to", "editing", "sequencing"],
            summary: "The pilot activates direct-to a fix that is not currently part of the stored route.",
            expected_behavior: "The direct-to target should be representable without rewriting the filed route in place. The system should still be able to answer what the underlying route is once direct-to is canceled.",
        },
        PlanningScenario {
            id: "approach_activation_and_sequencing_after_terminal_phase_change",
            title: "Approach activation and sequencing",
            tags: &["procedure", "approach", "sequencing"],
            summary: "The aircraft transitions from enroute or arrival guidance into an approach and sequencing should continue through terminal phase changes.",
            expected_behavior: "Leg completion and approach activation should be explicit state transitions with tests, rather than accidental consequences of list indexing.",
        },
        PlanningScenario {
            id: "procedure_can_be_flattened_to_editable_waypoints",
            title: "Flatten procedure into explicit waypoints",
            tags: &["procedure", "editing"],
            summary: "A published procedure is intentionally converted into ordinary editable route legs.",
            expected_behavior: "The system should support an explicit flattening action so that later edits are unambiguous and do not pretend the procedure is still intact.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn sample_geometry_round_trips_to_json() {
        let json = sample_geometry_json();
        let parsed: GeometryBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.polygons.len(), 1);
        assert_eq!(parsed.polygons[0].id, "sec:boston");
    }

    #[test]
    fn planning_bootstrap_scenarios_have_unique_ids() {
        let scenarios = planning_bootstrap_scenarios();
        let unique: BTreeSet<_> = scenarios.iter().map(|scenario| scenario.id).collect();
        assert_eq!(unique.len(), scenarios.len());
    }

    #[test]
    fn planning_bootstrap_scenarios_cover_key_categories() {
        let scenarios = planning_bootstrap_scenarios();

        assert!(scenarios
            .iter()
            .any(|scenario| scenario.tags.contains(&"airway")));
        assert!(scenarios
            .iter()
            .any(|scenario| scenario.tags.contains(&"procedure")));
        assert!(scenarios
            .iter()
            .any(|scenario| scenario.tags.contains(&"direct_to")));
        assert!(scenarios
            .iter()
            .any(|scenario| scenario.tags.contains(&"sequencing")));
        assert!(scenarios
            .iter()
            .any(|scenario| scenario.tags.contains(&"editing")));
    }
}
