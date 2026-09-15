#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

import io
import subprocess
import sys
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import prod_manage as manage  # noqa: E402
import release_ci as ci  # noqa: E402
import release_reconciler as releases  # noqa: E402


class WatchCliTests(unittest.TestCase):
    def test_watch_is_optional_and_accepts_stage_or_status(self):
        for operation in ("--stage", "--qualification-status"):
            with self.subTest(operation=operation), mock.patch.object(sys, "argv", ["prod_manage.py", operation]):
                args = manage.parse_args()
                self.assertFalse(args.watch)
                self.assertEqual(args.watch_timeout, 3600)
            with mock.patch.object(sys, "argv", ["prod_manage.py", operation, "--watch", "--watch-timeout", "90"]):
                args = manage.parse_args()
                self.assertTrue(args.watch)
                self.assertEqual(args.watch_timeout, 90)
                self.assertTrue(manage.operation_requires_github_authentication(args))

    def test_invalid_watch_combinations_fail_before_doing_work(self):
        for arguments in (
            ["--promote", "--watch"], ["--reconcile", "--watch"],
            ["--prequalify", "--watch"], ["--candidate-status", "--watch"],
            ["--stage", "--watch-timeout", "30"],
            ["--stage", "--watch", "--watch-timeout", "0"],
            ["--stage", "--watch", "--watch-timeout", "-1"],
        ):
            with (
                self.subTest(arguments=arguments),
                mock.patch.object(sys, "argv", ["prod_manage.py", *arguments]),
                redirect_stderr(io.StringIO()), self.assertRaises(SystemExit),
            ):
                manage.parse_args()

    def test_main_routes_watch_options_to_both_operations(self):
        for operation, name in (("--stage", "stage"), ("--qualification-status", "qualification_status")):
            with (
                self.subTest(operation=operation),
                mock.patch.object(sys, "argv", ["prod_manage.py", operation, "--watch", "--watch-timeout", "90"]),
                mock.patch.object(manage, "github_authentication_command", return_value=None),
                mock.patch.object(manage, name, return_value=0) as execute,
            ):
                self.assertEqual(manage.main(), 0)
                execute.assert_called_once_with(
                    manage.DEFAULT_CONFIG, manage.DEFAULT_RELEASES,
                    watch=True, watch_timeout_seconds=90,
                )


