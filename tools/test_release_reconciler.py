#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import json
import copy
import subprocess
import sys
import tempfile
import unittest
from datetime import datetime, timezone
from pathlib import Path


TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import release_reconciler as releases  # noqa: E402
from test_live_feed_compatibility import provider, requirement  # noqa: E402


class DesiredReleaseTests(unittest.TestCase):
    def policy_document(self, version: int = 2) -> dict:
        return {
            "schema_version": version, "production": {"tag": "prod"},
            "staging": {"tag": "stage"},
            "sunset": [{"tag": "old", "until_utc": "2026-10-01T00:00:00Z"}],
        }

    def test_missing_sunset_policy_defaults_to_sharing_in_both_versions(self) -> None:
        for version in (1, 2):
            with self.subTest(version=version):
                desired = releases.parse_desired_releases(self.policy_document(version))
                self.assertEqual(desired.sunset[0].live_feeds, releases.LiveFeedsPolicy.SHARE_IF_COMPATIBLE)

    def test_dedicated_is_an_explicit_sunset_opt_out(self) -> None:
        document = self.policy_document()
        document["sunset"][0]["live_feeds"] = "dedicated"
        self.assertEqual(releases.parse_desired_releases(document).sunset[0].live_feeds, releases.LiveFeedsPolicy.DEDICATED)

    def test_unknown_or_malformed_policies_are_rejected(self) -> None:
        for policy in (None, True, 1, [], {}, "share", "SHARE_IF_COMPATIBLE"):
            with self.subTest(policy=policy), self.assertRaises(releases.ReleaseConfigError):
                document = self.policy_document()
                document["sunset"][0]["live_feeds"] = policy
                releases.parse_desired_releases(document)

    def test_policy_is_rejected_on_production_and_staging(self) -> None:
        for channel in ("production", "staging"):
            with self.subTest(channel=channel), self.assertRaisesRegex(releases.ReleaseConfigError, "unknown field"):
                document = self.policy_document()
                document[channel]["live_feeds"] = "dedicated"
                releases.parse_desired_releases(document)

    def test_v1_does_not_silently_accept_v2_fields(self) -> None:
        document = self.policy_document(1)
        document["sunset"][0]["live_feeds"] = "dedicated"
        with self.assertRaisesRegex(releases.ReleaseConfigError, "unknown field"):
            releases.parse_desired_releases(document)

    def test_desired_schema_version_requires_known_integer(self) -> None:
        for version in (None, True, 1.0, "2", 0, 3):
            with self.subTest(version=version), self.assertRaisesRegex(releases.ReleaseConfigError, "schema_version"):
                releases.parse_desired_releases(self.policy_document(version))

    def test_migration_is_explicit_idempotent_and_does_not_mutate_input(self) -> None:
        document = self.policy_document(1)
        before = json.dumps(document)
        migrated = releases.migrate_desired_releases(document)
        self.assertEqual(migrated["schema_version"], 2)
        self.assertEqual(migrated["sunset"][0]["live_feeds"], "share_if_compatible")
        self.assertEqual(migrated["sunset"][0]["until_utc"], document["sunset"][0]["until_utc"])
        self.assertEqual(releases.migrate_desired_releases(migrated), migrated)
        self.assertEqual(json.dumps(document), before)

    def test_rejects_the_same_release_in_production_and_staging(self) -> None:
        with self.assertRaisesRegex(releases.ReleaseConfigError, "production and staging"):
            releases.parse_desired_releases(
                {
                    "schema_version": 1,
                    "production": {"tag": "2026-08-22.1"},
                    "staging": {"tag": "2026-08-22.1"},
                    "sunset": [],
                }
            )

    def test_rejects_a_release_assigned_to_production_and_sunset(self) -> None:
        with self.assertRaisesRegex(releases.ReleaseConfigError, "production.*sunset"):
            releases.parse_desired_releases(
                {
                    "schema_version": 1,
                    "production": {"tag": "2026-08-15.1"},
                    "staging": None,
                    "sunset": [
                        {
                            "tag": "2026-08-15.1",
                            "until_utc": "2026-09-15T00:00:00Z",
                        }
                    ],
                }
            )

    def test_rejects_unknown_fields(self) -> None:
        with self.assertRaisesRegex(releases.ReleaseConfigError, "unknown field"):
            releases.parse_desired_releases(
                {
                    "schema_version": 1,
                    "production": {"tag": "2026-08-15.1", "branch": "main"},
                    "staging": None,
                    "sunset": [],
                }
            )

    def test_resolves_only_annotated_immutable_tags(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            repo = Path(temp_dir) / "repo"
            repo.mkdir()
            self.git(repo, "init", "--quiet")
            self.git(repo, "config", "user.name", "Aerobag Test")
            self.git(repo, "config", "user.email", "test@aerobag.invalid")
            self.git(
                repo,
                "commit",
                "--quiet",
                "--allow-empty",
                "-m",
                "release",
            )
            self.git(repo, "tag", "-a", "2026-08-22.1", "-m", "release")
            resolved = releases.resolve_release_tag(repo, "2026-08-22.1")
            self.assertEqual(len(resolved.commit), 40)
            self.assertEqual(len(resolved.tag_object), 40)

            self.git(repo, "tag", "lightweight")
            with self.assertRaisesRegex(releases.ReleaseConfigError, "annotated"):
                releases.resolve_release_tag(repo, "lightweight")

    def test_expired_sunset_release_is_removed_from_effective_state(self) -> None:
        desired = releases.parse_desired_releases(
            {
                "schema_version": 1,
                "production": {"tag": "production"},
                "staging": None,
                "sunset": [
                    {"tag": "expired", "until_utc": "2026-08-21T00:00:00Z"},
                    {"tag": "active", "until_utc": "2026-08-23T00:00:00Z"},
                ],
            }
        )
        effective = releases.effective_desired_releases(
            desired, datetime(2026, 8, 22, tzinfo=timezone.utc)
        )
        self.assertEqual([item.tag for item in effective.sunset], ["active"])

    def git(self, repo: Path, *args: str) -> None:
        subprocess.run(
            ["git", *args],
            cwd=repo,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )


class ReconciliationPlannerTests(unittest.TestCase):
    def desired(
        self,
        production: str,
        staging: str | None,
        sunset: tuple[str, ...] = (),
    ) -> releases.DesiredReleases:
        return releases.parse_desired_releases(
            {
                "schema_version": 1,
                "production": {"tag": production},
                "staging": None if staging is None else {"tag": staging},
                "sunset": [
                    {"tag": tag, "until_utc": "2026-09-15T00:00:00Z"}
                    for tag in sunset
                ],
            }
        )

    def observed_release(
        self,
        tag: str,
        *,
        built: bool = True,
        qualified: bool = True,
    ) -> releases.ObservedRelease:
        return releases.ObservedRelease(
            tag=tag,
            tag_object=(tag.encode().hex() + "0" * 40)[:40],
            commit=(tag.encode().hex() + "1" * 40)[:40],
            build_status="passed" if built else "pending",
            qualification_status="passed" if qualified else "pending",
            deployment_status="passed" if built else "pending",
            product_manifest=f"published/{tag}/product_artifacts.json" if built else None,
            release_root=f"release-builds/{tag}" if built else None,
            live_feed_endpoint=f"http://127.0.0.1/{tag}" if built else None,
            live_feed_status="running" if built else "pending",
        )

    def test_new_staging_candidate_never_changes_production(self) -> None:
        production = self.observed_release("2026-08-15.1")
        observed = releases.ObservedState.empty()
        observed.releases[production.tag] = production
        observed.production = production.tag
        observed.generation = 1

        plan = releases.plan_reconciliation(
            self.desired(production.tag, "2026-08-22.1"), observed
        )

        self.assertEqual(
            [action.kind for action in plan.actions],
            ["build_release"],
        )
        self.assertEqual(plan.actions[0].tag, "2026-08-22.1")
        self.assertEqual(observed.production, production.tag)

    def test_qualified_staging_promotion_is_only_a_pointer_change(self) -> None:
        old = self.observed_release("2026-08-15.1")
        candidate = self.observed_release("2026-08-22.1")
        observed = releases.ObservedState.empty()
        observed.releases = {old.tag: old, candidate.tag: candidate}
        observed.production = old.tag
        observed.staging = candidate.tag

        plan = releases.plan_reconciliation(
            self.desired(candidate.tag, None, (old.tag,)), observed
        )

        self.assertEqual(
            [action.kind for action in plan.actions],
            ["activate_generation"],
        )
        self.assertFalse(any(action.kind == "build_release" for action in plan.actions))

    def test_qualified_retained_release_rolls_back_without_rebuild_or_restaging(self) -> None:
        old = self.observed_release("2026-08-15.1")
        current = self.observed_release("2026-08-22.1")
        observed = releases.ObservedState.empty()
        observed.releases = {old.tag: old, current.tag: current}
        observed.production = current.tag
        observed.staging = None
        observed.sunset = [old.tag]
        observed.generation = 9

        plan = releases.plan_reconciliation(
            self.desired(old.tag, None, (current.tag,)), observed
        )

        self.assertEqual(
            [action.kind for action in plan.actions], ["activate_generation"]
        )

    def test_unqualified_release_must_be_staged_before_becoming_production(self) -> None:
        current = self.observed_release("2026-08-15.1")
        candidate = self.observed_release("2026-08-22.1", qualified=False)
        observed = releases.ObservedState.empty()
        observed.releases = {current.tag: current, candidate.tag: candidate}
        observed.production = current.tag
        observed.generation = 9

        plan = releases.plan_reconciliation(
            self.desired(candidate.tag, None, (current.tag,)), observed
        )

        self.assertEqual(plan.actions, [])
        self.assertIn("qualified on staging", plan.blocked_reason or "")

    def test_unqualified_candidate_cannot_become_production(self) -> None:
        old = self.observed_release("2026-08-15.1")
        candidate = self.observed_release("2026-08-22.1", qualified=False)
        observed = releases.ObservedState.empty()
        observed.releases = {old.tag: old, candidate.tag: candidate}
        observed.production = old.tag
        observed.staging = candidate.tag

        plan = releases.plan_reconciliation(
            self.desired(candidate.tag, None, (old.tag,)), observed
        )

        self.assertEqual([action.kind for action in plan.actions], ["qualify_release"])
        self.assertIn("not qualified", plan.blocked_reason or "")

    def test_exact_forced_staging_candidate_can_become_production(self) -> None:
        old = self.observed_release("2026-08-15.1")
        candidate = self.observed_release("2026-08-22.1", qualified=False)
        observed = releases.ObservedState.empty()
        observed.releases = {old.tag: old, candidate.tag: candidate}
        observed.production = old.tag
        observed.staging = candidate.tag

        plan = releases.plan_reconciliation(
            self.desired(candidate.tag, None, (old.tag,)),
            observed,
            force_production_tag=candidate.tag,
        )

        self.assertEqual(
            [action.kind for action in plan.actions], ["activate_generation"]
        )

    def test_force_does_not_apply_to_a_different_tag(self) -> None:
        old = self.observed_release("2026-08-15.1")
        candidate = self.observed_release("2026-08-22.1", qualified=False)
        observed = releases.ObservedState.empty()
        observed.releases = {old.tag: old, candidate.tag: candidate}
        observed.production = old.tag
        observed.staging = candidate.tag

        plan = releases.plan_reconciliation(
            self.desired(candidate.tag, None, (old.tag,)),
            observed,
            force_production_tag="2026-08-23.1",
        )

        self.assertEqual([action.kind for action in plan.actions], ["qualify_release"])

    def test_force_requires_candidate_to_be_active_on_staging(self) -> None:
        old = self.observed_release("2026-08-15.1")
        candidate = self.observed_release("2026-08-22.1", qualified=False)
        observed = releases.ObservedState.empty()
        observed.releases = {old.tag: old, candidate.tag: candidate}
        observed.production = old.tag

        plan = releases.plan_reconciliation(
            self.desired(candidate.tag, None, (old.tag,)),
            observed,
            force_production_tag=candidate.tag,
        )

        self.assertEqual(plan.actions, [])
        self.assertIn("not active on staging", plan.blocked_reason or "")

    def test_converged_state_is_a_no_op(self) -> None:
        production = self.observed_release("2026-08-22.1")
        old = self.observed_release("2026-08-15.1")
        observed = releases.ObservedState.empty()
        observed.releases = {production.tag: production, old.tag: old}
        observed.production = production.tag
        observed.staging = None
        observed.sunset = [old.tag]
        observed.generation = 7

        plan = releases.plan_reconciliation(
            self.desired(production.tag, None, (old.tag,)), observed
        )

        self.assertEqual(plan.actions, [])
        self.assertTrue(plan.converged)

    def test_legacy_records_need_deployment_checks_not_another_staging(self) -> None:
        production = self.observed_release("prod", qualified=False)
        old_document = {
            "tag": production.tag, "tag_object": production.tag_object,
            "commit": production.commit, "build_status": "passed",
            "qualification_status": "pending", "live_feed_status": "running",
            "live_feed_endpoint": "http://127.0.0.1:8100",
        }
        production = releases.ObservedRelease.from_dict(old_document, "old record")
        self.assertEqual(production.deployment_status, "pending")
        self.assertIsNone(production.deployment_record)
        observed = releases.ObservedState(releases={"prod": production}, production="prod", generation=1)
        desired = self.desired("prod", None)
        self.assertEqual(
            releases.plan_reconciliation(desired, observed).actions,
            [releases.ReconcileAction("check_deployment", "prod")],
        )
        self.assertTrue(releases.deployment_checks_are_only_pending_work(desired, observed))
        self.assertEqual(production.deployment_status, "pending", "inspection must not mutate real state")

    def test_deployment_checks_only_does_not_hide_pending_staging_work(self) -> None:
        production = self.observed_release("prod")
        production.deployment_status = "pending"
        observed = releases.ObservedState(releases={"prod": production}, production="prod", generation=1)
        desired = self.desired("prod", "candidate")
        self.assertEqual(
            releases.plan_reconciliation(desired, observed).actions,
            [releases.ReconcileAction("check_deployment", "prod")],
        )
        self.assertFalse(releases.deployment_checks_are_only_pending_work(desired, observed))
        observed.releases["candidate"] = self.observed_release("candidate", qualified=False)
        observed.staging = "candidate"
        self.assertFalse(releases.deployment_checks_are_only_pending_work(desired, observed))

    def test_sunset_assignment_changes_before_checking_its_endpoints(self) -> None:
        production = self.observed_release("prod")
        old = self.observed_release("old")
        old.deployment_status = "pending"
        observed = releases.ObservedState(
            releases={"prod": production, "old": old}, production="prod", generation=1,
        )
        desired = self.desired("prod", None, ("old",))
        self.assertEqual(
            releases.plan_reconciliation(desired, observed).actions,
            [releases.ReconcileAction("activate_generation")],
        )
        observed.sunset = ["old"]
        self.assertEqual(
            releases.plan_reconciliation(desired, observed).actions,
            [releases.ReconcileAction("check_deployment", "old")],
        )

    def test_adopted_legacy_state_still_materializes_first_generation(self) -> None:
        production = self.observed_release("2026-08-22.1")
        observed = releases.ObservedState.empty()
        observed.releases = {production.tag: production}
        observed.production = production.tag

        plan = releases.plan_reconciliation(
            self.desired(production.tag, None), observed
        )

        self.assertEqual([action.kind for action in plan.actions], ["activate_generation"])

    def test_observed_tag_mutation_is_rejected(self) -> None:
        observed = self.observed_release("2026-08-22.1")
        resolved = releases.ResolvedTag(
            tag=observed.tag,
            tag_object="f" * 40,
            commit=observed.commit,
        )
        with self.assertRaisesRegex(releases.ReleaseConfigError, "changed tag object"):
            releases.verify_release_identity(resolved, observed)

    def test_production_refresh_is_activated_before_broken_staging_build(self) -> None:
        production = self.observed_release("2026-08-15.1")
        broken_staging = self.observed_release("2026-08-22.1", built=False)
        observed = releases.ObservedState.empty()
        observed.releases = {
            production.tag: production,
            broken_staging.tag: broken_staging,
        }
        observed.production = production.tag
        observed.generation = 8
        observed.channel_inputs_dirty = True

        plan = releases.plan_reconciliation(
            self.desired(production.tag, broken_staging.tag), observed
        )

        self.assertEqual(
            [action.kind for action in plan.actions], ["activate_generation"]
        )


class LiveFeedBindingPlannerTests(unittest.TestCase):
    desired = ReconciliationPlannerTests.desired
    observed_release = ReconciliationPlannerTests.observed_release

    def state(self, sunset: tuple[str, ...] = ("old",), staging: str | None = "stage"):
        desired = self.desired("prod", staging, sunset)
        observed = releases.ObservedState(production="prod", staging=staging, sunset=list(sunset), generation=2)
        for tag in desired.tags():
            record = self.observed_release(tag)
            record.live_feed_endpoint = None
            record.live_feed_status = "migrated"
            record.live_feed_requirements = requirement(tag)
            evidence = provider(tag)
            instance = releases.LiveFeedInstance(
                instance_id=evidence["launch_instance_id"], release_tag=tag, launch_digest="d" * 64,
                endpoint=f"http://127.0.0.1:{8100 + len(observed.releases)}",
                unit=f"aerobag-live-feeds-release@{tag}-instance.service",
                manifest=evidence["startup_publication"]["path"],
                manifest_sha256=evidence["startup_publication"]["sha256"],
                evidence=evidence, status="stopped" if tag in sunset else "running",
            )
            record.live_feed_instance = instance.instance_id
            observed.releases[tag] = record
            observed.live_feed_instances[instance.instance_id] = instance
        self.activate(desired, observed)
        return desired, observed

    def activate(self, desired, observed):
        for tag, binding in releases.resolve_live_feed_bindings(desired, observed).items():
            observed.releases[tag].live_feed_provider = binding.provider
            observed.releases[tag].live_feed_reason = binding.reason

    def own(self, observed, tag):
        return observed.live_feed_instances[observed.releases[tag].live_feed_instance]

    def test_compatible_sunset_shares_without_starting_its_own_daemon(self) -> None:
        desired, observed = self.state()
        bindings = releases.resolve_live_feed_bindings(desired, observed)
        self.assertEqual(bindings["old"].provider, "prod-instance")
        self.assertEqual(bindings["old"].reason, "shared with prod")
        self.assertEqual(self.own(observed, "old").status, "stopped")
        self.assertTrue(releases.plan_reconciliation(desired, observed).converged)

    def test_staging_always_uses_a_separate_own_instance(self) -> None:
        desired, observed = self.state()
        bindings = releases.resolve_live_feed_bindings(desired, observed)
        self.assertEqual(bindings["stage"].provider, "stage-instance")
        self.assertFalse(bindings["stage"].shared)
        self.own(observed, "stage").status = "stopped"
        plan = releases.plan_reconciliation(desired, observed)
        self.assertEqual(plan.actions, [releases.ReconcileAction("start_live_feeds", "stage")])

    def test_opt_out_prepares_previously_stopped_own_instance_before_activation(self) -> None:
        _, observed = self.state()
        desired = releases.DesiredReleases(
            releases.ReleaseBinding("prod"), releases.ReleaseBinding("stage"),
            (releases.SunsetBinding("old", "2026-10-01T00:00:00Z", releases.LiveFeedsPolicy.DEDICATED),),
        )
        plan = releases.plan_reconciliation(desired, observed)
        self.assertEqual(plan.actions, [releases.ReconcileAction("start_live_feeds", "old")])
        self.own(observed, "old").status = "running"
        self.assertEqual(releases.plan_reconciliation(desired, observed).actions, [releases.ReconcileAction("activate_generation")])
        self.activate(desired, observed)
        self.assertTrue(releases.plan_reconciliation(desired, observed).converged)

    def test_missing_requirement_is_dedicated_not_a_failed_deployment(self) -> None:
        desired, observed = self.state()
        observed.releases["old"].live_feed_requirements = None
        binding = releases.resolve_live_feed_bindings(desired, observed)["old"]
        self.assertFalse(binding.shared)
        self.assertIn("metadata unavailable", binding.reason)
        plan = releases.plan_reconciliation(desired, observed)
        self.assertIsNone(plan.blocked_reason)
        self.assertEqual(plan.actions, [releases.ReconcileAction("start_live_feeds", "old")])

    def test_unready_compatibility_is_dedicated_without_restart_loop(self) -> None:
        desired, observed = self.state()
        self.own(observed, "prod").evidence["ready"] = False
        self.own(observed, "old").status = "running"
        self.activate(desired, observed)
        self.assertFalse(releases.resolve_live_feed_bindings(desired, observed)["old"].shared)
        self.assertTrue(releases.plan_reconciliation(desired, observed).converged)

    def test_both_incompatible_shared_sunsets_restart_dedicated_before_activation(self) -> None:
        desired, observed = self.state(("old", "older"))
        self.own(observed, "prod").evidence["notam_catalog"]["sha256"] = "e" * 64
        for tag in ("old", "older"):
            plan = releases.plan_reconciliation(desired, observed)
            self.assertIsNone(plan.blocked_reason)
            self.assertEqual(plan.actions, [releases.ReconcileAction("start_live_feeds", tag)])
            self.own(observed, tag).status = "running"
        self.assertEqual(releases.plan_reconciliation(desired, observed).actions, [releases.ReconcileAction("activate_generation")])
        self.activate(desired, observed)
        self.assertTrue(releases.plan_reconciliation(desired, observed).converged)

    def test_failed_required_dedicated_instance_never_activates(self) -> None:
        desired, observed = self.state()
        self.own(observed, "prod").evidence = None
        self.own(observed, "old").status = "failed"
        self.assertEqual(releases.plan_reconciliation(desired, observed).actions, [releases.ReconcileAction("start_live_feeds", "old")])
        with self.assertRaisesRegex(releases.ReleaseConfigError, "not ready"):
            releases.validate_live_feed_bindings(desired, observed, releases.resolve_live_feed_bindings(desired, observed))

    def test_no_transitive_sharing_through_a_compatible_sunset(self) -> None:
        desired, observed = self.state(("old", "older"))
        self.own(observed, "prod").evidence["notam_catalog"]["sha256"] = "e" * 64
        self.own(observed, "old").status = "running"
        bindings = releases.resolve_live_feed_bindings(desired, observed)
        self.assertEqual(bindings["older"].provider_tag, "older")
        self.assertFalse(bindings["older"].shared)

    def test_rollback_prepares_previously_shared_release_as_production(self) -> None:
        _, observed = self.state()
        desired = self.desired("old", None, ("prod",))
        self.assertEqual(releases.plan_reconciliation(desired, observed).actions, [releases.ReconcileAction("start_live_feeds", "old")])
        self.own(observed, "old").status = "running"
        self.assertEqual(releases.plan_reconciliation(desired, observed).actions, [releases.ReconcileAction("activate_generation", "old")])

    def test_same_tag_replacement_prepares_new_instance_before_binding_change(self) -> None:
        desired, observed = self.state()
        replacement = copy.deepcopy(self.own(observed, "prod"))
        replacement.instance_id = "prod-replacement"
        replacement.status = "pending"
        replacement.evidence["launch_instance_id"] = replacement.instance_id
        observed.live_feed_instances[replacement.instance_id] = replacement
        observed.releases["prod"].live_feed_instance = replacement.instance_id
        self.assertEqual(releases.plan_reconciliation(desired, observed).actions, [releases.ReconcileAction("start_live_feeds", "prod")])
        replacement.status = "running"
        self.assertEqual(releases.plan_reconciliation(desired, observed).actions, [releases.ReconcileAction("activate_generation")])

    def test_expiring_one_sunset_does_not_remove_shared_provider_health_target(self) -> None:
        _, observed = self.state(("old", "older"))
        desired = self.desired("prod", "stage", ("older",))
        targets = releases.live_feed_health_targets(desired, observed)
        self.assertEqual(targets[self.own(observed, "prod").unit], ("prod", "older"))
        self.assertEqual(len(targets), 2)
        self.assertIn("old", observed.releases)

    def test_ordinary_product_advance_does_not_invalidate_activation(self) -> None:
        desired, observed = self.state()
        proposed = releases.resolve_live_feed_bindings(desired, observed)
        self.own(observed, "prod").evidence["published_state_id"] = "state-2"
        releases.validate_live_feed_bindings(desired, observed, proposed)

    def test_process_restart_invalidates_activation_even_on_same_endpoint(self) -> None:
        desired, observed = self.state()
        proposed = releases.resolve_live_feed_bindings(desired, observed)
        self.own(observed, "prod").evidence["process_instance_id"] = "process-2"
        with self.assertRaisesRegex(releases.ReleaseConfigError, "changed"):
            releases.validate_live_feed_bindings(desired, observed, proposed)

    def test_publication_change_invalidates_activation_even_when_catalog_matches(self) -> None:
        desired, observed = self.state()
        proposed = releases.resolve_live_feed_bindings(desired, observed)
        observed.releases["old"].live_feed_requirements["startup_publication"]["sha256"] = "e" * 64
        with self.assertRaisesRegex(releases.ReleaseConfigError, "changed"):
            releases.validate_live_feed_bindings(desired, observed, proposed)

    def test_missing_binding_in_complete_table_rejects_activation(self) -> None:
        desired, observed = self.state()
        proposed = releases.resolve_live_feed_bindings(desired, observed)
        del proposed["stage"]
        with self.assertRaisesRegex(releases.ReleaseConfigError, "changed"):
            releases.validate_live_feed_bindings(desired, observed, proposed)

    def test_observed_instance_model_round_trip_does_not_recreate_legacy_daemons(self) -> None:
        _, observed = self.state()
        restored = releases.ObservedState.from_dict(observed.to_dict())
        self.assertEqual(restored.to_dict(), observed.to_dict())
        self.assertIsNone(restored.releases["old"].live_feed_endpoint)
        self.assertEqual(restored.releases["old"].live_feed_provider, "prod-instance")
        self.assertEqual(restored.releases["old"].live_feed_instance, "old-instance")

    def test_v1_observed_state_remains_explicitly_legacy_without_guessed_instances(self) -> None:
        document = {
            "schema_version": 1, "production": "prod",
            "releases": {"prod": {
                "tag": "prod", "commit": "a" * 40, "tag_object": "b" * 40,
                "build_status": "passed", "qualification_status": "passed",
                "live_feed_endpoint": "http://127.0.0.1:8100", "live_feed_status": "running",
            }},
        }
        observed = releases.ObservedState.from_dict(document)
        self.assertEqual(observed.live_feed_instances, {})
        self.assertIsNone(observed.releases["prod"].live_feed_requirements)
        self.assertEqual(observed.to_dict()["schema_version"], 2)
        self.assertTrue(releases.dedicated_live_feed_ready(observed, "prod"))

    def test_v1_observed_state_rejects_v2_instance_metadata(self) -> None:
        _, observed = self.state()
        document = observed.to_dict()
        document["schema_version"] = 1
        with self.assertRaisesRegex(releases.ReleaseConfigError, "schema_version 2"):
            releases.ObservedState.from_dict(document)
        del document["live_feed_instances"]
        with self.assertRaisesRegex(releases.ReleaseConfigError, "schema_version 2"):
            releases.ObservedState.from_dict(document)

    def test_observed_state_rejects_missing_or_wrong_instance_ownership(self) -> None:
        _, observed = self.state()
        for field, value in (("live_feed_instance", "missing"), ("live_feed_provider", "missing"), ("live_feed_instance", "old-instance")):
            with self.subTest(field=field, value=value), self.assertRaises(releases.ReleaseConfigError):
                document = observed.to_dict()
                document["releases"]["prod"][field] = value
                releases.ObservedState.from_dict(document)

    def test_observed_state_rejects_malformed_instance_metadata(self) -> None:
        _, observed = self.state()
        for field, value in (
            ("instance_id", "../outside"), ("status", []), ("status", "unknown"),
            ("manifest_sha256", "A" * 64), ("gc_paths", [None]), ("roots", "path"),
        ):
            with self.subTest(field=field, value=value), self.assertRaises(releases.ReleaseConfigError):
                document = observed.to_dict()
                document["live_feed_instances"]["prod-instance"][field] = value
                releases.ObservedState.from_dict(document)

    def test_wrong_launch_release_or_startup_publication_cannot_approve_sharing(self) -> None:
        for key, value in (
            ("release_tag", "different"), ("launch_instance_id", "different"),
            ("startup_publication", {"path": "/other.json", "sha256": "e" * 64}),
        ):
            with self.subTest(key=key):
                desired, observed = self.state()
                self.own(observed, "prod").evidence[key] = value
                result = releases.resolve_live_feed_bindings(desired, observed)["old"]
                self.assertFalse(result.shared)
                self.assertIn("instance identity differs", result.reason)

    def test_instance_state_is_authoritative_over_stale_legacy_fields(self) -> None:
        desired, observed = self.state()
        record = observed.releases["prod"]
        record.live_feed_endpoint = "http://127.0.0.1:9999"
        record.live_feed_status = "running"
        self.own(observed, "prod").status = "stopped"
        self.assertFalse(releases.dedicated_live_feed_ready(observed, "prod"))
        self.assertEqual(releases.plan_reconciliation(desired, observed).actions, [releases.ReconcileAction("start_live_feeds", "prod")])

class ChannelGenerationTests(unittest.TestCase):
    def test_generation_activation_and_registry_preserve_explicit_launch_pins(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pin = "live-feed-launches/prod-digest/packages/current_artifacts.json"
            pinned = root / pin
            pinned.parent.mkdir(parents=True)
            pinned.write_text("[]")
            generation = root / "channel-generations/1"
            generation.mkdir(parents=True)
            (generation / "current_artifacts.json").write_text("[]")
            (generation / "gc-root-manifests.json").write_text(json.dumps({
                "schema_version": 1, "current_artifacts_paths": ["current_artifacts.json"],
            }))
            releases.activate_channel_generation(root, generation, pinned_gc_paths=[pin])
            retention = releases.generation_retention(root, pinned_gc_paths=[pin])
            self.assertIn(pin, retention.gc_paths)
            registry = json.loads((root / releases.RELEASE_GC_ROOTS).read_text())
            self.assertIn(pin, registry["current_artifacts_paths"])
            self.assertNotIn(pin, releases.generation_retention(root).gc_paths)

    def test_launch_gc_pin_is_retained_without_an_active_generation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pin = "live-feed-launches/prod-digest/packages/current_artifacts.json"
            path = root / pin
            path.parent.mkdir(parents=True)
            path.write_text("{}")
            retention = releases.generation_retention(root, pinned_gc_paths=[pin, pin])
            self.assertEqual(retention.gc_paths, (pin,))
            self.assertEqual(retention.retained, ())

    def test_launch_gc_pin_rejects_missing_traversal_and_symlink_paths(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for pin in (
                "/live-feed-launches/prod/packages/current_artifacts.json",
                "live-feed-launches/../packages/current_artifacts.json",
                "live-feed-launches/prod/other/current_artifacts.json",
                "live-feed-launches/prod/packages/current_artifacts.json",
                "live-feed-launches/./prod/packages/current_artifacts.json",
                "channel-generations/1/packages/current_artifacts.json",
            ):
                with self.subTest(pin=pin), self.assertRaises(releases.ReleaseConfigError):
                    releases.generation_retention(root, pinned_gc_paths=[pin])
            (root / "external").mkdir()
            (root / "live-feed-launches").symlink_to(root / "external", target_is_directory=True)
            with self.assertRaisesRegex(releases.ReleaseConfigError, "symlink"):
                releases.generation_retention(root, pinned_gc_paths=["live-feed-launches/prod/packages/current_artifacts.json"])

    def test_discovery_prefers_controlling_release_without_discarding_distinct_contract_sets(self) -> None:
        def manifest(tag, contracts):
            return releases.ChannelManifest(tag, Path(tag), {"contracts": contracts}, ())

        production = manifest("production", {"nav-db": "NAV25", "terrain": "TER2"})
        old = manifest("old", {"terrain": "TER2", "nav-db": "NAV25"})
        legacy = manifest("legacy", {"nav-db": "NAV23", "terrain": "TER2"})
        changed_family = manifest("changed-family", {"nav-db": "NAV25", "terrain": "TER1"})
        extra_family = manifest("extra-family", {"nav-db": "NAV25", "terrain": "TER2", "new": "NEW1"})
        manifests = [production, old, legacy, changed_family, extra_family]
        self.assertEqual(releases.discovery_manifests(manifests), [production, legacy, changed_family, extra_family])
        self.assertEqual(len(manifests), 5)
        self.assertEqual(releases.discovery_manifests([old, production]), [old])

    def test_discovery_does_not_treat_missing_or_invalid_contracts_as_duplicates(self) -> None:
        for contracts in (None, {}, [], {"nav-db": None}, {"": "NAV25"}, {"nav-db": 25}):
            with self.subTest(contracts=contracts), self.assertRaises(releases.ReleaseConfigError):
                releases.discovery_manifests([
                    releases.ChannelManifest("tag", Path("manifest"), {"contracts": contracts}, ()),
                ])

    def test_materialized_discovery_deduplicates_without_losing_release_views(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifests = [releases.ChannelManifest(
                tag, root / tag, {"contracts": {"nav-db": contract}, "as_of_date": tag}, (),
            ) for tag, contract in (("new", "NAV25"), ("old", "NAV25"), ("legacy", "NAV23"))]
            output = root / "generation"
            releases.materialize_channel_generation(
                output, root / "published", production_manifests=manifests, staging_manifests=[],
            )
            combined = json.loads((output / "production/packages/current_artifacts.json").read_text())
            self.assertEqual([value["as_of_date"] for value in combined], ["new", "legacy"])
            for manifest in manifests:
                scoped = output / "releases" / manifest.release_tag / "packages/current_artifacts.json"
                self.assertEqual(json.loads(scoped.read_text()), [manifest.document])

    def test_real_nested_artifact_roots_link_at_the_url_root_component(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            manifest_path = root / "published/release-abc/20260822/product_artifacts.json"
            manifest_path.parent.mkdir(parents=True)
            manifest_path.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "contracts": {"nav-db": "NAV23"},
                        "artifact_roots": {
                            "packaged": "release-abc/20260822/packaged/",
                            "unpacked": "release-abc/20260822/unpacked/",
                        },
                        "bundles": [],
                    }
                ),
                encoding="utf-8",
            )

            manifest = releases.load_channel_manifest("release", manifest_path)

            self.assertEqual(manifest.publication_roots, ("release-abc",))

    def test_channel_views_have_distinct_discovery_and_shared_publication(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            published = root / "published"
            (published / "prod-build").mkdir(parents=True)
            (published / "stage-build").mkdir()
            prod_release = root / "release-builds/prod"
            stage_release = root / "release-builds/stage"
            for release in [prod_release, stage_release]:
                (release / "web").mkdir(parents=True)
                (release / "downloads").mkdir()
            output = root / "generation"

            releases.materialize_channel_generation(
                output,
                published,
                production_manifests=[
                    releases.ChannelManifest(
                        release_tag="2026-08-15.1",
                        source_path=published / "prod-build" / "product_artifacts.json",
                        document={"contracts": {"nav-db": "NAV22"}},
                        publication_roots=("prod-build",),
                    )
                ],
                staging_manifests=[
                    releases.ChannelManifest(
                        release_tag="2026-08-22.1",
                        source_path=published / "stage-build" / "product_artifacts.json",
                        document={"contracts": {"nav-db": "NAV23"}},
                        publication_roots=("stage-build",),
                    )
                ],
                release_assets={
                    "2026-08-15.1": releases.ReleaseAssets(
                        prod_release, "http://127.0.0.1:8101"
                    ),
                    "2026-08-22.1": releases.ReleaseAssets(
                        stage_release, "http://127.0.0.1:8102"
                    ),
                },
            )

            production_current = output / "production/packages/current_artifacts.json"
            staging_current = output / "staging/packages/current_artifacts.json"
            self.assertNotEqual(production_current.resolve(), staging_current.resolve())
            self.assertEqual(
                json.loads(production_current.read_text(encoding="utf-8"))[0]["contracts"],
                {"nav-db": "NAV22"},
            )
            self.assertEqual(
                json.loads(staging_current.read_text(encoding="utf-8"))[0]["contracts"],
                {"nav-db": "NAV23"},
            )
            self.assertEqual(
                (output / "production/packages/prod-build").resolve(),
                published / "prod-build",
            )
            self.assertEqual(
                (output / "staging/packages/stage-build").resolve(),
                published / "stage-build",
            )
            gc_roots = json.loads(
                (output / "gc-root-manifests.json").read_text(encoding="utf-8")
            )
            self.assertIn(
                "production/packages/current_artifacts.json",
                gc_roots["current_artifacts_paths"],
            )
            self.assertIn(
                "staging/packages/current_artifacts.json",
                gc_roots["current_artifacts_paths"],
            )
            self.assertEqual((output / "production/web").resolve(), prod_release / "web")
            routes = json.loads(
                (output / "live-feed-routes.json").read_text(encoding="utf-8")
            )
            self.assertEqual(routes["production"], "http://127.0.0.1:8101")
            self.assertEqual(routes["staging"], "http://127.0.0.1:8102")

    def test_activation_is_atomic_and_roots_the_previous_generation(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            published = root / "published"
            (published / "first").mkdir(parents=True)
            (published / "second").mkdir()

            def make_generation(number: int, publication: str) -> Path:
                output = root / "channel-generations" / str(number)
                releases.materialize_channel_generation(
                    output,
                    published,
                    production_manifests=[
                        releases.ChannelManifest(
                            release_tag=f"2026-08-{number:02d}.1",
                            source_path=(
                                published / publication / "product_artifacts.json"
                            ),
                            document={"contracts": {"nav-db": f"NAV{number}"}},
                            publication_roots=(publication,),
                        )
                    ],
                    staging_manifests=[],
                )
                return output

            first = make_generation(1, "first")
            second = make_generation(2, "second")
            releases.activate_channel_generation(root, first)
            releases.activate_channel_generation(root, second)

            self.assertEqual((root / "channel-current").resolve(), second)
            registry = json.loads(
                (root / "state/release-gc-roots.json").read_text(encoding="utf-8")
            )
            self.assertEqual(
                registry["current_artifacts_paths"],
                [
                    "channel-generations/1/production/packages/current_artifacts.json",
                    "channel-generations/1/releases/2026-08-01.1/packages/current_artifacts.json",
                    "channel-generations/2/production/packages/current_artifacts.json",
                    "channel-generations/2/releases/2026-08-02.1/packages/current_artifacts.json",
                ],
            )

    def test_promotion_serves_the_exact_apk_previously_staged(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            published = root / "published"
            (published / "candidate-products").mkdir(parents=True)
            candidate = releases.ChannelManifest(
                release_tag="2026-08-22.1",
                source_path=published / "candidate-products/product_artifacts.json",
                document={"contracts": {"nav-db": "NAV23"}},
                publication_roots=("candidate-products",),
            )
            release_root = root / "release-builds/candidate"
            (release_root / "web").mkdir(parents=True)
            downloads = release_root / "downloads"
            downloads.mkdir()
            apk = downloads / "aerobag-candidate.apk"
            apk.write_bytes(b"exact candidate apk")
            assets = {
                candidate.release_tag: releases.ReleaseAssets(
                    release_root, "http://127.0.0.1:8102"
                )
            }

            staged = root / "channel-generations/staged"
            releases.materialize_channel_generation(
                staged,
                published,
                production_manifests=[candidate],
                staging_manifests=[candidate],
                release_assets=assets,
            )
            promoted = root / "channel-generations/promoted"
            releases.materialize_channel_generation(
                promoted,
                published,
                production_manifests=[candidate],
                staging_manifests=[],
                release_assets=assets,
            )

            self.assertEqual(
                (staged / "staging/downloads/aerobag-candidate.apk").resolve(),
                apk,
            )
            self.assertEqual(
                (promoted / "production/downloads/aerobag-candidate.apk").resolve(),
                apk,
            )

    def test_live_feed_routes_keep_candidate_and_production_daemons_separate(self) -> None:
        nginx = releases.render_live_feed_nginx_routes(
            production_endpoint="http://127.0.0.1:8101",
            staging_endpoint="http://127.0.0.1:8102",
            release_endpoints={
                "2026-08-15.1": "http://127.0.0.1:8101",
                "2026-08-22.1": "http://127.0.0.1:8102",
            },
        )
        self.assertIn("location /live-feeds/", nginx)
        self.assertIn("proxy_pass http://127.0.0.1:8101;", nginx)
        self.assertIn("location /staging/live-feeds/", nginx)
        self.assertIn("proxy_pass http://127.0.0.1:8102/live-feeds/;", nginx)
        self.assertIn("location /releases/2026-08-22.1/live-feeds/", nginx)


if __name__ == "__main__":
    unittest.main()
