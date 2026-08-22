#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import os
import sys
import unittest
from datetime import datetime, timezone
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
    def test_public_cli_exposes_only_stage_and_promote(self) -> None:
        with mock.patch.object(sys, "argv", ["prod_manage.py", "--stage"]):
            args = prod_manage.parse_args()
        self.assertTrue(args.stage)
        self.assertFalse(args.promote)
        self.assertFalse(hasattr(args, "config"))
        self.assertFalse(hasattr(args, "releases"))

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

    def test_stage_refuses_to_overwrite_an_existing_candidate(self) -> None:
        with self.assertRaisesRegex(prod_manage.ManagementError, "already names"):
            prod_manage.stage_document(
                desired_document(staging="2026-08-21.1"), "2026-08-22.1"
            )

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


class PromotionGateTests(unittest.TestCase):
    def desired(self) -> releases.DesiredReleases:
        return releases.parse_desired_releases(
            desired_document(staging="2026-08-22.1")
        )

    def observed(self, *, status: str = "passed") -> dict:
        return {
            "production": "2026-08-20.1",
            "staging": "2026-08-22.1",
            "releases": {
                "2026-08-22.1": {"qualification_status": status},
            },
        }

    def test_qualified_active_staging_release_can_be_promoted(self) -> None:
        with mock.patch.object(
            prod_manage, "load_remote_observed", return_value=self.observed()
        ):
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
        observed["staging"] = "some-other-release"
        with mock.patch.object(
            prod_manage, "load_remote_observed", return_value=observed
        ):
            with self.assertRaisesRegex(prod_manage.ManagementError, "not the currently"):
                prod_manage.assert_staging_is_qualified({}, self.desired())


class ReconciliationCompletionTests(unittest.TestCase):
    def test_deploy_waits_for_the_async_reconciler_terminal_result(self) -> None:
        config = {"ssh_user": "root", "ssh_host": "prod"}
        with (
            mock.patch.object(prod_manage, "run"),
            mock.patch.object(
                prod_manage.deploy_prod, "load_config", return_value=config
            ),
            mock.patch.object(prod_manage.deploy_prod, "run_ssh") as run_ssh,
        ):
            prod_manage.deploy(prod_manage.DEFAULT_CONFIG)

        command = run_ssh.call_args.args[1]
        self.assertIn("while true", command)
        self.assertIn("active|activating|reloading|deactivating", command)
        self.assertIn("release reconciliation completed successfully", command)
        self.assertIn("journalctl -u", command)


class StageOrderingTests(unittest.TestCase):
    def test_existing_staging_intent_resumes_deployment_without_new_git_changes(self) -> None:
        document = desired_document(staging="2026-08-22.1")

        def fake_git(*args: str, capture: bool = True) -> str:
            if args[:2] == ("branch", "--show-current"):
                return "main"
            if args[:3] == ("rev-list", "--left-right", "--count"):
                return "0 0"
            if args[:2] == ("status", "--porcelain"):
                return ""
            return ""

        with (
            mock.patch.object(prod_manage.deploy_prod, "load_config", return_value={}),
            mock.patch.object(prod_manage, "git", side_effect=fake_git) as git,
            mock.patch.object(prod_manage, "assert_remote_idle") as idle,
            mock.patch.object(prod_manage, "load_release_document", return_value=document),
            mock.patch.object(prod_manage, "print_proposal") as proposal,
            mock.patch.object(prod_manage, "confirmed", return_value=True),
            mock.patch.object(prod_manage, "deploy") as deploy,
        ):
            result = prod_manage.stage(
                prod_manage.DEFAULT_CONFIG, prod_manage.DEFAULT_RELEASES
            )

        self.assertEqual(result, 0)
        self.assertEqual(idle.call_count, 2)
        self.assertFalse(
            any(
                call.args and call.args[0] in {"add", "commit", "tag", "push"}
                for call in git.call_args_list
            )
        )
        self.assertIn("resume staging 2026-08-22.1", proposal.call_args.args[0])
        deploy.assert_called_once_with(prod_manage.DEFAULT_CONFIG)

    def test_running_reconciler_aborts_before_confirmation_or_git_mutation(self) -> None:
        document = desired_document()

        def fake_git(*args: str, capture: bool = True) -> str:
            if args[:2] == ("branch", "--show-current"):
                return "main"
            if args[:3] == ("rev-list", "--left-right", "--count"):
                return "0 0"
            return ""

        with (
            mock.patch.object(prod_manage.deploy_prod, "load_config", return_value={}),
            mock.patch.object(prod_manage, "git", side_effect=fake_git),
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
            if args[:2] == ("branch", "--show-current"):
                return "main"
            if args[:3] == ("rev-list", "--left-right", "--count"):
                return "0 0"
            if args[:2] == ("status", "--porcelain"):
                return " M app"
            return ""

        with (
            mock.patch.object(prod_manage.deploy_prod, "load_config", return_value={}),
            mock.patch.object(prod_manage, "git", side_effect=fake_git),
            mock.patch.object(prod_manage, "assert_remote_idle") as idle,
            mock.patch.object(prod_manage, "load_release_document", return_value=document),
            mock.patch.object(
                prod_manage, "next_release_name", return_value="2026-08-22.1"
            ),
            mock.patch.object(prod_manage, "print_proposal"),
            mock.patch.object(prod_manage, "confirmed", return_value=True),
            mock.patch.object(prod_manage, "write_atomic") as write_atomic,
            mock.patch.object(prod_manage, "deploy") as deploy,
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
                ("add", "-A"),
                ("commit", "-m", "Prepare release 2026-08-22.1"),
                ("tag", "-a", "2026-08-22.1", "-m", "Aerobag 2026-08-22.1"),
                ("push", "origin", "main"),
                ("push", "origin", "2026-08-22.1"),
                ("add", "deploy/releases.json"),
                ("commit", "-m", "Stage 2026-08-22.1"),
                ("push", "origin", "main"),
            ],
        )
        write_atomic.assert_called_once()
        deploy.assert_called_once_with(prod_manage.DEFAULT_CONFIG)


if __name__ == "__main__":
    unittest.main()