class ScheduledRefreshWaitTests(unittest.TestCase):
    def setUp(self):
        self.now = 0.0
        self.output = io.StringIO()
        self.enterContext(redirect_stdout(self.output))
        self.enterContext(mock.patch.object(manage.time, "monotonic", side_effect=lambda: self.now))
        self.sleep = self.enterContext(mock.patch.object(manage.time, "sleep", side_effect=self.advance))
        self.probe = self.enterContext(mock.patch.object(manage.deployment, "assert_release_reconciliation_idle"))

    def advance(self, seconds):
        self.now += seconds

    def busy(self, *, automatic=True, kind="service", progress="Refreshing cycle products for release A"):
        return manage.deployment.ReleaseReconciliationBusy(
            kind=kind, active_state="activating", automatic=automatic, progress=progress,
        )

    def test_idle_returns_without_sleep_or_wait_message(self):
        manage.assert_remote_idle({}, wait_for_scheduled_refresh=True)
        self.sleep.assert_not_called()
        self.probe.assert_called_once_with({}, dry_run=False, timeout_seconds=30)
        self.assertEqual(self.output.getvalue(), "")

    def test_scheduled_refresh_resumes_and_reports_changing_progress(self):
        self.probe.side_effect = [self.busy(), self.busy(progress="Switching release channels"), None]
        manage.assert_remote_idle({}, wait_for_scheduled_refresh=True)
        self.assertEqual(self.sleep.call_args_list, [mock.call(10), mock.call(10)])
        self.assertEqual(self.probe.call_count, 3)
        self.assertIn("Refreshing cycle products for release A", self.output.getvalue())
        self.assertIn("Switching release channels", self.output.getvalue())
        self.assertIn("finished; continuing staging", self.output.getvalue())

    def test_plain_stage_still_fails_fast(self):
        self.probe.side_effect = self.busy()
        with self.assertRaisesRegex(manage.ManagementError, "automatic scheduled product refresh"):
            manage.assert_remote_idle({})
        self.sleep.assert_not_called()
        self.probe.assert_called_once_with({}, dry_run=False)

    def test_operator_reconciliation_and_unknown_locks_are_not_retried(self):
        for error in (self.busy(automatic=False), self.busy(automatic=False, kind="lock")):
            with self.subTest(error=str(error)):
                self.probe.side_effect = error
                with self.assertRaises(manage.ManagementError):
                    manage.assert_remote_idle({}, wait_for_scheduled_refresh=True)
        self.assertEqual(self.probe.call_count, 2)
        self.sleep.assert_not_called()

    def test_new_operator_reconciliation_is_not_mistaken_for_the_previous_refresh(self):
        self.probe.side_effect = [self.busy(), self.busy(automatic=False)]
        with self.assertRaisesRegex(manage.ManagementError, "another production release reconciliation"):
            manage.assert_remote_idle({}, wait_for_scheduled_refresh=True)
        self.sleep.assert_called_once_with(10)

    def test_twenty_minute_timeout_is_bounded_and_preserves_last_progress(self):
        self.probe.side_effect = self.busy()
        with self.assertRaisesRegex(manage.ManagementError, "Timed out after 1200s.*staging has not started.*release A"):
            manage.assert_remote_idle({}, wait_for_scheduled_refresh=True)
        self.assertEqual(self.now, 1200)
        self.assertEqual(self.probe.call_count, 120)
        self.assertEqual(self.probe.call_args.kwargs["timeout_seconds"], 10)
        self.assertNotIn("finished; continuing", self.output.getvalue())

    def test_slow_probes_consume_the_same_budget_as_sleep(self):
        def slow_probe(*_args, **kwargs):
            self.advance(kwargs["timeout_seconds"])
            raise self.busy()

        self.probe.side_effect = slow_probe
        with self.assertRaisesRegex(manage.ManagementError, "Timed out after 1200s"):
            manage.assert_remote_idle({}, wait_for_scheduled_refresh=True)
        self.assertEqual(self.now, 1200)
        self.assertEqual(self.probe.call_count, 30)
        self.assertEqual(self.probe.call_args.kwargs["timeout_seconds"], 30)

    def test_status_read_failures_stop_without_retry(self):
        for error, expected, message in (
            (subprocess.CalledProcessError(255, "ssh"), subprocess.CalledProcessError, "255"),
            (subprocess.TimeoutExpired("ssh", 30), manage.ManagementError, "status probe timed out"),
        ):
            with self.subTest(error=str(error)):
                self.probe.side_effect = error
                with self.assertRaisesRegex(expected, message):
                    manage.assert_remote_idle({}, wait_for_scheduled_refresh=True)
        self.sleep.assert_not_called()
        self.assertNotIn("continuing staging", self.output.getvalue())

    def test_interrupt_during_probe_or_sleep_propagates_to_cli(self):
        self.probe.side_effect = KeyboardInterrupt()
        with self.assertRaises(KeyboardInterrupt):
            manage.assert_remote_idle({}, wait_for_scheduled_refresh=True)
        self.probe.side_effect = self.busy()
        self.sleep.side_effect = KeyboardInterrupt()
        with self.assertRaises(KeyboardInterrupt):
            manage.assert_remote_idle({}, wait_for_scheduled_refresh=True)
        self.assertNotIn("finished; continuing", self.output.getvalue())


