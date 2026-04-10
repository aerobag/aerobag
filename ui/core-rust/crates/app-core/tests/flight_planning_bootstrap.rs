use app_fixtures::planning_bootstrap_scenarios;

#[test]
fn planning_bootstrap_suite_contains_priority_cases() {
    let scenarios = planning_bootstrap_scenarios();

    let expected_ids = [
        "airway_insert_between_existing_fixes",
        "airway_branch_selection_uses_internal_unique_key",
        "delete_waypoint_inside_airway_requires_clean_decomposition",
        "select_sid_with_transition_preserves_procedure_identity",
        "remove_single_leg_from_procedure_requires_flatten_or_whole_remove",
        "direct_to_fix_ahead_in_active_plan",
        "direct_to_off_plan_fix_preserves_underlying_route",
        "approach_activation_and_sequencing_after_terminal_phase_change",
        "procedure_can_be_flattened_to_editable_waypoints",
    ];

    for expected_id in expected_ids {
        assert!(
            scenarios.iter().any(|scenario| scenario.id == expected_id),
            "missing planning bootstrap scenario: {expected_id}"
        );
    }
}

#[test]
fn planning_bootstrap_suite_uses_outcome_oriented_expectations() {
    let scenarios = planning_bootstrap_scenarios();

    for scenario in &scenarios {
        assert!(
            !scenario.expected_behavior.trim().is_empty(),
            "scenario {} is missing expected behavior",
            scenario.id
        );
        assert!(
            scenario.expected_behavior.contains("should"),
            "scenario {} should describe expected behavior as a testable outcome",
            scenario.id
        );
    }
}
