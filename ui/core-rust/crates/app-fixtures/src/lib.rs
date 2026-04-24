use app_core::{GeometryBundle, PolygonRecord};
use serde::Serialize;

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