class QualificationWatchTests(unittest.TestCase):
    def setUp(self):
        self.identity = releases.ResolvedTag("2026-09-10.2", "b" * 40, "a" * 40)
        self.config = {"github_repository": "owner/project"}
        self.record = releases.ObservedRelease(
            tag=self.identity.tag, tag_object=self.identity.tag_object,
            commit=self.identity.commit, build_status="passed", qualification_status="passed",
        )
        self.observed = releases.ObservedState(
            staging=self.identity.tag, releases={self.identity.tag: self.record},
        )
        self.now = 0
        self.sleeps = []
        self.output = io.StringIO()
        self.enterContext(redirect_stdout(self.output))
        self.enterContext(mock.patch.object(manage.time, "monotonic", side_effect=lambda: self.now))
        self.sleep = self.enterContext(mock.patch.object(manage.time, "sleep", side_effect=self.advance))
        self.remote = self.enterContext(mock.patch.object(manage, "load_remote_observed", return_value=self.observed))
        self.hosted = self.enterContext(mock.patch.object(ci, "release_qualification", return_value=self.result()))
        self.success = self.enterContext(mock.patch.object(manage, "print_success"))
        self.failure = self.enterContext(mock.patch.object(manage, "print_warning"))

    def advance(self, seconds):
        self.sleeps.append(seconds)
        self.now += seconds

    def result(self, ordinary="passed", journeys="passed", detail=None):
        return ci.ReleaseQualification(
            self.identity.tag, self.identity.commit,
            ci.WorkflowQualification("ordinary CI", ordinary, ordinary, "https://github.com/ci"),
            ci.WorkflowQualification("release journeys", journeys, detail or journeys, "https://github.com/journeys"),
        )

    def watch(self, **kwargs):
        return manage.watch_qualification(self.config, self.identity, **kwargs)

    def test_immediate_pass_does_not_sleep(self):
        self.assertEqual(self.watch(), 0)
        self.sleep.assert_not_called()
        self.success.assert_called_once_with(f"Staging qualification {self.identity.tag} PASSED")
        self.remote.assert_called_once_with(self.config, timeout_seconds=30)
        self.hosted.assert_called_once_with("owner/project", self.identity.tag, self.identity.commit)

    def test_missing_and_pending_runs_wait_for_every_check_without_repetition(self):
        self.hosted.side_effect = [
            self.result("missing", "missing"), self.result("pending", "pending"),
            self.result("passed", "pending"), self.result(),
        ]
        self.assertEqual(self.watch(), 0)
        self.assertEqual(self.sleeps, [30, 30, 30])
        self.assertEqual(self.hosted.call_count, 4)
        self.assertTrue(all(call == mock.call("owner/project", self.identity.tag, self.identity.commit)
                            for call in self.hosted.call_args_list))
        self.assertIn("https://github.com/journeys", self.output.getvalue())
        self.failure.assert_not_called()

    def test_unchanged_pending_status_prints_heartbeat_without_repeating_details(self):
        pending = self.result(journeys="pending")
        self.hosted.side_effect = [pending, pending, self.result()]
        self.assertEqual(self.watch(), 0)
        self.assertEqual(self.output.getvalue().count("release journeys: pending"), 1)
        self.assertEqual(self.output.getvalue().count("Still waiting"), 2)

    def test_deployed_checks_are_required_even_when_hosted_is_green(self):
        self.record.qualification_status = "pending"

        def qualify(seconds):
            self.advance(seconds)
            self.record.qualification_status = "passed"

        self.sleep.side_effect = qualify
        self.assertEqual(self.watch(), 0)
        self.assertEqual(self.sleeps, [30])
        self.assertIn("Deployed staging checks: pending", self.output.getvalue())

    def test_terminal_hosted_failure_or_cancellation_stops_without_sleep(self):
        for ordinary, journeys, detail in (
            ("failed", "pending", "in_progress"),
            ("passed", "failed", "failure"),
            ("passed", "failed", "cancelled"),
            ("passed", "failed", "timed_out"),
        ):
            with self.subTest(detail=detail):
                self.hosted.return_value = self.result(ordinary, journeys, detail)
                self.assertEqual(self.watch(), 1)
        self.sleep.assert_not_called()
        self.success.assert_not_called()
        self.assertEqual(self.failure.call_count, 4)

    def test_deployed_failures_include_diagnostics_and_cannot_pass(self):
        for field in ("build_status", "deployment_status", "qualification_status"):
            with self.subTest(field=field):
                old = getattr(self.record, field)
                setattr(self.record, field, "failed")
                self.record.last_error = "deployed /about check failed"
                self.assertEqual(self.watch(), 1)
                setattr(self.record, field, old)
        self.assertIn("deployed /about check failed", self.output.getvalue())
        self.sleep.assert_not_called()
        self.success.assert_not_called()

    def test_wrong_commit_or_tag_object_cannot_qualify(self):
        for field in ("commit", "tag_object"):
            with self.subTest(field=field):
                old = getattr(self.record, field)
                setattr(self.record, field, "c" * 40)
                self.assertEqual(self.watch(), 1)
                setattr(self.record, field, old)
        self.assertIn("identity does not match", self.output.getvalue())
        self.success.assert_not_called()

    def test_replaced_active_staging_release_cannot_qualify(self):
        self.observed.staging = "2026-09-11.1"
        self.assertEqual(self.watch(), 1)
        self.assertIn("not active on staging", self.output.getvalue())
        self.success.assert_not_called()

    def test_missing_deployment_or_missing_hosted_run_times_out_not_passes(self):
        for missing in ("deployment", "hosted"):
            with self.subTest(missing=missing):
                self.now, self.sleeps = 0, []
                self.remote.return_value = releases.ObservedState.empty() if missing == "deployment" else self.observed
                self.hosted.return_value = self.result(journeys="missing" if missing == "hosted" else "passed")
                with self.assertRaisesRegex(manage.ManagementError, "TIMED OUT.*not failed.*Resume"):
                    self.watch(timeout_seconds=10)
                self.assertEqual(self.sleeps, [10])
        self.success.assert_not_called()
        self.failure.assert_not_called()

    def test_status_read_errors_stop_with_resume_advice_not_a_false_ci_failure(self):
        for error in (ci.ReleaseCiError("GitHub unavailable"), subprocess.TimeoutExpired("ssh", 30)):
            with self.subTest(error=str(error)):
                self.hosted.side_effect = error
                with self.assertRaisesRegex(manage.ManagementError, "could not read status.*Resume"):
                    self.watch()
        self.sleep.assert_not_called()
        self.success.assert_not_called()
        self.failure.assert_not_called()

    def test_interrupt_during_read_or_wait_stops_only_watching(self):
        self.hosted.side_effect = KeyboardInterrupt()
        self.assertEqual(self.watch(), 130)
        self.hosted.side_effect = None
        self.hosted.return_value = self.result(journeys="pending")
        self.sleep.side_effect = KeyboardInterrupt()
        self.assertEqual(self.watch(), 130)
        self.assertIn("jobs were not canceled", self.output.getvalue())
        self.assertIn("--qualification-status --watch", self.output.getvalue())
        self.success.assert_not_called()
        self.failure.assert_not_called()

    def test_resume_resolves_the_desired_tag_once_not_on_every_poll(self):
        self.hosted.side_effect = [self.result(journeys="pending"), self.result()]
        with (
            mock.patch.object(manage, "load_release_document") as document,
            mock.patch.object(releases, "parse_desired_releases") as desired,
            mock.patch.object(manage.deployment, "load_config", return_value=self.config),
            mock.patch.object(releases, "resolve_release_tag", return_value=self.identity) as resolve,
        ):
            desired.return_value.staging.tag = self.identity.tag
            self.assertEqual(manage.qualification_status(Path("config"), Path("releases"), watch=True), 0)
        document.assert_called_once_with(Path("releases"))
        resolve.assert_called_once_with(manage.REPO_ROOT, self.identity.tag)
        self.assertEqual(self.hosted.call_count, 2)

    def test_one_shot_status_still_returns_pending_without_sleeping(self):
        self.hosted.return_value = self.result(journeys="pending")
        with (
            mock.patch.object(manage, "load_release_document"),
            mock.patch.object(releases, "parse_desired_releases") as desired,
            mock.patch.object(manage.deployment, "load_config", return_value=self.config),
            mock.patch.object(releases, "resolve_release_tag", return_value=self.identity),
        ):
            desired.return_value.staging.tag = self.identity.tag
            self.assertEqual(manage.qualification_status(Path("config"), Path("releases")), 1)
        self.sleep.assert_not_called()
        self.hosted.assert_called_once()


if __name__ == "__main__":
    unittest.main()
