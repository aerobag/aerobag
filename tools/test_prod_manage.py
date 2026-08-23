#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import os
import io
import subprocess
import sys
import tempfile
import unittest
from contextlib import ExitStack, redirect_stderr, redirect_stdout
from datetime import datetime, timezone
from pathlib import Path
from types import SimpleNamespace
from unittest import mock


TOOLS_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS_DIR)

import prod_manage  # noqa: E402
import release_reconciler as releases  # noqa: E402


def desired_document(*, staging: str | None = None) -> dict:
    return {
        "schema_version": 1,
        "production": {"tag": "2026-08-20.1"},
        "staging": None if staging is None else {"tag": staging},
        "sunset": [
            {"tag": "2026-08-01.1", "until_utc": "2026-09-01T00:00:00Z"}
        ],
    }


class DesiredStateMutationTests(unittest.TestCase):
    def test_prod_manage_is_the_only_public_production_cli(self) -> None:
        self.assertFalse((Path(TOOLS_DIR) / "deploy_prod").exists())
        self.assertFalse((Path(TOOLS_DIR) / "deploy_prod.py").exists())

    def test_public_cli_exposes_stage_promote_and_reconcile(self) -> None:
        with mock.patch.object(sys, "argv", ["prod_manage.py", "--stage"]):
            args = prod_manage.parse_args()
        self.assertTrue(args.stage)
        self.assertFalse(args.promote)
        self.assertFalse(args.reconcile)
        self.assertFalse(hasattr(args, "config"))
        self.assertFalse(hasattr(args, "releases"))

        with mock.patch.object(sys, "argv", ["prod_manage.py", "--reconcile"]):
            args = prod_manage.parse_args()
        self.assertTrue(args.reconcile)


class OperationLogTests(unittest.TestCase):
    @staticmethod
    def reconcile_args() -> SimpleNamespace:
        return SimpleNamespace(stage=False, promote=False, reconcile=True)

    def test_success_discards_quiet_operation_log(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            log_path = Path(temp_dir) / "operation.log"

            def successful_reconcile(*_args: object) -> int:
                prod_manage.deployment.append_command_log("hidden shell fluff")
                return 0

            stdout = io.StringIO()
            stderr = io.StringIO()
            with (
                mock.patch.object(
                    prod_manage, "parse_args", return_value=self.reconcile_args()
                ),
                mock.patch.object(
                    prod_manage, "create_operation_log", return_value=log_path
                ),
                mock.patch.object(
                    prod_manage, "reconcile", side_effect=successful_reconcile
                ),
                redirect_stdout(stdout),
                redirect_stderr(stderr),
            ):
                result = prod_manage.main()

            self.assertEqual(result, 0)
            self.assertFalse(log_path.exists())
            self.assertNotIn("shell fluff", stdout.getvalue())
            self.assertEqual(stderr.getvalue(), "")

    def test_failure_retains_log_and_reports_path(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            log_path = Path(temp_dir) / "operation.log"

            def failed_reconcile(*_args: object) -> int:
                prod_manage.deployment.append_command_log("subprocess details")
                raise prod_manage.ManagementError("reconciliation failed")

            stderr = io.StringIO()
            with (
                mock.patch.object(
                    prod_manage, "parse_args", return_value=self.reconcile_args()
                ),
                mock.patch.object(
                    prod_manage, "create_operation_log", return_value=log_path
                ),
                mock.patch.object(
                    prod_manage, "reconcile", side_effect=failed_reconcile
                ),
                redirect_stderr(stderr),
            ):
                result = prod_manage.main()

            self.assertEqual(result, 2)
            log = log_path.read_text(encoding="utf-8")
            self.assertIn("subprocess details", log)
            self.assertIn("reconciliation failed", log)
            self.assertIn("reconciliation failed", stderr.getvalue())
            self.assertIn(str(log_path), stderr.getvalue())


class DesiredStateBehaviorTests(unittest.TestCase):
    def test_next_release_name_increments_only_the_current_utc_day(self) -> None:
        self.assertEqual(
            prod_manage.next_release_name(
                {"2026-08-21.9", "2026-08-22.1", "2026-08-22.3"},
                datetime(2026, 8, 22, 18, tzinfo=timezone.utc),
            ),
            "2026-08-22.4",
        )

    def test_stage_changes_only_the_staging_binding(self) -> None:
        original = desired_document()
        proposed = prod_manage.stage_document(original, "2026-08-22.1")

        self.assertEqual(proposed["staging"], {"tag": "2026-08-22.1"})
        self.assertEqual(proposed["production"], original["production"])
        self.assertEqual(proposed["sunset"], original["sunset"])
        self.assertIsNone(original["staging"])

    def test_stage_replaces_an_existing_candidate_without_changing_other_bindings(self) -> None:
        original = desired_document(staging="2026-08-21.1")
        proposed = prod_manage.stage_document(original, "2026-08-22.1")

        self.assertEqual(proposed["staging"], {"tag": "2026-08-22.1"})
        self.assertEqual(proposed["production"], original["production"])
        self.assertEqual(proposed["sunset"], original["sunset"])

    def test_promotion_moves_staging_to_production_without_guessing_sunset(self) -> None:
        original = desired_document(staging="2026-08-22.1")
        proposed, old, candidate = prod_manage.promotion_document(original)

        self.assertEqual(old, "2026-08-20.1")
        self.assertEqual(candidate, "2026-08-22.1")
        self.assertEqual(proposed["production"], {"tag": candidate})
        self.assertIsNone(proposed["staging"])
        self.assertEqual(proposed["sunset"], original["sunset"])

    def test_promotion_requires_a_staging_candidate(self) -> None:
        with self.assertRaisesRegex(prod_manage.ManagementError, "no staging"):
            prod_manage.promotion_document(desired_document())


class PromotionContractCompatibilityTests(unittest.TestCase):
    def test_product_registry_parser_discovers_new_families_generically(self) -> None:
        source = """
pub const NAV_DB_CONTRACT_ID: &str = "NAV9";
pub const NEW_THING_CONTRACT_ID: &str = "NEW1";
pub const PRODUCT_CONTRACTS: &[ProductContract] = &[
    ProductContract {
        family_id: "nav-db",
        contract_id: NAV_DB_CONTRACT_ID,
    },
    ProductContract {
        family_id: "new-thing",
        contract_id: NEW_THING_CONTRACT_ID,
    },
];
"""

        self.assertEqual(
            prod_manage.parse_product_contracts(source, ref="candidate"),
            {"nav-db": "NAV9", "new-thing": "NEW1"},
        )

    def test_product_registry_parser_fails_closed_on_unknown_layout(self) -> None:
        source = """
pub const NAV_DB_CONTRACT_ID: &str = "NAV9";
pub const PRODUCT_CONTRACTS: &[ProductContract] = &[
    ProductContract::new("nav-db", NAV_DB_CONTRACT_ID),
];
"""

        with self.assertRaisesRegex(
            prod_manage.ManagementError, "cannot safely enumerate every"
        ):
            prod_manage.parse_product_contracts(source, ref="candidate")

    def test_changed_production_contracts_are_reported(self) -> None:
        document = desired_document(staging="2026-08-22.1")
        document["sunset"][0]["until_utc"] = "2099-01-01T00:00:00Z"
        desired = releases.parse_desired_releases(document)
        contract_sets = {
            "2026-08-20.1": {"nav-db": "NAV9", "live-feeds": "v2"},
            "2026-08-22.1": {"nav-db": "NAV10", "live-feeds": "v3"},
            "2026-08-01.1": {"nav-db": "NAV8", "live-feeds": "v1"},
        }

        with mock.patch.object(
            prod_manage, "release_contracts", side_effect=contract_sets.__getitem__
        ):
            changed = prod_manage.changed_contracts_after_promotion(desired)

        self.assertEqual(
            changed,
            (
                prod_manage.ContractRequirement("live-feeds", "v2"),
                prod_manage.ContractRequirement("nav-db", "NAV9"),
            ),
        )

    def test_existing_sunset_does_not_hide_changed_production_contracts(self) -> None:
        document = desired_document(staging="2026-08-22.1")
        document["sunset"][0]["until_utc"] = "2099-01-01T00:00:00Z"
        desired = releases.parse_desired_releases(document)
        contract_sets = {
            "2026-08-20.1": {"nav-db": "NAV9", "live-feeds": "v2"},
            "2026-08-22.1": {"nav-db": "NAV10", "live-feeds": "v3"},
            "2026-08-01.1": {"nav-db": "NAV9", "live-feeds": "v2"},
        }

        with mock.patch.object(
            prod_manage, "release_contracts", side_effect=contract_sets.__getitem__
        ):
            changed = prod_manage.changed_contracts_after_promotion(desired)

        self.assertEqual(
            changed,
            (
                prod_manage.ContractRequirement("live-feeds", "v2"),
                prod_manage.ContractRequirement("nav-db", "NAV9"),
            ),
        )

    def test_warning_recommends_retaining_current_production(self) -> None:
        warning = prod_manage.promotion_compatibility_warning(
            "2026-08-20.1",
            (prod_manage.ContractRequirement("nav-db", "NAV9"),),
        )

        self.assertIn("release-scoped package and live-feed endpoints", warning)
        self.assertIn("nav-db contract NAV9", warning)
        self.assertIn("adding 2026-08-20.1 to the sunset list", warning)

    def test_unchanged_contracts_still_warn_about_release_scoped_urls(self) -> None:
        warning = prod_manage.promotion_compatibility_warning("2026-08-20.1", ())

        self.assertIn("release-scoped package and live-feed endpoints", warning)
        self.assertIn("adding 2026-08-20.1 to the sunset list", warning)


class PromotionGateTests(unittest.TestCase):
    def desired(self) -> releases.DesiredReleases:
        return releases.parse_desired_releases(
            desired_document(staging="2026-08-22.1")
        )

    def observed(self, *, status: str = "passed") -> releases.ObservedState:
        candidate = releases.ObservedRelease(
            tag="2026-08-22.1",
            tag_object="1" * 40,
            commit="2" * 40,
            qualification_status=status,
        )
        return releases.ObservedState(
            production="2026-08-20.1",
            staging=candidate.tag,
            releases={candidate.tag: candidate},
        )

    def test_qualified_active_staging_release_can_be_promoted(self) -> None:
        with (
            mock.patch.object(
                prod_manage, "load_remote_observed", return_value=self.observed()
            ),
            mock.patch.object(
                prod_manage,
                "reconciliation_plan",
                return_value=releases.ReconciliationPlan([]),
            ),
        ):
            prod_manage.assert_staging_is_qualified({}, self.desired())

    def test_qualified_but_nonconverged_staging_requires_reconciliation(self) -> None:
        pending = releases.ReconciliationPlan(
            [releases.ReconcileAction("start_live_feeds", "2026-08-22.1")]
        )
        with (
            mock.patch.object(
                prod_manage, "load_remote_observed", return_value=self.observed()
            ),
            mock.patch.object(
                prod_manage, "reconciliation_plan", return_value=pending
            ),
        ):
            with self.assertRaisesRegex(prod_manage.ManagementError, "fully converged"):
                prod_manage.assert_staging_is_qualified({}, self.desired())

    def test_unqualified_staging_release_is_rejected_before_git_mutation(self) -> None:
        with mock.patch.object(
            prod_manage,
            "load_remote_observed",
            return_value=self.observed(status="pending"),
        ):
            with self.assertRaisesRegex(prod_manage.ManagementError, "not passed"):
                prod_manage.assert_staging_is_qualified({}, self.desired())

    def test_wrong_active_staging_release_is_rejected(self) -> None:
        observed = self.observed()
        observed.staging = "some-other-release"
        with mock.patch.object(
            prod_manage, "load_remote_observed", return_value=observed
        ):
            with self.assertRaisesRegex(prod_manage.ManagementError, "not the currently"):
                prod_manage.assert_staging_is_qualified({}, self.desired())


class ReconciliationCompletionTests(unittest.TestCase):
    def test_timed_operation_reports_phase_and_elapsed_time(self) -> None:
        output = io.StringIO()
        with (
            mock.patch.object(prod_manage.time, "monotonic", side_effect=[10.0, 12.5]),
            redirect_stdout(output),
        ):
            prod_manage.timed_operation(
                "Promoting release",
                lambda: prod_manage.report_deployment_progress("Switching channel"),
            )

        self.assertIn("Promoting release...", output.getvalue())
        self.assertIn("Switching channel...", output.getvalue())
        self.assertIn("Promoting release complete (2.5s).", output.getvalue())

    def test_deploy_calls_internal_full_host_reconciliation(self) -> None:
        config = {"host": "prod"}
        with (
            mock.patch.object(prod_manage.deployment, "load_config", return_value=config),
            mock.patch.object(prod_manage.deployment, "reconcile_host") as reconcile_host,
        ):
            prod_manage.deploy(prod_manage.DEFAULT_CONFIG)

        reconcile_host.assert_called_once_with(
            config,
            progress=prod_manage.report_deployment_progress,
        )

    def test_multiline_ssh_failure_is_reported_without_echoing_shell_body(self) -> None:
        summary = prod_manage.failed_command_summary(
            [
                "ssh",
                "-o",
                "BatchMode=yes",
                "root@aerobag-prod",
                "set -euo pipefail\nwhile true; do\n  sleep 1\ndone",
            ]
        )

        self.assertEqual(summary, "remote command on root@aerobag-prod")
        self.assertNotIn("while", summary)

    def test_runtime_repair_calls_internal_config_repair(self) -> None:
        config = {"host": "prod"}
        with (
            mock.patch.object(prod_manage.deployment, "load_config", return_value=config),
            mock.patch.object(prod_manage.deployment, "repair_runtime") as repair,
        ):
            prod_manage.repair_runtime(prod_manage.DEFAULT_CONFIG)

        repair.assert_called_once_with(
            config,
            progress=prod_manage.report_deployment_progress,
        )


class StageOrderingTests(unittest.TestCase):
    @staticmethod
    def clean_git(*args: str, capture: bool = True) -> str:
        if args[:2] == ("status", "--porcelain"):
            return ""
        if args[:2] == ("branch", "--show-current"):
            return "main"
        if args[:3] == ("rev-list", "--left-right", "--count"):
            return "0 0"
        if args[:2] == ("rev-parse", "HEAD"):
            return "a" * 40
        if args[:2] == ("tag", "--list"):
            return ""
        return ""

    def test_dirty_checkout_is_rejected_before_network_or_mutation(self) -> None:
        with (
            mock.patch.object(prod_manage, "git", return_value=" M app") as git,
            mock.patch.object(prod_manage.deployment, "load_config") as load_config,
        ):
            with self.assertRaisesRegex(
                prod_manage.ManagementError, "can't stage with uncommitted work"
            ):
                prod_manage.stage(
                    prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
                )

        git.assert_called_once_with("status", "--porcelain")
        load_config.assert_not_called()

    def test_commit_already_assigned_to_staging_exits_without_prod_access(self) -> None:
        document = desired_document(staging="2026-08-22.1")

        with (
            mock.patch.object(prod_manage, "git", side_effect=self.clean_git),
            mock.patch.object(prod_manage, "load_release_document", return_value=document),
            mock.patch.object(
                prod_manage.releases,
                "resolve_release_tag",
                return_value=releases.ResolvedTag(
                    tag="2026-08-22.1",
                    tag_object="b" * 40,
                    commit="a" * 40,
                ),
            ),
            mock.patch.object(prod_manage.deployment, "load_config") as load_config,
            mock.patch.object(prod_manage, "assert_remote_idle") as idle,
        ):
            with self.assertRaisesRegex(
                prod_manage.ManagementError,
                "already staged as 2026-08-22.1.*--reconcile",
            ):
                prod_manage.stage(
                    prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
                )

        load_config.assert_not_called()
        idle.assert_not_called()

    def test_running_reconciler_aborts_before_confirmation_or_git_mutation(self) -> None:
        document = desired_document()

        with (
            mock.patch.object(prod_manage.deployment, "load_config", return_value={}),
            mock.patch.object(prod_manage, "git", side_effect=self.clean_git),
            mock.patch.object(
                prod_manage,
                "assert_remote_idle",
                side_effect=prod_manage.ManagementError("still running"),
            ),
            mock.patch.object(prod_manage, "load_release_document", return_value=document),
            mock.patch.object(prod_manage, "confirmed") as confirmed,
        ):
            with self.assertRaisesRegex(prod_manage.ManagementError, "still running"):
                prod_manage.stage(
                    prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
                )

        confirmed.assert_not_called()

    def test_confirmed_stage_executes_the_printed_git_transaction_in_order(self) -> None:
        document = desired_document()
        git_calls: list[tuple[tuple[str, ...], bool]] = []

        def fake_git(*args: str, capture: bool = True) -> str:
            git_calls.append((args, capture))
            return self.clean_git(*args, capture=capture)

        with (
            mock.patch.object(prod_manage.deployment, "load_config", return_value={}),
            mock.patch.object(prod_manage, "git", side_effect=fake_git),
            mock.patch.object(prod_manage, "assert_remote_idle") as idle,
            mock.patch.object(prod_manage, "load_release_document", return_value=document),
            mock.patch.object(
                prod_manage, "next_release_name", return_value="2026-08-22.1"
            ),
            mock.patch.object(prod_manage, "print_proposal"),
            mock.patch.object(prod_manage, "confirmed", return_value=True),
            mock.patch.object(prod_manage, "write_atomic") as write_atomic,
            mock.patch.object(prod_manage, "reconcile", return_value=0) as reconcile,
        ):
            result = prod_manage.stage(
                prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
            )

        self.assertEqual(result, 0)
        self.assertEqual(idle.call_count, 2)
        mutation_calls = [
            args
            for args, capture in git_calls
            if not capture and args[0] not in {"fetch"}
        ]
        self.assertEqual(
            mutation_calls,
            [
                ("add", "deploy/releases.json"),
                ("commit", "-m", "Stage 2026-08-22.1"),
                ("tag", "-a", "2026-08-22.1", "-m", "Aerobag 2026-08-22.1"),
                ("push", "--atomic", "origin", "main", "2026-08-22.1"),
            ],
        )
        write_atomic.assert_called_once()
        reconcile.assert_called_once_with(
            prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
        )


class ReconcileCommandTests(unittest.TestCase):
    def desired(self) -> releases.DesiredReleases:
        return releases.parse_desired_releases(desired_document())

    def common_patches(self, plan: releases.ReconciliationPlan):
        return (
            mock.patch.object(prod_manage, "assert_clean_checkout"),
            mock.patch.object(prod_manage, "git", return_value=""),
            mock.patch.object(prod_manage, "assert_main_not_behind"),
            mock.patch.object(
                prod_manage, "load_release_document", return_value=desired_document()
            ),
            mock.patch.object(prod_manage.deployment, "load_config", return_value={}),
            mock.patch.object(prod_manage, "assert_remote_idle"),
            mock.patch.object(
                prod_manage,
                "load_remote_observed",
                return_value=releases.ObservedState.empty(),
            ),
            mock.patch.object(
                prod_manage, "remote_runtime_failures", return_value=[]
            ),
            mock.patch.object(prod_manage, "reconciliation_plan", return_value=plan),
        )

    def test_converged_reconcile_reports_success_without_deploying(self) -> None:
        patches = self.common_patches(releases.ReconciliationPlan([]))
        with ExitStack() as stack:
            for patcher in patches:
                stack.enter_context(patcher)
            deploy = stack.enter_context(mock.patch.object(prod_manage, "deploy"))
            success = stack.enter_context(
                mock.patch.object(prod_manage, "print_success")
            )
            result = prod_manage.reconcile(
                prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
            )

        self.assertEqual(result, 0)
        deploy.assert_not_called()
        self.assertIn("Production is reconciled", success.call_args.args[0])

    def test_runtime_probe_does_not_require_latest_local_controller_commit(self) -> None:
        desired = self.desired()
        completed = subprocess.CompletedProcess(
            args=["ssh"], returncode=0, stdout="", stderr=""
        )
        config = {"source_root": "/source", "artifact_root": "/artifacts"}
        with mock.patch.object(
            prod_manage.deployment, "run_ssh", return_value=completed
        ) as run_ssh:
            failures = prod_manage.remote_runtime_failures(
                config, desired, releases.ObservedState.empty()
            )

        self.assertEqual(failures, [])
        command = run_ssh.call_args.args[1]
        self.assertIn(f"test -s {prod_manage.deployment.DEPLOYED_REV_FILE}", command)
        self.assertNotIn("installed controller revision differs", command)

    def test_nonconverged_reconcile_deploys_then_verifies_convergence(self) -> None:
        pending = releases.ReconciliationPlan(
            [releases.ReconcileAction("build_release", "2026-08-20.1")]
        )
        patches = self.common_patches(pending)
        with ExitStack() as stack:
            for patcher in patches[:-1]:
                stack.enter_context(patcher)
            stack.enter_context(
                mock.patch.object(
                    prod_manage,
                    "reconciliation_plan",
                    side_effect=[pending, releases.ReconciliationPlan([])],
                )
            )
            deploy = stack.enter_context(mock.patch.object(prod_manage, "deploy"))
            success = stack.enter_context(
                mock.patch.object(prod_manage, "print_success")
            )
            result = prod_manage.reconcile(
                prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
            )

        self.assertEqual(result, 0)
        deploy.assert_called_once_with(prod_manage.DEFAULT_CONFIG)
        self.assertIn("reconciliation completed", success.call_args.args[0])

    def test_converged_journal_with_missing_runtime_is_repaired(self) -> None:
        converged = releases.ReconciliationPlan([])
        patches = self.common_patches(converged)
        with ExitStack() as stack:
            for patcher in patches[:-2]:
                stack.enter_context(patcher)
            stack.enter_context(
                mock.patch.object(
                    prod_manage,
                    "remote_runtime_failures",
                    side_effect=[
                        [
                            prod_manage.RuntimeFailure(
                                "host", "controller source is not installed"
                            )
                        ],
                        [],
                    ],
                )
            )
            stack.enter_context(
                mock.patch.object(
                    prod_manage, "reconciliation_plan", return_value=converged
                )
            )
            deploy = stack.enter_context(mock.patch.object(prod_manage, "deploy"))
            stack.enter_context(mock.patch.object(prod_manage, "print_success"))
            result = prod_manage.reconcile(
                prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
            )

        self.assertEqual(result, 0)
        deploy.assert_called_once_with(prod_manage.DEFAULT_CONFIG)

    def test_service_only_drift_uses_runtime_repair_without_full_deploy(self) -> None:
        converged = releases.ReconciliationPlan([])
        patches = self.common_patches(converged)
        with ExitStack() as stack:
            for patcher in patches[:-2]:
                stack.enter_context(patcher)
            stack.enter_context(
                mock.patch.object(
                    prod_manage,
                    "remote_runtime_failures",
                    side_effect=[
                        [
                            prod_manage.RuntimeFailure(
                                "service", "aerobag-cloud-server.service is not active"
                            )
                        ],
                        [],
                    ],
                )
            )
            stack.enter_context(
                mock.patch.object(
                    prod_manage, "reconciliation_plan", return_value=converged
                )
            )
            deploy = stack.enter_context(mock.patch.object(prod_manage, "deploy"))
            repair = stack.enter_context(
                mock.patch.object(prod_manage, "repair_runtime")
            )
            stack.enter_context(mock.patch.object(prod_manage, "print_success"))
            result = prod_manage.reconcile(
                prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
            )

        self.assertEqual(result, 0)
        deploy.assert_not_called()
        repair.assert_called_once_with(prod_manage.DEFAULT_CONFIG)


class PromoteCommandTests(unittest.TestCase):
    def test_nothing_staged_exits_without_prod_access(self) -> None:
        with (
            mock.patch.object(
                prod_manage,
                "git",
                side_effect=StageOrderingTests.clean_git,
            ),
            mock.patch.object(
                prod_manage,
                "load_release_document",
                return_value=desired_document(),
            ),
            mock.patch.object(prod_manage.deployment, "load_config") as load_config,
            mock.patch.object(prod_manage, "assert_remote_idle") as idle,
        ):
            with self.assertRaisesRegex(
                prod_manage.ManagementError, "nothing is staged.*--reconcile"
            ):
                prod_manage.promote(
                    prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
                )

        load_config.assert_not_called()
        idle.assert_not_called()

    def test_successful_promotion_commits_intent_then_reconciles(self) -> None:
        document = desired_document(staging="2026-08-22.1")
        git_calls: list[tuple[tuple[str, ...], bool]] = []

        def fake_git(*args: str, capture: bool = True) -> str:
            git_calls.append((args, capture))
            return StageOrderingTests.clean_git(*args, capture=capture)

        with (
            mock.patch.object(prod_manage, "git", side_effect=fake_git),
            mock.patch.object(
                prod_manage, "load_release_document", return_value=document
            ),
            mock.patch.object(prod_manage.deployment, "load_config", return_value={}),
            mock.patch.object(prod_manage, "assert_remote_idle") as idle,
            mock.patch.object(prod_manage, "assert_staging_is_qualified"),
            mock.patch.object(prod_manage, "print_proposal"),
            mock.patch.object(prod_manage, "confirmed", return_value=True),
            mock.patch.object(prod_manage, "write_atomic"),
            mock.patch.object(
                prod_manage,
                "changed_contracts_after_promotion",
                return_value=(prod_manage.ContractRequirement("nav-db", "NAV9"),),
            ),
            mock.patch.object(prod_manage, "print_warning") as print_warning,
            mock.patch.object(prod_manage, "activate_release_intent") as activate,
            mock.patch.object(prod_manage, "reconcile", return_value=0) as reconcile,
        ):
            result = prod_manage.promote(
                prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
            )

        self.assertEqual(result, 0)
        self.assertEqual(idle.call_count, 2)
        mutation_calls = [
            args
            for args, capture in git_calls
            if not capture and args[0] not in {"fetch"}
        ]
        self.assertEqual(
            mutation_calls,
            [
                ("add", "deploy/releases.json"),
                ("commit", "-m", "Promote 2026-08-22.1"),
                ("push", "origin", "main"),
            ],
        )
        activate.assert_called_once_with(prod_manage.DEFAULT_CONFIG)
        self.assertIn("nav-db contract NAV9", print_warning.call_args.args[0])
        reconcile.assert_called_once_with(
            prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
        )


if __name__ == "__main__":
    unittest.main()
